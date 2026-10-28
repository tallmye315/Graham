import { useRecordSession, type RecordUndo } from './record-session';
import { useEffect, useState, type FormEvent } from 'react';
import { ArrowDownToLine, DatabaseBackup, Landmark, LayoutDashboard, Plus, ShieldCheck, Wallet } from 'lucide-react';
import { Button } from './components/ui/button';
import { Card, CardContent } from './components/ui/card';
import { Input } from './components/ui/input';
import { Label } from './components/ui/label';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog';
import { cashTotal, createAccount, updateAccount, deleteEmptyAccount, setAccountArchived, createDesktopShortcut, displayAmount, loadLedger, recordDeposit, reviseDeposit, setDepositDeleted, restoreRevision, type Currency, type Snapshot, type Entry, type Account, openingCovers } from './ledger';
import { RecordHistory } from './components/record-history';
import { AnalysisPanel } from './components/analysis-panel';
import { IncomePanel } from './components/income-panel';
import { ReleasePanel } from './components/release-panel';
import { MoneyWeightedPanel } from './components/money-weighted-panel';
import { PerformancePanel } from './components/performance-panel';
import { ValuationPanel } from './components/valuation-panel';
import { MarketPanel } from './components/market-panel';
import { getMarketQuotes, type QuoteState } from './ledger';
import { ImportPanel } from './components/import-panel';
import { DividendPanel } from './components/dividend-panel';
import { MovementPanel } from './components/movement-panel';
import { OpeningPanel } from './components/opening-panel';
import { TradePanel } from './components/trade-panel';
import { BackupPanel } from './components/backup-panel';
import { RecordDeletionDialog } from './components/record-deletion-dialog';
import { permanentlyDeleteDeposits, setDepositsDeleted, undoDepositBatch, type RecordRestore } from './ledger';
import wordmark from '../assets/brand/wordmark.svg';

const currencies: Currency[] = ['CNY', 'USD', 'HKD'];
const currencyNames = { CNY: '人民币', USD: '美元', HKD: '港币' };
function today() {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;
}

export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [dialog, setDialog] = useState<'account' | 'deposit' | 'history' | 'delete' | 'archive' | 'deleteAccount' | null>(null);
  const [editing, setEditing] = useState<Entry | null>(null);
  const [editingAccount, setEditingAccount] = useState<Account | null>(null);
  const [shortcutMessage, setShortcutMessage] = useState<string | null>(null);
  const [batchUndo, setBatchUndo] = useState<RecordRestore[] | null>(null);
  const [saving, setSaving] = useState(false);
  const [marketState,setMarketState]=useState<QuoteState|null>(null);
  useEffect(()=>{
    let alive=true;setMarketState(null);
    if(!snapshot?.positions?.some(p=>p.quantity!=='0'&&(['CN','HK','US'].includes(p.market))))return;
    const read=()=>{void getMarketQuotes(false).then(s=>{if(alive)setMarketState(s);}).catch(()=>{});};read();const timer=setInterval(read,5000);return()=>{alive=false;clearInterval(timer);};
  },[snapshot]);
  const [recordState, setRecordState] = useState('active');
  const [accountFilter, setAccountFilter] = useState('all');
  const [page, setPage] = useState<'overview' | 'records' | 'backup' | 'trades' | 'opening' | 'movements' | 'dividends' | 'imports' | 'market' | 'valuation' | 'performance' | 'money' | 'analysis' | 'release'>('overview');
  const [pendingDeletion, setPendingDeletion] = useState<Entry[] | null>(null);
  const [selected, setSelected] = useState<Entry[]>([]);

  const session = useRecordSession<Entry>({
    busy: saving, onBusyChange: setSaving, onChanged: applySnapshot,
    records: result => result.entries,
    onSuccess: () => { setDialog(null); setEditing(null); setEditingAccount(null); },
  });
  const { error, setError } = session;
  const undo = batchUndo ? { batch: batchUndo } : session.undo;
  function setUndo(value: RecordUndo | { batch: RecordRestore[] } | null) {
    setBatchUndo(value && 'batch' in value ? value.batch : null);
    session.setUndo(value && !('batch' in value) ? value : null);
  }

  function applySnapshot(result: Snapshot) {
    setSnapshot(result); setSelected([]); setBatchUndo(null);
  }
  function applyRestoredSnapshot(result: Snapshot) {
    applySnapshot(result); setUndo(null); setDialog(null); setEditing(null); setEditingAccount(null); setPendingDeletion(null); setAccountFilter('all'); setRecordState('active'); setError(null);
  }
  async function changeBatch(entries: Entry[], deleted: boolean) {
    if (saving) return;
    setSaving(true); setError(null);
    try {
      const result = await setDepositsDeleted(entries.map(({ id, revisionId }) => ({ id, revisionId })), deleted);
      applySnapshot(result.snapshot); setUndo({ batch: result.undo }); setPendingDeletion(null);
    } catch (e) { setError(typeof e === 'string' ? e : '批量操作失败，请刷新记录后重试。'); }
    finally { setSaving(false); }
  }
  async function undoBatch(restore: RecordRestore[]) {
    if (saving) return;
    setSaving(true); setError(null);
    try { applySnapshot(await undoDepositBatch(restore)); setUndo(null); }
    catch (e) { setError(typeof e === 'string' ? e : '撤回失败，请刷新记录后重试。'); }
    finally { setSaving(false); }
  }
  async function refreshRecords() {
    if (saving) return;
    setSaving(true); setError(null);
    try { applySnapshot(await loadLedger()); setPendingDeletion(null); setDialog(null); setEditing(null); setUndo(null); }
    catch { setError('刷新记录失败，请保留当前账本后重试。'); }
    finally { setSaving(false); }
  }
  async function purge(entries: Entry[]) {
    if (saving || !entries.length || entries.some(entry => !entry.deleted)) return;
    setSaving(true); setError(null);
    try {
      applySnapshot(await permanentlyDeleteDeposits(entries.map(({ id, revisionId }) => ({ id, revisionId }))));
      setPendingDeletion(null); setEditing(null); setDialog(null); setUndo(null);
    } catch (e) { setError(typeof e === 'string' ? e : '彻底删除失败，请刷新记录后重试。'); }
    finally { setSaving(false); }
  }

  async function reload() {
    setError(null);
    try { applySnapshot(await loadLedger()); }
    catch { setError('无法打开本地账本，请从 Graham 桌面软件启动，或重试。'); }
  }
  useEffect(() => { void reload(); }, []);

  async function restore(id: string, expectedRevision: number, revisionId: number) {
    await session.change(() => restoreRevision(id, expectedRevision, revisionId));
  }
  async function changeDeleted(entry: Entry, deleted: boolean) {
    await session.change(() => setDepositDeleted(entry.id, entry.revisionId, deleted), entry);
  }
  async function accountAction(account: Account, action: 'delete' | 'archive' | 'unarchive') {
    if (saving) return;
    setSaving(true); setError(null);
    try {
      applySnapshot(action === 'delete' ? await deleteEmptyAccount(account.id) : await setAccountArchived(account.id, action === 'archive'));
      if (action === 'delete') setAccountFilter('all');
      setDialog(null); setUndo(null);
    } catch (e) { setError(typeof e === 'string' ? e : '账户操作失败，请重试。'); }
    finally { setSaving(false); }
  }
  async function makeShortcut() {
    setSaving(true); setError(null); setShortcutMessage(null);
    try { setShortcutMessage(await createDesktopShortcut()); }
    catch (e) { setError(typeof e === 'string' ? e : '创建桌面快捷方式失败，请重试。'); }
    finally { setSaving(false); }
  }
  function showHistory(entry: Entry) {
    setEditing(entry); setError(null); setDialog('history');
  }

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await session.change(() => dialog === 'account'
        ? editingAccount
          ? updateAccount(editingAccount.id, String(form.get('name')), editingAccount.hasHistory ? editingAccount.currency : String(form.get('currency')))
          : createAccount(String(form.get('name')), String(form.get('currency')))
        : editing
          ? reviseDeposit(editing.id, editing.revisionId, String(form.get('date')), String(form.get('amount')), String(form.get('note') || ''))
          : recordDeposit(String(form.get('account')), String(form.get('date')), String(form.get('amount')), String(form.get('note') || '')), editing || undefined);
  }

  const accounts = snapshot?.accounts || [];
  const activeAccounts = accounts.filter(a => !a.archived);
  const scoped = accounts.filter(a => accountFilter === 'all' || a.id === accountFilter);
  const entries = (snapshot?.entries || []).filter(e => (accountFilter === 'all' || e.accountId === accountFilter) && !!e.deleted === (recordState === 'deleted'));

  return <div className="desktop-shell">
    <aside className="sidebar"><div className="wordmark"><img src={wordmark} alt="Graham" /></div>
      <nav aria-label="主导航"><button className={page === 'overview' ? 'active' : ''} disabled={saving} onClick={() => setPage('overview')}><LayoutDashboard size={18} />总览</button><button className={page === 'records' ? 'active' : ''} disabled={saving} onClick={() => setPage('records')}><Landmark size={18} />账户与记录</button><button className={page === 'backup' ? 'active' : ''} disabled={saving} onClick={() => setPage('backup')}><DatabaseBackup size={18} />备份与恢复</button><button className={page === 'trades' ? 'active' : ''} disabled={saving} onClick={() => setPage('trades')}><Wallet size={18} />买卖与持仓</button><button className={page === 'opening' ? 'active' : ''} disabled={saving} onClick={() => setPage('opening')}><Landmark size={18} />期初快照</button><button className={page === 'movements' ? 'active' : ''} disabled={saving} onClick={() => setPage('movements')}><ArrowDownToLine size={18} />转账与换汇</button><button className={page === 'dividends' ? 'active' : ''} disabled={saving} onClick={() => setPage('dividends')}><Wallet size={18} />分红记录</button><button className={page === 'imports' ? 'active' : ''} disabled={saving} onClick={() => setPage('imports')}><ArrowDownToLine size={18} />CSV 导入</button><button className={page === 'performance' ? 'active' : ''} disabled={saving} onClick={() => setPage('performance')}><LayoutDashboard size={18} />投资表现曲线</button><button className={page === 'money' ? 'active' : ''} disabled={saving} onClick={() => setPage('money')}><Wallet size={18} />个人资金回报</button><button className={page === 'analysis' ? 'active' : ''} disabled={saving} onClick={() => setPage('analysis')}><LayoutDashboard size={18} />组合分析</button><button className={page === 'valuation' ? 'active' : ''} disabled={saving} onClick={() => setPage('valuation')}><LayoutDashboard size={18} />多币种估值</button><button className={page === 'market' ? 'active' : ''} disabled={saving} onClick={() => setPage('market')}><LayoutDashboard size={18} />行情</button><button className={page === 'release' ? 'active' : ''} disabled={saving} onClick={() => setPage('release')}><ShieldCheck size={18} />版本与使用说明</button></nav>
      <div className="local-state"><ShieldCheck size={16} /><span>本地账本</span><i /></div><button className="shortcut-button" disabled={saving} onClick={() => void makeShortcut()}>创建桌面快捷方式</button>
    </aside>
    <main><header className="topbar"><span>工作台 / {page === 'release' ? '版本与使用说明' : page === 'overview' ? '总览' : page === 'backup' ? '备份与恢复' : page === 'trades' ? '买卖与持仓' : page === 'opening' ? '期初快照' : page === 'movements' ? '转账与换汇' : page === 'dividends' ? '分红记录' : page === 'imports' ? 'CSV 导入' : page === 'performance' ? '投资表现曲线' : page === 'money' ? '个人资金回报' : page === 'analysis' ? '组合分析' : page === 'valuation' ? '多币种估值' : page === 'market' ? 'A 股 / 港股 / 美股行情' : '账户与记录'}</span><span className="status"><i />{snapshot ? '已读取本地记录' : '正在连接账本'}</span></header>
      <div className="content"><div className="page-title"><div><h1>{page === 'release' ? '版本与使用说明' : page === 'overview' ? '投资总览' : page === 'backup' ? '备份与恢复' : page === 'trades' ? '买卖与持仓' : page === 'opening' ? '期初快照' : page === 'movements' ? '转账与换汇' : page === 'dividends' ? '分红记录' : page === 'imports' ? 'CSV 导入' : page === 'performance' ? '投资表现曲线' : page === 'money' ? '个人资金回报' : page === 'analysis' ? '组合分析' : page === 'valuation' ? '多币种估值' : page === 'market' ? 'A 股 / 港股 / 美股行情' : '账户与记录'}</h1></div>{page !== 'backup' && page !== 'trades' && page !== 'opening' && page !== 'movements' && page !== 'dividends' && page !== 'imports' && page !== 'market' && page !== 'valuation' && page !== 'performance' && page !== 'money' && page !== 'analysis' && page !== 'release' && <div className="actions"><Button variant="outline" disabled={!snapshot || saving} onClick={() => { setEditing(null); setEditingAccount(null); setError(null); setDialog('account'); }}><Plus />创建账户</Button><Button disabled={!activeAccounts.length || saving} onClick={() => { setEditing(null); setError(null); setDialog('deposit'); }}><ArrowDownToLine />录入入金</Button></div>}</div>
      {shortcutMessage && <p className="shortcut-message" role="status">{shortcutMessage}</p>}
      {undo && <div className="undo-banner" role="status">记录已更新<Button variant="outline" size="sm" disabled={saving} onClick={() => 'batch' in undo ? void undoBatch(undo.batch) : void restore(undo.id, undo.current, undo.previous)}>{'batch' in undo ? '撤回本次操作' : '撤回'}</Button><button aria-label="关闭撤回提示" onClick={() => setUndo(null)}>×</button></div>}
      {error && !dialog && !pendingDeletion && <div className="error-banner" role="alert">{error}<Button size="sm" variant="outline" disabled={saving} onClick={() => void refreshRecords()}>刷新记录</Button></div>}
      {!snapshot && !error && <p role="status" className="loading">正在打开本地账本…</p>}
      {page === 'release' ? <ReleasePanel/> : page === 'analysis' ? snapshot && <AnalysisPanel snapshot={snapshot}/> : page === 'money' ? snapshot && <MoneyWeightedPanel snapshot={snapshot}/> : page === 'performance' ? snapshot && <PerformancePanel snapshot={snapshot}/> : page === 'valuation' ? snapshot && <ValuationPanel snapshot={snapshot}/> : page === 'market' ? snapshot && <MarketPanel snapshot={snapshot} state={marketState} onChanged={setMarketState}/> : page === 'imports' ? snapshot && <ImportPanel snapshot={snapshot} busy={saving} onBusyChange={setSaving} onChanged={result => { applySnapshot(result); setUndo(null); }} /> : page === 'dividends' ? snapshot && <DividendPanel snapshot={snapshot} busy={saving} onBusyChange={setSaving} onChanged={result => { applySnapshot(result); setUndo(null); }} /> : page === 'movements' ? snapshot && <MovementPanel snapshot={snapshot} busy={saving} onBusyChange={setSaving} onChanged={result => { applySnapshot(result); setUndo(null); }} /> : page === 'opening' ? snapshot && <OpeningPanel snapshot={snapshot} busy={saving} onBusyChange={setSaving} onChanged={result => { applySnapshot(result); setUndo(null); }} /> : page === 'trades' ? snapshot && <TradePanel market={marketState} snapshot={snapshot} busy={saving} onBusyChange={setSaving} onChanged={result => { applySnapshot(result); setUndo(null); }} /> : page === 'backup' ? <BackupPanel busy={saving} ready={!!snapshot} onBusyChange={setSaving} onRestored={applyRestoredSnapshot} /> : snapshot && <>
        {!accounts.length ? <Card className="empty-card"><CardContent><div className="empty-icon"><Wallet size={30} /></div><h2>建立第一个投资账户</h2><p>添加账户后，即可录入你的第一笔资金记录。</p></CardContent></Card> : <>
          <div className="scope"><span>{accounts.length} 个投资账户</span><Label htmlFor="account-filter" className="sr-only">选择账户</Label><select id="account-filter" value={accountFilter} disabled={saving} onChange={e => { setAccountFilter(e.target.value); setSelected([]); }}><option value="all">全部账户</option>{accounts.map(a => <option key={a.id} value={a.id}>{a.name}{a.archived ? '（已归档）' : ''}</option>)}</select></div>
          {page === 'overview' && <><IncomePanel snapshot={snapshot} account={accountFilter}/><div className="cash-cards">{currencies.map(currency => <Card key={currency}><CardContent><div className="metric-label">账户现金 <span>{currency}</span></div><div className="cash-value">{displayAmount(cashTotal(scoped, currency), currency)}</div><small>{currencyNames[currency]} · 原币余额</small></CardContent></Card>)}</div></>}
          {snapshot.openings?.filter(o=>!o.deleted&&(accountFilter==='all'||o.input.accountId===accountFilter)).map(o=><p className="opening-coverage" key={o.id}>{accounts.find(a=>a.id===o.input.accountId)?.name}：从 {o.input.occurredOn} 收盘期初开始核算，此前收益数据不足。</p>)}
          <section className="ledger-section"><h2>投资账户</h2><div className="account-list">{scoped.map(a => <div className="account-row" key={a.id}><span className="account-icon"><Landmark size={20}/></span><div><strong>{a.name}</strong><small>{currencyNames[a.currency]}账户{a.archived ? ' · 已归档' : ''}</small></div><strong className="account-balance">{displayAmount(a.balance, a.currency)}</strong><div className="row-actions"><Button size="sm" variant="ghost" aria-label="修改账户" disabled={saving} onClick={() => { setEditing(null); setEditingAccount(a); setError(null); setDialog('account'); }}>修改</Button>{a.archived ? <Button size="sm" variant="ghost" disabled={saving} onClick={() => void accountAction(a, 'unarchive')}>取消归档</Button> : <Button size="sm" variant="ghost" aria-label="归档账户" disabled={saving} onClick={() => { setEditingAccount(a); setError(null); setDialog('archive'); }}>归档</Button>}{!a.hasHistory && <Button size="sm" variant="ghost" aria-label="删除空账户" disabled={saving} onClick={() => { setEditingAccount(a); setError(null); setDialog('deleteAccount'); }}>删除</Button>}</div></div>)}</div></section>
          <section className="ledger-section"><div className="section-heading"><h2>资金记录 <span className="count">{entries.length}</span></h2><Label htmlFor="record-state" className="sr-only">记录状态</Label><select id="record-state" value={recordState} disabled={saving} onChange={e => { setRecordState(e.target.value); setSelected([]); }}><option value="active">有效记录</option><option value="deleted">已删除</option></select></div>{!entries.length ? <div className="no-records">{recordState === 'deleted' ? '暂无已删除记录' : '暂无资金记录'}</div> : <><div className="batch-toolbar"><span>已选择 {selected.length} 条</span><Button size="sm" variant="ghost" disabled={saving || !selected.length} onClick={() => setSelected([])}>取消选择</Button>{recordState === 'deleted' ? <><Button size="sm" variant="outline" disabled={saving || !selected.length} onClick={() => void changeBatch(selected, false)}>批量恢复</Button><Button size="sm" variant="destructive" disabled={saving || !selected.length} onClick={() => void purge([...selected])}>批量彻底删除</Button></> : <Button size="sm" variant="outline" disabled={saving || !selected.length} onClick={() => { setError(null); setPendingDeletion([...selected]); }}>批量删除</Button>}<Button size="sm" variant="ghost" disabled={saving} onClick={() => void refreshRecords()}>刷新记录</Button></div><div className="table-scroll"><table><thead><tr><th className="selection-cell"><input type="checkbox" aria-label="全选当前筛选记录" disabled={saving} checked={selected.length === entries.length} ref={input => { if (input) input.indeterminate = selected.length > 0 && selected.length < entries.length; }} onChange={event => setSelected(event.target.checked ? [...entries] : [])} /></th><th>日期</th><th>账户</th><th>类型</th><th>金额 / 原币</th><th>备注</th><th>来源</th><th>操作</th></tr></thead><tbody>{entries.map(entry => { const a = accounts.find(a => a.id === entry.accountId)!; return <tr key={entry.id}><td className="selection-cell"><input type="checkbox" aria-label={`选择记录 ${entry.occurredOn} ${a.name} ${entry.amount} ${entry.note}`} disabled={saving} checked={selected.some(e => e.id === entry.id)} onChange={event => setSelected(event.target.checked ? [...selected, entry] : selected.filter(e => e.id !== entry.id))} /></td><td>{entry.occurredOn}</td><td>{a.name}</td><td><span className="type-badge">入金</span></td><td className="money">+{displayAmount(entry.amount, a.currency)}</td><td className="entry-note">{entry.note || '—'}</td><td className="subtle">{entry.source}{openingCovers(snapshot,entry.accountId,entry.occurredOn)&&<small className="security-code">期初已覆盖，不重复计入</small>}</td><td className="row-actions">{entry.deleted ? <><Button size="sm" variant="ghost" aria-label="恢复记录" disabled={saving} onClick={() => void changeDeleted(entry, false)}>恢复</Button><Button size="sm" variant="ghost" aria-label="彻底删除记录" disabled={saving} onClick={() => void purge([entry])}>彻底删除</Button></> : <><Button size="sm" variant="ghost" aria-label="修改记录" disabled={saving} onClick={() => { setEditing(entry); setError(null); setDialog('deposit'); }}>修改</Button><Button size="sm" variant="ghost" aria-label="删除记录" disabled={saving} onClick={() => { setEditing(entry); setError(null); setDialog('delete'); }}>删除</Button></>}<Button size="sm" variant="ghost" aria-label="记录历史" disabled={saving} onClick={() => void showHistory(entry)}>历史</Button></td></tr>; })}</tbody></table></div></>}</section>
        </>}
      </>}
      </div>
    </main>
    {pendingDeletion && <RecordDeletionDialog entries={pendingDeletion} accounts={accounts} saving={saving} error={error} onCancel={() => { setPendingDeletion(null); setError(null); }} onConfirm={() => void changeBatch(pendingDeletion, true)} onRefresh={() => void refreshRecords()} />}
    <Dialog open={dialog !== null} onOpenChange={open => { if (!open && !saving) { setDialog(null); setError(null); } }}><DialogContent className="entry-dialog"><DialogHeader><DialogTitle>{dialog === 'archive' ? '归档这个账户？' : dialog === 'deleteAccount' ? '删除空账户？' : dialog === 'delete' ? '删除这条记录？' : dialog === 'history' ? '记录历史' : dialog === 'account' ? editingAccount ? '修改投资账户' : '创建投资账户' : editing ? '修改入金' : '录入入金'}</DialogTitle><DialogDescription>{dialog === 'archive' ? '归档后停止新增入金，已有余额与历史仍计入总览，可随时取消归档。' : dialog === 'deleteAccount' ? '仅无任何历史记录的账户可删除。删除后不可恢复，可重新创建。' : dialog === 'delete' ? '删除后不再计入余额，原始记录会保留，可在已删除列表恢复。' : dialog === 'history' ? '每次修改和恢复均保留历史，可恢复到指定版本。' : dialog === 'account' ? '记录将保存在这台电脑的本地账本中。' : '记录资金转入，不会执行真实转账。'}</DialogDescription></DialogHeader>
      {dialog === 'archive' || dialog === 'deleteAccount' ? <div className="entry-form"><p>{editingAccount?.name}</p>{error && <p role="alert">{error}</p>}<div className="form-actions"><Button variant="outline" disabled={saving} onClick={() => setDialog(null)}>取消</Button><Button disabled={saving} onClick={() => editingAccount && void accountAction(editingAccount, dialog === 'archive' ? 'archive' : 'delete')}>{dialog === 'archive' ? '确认归档' : '确认删除空账户'}</Button></div></div> : dialog === 'delete' ? <div className="entry-form"><p>{editing?.occurredOn} · {editing?.amount}</p>{error && <p role="alert">{error}</p>}<div className="form-actions"><Button variant="outline" disabled={saving} onClick={() => setDialog(null)}>取消</Button><Button disabled={saving} onClick={() => editing && void changeDeleted(editing, true)}>确认删除</Button></div></div> : dialog === 'history' && editing ? <><RecordHistory key={editing.id} entry={editing} currency={accounts.find(a => a.id === editing.accountId)?.currency} saving={saving} onRestore={revisionId => void restore(editing.id, editing.revisionId, revisionId)} />{error && <p className="form-error" role="alert">{error}</p>}</> : <form onSubmit={save} className="entry-form">
        {dialog === 'account' ? <><Label htmlFor="account-name">账户名称</Label><Input id="account-name" name="name" required maxLength={80} placeholder="例如：华龙证券" defaultValue={editingAccount?.name} autoFocus/><Label htmlFor="currency">记账币种</Label><select id="currency" name="currency" disabled={!!editingAccount?.hasHistory} defaultValue={editingAccount?.currency || 'CNY'}>{currencies.map(c => <option key={c} value={c}>{c} · {currencyNames[c]}</option>)}</select></> : <><Label htmlFor="deposit-account">投资账户</Label><select id="deposit-account" name="account" disabled={!!editing} defaultValue={editing?.accountId || (activeAccounts.some(a => a.id === accountFilter) ? accountFilter : activeAccounts[0]?.id)}>{(editing ? accounts : activeAccounts).map(a => <option key={a.id} value={a.id}>{a.name} · {a.currency}</option>)}</select><Label htmlFor="date">入金日期</Label><Input id="date" name="date" type="date" required defaultValue={editing?.occurredOn || today()}/><Label htmlFor="amount">入金金额 / 原币</Label><Input id="amount" name="amount" inputMode="decimal" placeholder="0.00" defaultValue={editing?.amount} required autoFocus/><Label htmlFor="note">备注</Label><Input id="note" name="note" maxLength={500} defaultValue={editing?.note} placeholder="选填"/></>}
        {error && <p className="form-error" role="alert">{error}</p>}
        <div className="form-actions"><Button type="button" variant="outline" disabled={saving} onClick={() => { setDialog(null); setError(null); }}>取消</Button><Button type="submit" disabled={saving}>{saving ? '正在保存…' : '保存'}</Button></div>
      </form>}
      {error && (dialog === 'deposit' || dialog === 'delete' || dialog === 'history') && <Button type="button" variant="outline" disabled={saving} onClick={() => void refreshRecords()}>刷新记录</Button>}
    </DialogContent></Dialog>
  </div>;
}
