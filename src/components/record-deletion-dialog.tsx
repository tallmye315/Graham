import { Button } from './ui/button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './ui/dialog';
import type { Account, Entry } from '../ledger';

export function RecordDeletionDialog({ entries, accounts, saving, error, onCancel, onConfirm, onRefresh }: {
  entries: Entry[]; accounts: Account[]; saving: boolean; error: string | null;
  onCancel: () => void; onConfirm: () => void; onRefresh: () => void;
}) {
  const scope = accounts.filter(a => entries.some(e => e.accountId === a.id));
  return <Dialog open onOpenChange={open => { if (!open && !saving) onCancel(); }}>
    <DialogContent className="entry-dialog"><DialogHeader>
      <DialogTitle>删除 {entries.length} 条记录？</DialogTitle>
      <DialogDescription>所选记录不再计入余额，原始内容和历史会保留，可在已删除列表恢复。</DialogDescription>
    </DialogHeader><form className="entry-form" onSubmit={event => { event.preventDefault(); if (!saving) onConfirm(); }}>
      <div className="purge-scope">{scope.map(a => <p key={a.id}>{a.name} · {entries.filter(e => e.accountId === a.id).length} 条</p>)}</div>
      {error && <><p role="alert" className="form-error">{error}</p><Button type="button" variant="outline" disabled={saving} onClick={onRefresh}>刷新记录并重新选择</Button></>}
      <div className="form-actions"><Button type="button" variant="outline" disabled={saving} onClick={onCancel}>取消</Button><Button type="submit" disabled={saving}>{saving ? '正在处理…' : '确认删除'}</Button></div>
    </form></DialogContent>
  </Dialog>;
}
