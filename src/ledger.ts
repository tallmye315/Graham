import { invoke } from '@tauri-apps/api/core';

export type Currency = 'CNY' | 'USD' | 'HKD';
export interface Account { id: string; name: string; currency: Currency; balance: string; archived: boolean; hasHistory: boolean }
export interface Entry { id: string; accountId: string; amount: string; occurredOn: string; note: string; source: string; deleted: boolean; revisionId: number }
export interface Revision { id: number; amount: string; occurredOn: string; note: string; deleted: boolean; changedAt: string; action: string }
export interface Snapshot { accounts: Account[]; entries: Entry[]; trades?: Trade[]; positions?: Position[]; openings?: Opening[]; movements?: Movement[]; dividends?: Dividend[]; dividendTotals?: DividendTotal[] }
export interface TradeInput { accountId: string; occurredOn: string; market: 'CN' | 'HK' | 'US'; code: string; kind: 'stock' | 'etf'; name: string; currency: Currency; side: 'buy' | 'sell'; quantity: string; price: string; fee: string; tax: string; note: string }
export interface Trade { id: string; input: TradeInput; source: string; deleted: boolean; revisionId: number }
export interface TradeRevision { id: number; tradeId: string; input: TradeInput; deleted: boolean; changedAt: string; action: string }
export interface Position { accountId: string; market: string; code: string; kind: string; name: string; currency: Currency; quantity: string; cost: string; averageCost: string; realized: string; since?: string | null }
export interface PositionValue { marketValue: string; unrealized: string }
export const recordTrade = (input: TradeInput) => invoke<Snapshot>('record_trade', { input });
export const reviseTrade = (id: string, expectedRevision: number, input: TradeInput) => invoke<Snapshot>('revise_trade', { id, expectedRevision, input });
export const setTradeDeleted = (id: string, expectedRevision: number, deleted: boolean) => invoke<Snapshot>('set_trade_deleted', { id, expectedRevision, deleted });
export const permanentlyDeleteTrade = (id: string, expectedRevision: number) => invoke<Snapshot>('permanently_delete_trade', { id, expectedRevision });
export const tradeHistory = (id: string) => invoke<TradeRevision[]>('trade_history', { id });
export const restoreTradeRevision = (id: string, expectedRevision: number, restoreRevision: number) => invoke<Snapshot>('restore_trade_revision', { id, expectedRevision, restoreRevision });
export const valuePosition = (account: string, market: string, code: string, price: string) => invoke<PositionValue>('value_position', { account, market, code, price });
export interface RecordSelection { id: string; revisionId: number }
export interface RecordRestore { current: RecordSelection; restoreRevisionId: number }
export interface BatchUpdate { snapshot: Snapshot; undo: RecordRestore[] }

export const loadLedger = () => invoke<Snapshot>('ledger_snapshot');
export const createAccount = (name: string, currency: string) => invoke<Snapshot>('create_account', { name, currency });
export const recordDeposit = (accountId: string, date: string, amount: string, note: string) => invoke<Snapshot>('record_deposit', { accountId, date, amount, note });
export const reviseDeposit = (id: string, expectedRevision: number, date: string, amount: string, note: string) => invoke<Snapshot>('revise_deposit', { id, expectedRevision, date, amount, note });
export const entryHistory = (id: string) => invoke<Revision[]>('entry_history', { id });
export const restoreRevision = (id: string, expectedRevision: number, revisionId: number) => invoke<Snapshot>('restore_deposit_revision', { id, expectedRevision, revisionId });
export const setDepositDeleted = (id: string, expectedRevision: number, deleted: boolean) => invoke<Snapshot>('set_deposit_deleted', { id, expectedRevision, deleted });
export const permanentlyDeleteDeposits = (selection: RecordSelection[]) => invoke<Snapshot>('permanently_delete_deposits', { selection });
export const setDepositsDeleted = (selection: RecordSelection[], deleted: boolean) => invoke<BatchUpdate>('set_deposits_deleted', { selection, deleted });
export const undoDepositBatch = (restore: RecordRestore[]) => invoke<Snapshot>('undo_deposit_batch', { restore });
export const updateAccount = (id: string, name: string, currency: string) => invoke<Snapshot>('update_account', { id, name, currency });
export const deleteEmptyAccount = (id: string) => invoke<Snapshot>('delete_empty_account', { id });
export const setAccountArchived = (id: string, archived: boolean) => invoke<Snapshot>('set_account_archived', { id, archived });
export const createDesktopShortcut = () => invoke<string>('create_desktop_shortcut');
export const chooseBackupFile = (action: 'export' | 'restore') => invoke<string | null>('choose_backup_file', { action });
export const exportBackup = (path: string, password: string) => invoke<void>('export_backup', { path, password });
export interface BackupPreview { createdAt: string; accountCount: number; recordCount: number; deletedCount: number; fingerprint: string }
export interface BackupRestored { snapshot: Snapshot; recoveryPath: string }
export const previewBackup = (path: string, password: string) => invoke<BackupPreview>('preview_backup', { path, password });
export const restoreBackup = (path: string, password: string, fingerprint: string) => invoke<BackupRestored>('restore_backup', { path, password, fingerprint });

export function displayAmount(amount: string, currency: Currency) {
  const [whole, fraction = ''] = amount.trim().split('.');
  return `${{ CNY: '¥', USD: '$', HKD: 'HK$' }[currency]}${whole.replace(/\B(?=(\d{3})+(?!\d))/g, ',')}.${fraction.padEnd(2, '0')}`;
}

export function cashTotal(accounts: Account[], currency: Currency) {
  const cents = accounts.filter(a => a.currency === currency).reduce((total, a) => total + BigInt(a.balance.replace('.', '')), 0n);
  return `${cents / 100n}.${(cents % 100n).toString().padStart(2, '0')}`;
}

export interface OpeningPosition {market: 'CN'|'HK'|'US';code:string;kind:'stock'|'etf';name:string;currency:Currency;quantity:string;cost:string;referencePrice:string|null}
export interface OpeningInput {accountId:string;occurredOn:string;cash:string;positions:OpeningPosition[];note:string}
export interface Opening {id:string;input:OpeningInput;source:string;deleted:boolean;revisionId:number}
export interface OpeningRevision {id:number;openingId:string;input:OpeningInput;deleted:boolean;changedAt:string;action:string}
export interface DatedSnapshot {date:string;accounts:{accountId:string;name:string;currency:Currency;balance:string|null;positions:Position[];unavailableReason:string|null}[]}
export const saveOpening=(input:OpeningInput,expectedRevision:number|null)=>invoke<Snapshot>('save_opening',{input,expectedRevision});
export const setOpeningDeleted=(id:string,expectedRevision:number,deleted:boolean)=>invoke<Snapshot>('set_opening_deleted',{id,expectedRevision,deleted});
export const restoreOpeningRevision=(id:string,expectedRevision:number,restoreRevision:number)=>invoke<Snapshot>('restore_opening_revision',{id,expectedRevision,restoreRevision});
export const permanentlyDeleteOpening=(id:string,expectedRevision:number)=>invoke<Snapshot>('permanently_delete_opening',{id,expectedRevision});
export const openingHistory=(id:string)=>invoke<OpeningRevision[]>('opening_history',{id});
export const snapshotOn=(date:string)=>invoke<DatedSnapshot>('snapshot_on',{date});
export function openingCovers(snapshot:Snapshot,accountId:string,date:string){return !!snapshot.openings?.some(o=>!o.deleted&&o.input.accountId===accountId&&date<=o.input.occurredOn);}

export interface MovementInput{fromAccountId:string;toAccountId:string;occurredOn:string;kind:'transfer'|'exchange';fromCurrency:Currency;toCurrency:Currency;fromAmount:string;toAmount:string;actualRate:string;fee:string;feeSide:'from'|'to';note:string}
export interface Movement{id:string;input:MovementInput;source:string;deleted:boolean;revisionId:number;eventOrder:number}
export interface MovementRevision{id:number;movementId:string;input:MovementInput;deleted:boolean;changedAt:string;action:string}
export interface FlowSummary{startOn:string;endOn:string;totals:{currency:Currency;inflow:string;outflow:string;netCapital:string;fees:string}[];internalCount:number;unavailableReason:string|null}
export const recordMovement=(input:MovementInput)=>invoke<Snapshot>('record_movement',{input});
export const reviseMovement=(id:string,expectedRevision:number,input:MovementInput)=>invoke<Snapshot>('revise_movement',{id,expectedRevision,input});
export const setMovementDeleted=(id:string,expectedRevision:number,deleted:boolean)=>invoke<Snapshot>('set_movement_deleted',{id,expectedRevision,deleted});
export const restoreMovementRevision=(id:string,expectedRevision:number,restoreRevision:number)=>invoke<Snapshot>('restore_movement_revision',{id,expectedRevision,restoreRevision});
export const permanentlyDeleteMovement=(id:string,expectedRevision:number)=>invoke<Snapshot>('permanently_delete_movement',{id,expectedRevision});
export const movementHistory=(id:string)=>invoke<MovementRevision[]>('movement_history',{id});
export const cashFlows=(accountIds:string[],startOn:string,endOn:string)=>invoke<FlowSummary>('cash_flows',{accountIds,startOn,endOn});

export interface DividendInput{accountId:string;occurredOn:string;paidOn:string|null;status:'pending'|'paid';market:'CN'|'HK'|'US';code:string;kind:'stock'|'etf';name:string;currency:Currency;gross:string;tax:string;net:string;note:string}
export interface Dividend{id:string;input:DividendInput;source:string;deleted:boolean;revisionId:number;eventOrder:number}
export interface DividendRevision{id:number;dividendId:string;input:DividendInput;deleted:boolean;changedAt:string;action:string}
export interface DividendTotal{accountId:string;market:string;code:string;name:string;kind:string;currency:Currency;gross:string;tax:string;net:string;pending:string;paid:string;recordIds:string[]}
export const recordDividend=(input:DividendInput)=>invoke<Snapshot>('record_dividend',{input});
export const reviseDividend=(id:string,expectedRevision:number,input:DividendInput)=>invoke<Snapshot>('revise_dividend',{id,expectedRevision,input});
export const setDividendDeleted=(id:string,expectedRevision:number,deleted:boolean)=>invoke<Snapshot>('set_dividend_deleted',{id,expectedRevision,deleted});
export const restoreDividendRevision=(id:string,expectedRevision:number,restoreRevision:number)=>invoke<Snapshot>('restore_dividend_revision',{id,expectedRevision,restoreRevision});
export const permanentlyDeleteDividend=(id:string,expectedRevision:number)=>invoke<Snapshot>('permanently_delete_dividend',{id,expectedRevision});
export const dividendHistory=(id:string)=>invoke<DividendRevision[]>('dividend_history',{id});

export interface ImportChange{at:string;action:string;record:{type:string;input:Record<string,unknown>};deleted:boolean;csv:string}
export interface ImportItem{id:string;batchId:string;externalId:string;accountId:string;recordType:string;recordId:string;eventOrder:number;line:number;raw:string;original:{type:string;input:Record<string,unknown>};current:{type:string;input:Record<string,unknown>};currentCsv:string;deleted:boolean;history:ImportChange[]}
export interface ImportBatch{id:string;createdAt:string;rawCsv:string;withdrawnAt:string|null}
export interface ImportLog{fingerprint:string;batches:ImportBatch[];items:ImportItem[]}
export interface CsvRow{line:number;raw:string;externalId:string;recordType:string;accountId:string;date:string;currency:string;fields:Record<string,string>;status:string;error:string|null}
export interface CsvPreview{fingerprint:string;rows:CsvRow[];newCount:number;duplicateCount:number;canCommit:boolean;error:string|null}
export interface ImportResult{snapshot:Snapshot;log:ImportLog}
export interface BatchUndoPreview{fingerprint:string;items:ImportItem[];accountIds:string[];canUndo:boolean;error:string|null}
export const importLog=()=>invoke<ImportLog>('import_log');
export const previewCsv=(text:string)=>invoke<CsvPreview>('preview_csv',{text});
export const commitCsv=(text:string,expectedFingerprint:string)=>invoke<ImportResult>('commit_csv',{text,expectedFingerprint});
export const reviseImportItem=(id:string,text:string,expectedFingerprint:string)=>invoke<ImportResult>('revise_import_item',{id,text,expectedFingerprint});
export const setImportItemDeleted=(id:string,expectedFingerprint:string,deleted:boolean)=>invoke<ImportResult>('set_import_item_deleted',{id,expectedFingerprint,deleted});
export const previewImportBatchUndo=(batchId:string)=>invoke<BatchUndoPreview>('preview_import_batch_undo',{batchId});
export const undoImportBatch=(batchId:string,expectedFingerprint:string)=>invoke<ImportResult>('undo_import_batch',{batchId,expectedFingerprint});
export const chooseCsvFile=()=>invoke<string|null>('choose_csv_file');

export interface QuoteKey{market:string;code:string;kind:string}
export interface DailyClose{date:string;close:string}
export interface QuoteView{history?:DailyClose[];key:QuoteKey;price:string|null;currency:Currency;source:string;sourceTime:string|null;quotedAt:string|null;receivedAt:string|null;attemptedAt:string|null;status:string;error:string|null;retryAt:string|null;refreshing:boolean;marketOpen:boolean;delaySeconds:number|null}
export interface QuotedHolding{accountId:string;key:QuoteKey;quantity:string;cost:string;marketValue:string|null;unrealized:string|null;valuationError:string|null}
export interface QuoteState{quotes:QuoteView[];holdings:QuotedHolding[];cacheWarning:string|null;automatic:boolean}
export const getMarketQuotes=(probe=false)=>invoke<QuoteState>('get_market_quotes',{probe});
export const refreshMarketQuotes=(probe=false)=>invoke<QuoteState>('refresh_market_quotes',{probe});
export const setMarketAuto=(enabled:boolean)=>invoke<QuoteState>('set_market_auto',{enabled});
export function quoteIdentity(key:QuoteKey){return`${key.market}:${key.code}:${key.kind}`;}

export interface ValuationQuery{accountIds:string[];currency:Currency;date:string|null}
export interface ValuedAsset{accountId:string;name:string;kind:string;currency:Currency;quantity:string|null;price:string|null;originalValue:string|null;convertedValue:string|null;source:string;sourceTime:string|null;reason:string|null}
export interface CurrencyDistribution{currency:Currency;cash:string|null;securities:string|null;receivables:string|null;knownOriginal:string|null;convertedValue:string|null;share:string|null;complete:boolean}
export interface ValuationSource{key:QuoteKey;date:string|null;value:string|null;currency:Currency;source:string;basis:string;sourceTime:string|null;quotedAt:string|null;receivedAt:string|null;attemptedAt:string|null;status:string;error:string|null;retryAt:string|null}
export interface ValuationState{currency:Currency;date:string;historical:boolean;complete:boolean;total:string|null;knownTotal:string|null;missing:string[];warnings:string[];assets:ValuedAsset[];distribution:CurrencyDistribution[];sources:ValuationSource[];cacheWarning:string|null}
export const getValuation=(query:ValuationQuery)=>invoke<ValuationState>('get_valuation',{query});
export const refreshValuation=(query:ValuationQuery,probe=false)=>invoke<ValuationState>('refresh_valuation',{query,probe});

export interface PerformanceQuery{accountIds:string[];currency:Currency;range:'month'|'ytd'|'all'|'custom';startOn:string|null;endOn:string|null}
export interface PerformancePoint{date:string;value:string|null;inflow:string|null;outflow:string|null;dailyReturn:string|null;cumulativeReturn:string|null;status:string;reasons:string[]}
export interface PerformanceState{startOn:string;endOn:string;currency:Currency;baselineOn:string|null;totalReturn:string|null;complete:boolean;method:string;points:PerformancePoint[];warnings:string[];refresh:{pending:number;retryAt:string|null;error:string|null;active:boolean}}
export const getPerformance=(query:PerformanceQuery)=>invoke<PerformanceState>('get_performance',{query});
export const refreshPerformance=(query:PerformanceQuery)=>invoke<PerformanceState>('refresh_performance',{query});
export const stopPerformanceRefresh=()=>invoke<void>('stop_performance_refresh');

export type MoneyWeightedFlow={date:string;kind:string;amount:string|null;reasons:string[]};
export type MoneyWeightedState={startOn:string;endOn:string;baselineOn:string|null;currency:Currency;elapsedDays:number;annualizedReturn:string|null;periodReturn:string|null;status:string;reason:string|null;flows:MoneyWeightedFlow[];warnings:string[];refresh:PerformanceState['refresh']};
export const getMoneyWeighted=(query:PerformanceQuery)=>invoke<MoneyWeightedState>('get_money_weighted',{query});
export const refreshMoneyWeighted=(query:PerformanceQuery)=>invoke<MoneyWeightedState>('refresh_money_weighted',{query});

export type AnalysisRow={accountId:string;accountName:string;key:QuoteKey;name:string;currency:Currency;industry:string;industrySource:string|null;quantity:string;cost:string;realized:string;unrealized:string|null;netDividend:string;originalMarketValue:string|null;marketValue:string|null;realizedValue:string|null;unrealizedValue:string|null;dividendValue:string|null;netContribution:string|null;since:string|null;source:string;sourceTime:string|null;reasons:string[]};
export type AllocationGroup={dimension:string;key:string;label:string;knownValue:string|null;share:string|null;missingCount:number;memberCount:number};
export type AnalysisBar={key:QuoteKey;name:string;amount:string|null;share:string|null;excluded:number};
export type AnalysisContribution={metric:string;label:string;knownTotal:string|null;complete:boolean;bars:AnalysisBar[]};
export type AnalysisState={date:string;currency:Currency;totalAssets:string|null;knownAssets:string|null;assetComplete:boolean;sources:ValuationSource[];rows:AnalysisRow[];allocations:AllocationGroup[];contributions:AnalysisContribution[];warnings:string[];missing:string[]};
export const getAnalysis=(query:ValuationQuery)=>invoke<AnalysisState>('get_analysis',{query});
export const refreshAnalysis=(query:ValuationQuery)=>invoke<AnalysisState>('refresh_analysis',{query});

export interface IncomeAmounts {
  realized: string; unrealized: string | null; netDividend: string; pendingDividend: string; paidDividend: string;
  total: string | null; fee: string; tax: string; dividendTax: string; today: string | null;
  todayDividend: string; todayFee: string; todayTax: string; todayDividendTax: string; dailyReturn: string | null;
}
export interface IncomeRow {
  accountId: string; accountName: string; key: QuoteKey; name: string; quantity: string; cost: string; closed: boolean;
  amounts: IncomeAmounts; sourceTime: string | null; receivedAt: string | null; reasons: string[];
}
export interface MarketIncome {
  market: string; currency: Currency; tradingDate: string | null; previousDate: string | null;
  status: string; amounts: IncomeAmounts; rows: IncomeRow[];
}
export interface IncomeState { markets: MarketIncome[]; cacheWarning: string | null; method: string }
export const getIncome = (accountIds: string[]) => invoke<IncomeState>('get_income', { query: { accountIds } });
export const refreshIncome = (accountIds: string[]) => invoke<IncomeState>('refresh_income', { query: { accountIds } });
