import { useEffect, useRef, useState } from 'react';
import type { Snapshot } from './ledger';

type VersionedRecord = { id: string; revisionId: number };
export type RecordUndo = { id: string; current: number; previous: number };

export function useRecordHistory<H>(record: VersionedRecord | null, load?: (id: string) => Promise<H[]>) {
  const [history, setHistory] = useState<H[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    setHistory(null); setError(null);
    if (record && load) {
      void load(record.id).then(
        value => { if (active) setHistory(value); },
        reason => { if (active) setError(typeof reason === 'string' ? reason : '历史读取失败，请刷新后重试'); },
      );
    }
    return () => { active = false; };
  }, [record?.id, record?.revisionId, load]);
  return { history, error };
}

export function useRecordSession<R extends VersionedRecord, H = never>(options: {
  busy: boolean;
  onBusyChange: (busy: boolean) => void;
  onChanged: (snapshot: Snapshot) => void;
  records: (snapshot: Snapshot) => R[];
  onSuccess: () => void;
  historyRecord?: R | null;
  historyLoader?: (id: string) => Promise<H[]>;
}) {
  const [error, setError] = useState<string | null>(null);
  const [undo, setUndo] = useState<RecordUndo | null>(null);
  const pending = useRef(false);
  const history = useRecordHistory(options.historyRecord ?? null, options.historyLoader);

  async function change(action: () => Promise<Snapshot>, previous?: R) {
    if (options.busy || pending.current) return;
    pending.current = true;
    options.onBusyChange(true); setError(null);
    try {
      const result = await action();
      const current = previous && options.records(result).find(record => record.id === previous.id);
      setUndo(previous && current ? { id: current.id, current: current.revisionId, previous: previous.revisionId } : null);
      options.onChanged(result);
      options.onSuccess();
    } catch (reason) {
      setError(typeof reason === 'string' ? reason : '操作失败，请刷新后重试');
    } finally {
      pending.current = false;
      options.onBusyChange(false);
    }
  }

  return { error: error ?? history.error, setError, history: history.history, undo, setUndo, change };
}
