import type { InvokeArgs, InvokeOptions } from '@tauri-apps/api/core';

// Keep legacy record-action fixtures independent of new read-only dashboard queries.
type Native = (command: string, args?: InvokeArgs, options?: InvokeOptions) => unknown;
export function withIncome(native: Native) {
  return async (command: string, args?: InvokeArgs, options?: InvokeOptions) => {
    if (command === 'get_income' || command === 'refresh_income') return { markets: [], cacheWarning: null, method: '' };
    if (command === 'get_market_quotes') return { quotes: [], holdings: [], cacheWarning: null, automatic: true };
    return native(command, args, options);
  };
}
export function nativeReplies(replies: { value: unknown; reject?: boolean }[], fallback?: unknown) {
  return withIncome(() => {
    const next = replies.shift();
    if (next?.reject) return Promise.reject(next.value);
    return next ? next.value : fallback;
  });
}
