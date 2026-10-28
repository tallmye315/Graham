use super::{dividends,Dividend,DividendRevision,movements, Movement, MovementRevision, opening, Opening, OpeningRevision, trades, Trade, TradeRevision, amount_text, database_error, snapshot_from, validate_account, validate_deposit, Ledger, Snapshot};
use age::secrecy::SecretString;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::{Read, Write}, path::Path};
use uuid::Uuid;
use std::collections::HashSet;
use zeroize::Zeroizing;

const MAX_PLAINTEXT: usize = 64 * 1024 * 1024;
const MAX_CIPHERTEXT: usize = MAX_PLAINTEXT + 1024 * 1024;
const WORK_FACTOR: u8 = 18;
const INVALID_BACKUP: &str = "备份内容损坏或不完整，当前账本未修改";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupDocument {
    format: String,
    version: u32,
    schema_version: u32,
    created_at: String,
    revision_sequence: i64,
    accounts: Vec<StoredAccount>,
    deposits: Vec<StoredDeposit>,
    revisions: Vec<StoredRevision>,
    #[serde(default)]
    trades: Vec<Trade>,
    #[serde(default)]
    trade_revisions: Vec<TradeRevision>,
    #[serde(default)]
    trade_revision_sequence: i64,
    #[serde(default)]
    openings: Vec<Opening>,
    #[serde(default)]
    opening_revisions: Vec<OpeningRevision>,
    #[serde(default)]
    opening_revision_sequence: i64,
    #[serde(default)]
    movements: Vec<Movement>,
    #[serde(default)]
    movement_revisions: Vec<MovementRevision>,
    #[serde(default)]
    movement_revision_sequence: i64,
    #[serde(default)]
    dividends: Vec<Dividend>,
    #[serde(default)]
    dividend_revisions: Vec<DividendRevision>,
    #[serde(default)]
    dividend_revision_sequence: i64,
    #[serde(default)]
    imports: super::ImportArchive,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredAccount { id: String, name: String, currency: String, archived: bool }

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredDeposit { id: String, account_id: String, date: String, cents: i64, note: String, source: String, deleted: bool }

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredRevision { id: i64, entry_id: String, date: String, cents: i64, note: String, deleted: bool, changed_at: String, action: String }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    pub created_at: String,
    pub account_count: usize,
    pub record_count: usize,
    pub deleted_count: usize,
    pub fingerprint: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestored {
    pub snapshot: Snapshot,
    pub recovery_path: String,
}

fn rows<T>(connection: &Connection, sql: &str, map: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>) -> Result<Vec<T>, String> {
    let mut statement = connection.prepare(sql).map_err(database_error)?;
    let result = statement.query_map([], map).map_err(database_error)?.collect::<Result<Vec<_>, _>>().map_err(database_error)?;
    Ok(result)
}

fn capture(connection: &Connection) -> Result<BackupDocument, String> {
    Ok(BackupDocument {
        format: "GrahamBackup".into(), version: 6, schema_version: 7,
        created_at: Utc::now().to_rfc3339(),
        revision_sequence: connection.query_row("SELECT seq FROM sqlite_sequence WHERE name='entry_revisions'", [], |row| row.get(0)).optional().map_err(database_error)?.unwrap_or(0),
        imports: super::imports::load_archive(connection)?,
        dividends: dividends::load_dividends(connection)?,
        dividend_revisions: dividends::load_history(connection)?,
        dividend_revision_sequence: connection.query_row("SELECT seq FROM sqlite_sequence WHERE name='dividend_revisions'",[],|r|r.get(0)).optional().map_err(database_error)?.unwrap_or(0),
        movements: movements::load_movements(connection)?,
        movement_revisions: movements::load_history(connection)?,
        movement_revision_sequence: connection.query_row("SELECT seq FROM sqlite_sequence WHERE name='movement_revisions'",[],|r|r.get(0)).optional().map_err(database_error)?.unwrap_or(0),
        openings: opening::load_openings(connection)?,
        opening_revisions: opening::load_history(connection)?,
        opening_revision_sequence: connection.query_row("SELECT seq FROM sqlite_sequence WHERE name='opening_revisions'",[],|r|r.get(0)).optional().map_err(database_error)?.unwrap_or(0),
        trades: trades::load_trades(connection)?,
        trade_revisions: {
            let mut all = Vec::new();
            for (id, data, deleted, changed_at, action, trade_id) in rows(connection, "SELECT id,data,deleted,changed_at,action,trade_id FROM trade_revisions ORDER BY id", |r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,bool>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?)))? {
                all.push(TradeRevision { id, trade_id, input: serde_json::from_str(&data).map_err(|_| INVALID_BACKUP)?, deleted, changed_at, action });
            }
            all
        },
        trade_revision_sequence: connection.query_row("SELECT seq FROM sqlite_sequence WHERE name='trade_revisions'", [], |r| r.get(0)).optional().map_err(database_error)?.unwrap_or(0),
        accounts: rows(connection, "SELECT id,name,currency,archived FROM accounts ORDER BY rowid", |row| Ok(StoredAccount { id: row.get(0)?, name: row.get(1)?, currency: row.get(2)?, archived: row.get(3)? }))?,
        deposits: rows(connection, "SELECT id,account_id,occurred_on,amount_cents,note,source,deleted FROM deposits ORDER BY rowid", |row| Ok(StoredDeposit { id: row.get(0)?, account_id: row.get(1)?, date: row.get(2)?, cents: row.get(3)?, note: row.get(4)?, source: row.get(5)?, deleted: row.get(6)? }))?,
        revisions: rows(connection, "SELECT id,entry_id,occurred_on,amount_cents,note,deleted,changed_at,action FROM entry_revisions ORDER BY id", |row| Ok(StoredRevision { id: row.get(0)?, entry_id: row.get(1)?, date: row.get(2)?, cents: row.get(3)?, note: row.get(4)?, deleted: row.get(5)?, changed_at: row.get(6)?, action: row.get(7)? }))?,
    })
}

fn replace_records(transaction: &Transaction<'_>, document: &BackupDocument, previous_sequences: Option<(i64,i64,i64,i64,i64)>) -> Result<(), String> {
    let previous_sequence = previous_sequences.map(|p| p.0);
    let mut revisions: Vec<_> = document.revisions.iter().collect();
    revisions.sort_by_key(|revision| revision.id);
    let next_sequence = match previous_sequence {
        Some(previous) => previous.checked_add(i64::try_from(revisions.len()).map_err(|_| INVALID_BACKUP)?).filter(|next| *next <= 9_007_199_254_740_991).ok_or("恢复后的版本编号超出支持范围，当前账本未修改")?,
        None => document.revision_sequence,
    };
    transaction.execute_batch("DELETE FROM import_items; DELETE FROM import_batches; DELETE FROM dividend_revisions; DELETE FROM dividends; DELETE FROM movement_revisions; DELETE FROM movements; DELETE FROM opening_revisions; DELETE FROM openings; DELETE FROM trade_revisions; DELETE FROM trades; DELETE FROM entry_revisions; DELETE FROM deposits; DELETE FROM accounts; DELETE FROM sqlite_sequence WHERE name='entry_revisions';").map_err(database_error)?;
    for account in &document.accounts {
        transaction.execute("INSERT INTO accounts(id,name,currency,archived) VALUES(?1,?2,?3,?4)", params![account.id, account.name, account.currency, account.archived]).map_err(database_error)?;
    }
    for entry in &document.deposits {
        transaction.execute("INSERT INTO deposits(id,account_id,occurred_on,amount_cents,note,source,deleted) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![entry.id, entry.account_id, entry.date, entry.cents, entry.note, entry.source, entry.deleted]).map_err(database_error)?;
    }
    for (index, revision) in revisions.iter().enumerate() {
        let id = previous_sequence.map_or(revision.id, |previous| previous + index as i64 + 1);
        transaction.execute("INSERT INTO entry_revisions(id,entry_id,occurred_on,amount_cents,note,deleted,changed_at,action) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![id, revision.entry_id, revision.date, revision.cents, revision.note, revision.deleted, revision.changed_at, revision.action]).map_err(database_error)?;
    }
    transaction.execute("DELETE FROM sqlite_sequence WHERE name='entry_revisions'", []).map_err(database_error)?;
    transaction.execute("INSERT INTO sqlite_sequence(name,seq) VALUES('entry_revisions',?1)", [next_sequence]).map_err(database_error)?;
    for trade in &document.trades {
        transaction.execute("INSERT INTO trades(id,account_id,data,source,deleted,event_order) VALUES(?1,?2,?3,?4,?5,?6)", params![trade.id,trade.input.account_id,serde_json::to_string(&trade.input).map_err(|_| INVALID_BACKUP)?,trade.source,trade.deleted,trade.event_order]).map_err(database_error)?;
    }
    let mut history: Vec<_> = document.trade_revisions.iter().collect();
    history.sort_by_key(|r| r.id);
    let previous_trade_sequence = previous_sequences.map(|p| p.1);
    let next = match previous_trade_sequence {
        Some(previous) => previous.checked_add(i64::try_from(history.len()).map_err(|_| INVALID_BACKUP)?).filter(|n| *n <= 9_007_199_254_740_991).ok_or(INVALID_BACKUP)?,
        None => document.trade_revision_sequence,
    };
    for (index, r) in history.into_iter().enumerate() {
        let id = previous_trade_sequence.map_or(r.id, |p| p + index as i64 + 1);
        transaction.execute("INSERT INTO trade_revisions(id,trade_id,data,deleted,changed_at,action) VALUES(?1,?2,?3,?4,?5,?6)", params![id,r.trade_id,serde_json::to_string(&r.input).map_err(|_| INVALID_BACKUP)?,r.deleted,r.changed_at,r.action]).map_err(database_error)?;
    }
    transaction.execute("DELETE FROM sqlite_sequence WHERE name='trade_revisions'", []).map_err(database_error)?;
    transaction.execute("INSERT INTO sqlite_sequence(name,seq) VALUES('trade_revisions',?1)", [next]).map_err(database_error)?;
    for o in &document.openings {
        transaction.execute("INSERT INTO openings(id,account_id,data,source,deleted) VALUES(?1,?2,?3,?4,?5)",params![o.id,o.input.account_id,serde_json::to_string(&o.input).map_err(|_|INVALID_BACKUP)?,o.source,o.deleted]).map_err(database_error)?;
    }
    let mut history: Vec<_>=document.opening_revisions.iter().collect();history.sort_by_key(|r|r.id);
    let previous=previous_sequences.map(|s|s.2);
    let next=match previous{Some(p)=>p.checked_add(i64::try_from(history.len()).map_err(|_|INVALID_BACKUP)?).filter(|n|*n<=9_007_199_254_740_991).ok_or(INVALID_BACKUP)?,None=>document.opening_revision_sequence};
    for (index,r) in history.into_iter().enumerate(){
        let id=previous.map_or(r.id,|p|p+index as i64+1);
        transaction.execute("INSERT INTO opening_revisions(id,opening_id,data,deleted,changed_at,action) VALUES(?1,?2,?3,?4,?5,?6)",params![id,r.opening_id,serde_json::to_string(&r.input).map_err(|_|INVALID_BACKUP)?,r.deleted,r.changed_at,r.action]).map_err(database_error)?;
    }
    transaction.execute("DELETE FROM sqlite_sequence WHERE name='opening_revisions'",[]).map_err(database_error)?;
    transaction.execute("INSERT INTO sqlite_sequence(name,seq) VALUES('opening_revisions',?1)",[next]).map_err(database_error)?;
    for m in &document.movements{
        transaction.execute("INSERT INTO movements(id,from_account_id,to_account_id,data,source,deleted,event_order) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![m.id,m.input.from_account_id,m.input.to_account_id,serde_json::to_string(&m.input).map_err(|_|INVALID_BACKUP)?,m.source,m.deleted,m.event_order]).map_err(database_error)?;
    }
    let mut history:Vec<_>=document.movement_revisions.iter().collect();history.sort_by_key(|r|r.id);let previous=previous_sequences.map(|s|s.3);
    let next=match previous{Some(p)=>p.checked_add(i64::try_from(history.len()).map_err(|_|INVALID_BACKUP)?).filter(|n|*n<=9_007_199_254_740_991).ok_or(INVALID_BACKUP)?,None=>document.movement_revision_sequence};
    for(index,r)in history.into_iter().enumerate(){let id=previous.map_or(r.id,|p|p+index as i64+1);transaction.execute("INSERT INTO movement_revisions(id,movement_id,data,deleted,changed_at,action) VALUES(?1,?2,?3,?4,?5,?6)",params![id,r.movement_id,serde_json::to_string(&r.input).map_err(|_|INVALID_BACKUP)?,r.deleted,r.changed_at,r.action]).map_err(database_error)?;}
    transaction.execute("DELETE FROM sqlite_sequence WHERE name='movement_revisions'",[]).map_err(database_error)?;transaction.execute("INSERT INTO sqlite_sequence(name,seq) VALUES('movement_revisions',?1)",[next]).map_err(database_error)?;
    for d in &document.dividends{
        transaction.execute("INSERT INTO dividends(id,account_id,data,source,deleted,event_order) VALUES(?1,?2,?3,?4,?5,?6)",params![d.id,d.input.account_id,serde_json::to_string(&d.input).map_err(|_|INVALID_BACKUP)?,d.source,d.deleted,d.event_order]).map_err(database_error)?;
    }
    let mut history:Vec<_>=document.dividend_revisions.iter().collect();history.sort_by_key(|r|r.id);let previous=previous_sequences.map(|s|s.4);
    let next=match previous{Some(p)=>p.checked_add(i64::try_from(history.len()).map_err(|_|INVALID_BACKUP)?).filter(|n|*n<=9_007_199_254_740_991).ok_or(INVALID_BACKUP)?,None=>document.dividend_revision_sequence};
    for(index,r)in history.into_iter().enumerate(){let id=previous.map_or(r.id,|p|p+index as i64+1);transaction.execute("INSERT INTO dividend_revisions(id,dividend_id,data,deleted,changed_at,action) VALUES(?1,?2,?3,?4,?5,?6)",params![id,r.dividend_id,serde_json::to_string(&r.input).map_err(|_|INVALID_BACKUP)?,r.deleted,r.changed_at,r.action]).map_err(database_error)?;}
    transaction.execute("DELETE FROM sqlite_sequence WHERE name='dividend_revisions'",[]).map_err(database_error)?;transaction.execute("INSERT INTO sqlite_sequence(name,seq) VALUES('dividend_revisions',?1)",[next]).map_err(database_error)?;
    super::imports::replace_archive(transaction,&document.imports)?;
    Ok(())
}

fn valid_trade_id(id: &str) -> bool { !id.is_empty() && id.len() <= 128 }

fn supported(version: u32, schema: u32) -> bool { (version == 1 && schema == 2) || (version == 2 && schema == 3) || (version == 3 && schema == 4) || (version == 4 && schema == 5) || (version == 5 && schema == 6) || (version == 6 && schema == 7) }

fn validate(document: &BackupDocument) -> Result<(), String> {
    if document.format != "GrahamBackup" || !supported(document.version, document.schema_version) {
        return Err("备份版本不受支持，请使用兼容版本的 Graham".into());
    }
    if document.version<6&&(!document.imports.batches.is_empty()||!document.imports.items.is_empty()){return Err(INVALID_BACKUP.into());}
    if document.version == 1 && (!document.trades.is_empty() || !document.trade_revisions.is_empty() || document.trade_revision_sequence != 0) { return Err(INVALID_BACKUP.into()); }
    if document.version<4&&(!document.movements.is_empty()||!document.movement_revisions.is_empty()||document.movement_revision_sequence!=0){return Err(INVALID_BACKUP.into());}
    if document.version<5&&(!document.dividends.is_empty()||!document.dividend_revisions.is_empty()||document.dividend_revision_sequence!=0){return Err(INVALID_BACKUP.into());}
    let validate_dividend=|input:&super::DividendInput|->Result<(),String>{
        dividends::validate_values(input).map_err(|_|INVALID_BACKUP)?;
        let account=document.accounts.iter().find(|a|a.id==input.account_id).ok_or(INVALID_BACKUP)?;
        if account.currency!=input.currency{return Err(INVALID_BACKUP.into());}Ok(())
    };
    for d in &document.dividends{if !valid_trade_id(&d.id)||!matches!(d.source.as_str(),"手动录入"|"CSV 导入"){return Err(INVALID_BACKUP.into());}validate_dividend(&d.input)?;let r=document.dividend_revisions.iter().filter(|r|r.dividend_id==d.id).max_by_key(|r|r.id).ok_or(INVALID_BACKUP)?;if r.id!=d.revision_id||r.input!=d.input||r.deleted!=d.deleted{return Err(INVALID_BACKUP.into());}}
    for r in &document.dividend_revisions{if r.id<=0||r.action.is_empty()||r.action.chars().count()>80{return Err(INVALID_BACKUP.into());}validate_dividend(&r.input)?;DateTime::parse_from_rfc3339(&r.changed_at).map_err(|_|INVALID_BACKUP)?;let d=document.dividends.iter().find(|d|d.id==r.dividend_id).ok_or(INVALID_BACKUP)?;if d.input.account_id!=r.input.account_id{return Err(INVALID_BACKUP.into());}}
    if document.dividend_revision_sequence<document.dividend_revisions.iter().map(|r|r.id).max().unwrap_or(0){return Err(INVALID_BACKUP.into());}
    let mut orders=HashSet::new();
    for order in document.trades.iter().map(|t|t.event_order).chain(document.movements.iter().map(|m|m.event_order)).chain(document.dividends.iter().map(|d|d.event_order)){if order<=0||!orders.insert(order){return Err(INVALID_BACKUP.into());}}
    let validate_movement=|input:&super::MovementInput|->Result<(),String>{
        movements::validate_values(input).map_err(|_|INVALID_BACKUP)?;
        for(id,currency)in [(&input.from_account_id,&input.from_currency),(&input.to_account_id,&input.to_currency)]{let account=document.accounts.iter().find(|a|&a.id==id).ok_or(INVALID_BACKUP)?;if &account.currency!=currency{return Err(INVALID_BACKUP.into());}}Ok(())
    };
    for m in &document.movements{if !valid_trade_id(&m.id)||!matches!(m.source.as_str(),"手动录入"|"CSV 导入"){return Err(INVALID_BACKUP.into());}validate_movement(&m.input)?;let r=document.movement_revisions.iter().filter(|r|r.movement_id==m.id).max_by_key(|r|r.id).ok_or(INVALID_BACKUP)?;if r.id!=m.revision_id||r.input!=m.input||r.deleted!=m.deleted{return Err(INVALID_BACKUP.into());}}
    for r in &document.movement_revisions{if r.id<=0||r.action.is_empty()||r.action.chars().count()>80{return Err(INVALID_BACKUP.into());}validate_movement(&r.input)?;DateTime::parse_from_rfc3339(&r.changed_at).map_err(|_|INVALID_BACKUP)?;let m=document.movements.iter().find(|m|m.id==r.movement_id).ok_or(INVALID_BACKUP)?;if m.input.from_account_id!=r.input.from_account_id||m.input.to_account_id!=r.input.to_account_id{return Err(INVALID_BACKUP.into());}}
    if document.movement_revision_sequence<document.movement_revisions.iter().map(|r|r.id).max().unwrap_or(0){return Err(INVALID_BACKUP.into());}
    if document.version<3 && (!document.openings.is_empty() || !document.opening_revisions.is_empty() || document.opening_revision_sequence!=0){return Err(INVALID_BACKUP.into());}
    let validate_opening=|input:&super::OpeningInput|->Result<(),String>{
        opening::validate_input(input).map_err(|_|INVALID_BACKUP)?;
        let account=document.accounts.iter().find(|a|a.id==input.account_id).ok_or(INVALID_BACKUP)?;
        if input.positions.iter().any(|p|p.currency!=account.currency){return Err(INVALID_BACKUP.into());}Ok(())
    };
    for o in &document.openings{
        if !valid_trade_id(&o.id)||!matches!(o.source.as_str(),"手动核对"|"CSV 导入"){return Err(INVALID_BACKUP.into());}validate_opening(&o.input)?;
        let latest=document.opening_revisions.iter().filter(|r|r.opening_id==o.id).max_by_key(|r|r.id).ok_or(INVALID_BACKUP)?;
        if latest.id!=o.revision_id||latest.input!=o.input||latest.deleted!=o.deleted{return Err(INVALID_BACKUP.into());}
    }
    for r in &document.opening_revisions{
        if r.id<=0||r.action.is_empty()||r.action.chars().count()>80{return Err(INVALID_BACKUP.into());}
        validate_opening(&r.input)?;DateTime::parse_from_rfc3339(&r.changed_at).map_err(|_|INVALID_BACKUP)?;
        let current=document.openings.iter().find(|o|o.id==r.opening_id).ok_or(INVALID_BACKUP)?;
        if current.input.account_id!=r.input.account_id{return Err(INVALID_BACKUP.into());}
    }
    if document.opening_revision_sequence<document.opening_revisions.iter().map(|r|r.id).max().unwrap_or(0){return Err(INVALID_BACKUP.into());}
    for trade in &document.trades {
        if !valid_trade_id(&trade.id) || !matches!(trade.source.as_str(),"手动录入"|"CSV 导入") { return Err(INVALID_BACKUP.into()); }
        trades::validate_input(&trade.input).map_err(|_| INVALID_BACKUP)?;
    }
    for revision in &document.trade_revisions {
        if revision.id <= 0 || revision.action.is_empty() || revision.action.chars().count() > 80 { return Err(INVALID_BACKUP.into()); }
        trades::validate_input(&revision.input).map_err(|_| INVALID_BACKUP)?;
        DateTime::parse_from_rfc3339(&revision.changed_at).map_err(|_| INVALID_BACKUP)?;
        let current = document.trades.iter().find(|t| t.id == revision.trade_id).ok_or(INVALID_BACKUP)?;
        if current.input.account_id != revision.input.account_id { return Err(INVALID_BACKUP.into()); }
    }
    if document.trade_revision_sequence < document.trade_revisions.iter().map(|r| r.id).max().unwrap_or(0) { return Err(INVALID_BACKUP.into()); }
    for trade in &document.trades {
        let latest = document.trade_revisions.iter().filter(|r| r.trade_id == trade.id).max_by_key(|r| r.id).ok_or(INVALID_BACKUP)?;
        if trade.revision_id != latest.id || trade.input != latest.input || trade.deleted != latest.deleted { return Err(INVALID_BACKUP.into()); }
        let account = document.accounts.iter().find(|a| a.id == trade.input.account_id).ok_or(INVALID_BACKUP)?;
        if account.currency != trade.input.currency { return Err(INVALID_BACKUP.into()); }
    }
    DateTime::parse_from_rfc3339(&document.created_at).map_err(|_| INVALID_BACKUP)?;
    let valid_id = |id: &str| !id.is_empty() && id.len() <= 128;
    for account in &document.accounts {
        if !valid_id(&account.id) { return Err(INVALID_BACKUP.into()); }
        validate_account(&account.name, &account.currency).map_err(|_| INVALID_BACKUP)?;
    }
    for entry in &document.deposits {
        if !valid_id(&entry.id) || entry.cents <= 0 || !matches!(entry.source.as_str(),"手动录入"|"CSV 导入") { return Err(INVALID_BACKUP.into()); }
        validate_deposit(&entry.date, &amount_text(entry.cents), &entry.note).map_err(|_| INVALID_BACKUP)?;
    }
    for revision in &document.revisions {
        if revision.id <= 0 || revision.cents <= 0 || revision.action.is_empty() || revision.action.chars().count() > 80 { return Err(INVALID_BACKUP.into()); }
        validate_deposit(&revision.date, &amount_text(revision.cents), &revision.note).map_err(|_| INVALID_BACKUP)?;
        DateTime::parse_from_rfc3339(&revision.changed_at).map_err(|_| INVALID_BACKUP)?;
    }
    if document.revision_sequence < document.revisions.iter().map(|r| r.id).max().unwrap_or(0) { return Err(INVALID_BACKUP.into()); }
    let mut candidate = Ledger::open(Path::new(":memory:"))?;
    let transaction = candidate.connection.transaction().map_err(database_error)?;
    replace_records(&transaction, document, None).map_err(|_| INVALID_BACKUP)?;
    let mismatches: i64 = transaction.query_row("SELECT COUNT(*) FROM deposits d LEFT JOIN entry_revisions r ON r.id=(SELECT MAX(id) FROM entry_revisions WHERE entry_id=d.id) WHERE r.id IS NULL OR r.occurred_on<>d.occurred_on OR r.amount_cents<>d.amount_cents OR r.note<>d.note OR r.deleted<>d.deleted", [], |row| row.get(0)).map_err(database_error)?;
    if mismatches != 0 { return Err(INVALID_BACKUP.into()); }
    transaction.commit().map_err(database_error)?;
    candidate.snapshot().map_err(|_| INVALID_BACKUP)?;
    super::imports::validate_archive(&candidate.connection).map_err(|_|INVALID_BACKUP)?;
    Ok(())
}

fn encrypt(document: &BackupDocument, password: &str) -> Result<Vec<u8>, String> {
    let plaintext = Zeroizing::new(serde_json::to_vec(document).map_err(|_| "无法生成备份内容")?);
    if plaintext.len() > MAX_PLAINTEXT { return Err("账本超过当前备份大小上限（64 MiB），请保留原文件".into()); }
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(password.to_owned()));
    recipient.set_work_factor(WORK_FACTOR);
    let encryptor = age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient)).map_err(|_| "备份加密失败")?;
    let mut ciphertext = Vec::new();
    let mut writer = encryptor.wrap_output(&mut ciphertext).map_err(|_| "备份加密失败")?;
    writer.write_all(&plaintext).map_err(|_| "备份加密失败")?;
    writer.finish().map_err(|_| "备份加密未完成，请重新导出")?;
    Ok(ciphertext)
}

fn read_backup(path: &Path, password: &str) -> Result<(BackupDocument, String), String> {
    if password.is_empty() || password.len() > 4096 { return Err("请输入备份密码".into()); }
    let mut ciphertext = Vec::new();
    File::open(path).map_err(|_| "无法读取备份文件，请重新选择")?.take((MAX_CIPHERTEXT + 1) as u64).read_to_end(&mut ciphertext).map_err(|_| "读取备份失败")?;
    if ciphertext.len() > MAX_CIPHERTEXT { return Err("备份超过当前支持的大小上限".into()); }
    let decryptor = age::Decryptor::new_buffered(ciphertext.as_slice()).map_err(|_| "文件不是有效的加密备份")?;
    if !decryptor.is_scrypt() { return Err("请选择使用密码加密的 Graham 备份".into()); }
    let mut identity = age::scrypt::Identity::new(SecretString::from(password.to_owned()));
    identity.set_max_work_factor(WORK_FACTOR);
    let reader = decryptor.decrypt(std::iter::once(&identity as &dyn age::Identity)).map_err(|_| "密码错误、文件损坏或加密参数不受支持；当前账本未修改")?;
    let mut plaintext = Zeroizing::new(Vec::new());
    reader.take((MAX_PLAINTEXT + 1) as u64).read_to_end(&mut plaintext).map_err(|_| "密码错误或备份损坏，当前账本未修改")?;
    if plaintext.len() > MAX_PLAINTEXT { return Err("备份解密后超过大小上限".into()); }
    let header: serde_json::Value = serde_json::from_slice(&plaintext).map_err(|_| INVALID_BACKUP)?;
    if header.get("format").and_then(|v| v.as_str()) != Some("GrahamBackup") || !matches!((header.get("version").and_then(|v| v.as_u64()), header.get("schemaVersion").and_then(|v| v.as_u64())), (Some(1),Some(2)) | (Some(2),Some(3)) | (Some(3),Some(4)) | (Some(4),Some(5)) | (Some(5),Some(6)) | (Some(6),Some(7))) {
        return Err("备份版本不受支持，请使用兼容版本的 Graham".into());
    }
    let version=header.get("version").and_then(|v|v.as_u64()).ok_or(INVALID_BACKUP)?;
    let required_trades=["trades","tradeRevisions","tradeRevisionSequence"];
    let required_openings=["openings","openingRevisions","openingRevisionSequence"];
    if (version>=2 && required_trades.iter().any(|key|header.get(*key).is_none())) || (version>=3 && required_openings.iter().any(|key|header.get(*key).is_none())) { return Err(INVALID_BACKUP.into()); }
    let required_movements=["movements","movementRevisions","movementRevisionSequence"];
    if version>=4&&(required_movements.iter().any(|key|header.get(*key).is_none())||header.get("trades").and_then(|v|v.as_array()).is_none_or(|rows|rows.iter().any(|r|r.get("eventOrder").is_none()))){return Err(INVALID_BACKUP.into());}
    if version>=5&&["dividends","dividendRevisions","dividendRevisionSequence"].iter().any(|key|header.get(*key).is_none()){return Err(INVALID_BACKUP.into());}
    if version>=6&&header.get("imports").is_none(){return Err(INVALID_BACKUP.into());}
    let mut document: BackupDocument = serde_json::from_value(header).map_err(|_| INVALID_BACKUP)?;
    if document.version<4 {for(index,trade)in document.trades.iter_mut().enumerate(){trade.event_order=i64::try_from(index+1).map_err(|_|INVALID_BACKUP)?;}}
    validate(&document)?;
    let fingerprint = Sha256::digest(&ciphertext).iter().map(|byte| format!("{byte:02x}")).collect();
    Ok((document, fingerprint))
}

fn write_new_backup(path: &Path, ciphertext: &[u8]) -> Result<(), String> {
    if path.extension().and_then(|s| s.to_str()) != Some("grahambak") { return Err("请选择扩展名为 .grahambak 的备份文件名".into()); }
    if path.exists() { return Err("备份文件已存在，请选择新的文件名".into()); }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| "无法在所选目录创建备份")?;
    temporary.write_all(ciphertext).map_err(|_| "备份写入失败，当前账本未修改")?;
    temporary.as_file().sync_all().map_err(|_| "备份未能保存到磁盘")?;
    temporary.persist_noclobber(path).map_err(|_| "无法保存备份或同名文件已存在，请选择新的文件名")?;
    Ok(())
}

impl Ledger {
    pub fn export_backup(&mut self, destination: &Path, password: &str) -> Result<(), String> {
        if password.chars().count() < 12 || password.len() > 4096 || password.trim().is_empty() { return Err("请设置至少 12 个字符的备份密码".into()); }
        let transaction = self.connection.transaction().map_err(database_error)?;
        let document = capture(&transaction)?;
        transaction.commit().map_err(database_error)?;
        write_new_backup(destination, &encrypt(&document, password)?)
    }

    pub fn preview_backup(path: &Path, password: &str) -> Result<BackupPreview, String> {
        let (document, fingerprint) = read_backup(path, password)?;
        Ok(BackupPreview { created_at: document.created_at, account_count: document.accounts.len(), record_count: document.deposits.len() + document.trades.len() + document.openings.len() + document.movements.len() + document.dividends.len() + document.imports.items.iter().filter(|i|matches!(i.current,super::ImportRecord::Withdrawal(_))).count(), deleted_count: document.deposits.iter().filter(|e| e.deleted).count() + document.trades.iter().filter(|t| t.deleted).count() + document.openings.iter().filter(|o|o.deleted).count() + document.movements.iter().filter(|m|m.deleted).count() + document.dividends.iter().filter(|d|d.deleted).count() + document.imports.items.iter().filter(|i|i.deleted&&matches!(i.current,super::ImportRecord::Withdrawal(_))).count(), fingerprint })
    }

    pub fn restore_backup(&mut self, source: &Path, password: &str, expected_fingerprint: &str, recovery_directory: &Path) -> Result<BackupRestored, String> {
        let (document, fingerprint) = read_backup(source, password)?;
        if fingerprint != expected_fingerprint { return Err("备份文件已变化，请重新验证后再恢复".into()); }
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let current = capture(&transaction)?;
        let recovery = encrypt(&current, password)?;
        std::fs::create_dir_all(recovery_directory).map_err(|_| "无法创建恢复前备份目录，当前账本未修改")?;
        let saved = recovery_directory.join(format!("Graham-before-restore-{}.grahambak", Uuid::new_v4()));
        write_new_backup(&saved, &recovery)?;
        replace_records(&transaction, &document, Some((current.revision_sequence, current.trade_revision_sequence, current.opening_revision_sequence, current.movement_revision_sequence,current.dividend_revision_sequence)))?;
        let snapshot = snapshot_from(&transaction)?;
        let recovery_path = saved.to_string_lossy().into_owned();
        transaction.commit().map_err(database_error)?;
        Ok(BackupRestored { snapshot, recovery_path })
    }
}

pub(super) fn copy_to_memory(connection:&Connection)->Result<Ledger,String>{let document=capture(connection)?;let mut ledger=Ledger::open(Path::new(":memory:"))?;let tx=ledger.connection.transaction().map_err(database_error)?;replace_records(&tx,&document,None)?;tx.commit().map_err(database_error)?;Ok(ledger)}
pub(super) fn state_fingerprint(connection:&Connection)->Result<String,String>{let mut document=capture(connection)?;document.created_at.clear();let bytes=serde_json::to_vec(&document).map_err(|_|"无法核对账本版本")?;Ok(Sha256::digest(bytes).iter().map(|b|format!("{b:02x}")).collect())}
