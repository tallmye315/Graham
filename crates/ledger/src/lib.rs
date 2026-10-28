use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use uuid::Uuid;

mod analysis;
pub use analysis::{AnalysisState,AnalysisRow,AllocationGroup,AnalysisContribution,AnalysisBar};

mod income;
pub use income::{IncomeQuery,IncomeAmounts,IncomeRow,MarketIncome,IncomeState};

mod money_weighted;
pub use money_weighted::{MoneyWeightedFlow,MoneyWeightedState};

mod performance;
pub use performance::{PerformanceQuery,PerformancePoint,PerformanceRefresh,PerformanceState,PerformanceJob,HistorySeries,HistorySession,HistoryCalendar,HistoryResponse};

mod valuation;
pub use valuation::{ValuationQuery,ValuationJob,ValuationPoint,ValuationError,ValuationResponse,ValuationSource,ValuedAsset,CurrencyDistribution,ValuationState};

mod quotes;
pub use quotes::{DailyClose,QuoteKey,QuoteInput,QuoteResponse,QuoteJob,QuotePlan,QuoteView,QuotedHolding,QuoteState};

mod imports;
pub use imports::{CSV_HEADER,CashInput,ImportRecord,ImportChange,ImportItem,ImportBatch,ImportArchive,ImportLog,CsvRow,CsvPreview,ImportResult,BatchUndoPreview};

mod dividends;
pub use dividends::{DividendInput,Dividend,DividendRevision,DividendTotal};

mod movements;
pub use movements::{MovementInput,Movement,MovementRevision,CurrencyFlow,FlowSummary};

mod opening;
pub use opening::{OpeningInput, OpeningPosition, Opening, OpeningRevision, DatedAccount, DatedSnapshot};

mod trades;
pub use trades::{TradeInput, Trade, TradeRevision, Position, PositionValue};

mod backup;
pub use backup::{BackupPreview, BackupRestored};

pub struct Ledger { connection: Connection, market: quotes::QuoteStore }

pub mod market_refresh;

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub balance: String,
    pub archived: bool,
    pub has_history: bool,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub account_id: String,
    pub amount: String,
    pub occurred_on: String,
    pub note: String,
    pub source: String,
    pub deleted: bool,
    pub revision_id: i64,
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub accounts: Vec<Account>,
    pub entries: Vec<Entry>,
    pub trades: Vec<Trade>,
    pub positions: Vec<Position>,
    pub openings: Vec<Opening>,
    pub movements: Vec<Movement>,
    pub dividends: Vec<Dividend>,
    #[serde(rename="dividendTotals")]
    pub dividend_totals: Vec<DividendTotal>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSelection {
    pub id: String,
    pub revision_id: i64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordRestore {
    pub current: RecordSelection,
    pub restore_revision_id: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub id: i64,
    pub amount: String,
    pub occurred_on: String,
    pub note: String,
    pub deleted: bool,
    pub changed_at: String,
    pub action: String,
}

fn database_error(error: rusqlite::Error) -> String {
    eprintln!("ledger storage error: {error}");
    "本地账本读写失败，请保留当前文件并重试。".into()
}

fn amount_text(cents: i64) -> String { let magnitude = (cents as i128).abs(); format!("{}{}.{:02}", if cents < 0 { "-" } else { "" }, magnitude / 100, magnitude % 100) }

fn validate_account(name: &str, currency: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.trim().chars().count() > 80 { return Err("账户名称应为 1—80 个字符".into()); }
    if !["CNY", "USD", "HKD"].contains(&currency) { return Err("请选择人民币、美元或港币".into()); }
    Ok(())
}

fn deposit_cents(amount: &str) -> Result<i64, String> {
    let invalid = || "请输入大于零、最多两位小数且在支持范围内的金额".to_string();
    let amount = amount.trim();
    let (whole, fraction) = amount.split_once('.').unwrap_or((amount, ""));
    if whole.is_empty() || !whole.bytes().all(|c| c.is_ascii_digit())
        || !fraction.bytes().all(|c| c.is_ascii_digit()) || fraction.len() > 2
        || (amount.contains('.') && fraction.is_empty()) {
        return Err(invalid());
    }
    let whole = whole.parse::<i64>().map_err(|_| invalid())?;
    let fraction = format!("{fraction:0<2}").parse::<i64>().map_err(|_| invalid())?;
    whole.checked_mul(100).and_then(|n| n.checked_add(fraction)).filter(|n| *n > 0).ok_or_else(invalid)
}

fn validate_record_fields(date: &str, note: &str) -> Result<(), String> {
    let parsed_date = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| "请输入有效日期".to_string())?;
    if date.len() != 10 || parsed_date.to_string() != date || date.starts_with("0000") {
        return Err("请输入有效的 YYYY-MM-DD 日期".into());
    }
    if note.chars().count() > 500 { return Err("备注不能超过 500 个字符".into()); }
    Ok(())
}

fn validate_deposit(date: &str, amount: &str, note: &str) -> Result<i64, String> {
    validate_record_fields(date, note)?;
    deposit_cents(amount)
}

fn save_revision(transaction: &Connection, id: &str, action: &str) -> Result<(), String> {
    transaction.execute("INSERT INTO entry_revisions(entry_id,occurred_on,amount_cents,note,deleted,action) SELECT id,occurred_on,amount_cents,note,deleted,?2 FROM deposits WHERE id=?1", params![id, action]).map_err(database_error)?;
    Ok(())
}

struct DepositState { date: String, cents: i64, note: String, deleted: bool }

fn deposit_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<DepositState> {
    Ok(DepositState { date: row.get(0)?, cents: row.get(1)?, note: row.get(2)?, deleted: row.get(3)? })
}

fn apply_deposit(transaction: &rusqlite::Transaction<'_>, id: &str, state: DepositState, action: &str) -> Result<(), String> {
    imports::ensure_manual(transaction,"deposits",id)?;
    let _: String = transaction.query_row("SELECT account_id FROM deposits WHERE id=?1", [id], |r| r.get(0)).optional().map_err(database_error)?.ok_or("记录不存在，请刷新后重试")?;
    transaction.execute("UPDATE deposits SET occurred_on=?2,amount_cents=?3,note=?4,deleted=?5 WHERE id=?1", params![id,state.date,state.cents,state.note,state.deleted]).map_err(database_error)?;
    save_revision(transaction, id, action)?;
    trades::validate_ledger(transaction)
}

fn validate_selection(selection: &[RecordSelection]) -> Result<(), String> {
    let mut ids = HashSet::new();
    if selection.is_empty() || selection.iter().any(|entry| !ids.insert(&entry.id)) {
        return Err("请选择记录，同一条记录不能重复选择".into());
    }
    Ok(())
}

fn current_deposit(transaction: &rusqlite::Transaction<'_>, selection: &RecordSelection) -> Result<DepositState, String> {
    let current = transaction.query_row("SELECT occurred_on,amount_cents,note,deleted,source,(SELECT MAX(id) FROM entry_revisions WHERE entry_id=deposits.id) FROM deposits WHERE id=?1", [&selection.id], |row| {
        Ok((deposit_state(row)?, row.get::<_, String>(4)?, row.get::<_, i64>(5)?))
    }).optional().map_err(database_error)?.ok_or("记录已变化或不存在，请刷新记录后重新选择")?;
    if current.2 != selection.revision_id {
        return Err("记录已变化或不存在，请刷新记录后重新选择".into());
    }
    if current.1 != "手动录入" { return Err("本次操作仅支持手动入金记录".into()); }
    Ok(current.0)
}

fn selected_deposit(transaction: &rusqlite::Transaction<'_>, selection: &RecordSelection, deleted: bool) -> Result<DepositState, String> {
    let state = current_deposit(transaction, selection)?;
    if state.deleted != deleted {
        return Err("记录已变化或不存在，请刷新记录后重新选择".into());
    }
    Ok(state)
}

impl Ledger {
    pub fn set_deposits_deleted(&mut self, selection: &[RecordSelection], deleted: bool) -> Result<Vec<RecordRestore>, String> {
        validate_selection(selection)?;
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let mut undo = Vec::new();
        for entry in selection {
            let mut state = selected_deposit(&transaction, entry, !deleted)?;
            state.deleted = deleted;
            apply_deposit(&transaction, &entry.id, state, if deleted { "批量删除" } else { "批量恢复" })?;
            undo.push(RecordRestore {
                current: RecordSelection { id: entry.id.clone(), revision_id: transaction.last_insert_rowid() },
                restore_revision_id: entry.revision_id,
            });
        }
        transaction.commit().map_err(database_error)?;
        Ok(undo)
    }

    pub fn undo_deposit_batch(&mut self, restore: &[RecordRestore]) -> Result<(), String> {
        validate_selection(&restore.iter().map(|entry| entry.current.clone()).collect::<Vec<_>>())?;
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        for entry in restore {
            let state = transaction.query_row("SELECT occurred_on,amount_cents,note,deleted FROM entry_revisions WHERE id=?1 AND entry_id=?2", params![entry.restore_revision_id,entry.current.id], deposit_state).optional().map_err(database_error)?.ok_or("记录已变化或不存在，请刷新记录后重新选择")?;
            selected_deposit(&transaction, &entry.current, !state.deleted)?;
            apply_deposit(&transaction, &entry.current.id, state, "撤回批量操作")?;
        }
        transaction.commit().map_err(database_error)
    }

    pub fn permanently_delete_deposits(&mut self, selection: &[RecordSelection]) -> Result<(), String> {
        validate_selection(selection)?;
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        for entry in selection {
            selected_deposit(&transaction, entry, true)?;
            transaction.execute("DELETE FROM entry_revisions WHERE entry_id=?1", [&entry.id]).map_err(database_error)?;
            transaction.execute("DELETE FROM deposits WHERE id=?1", [&entry.id]).map_err(database_error)?;
        }
        transaction.commit().map_err(database_error)
    }

    pub fn update_account(&mut self, id: &str, name: &str, currency: &str) -> Result<(), String> {
        validate_account(name, currency)?;
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let current: String = transaction.query_row("SELECT currency FROM accounts WHERE id=?1", [id], |r| r.get(0)).optional().map_err(database_error)?.ok_or("投资账户不存在")?;
        let has_history: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM deposits WHERE account_id=?1 UNION ALL SELECT 1 FROM trades WHERE account_id=?1 UNION ALL SELECT 1 FROM openings WHERE account_id=?1 UNION ALL SELECT 1 FROM movements WHERE from_account_id=?1 OR to_account_id=?1 UNION ALL SELECT 1 FROM dividends WHERE account_id=?1 UNION ALL SELECT 1 FROM import_items WHERE account_id=?1)", [id], |r| r.get(0)).map_err(database_error)?;
        if has_history && current != currency { return Err("已有历史记录的账户不能修改币种".into()); }
        transaction.execute("UPDATE accounts SET name=?2,currency=?3 WHERE id=?1", params![id,name.trim(),currency]).map_err(database_error)?;
        transaction.commit().map_err(database_error)
    }

    pub fn delete_empty_account(&mut self, id: &str) -> Result<(), String> {
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let has_history: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM deposits WHERE account_id=?1 UNION ALL SELECT 1 FROM trades WHERE account_id=?1 UNION ALL SELECT 1 FROM openings WHERE account_id=?1 UNION ALL SELECT 1 FROM movements WHERE from_account_id=?1 OR to_account_id=?1 UNION ALL SELECT 1 FROM dividends WHERE account_id=?1 UNION ALL SELECT 1 FROM import_items WHERE account_id=?1)", [id], |r| r.get(0)).map_err(database_error)?;
        if has_history { return Err("账户有历史记录，请使用归档保留核对依据".into()); }
        if transaction.execute("DELETE FROM accounts WHERE id=?1", [id]).map_err(database_error)? == 0 { return Err("投资账户不存在".into()); }
        transaction.commit().map_err(database_error)
    }

    pub fn set_account_archived(&mut self, id: &str, archived: bool) -> Result<(), String> {
        if self.connection.execute("UPDATE accounts SET archived=?2 WHERE id=?1", params![id,archived]).map_err(database_error)? == 0 {
            return Err("投资账户不存在".into());
        }
        Ok(())
    }
    pub fn set_deposit_deleted(&mut self, id: &str, expected_revision: i64, deleted: bool) -> Result<Snapshot, String> {
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let mut state = current_deposit(&transaction, &RecordSelection { id: id.into(), revision_id: expected_revision })?;
        if state.deleted == deleted { return Err("记录状态已变化，请刷新后重试".into()); }
        state.deleted = deleted;
        apply_deposit(&transaction, id, state, if deleted { "删除" } else { "恢复记录" })?;
        let snapshot = snapshot_from(&transaction)?;
        transaction.commit().map_err(database_error)?;
        Ok(snapshot)
    }

    pub fn restore_deposit_revision(&mut self, id: &str, expected_revision: i64, revision_id: i64) -> Result<Snapshot, String> {
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        current_deposit(&transaction, &RecordSelection { id: id.into(), revision_id: expected_revision })?;
        let state = transaction.query_row("SELECT occurred_on,amount_cents,note,deleted FROM entry_revisions WHERE id=?1 AND entry_id=?2", params![revision_id,id], deposit_state).optional().map_err(database_error)?.ok_or("找不到这条记录的历史版本")?;
        apply_deposit(&transaction, id, state, "恢复版本")?;
        let snapshot = snapshot_from(&transaction)?;
        transaction.commit().map_err(database_error)?;
        Ok(snapshot)
    }
    pub fn revise_deposit(&mut self, id: &str, expected_revision: i64, date: &str, amount: &str, note: &str) -> Result<Snapshot, String> {
        let cents = validate_deposit(date, amount, note)?;
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let current = current_deposit(&transaction, &RecordSelection { id: id.into(), revision_id: expected_revision })?;
        if current.deleted { return Err("已删除记录请先恢复再修改".into()); }
        apply_deposit(&transaction, id, DepositState { date: date.into(), cents, note: note.into(), deleted: false }, "修改")?;
        let snapshot = snapshot_from(&transaction)?;
        transaction.commit().map_err(database_error)?;
        Ok(snapshot)
    }

    pub fn entry_history(&self, id: &str) -> Result<Vec<Revision>, String> {
        let mut query = self.connection.prepare("SELECT id,amount_cents,occurred_on,note,deleted,changed_at,action FROM entry_revisions WHERE entry_id=?1 ORDER BY id DESC").map_err(database_error)?;
        let revisions = query.query_map([id], |row| Ok(Revision {
            id: row.get(0)?, amount: amount_text(row.get(1)?), occurred_on: row.get(2)?, note: row.get(3)?, deleted: row.get(4)?, changed_at: row.get(5)?, action: row.get(6)?,
        })).map_err(database_error)?.collect::<Result<Vec<_>, _>>().map_err(database_error)?;
        Ok(revisions)
    }
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut connection = Connection::open(path).map_err(database_error)?;
        connection.busy_timeout(std::time::Duration::from_secs(5)).map_err(database_error)?;
        connection.execute_batch("PRAGMA foreign_keys=ON;").map_err(database_error)?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0)).map_err(database_error)?;
        if version > 7 { return Err("账本由较新版本创建，请使用兼容版本打开".into()); }
        if version == 0 {
        let transaction = connection.transaction().map_err(database_error)?;
        transaction.execute_batch("
            CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, currency TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS deposits (
                id TEXT PRIMARY KEY, account_id TEXT NOT NULL REFERENCES accounts(id),
                occurred_on TEXT NOT NULL, amount_cents INTEGER NOT NULL CHECK(amount_cents > 0),
                note TEXT NOT NULL, source TEXT NOT NULL
            );
            PRAGMA user_version=1;").map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        }
        if version < 2 {
            if version == 1 {
                let backup = path.with_file_name(format!("ledger-before-v2-{}.sqlite3", Uuid::new_v4()));
                connection.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()]).map_err(database_error)?;
            }
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
            transaction.execute_batch("
                ALTER TABLE accounts ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;
                ALTER TABLE deposits ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
                CREATE TABLE entry_revisions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    entry_id TEXT NOT NULL REFERENCES deposits(id),
                    occurred_on TEXT NOT NULL, amount_cents INTEGER NOT NULL,
                    note TEXT NOT NULL, deleted INTEGER NOT NULL,
                    changed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
                    action TEXT NOT NULL
                );
                INSERT INTO entry_revisions(entry_id,occurred_on,amount_cents,note,deleted,action)
                    SELECT id,occurred_on,amount_cents,note,0,'原始记录' FROM deposits;
                PRAGMA user_version=2;").map_err(database_error)?;
            transaction.commit().map_err(database_error)?;
        }
        if version < 3 {
            if version == 2 && path != Path::new(":memory:") {
                let backup = path.with_file_name(format!("ledger-before-v3-{}.sqlite3", Uuid::new_v4()));
                connection.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()]).map_err(database_error)?;
            }
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
            transaction.execute_batch("
                CREATE TABLE trades (id TEXT PRIMARY KEY, account_id TEXT NOT NULL REFERENCES accounts(id), data TEXT NOT NULL, source TEXT NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE trade_revisions (id INTEGER PRIMARY KEY AUTOINCREMENT, trade_id TEXT NOT NULL REFERENCES trades(id), data TEXT NOT NULL, deleted INTEGER NOT NULL, changed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), action TEXT NOT NULL);
                PRAGMA user_version=3;").map_err(database_error)?;
            transaction.commit().map_err(database_error)?;
        }
        if version < 4 {
            if version == 3 && path != Path::new(":memory:") {
                let backup=path.with_file_name(format!("ledger-before-v4-{}.sqlite3",Uuid::new_v4()));
                connection.execute("VACUUM INTO ?1",[backup.to_string_lossy().as_ref()]).map_err(database_error)?;
            }
            let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
            tx.execute_batch("CREATE TABLE openings(id TEXT PRIMARY KEY,account_id TEXT NOT NULL UNIQUE REFERENCES accounts(id),data TEXT NOT NULL,source TEXT NOT NULL,deleted INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE opening_revisions(id INTEGER PRIMARY KEY AUTOINCREMENT,opening_id TEXT NOT NULL REFERENCES openings(id),data TEXT NOT NULL,deleted INTEGER NOT NULL,changed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),action TEXT NOT NULL); PRAGMA user_version=4;").map_err(database_error)?;
            tx.commit().map_err(database_error)?;
        }
        if version < 5 {
            if version==4 && path!=Path::new(":memory:") {let backup=path.with_file_name(format!("ledger-before-v5-{}.sqlite3",Uuid::new_v4()));connection.execute("VACUUM INTO ?1",[backup.to_string_lossy().as_ref()]).map_err(database_error)?;}
            let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
            tx.execute_batch("ALTER TABLE trades ADD COLUMN event_order INTEGER NOT NULL DEFAULT 0; UPDATE trades SET event_order=rowid;
                CREATE TABLE movements(id TEXT PRIMARY KEY,from_account_id TEXT NOT NULL REFERENCES accounts(id),to_account_id TEXT NOT NULL REFERENCES accounts(id),data TEXT NOT NULL,source TEXT NOT NULL,deleted INTEGER NOT NULL DEFAULT 0,event_order INTEGER NOT NULL);
                CREATE TABLE movement_revisions(id INTEGER PRIMARY KEY AUTOINCREMENT,movement_id TEXT NOT NULL REFERENCES movements(id),data TEXT NOT NULL,deleted INTEGER NOT NULL,changed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),action TEXT NOT NULL); PRAGMA user_version=5;").map_err(database_error)?;tx.commit().map_err(database_error)?;
        }
        if version < 6 {
            if version==5 && path!=Path::new(":memory:") {let backup=path.with_file_name(format!("ledger-before-v6-{}.sqlite3",Uuid::new_v4()));connection.execute("VACUUM INTO ?1",[backup.to_string_lossy().as_ref()]).map_err(database_error)?;}
            let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
            tx.execute_batch("CREATE TABLE dividends(id TEXT PRIMARY KEY,account_id TEXT NOT NULL REFERENCES accounts(id),data TEXT NOT NULL,source TEXT NOT NULL,deleted INTEGER NOT NULL DEFAULT 0,event_order INTEGER NOT NULL);
                CREATE TABLE dividend_revisions(id INTEGER PRIMARY KEY AUTOINCREMENT,dividend_id TEXT NOT NULL REFERENCES dividends(id),data TEXT NOT NULL,deleted INTEGER NOT NULL,changed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),action TEXT NOT NULL); PRAGMA user_version=6;").map_err(database_error)?;tx.commit().map_err(database_error)?;
        }
        if version < 7 {
            if version==6 && path!=Path::new(":memory:") {let backup=path.with_file_name(format!("ledger-before-v7-{}.sqlite3",Uuid::new_v4()));connection.execute("VACUUM INTO ?1",[backup.to_string_lossy().as_ref()]).map_err(database_error)?;}
            let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
            tx.execute_batch("CREATE TABLE import_batches(id TEXT PRIMARY KEY,data TEXT NOT NULL);
                CREATE TABLE import_items(id TEXT PRIMARY KEY,batch_id TEXT NOT NULL REFERENCES import_batches(id),account_id TEXT NOT NULL REFERENCES accounts(id),external_id TEXT NOT NULL,record_type TEXT NOT NULL,record_id TEXT NOT NULL,data TEXT NOT NULL,UNIQUE(account_id,external_id));PRAGMA user_version=7;").map_err(database_error)?;tx.commit().map_err(database_error)?;
        }
        let market=quotes::QuoteStore::for_ledger(path)?;
        Ok(Self { connection, market })
    }

    pub fn create_account(&mut self, name: &str, currency: &str) -> Result<String, String> {
        validate_account(name, currency)?;
        let name = name.trim();
        let id = Uuid::new_v4().to_string();
        self.connection.execute("INSERT INTO accounts(id,name,currency) VALUES (?1,?2,?3)", params![id, name, currency]).map_err(database_error)?;
        Ok(id)
    }

    pub fn record_deposit(&mut self, account_id: &str, date: &str, amount: &str, note: &str) -> Result<(), String> {
        let cents = validate_deposit(date, amount, note)?;
        let transaction = self.connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(database_error)?;
        let active: bool = transaction.query_row("SELECT archived=0 FROM accounts WHERE id=?1", [account_id], |row| row.get(0)).optional().map_err(database_error)?.ok_or("投资账户不存在，请重新选择")?;
        if !active { return Err("投资账户已归档，请重新选择".into()); }
        let id = Uuid::new_v4().to_string();
        transaction.execute("INSERT INTO deposits(id,account_id,occurred_on,amount_cents,note,source) VALUES (?1,?2,?3,?4,?5,'手动录入')",
            params![id, account_id, date, cents, note]).map_err(database_error)?;
        save_revision(&transaction, &id, "录入")?;
        trades::validate_ledger(&transaction)?;
        transaction.commit().map_err(database_error)?;
        Ok(())
    }

    pub fn snapshot(&self) -> Result<Snapshot, String> {
        let transaction = self.connection.unchecked_transaction().map_err(database_error)?;
        let snapshot = snapshot_from(&transaction)?;
        transaction.commit().map_err(database_error)?;
        Ok(snapshot)
    }
}


fn snapshot_from(connection: &Connection) -> Result<Snapshot, String> {
    let trades = trades::load_trades(connection)?;
    let calculated = trades::calculate(connection, &trades)?;
    let mut query = connection.prepare("SELECT a.id,a.name,a.currency,0,a.archived,EXISTS(SELECT 1 FROM deposits h WHERE h.account_id=a.id UNION ALL SELECT 1 FROM trades t WHERE t.account_id=a.id UNION ALL SELECT 1 FROM openings o WHERE o.account_id=a.id UNION ALL SELECT 1 FROM movements m WHERE m.from_account_id=a.id OR m.to_account_id=a.id UNION ALL SELECT 1 FROM dividends d WHERE d.account_id=a.id UNION ALL SELECT 1 FROM import_items i WHERE i.account_id=a.id) FROM accounts a ORDER BY a.rowid").map_err(database_error)?;
    let mut accounts = query.query_map([], |row| Ok(Account {
        id: row.get(0)?, name: row.get(1)?, currency: row.get(2)?, balance: amount_text(row.get(3)?), archived: row.get(4)?, has_history: row.get(5)?,
    })).map_err(database_error)?.collect::<Result<Vec<_>, _>>().map_err(database_error)?;
    drop(query);
    let mut query = connection.prepare("SELECT d.id,d.account_id,d.amount_cents,d.occurred_on,d.note,d.source,d.deleted,(SELECT MAX(r.id) FROM entry_revisions r WHERE r.entry_id=d.id) FROM deposits d ORDER BY d.occurred_on DESC,d.rowid DESC").map_err(database_error)?;
    let entries = query.query_map([], |row| Ok(Entry {
        id: row.get(0)?, account_id: row.get(1)?, amount: amount_text(row.get(2)?), occurred_on: row.get(3)?, note: row.get(4)?, source: row.get(5)?, deleted: row.get(6)?, revision_id: row.get(7)?,
    })).map_err(database_error)?.collect::<Result<Vec<_>, _>>().map_err(database_error)?;
    drop(query);
    for account in &mut accounts { account.balance = amount_text(*calculated.balances.get(&account.id).unwrap_or(&0)); }
    let dividends=dividends::load_dividends(connection)?;
    let dividend_totals=dividends::totals(&dividends)?;
    Ok(Snapshot { dividends,dividend_totals,accounts, entries, trades, positions: calculated.positions, openings: opening::load_openings(connection)?, movements: movements::load_movements(connection)? })
}
