import { useRecordHistory } from '../record-session';
import { entryHistory, type Currency, type Entry, type Revision } from '../ledger';
import { Button } from './ui/button';

export function RecordHistory({ entry, currency, saving, onRestore }: {
  entry: Entry;
  currency: Currency | undefined;
  saving: boolean;
  onRestore: (revisionId: number) => void;
}) {
  const { history, error } = useRecordHistory<Revision>(entry, entryHistory);

  return <div className="history-list">
    {error && <p role="alert">读取记录历史失败，请关闭后重试。</p>}
    {!history && !error && <p role="status">正在读取历史…</p>}
    {history?.map(revision => <div className="history-item" key={revision.id}>
      <strong>{revision.action}{revision.deleted ? ' · 已删除' : ''}</strong>
      <time>{new Date(revision.changedAt).toLocaleString('zh-CN')}</time>
      <p>{revision.occurredOn} · {revision.amount} {currency}</p>
      <p>{revision.note || '无备注'}</p>
      <Button variant="outline" size="sm" disabled={saving} onClick={() => onRestore(revision.id)}>恢复此版本</Button>
    </div>)}
  </div>;
}
