import { useState, type FormEvent } from 'react';
import { chooseBackupFile, exportBackup, previewBackup, restoreBackup, type BackupPreview, type Snapshot } from '../ledger';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';

export function BackupPanel({ busy, ready, onBusyChange, onRestored }: { busy: boolean; ready: boolean; onBusyChange: (value: boolean) => void; onRestored: (snapshot: Snapshot) => void }) {
  const [password, setPassword] = useState('');
  const [repeat, setRepeat] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [source, setSource] = useState('');
  const [restorePassword, setRestorePassword] = useState('');
  const [preview, setPreview] = useState<BackupPreview | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const validPassword = Array.from(password).length >= 12 && password.trim().length > 0 && new TextEncoder().encode(password).length <= 4096;

  async function save(event: FormEvent) {
    event.preventDefault();
    if (busy || !ready || !validPassword || password !== repeat) return;
    onBusyChange(true); setMessage(null); setError(null);
    try {
      const path = await chooseBackupFile('export');
      if (!path) { setMessage('已取消导出'); return; }
      await exportBackup(path, password);
      setPassword(''); setRepeat(''); setMessage(`加密备份已保存：${path}`);
    } catch (e) { setError(typeof e === 'string' ? e : '导出失败，请选择其他位置后重试。'); }
    finally { onBusyChange(false); }
  }

  async function chooseSource() {
    if (busy) return;
    onBusyChange(true); setError(null); setMessage(null);
    try {
      const path = await chooseBackupFile('restore');
      if (path) { setSource(path); setRestorePassword(''); setPreview(null); setConfirmed(false); }
    } catch (e) { setError(typeof e === 'string' ? e : '无法选择备份文件，请重试。'); }
    finally { onBusyChange(false); }
  }

  async function verify(event: FormEvent) {
    event.preventDefault();
    if (busy || !source || !restorePassword) return;
    onBusyChange(true); setError(null); setMessage(null); setPreview(null); setConfirmed(false);
    try { setPreview(await previewBackup(source, restorePassword)); }
    catch (e) { setError(typeof e === 'string' ? e : '无法验证备份，当前账本未修改。'); }
    finally { onBusyChange(false); }
  }

  async function restore() {
    if (busy || !ready || !preview || !confirmed) return;
    onBusyChange(true); setError(null); setMessage(null);
    try {
      const result = await restoreBackup(source, restorePassword, preview.fingerprint);
      onRestored(result.snapshot); setSource(''); setRestorePassword(''); setPassword(''); setRepeat(''); setPreview(null); setConfirmed(false);
      setMessage(`账本已恢复。恢复前加密备份：${result.recoveryPath}。使用本次输入的恢复密码解密。`);
    } catch (e) { setPreview(null); setConfirmed(false); setError(typeof e === 'string' ? e : '恢复失败，当前账本未修改，请重新验证备份。'); }
    finally { onBusyChange(false); }
  }

  return <div className="backup-layout">
    {message && <p className="backup-message" role="status">{message}</p>}
    {error && <p className="error-banner" role="alert">{error}</p>}
    <section className="backup-card"><h2>导出加密备份</h2>
      <p>保存全部账户、资金、买卖、期初、转账换汇及分红记录、修改历史、CSV 原始批次/来源和账户配置。软件不保存密码，遗失后无法解密备份。</p>
      <form className="entry-form" onSubmit={event => void save(event)}>
        <Label htmlFor="backup-password">设置备份密码</Label><Input id="backup-password" type="password" autoComplete="new-password" value={password} disabled={busy} onChange={event => setPassword(event.target.value)} />
        <small>至少 12 个字符，建议使用便于保管的长口令。</small>
        <Label htmlFor="backup-repeat">再次输入密码</Label><Input id="backup-repeat" type="password" autoComplete="new-password" value={repeat} disabled={busy} onChange={event => setRepeat(event.target.value)} />
        <Button type="submit" disabled={busy || !ready || !validPassword || repeat !== password}>{busy ? '正在处理…' : '选择位置并导出'}</Button>
      </form>
    </section>
    <section className="backup-card"><h2>从备份恢复</h2>
      <p>恢复会用备份整体替换当前账本。开始前会自动保存当前账本的加密备份，使用本次输入的恢复密码。</p>
      <form className="entry-form" onSubmit={event => void verify(event)}>
        <Label htmlFor="restore-source">备份文件</Label><Input id="restore-source" readOnly value={source} placeholder="选择 .grahambak 文件" />
        <Button type="button" variant="outline" disabled={busy} onClick={() => void chooseSource()}>选择备份文件</Button>
        <Label htmlFor="restore-password">恢复密码</Label><Input id="restore-password" type="password" autoComplete="off" value={restorePassword} disabled={busy} onChange={event => { setRestorePassword(event.target.value); setPreview(null); setConfirmed(false); }} />
        <Button type="submit" variant="outline" disabled={busy || !source || !restorePassword}>验证备份</Button>
      </form>
      {preview && <div className="backup-preview"><h3>备份摘要</h3><p>创建时间：{new Date(preview.createdAt).toLocaleString('zh-CN')}</p><p>{preview.accountCount} 个账户 · {preview.recordCount} 条记录 · 其中 {preview.deletedCount} 条已删除</p><label className="restore-confirmation"><input type="checkbox" checked={confirmed} disabled={busy} onChange={event => setConfirmed(event.target.checked)} />我确认用此备份替换当前账本</label><Button variant="destructive" disabled={busy || !ready || !confirmed} onClick={() => void restore()}>{busy ? '正在恢复…' : '确认恢复'}</Button></div>}
    </section>
  </div>;
}
