import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, test, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import App from './App';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const account = { id: 'a', name: '验收账户', currency: 'CNY', balance: '0.00', archived: false, hasHistory: true };
const security = { accountId: 'a', occurredOn: '2026-10-27', market: 'CN', code: '600036', kind: 'stock', name: '招商银行', currency: 'CNY', note: '' };
const cases = [
  { page: '期初快照', state: '期初快照状态', button: '彻底删除期初快照', refresh: '刷新记录', field: 'openings', command: 'permanently_delete_opening', input: { accountId: 'a', occurredOn: '2026-10-27', cash: '0.00', positions: [], note: '' } },
  { page: '转账与换汇', state: '关联记录状态', button: '彻底删除关联记录', refresh: '刷新记录', field: 'movements', command: 'permanently_delete_movement', input: { kind: 'transfer', fromAccountId: 'a', toAccountId: 'b', occurredOn: '2026-10-27', fromCurrency: 'CNY', toCurrency: 'CNY', fromAmount: '10.00', toAmount: '10.00', actualRate: '1', fee: '0.00', feeSide: 'from', note: '' } },
  { page: '分红记录', state: '分红记录状态', button: '彻底删除分红', refresh: '刷新分红', field: 'dividends', command: 'permanently_delete_dividend', input: { ...security, gross: '10.00', tax: '0.00', net: '10.00', status: 'pending', paidOn: null } },
];

test.each(cases)('$page 点击彻底删除直接执行，过期失败可刷新重选', async item => {
  const user = userEvent.setup();
  let purges = 0;
  const record = { id: 'record', revisionId: 7, deleted: true, source: '手动录入', input: item.input };
  const original = { accounts: [account, { ...account, id: 'b', name: '另一个账户' }], entries: [], trades: [], positions: [], [item.field]: [record] };
  vi.mocked(invoke).mockReset().mockImplementation(async command => {
    if (command === 'ledger_snapshot') return { ...original, [item.field]: [{ ...record, revisionId: purges ? 8 : 7 }] };
    if (command === item.command) { if (++purges === 1) throw '记录已变化，请刷新重选'; return { ...original, [item.field]: [] }; }
    throw new Error('Unexpected command: ' + command);
  });
  render(<App />);
  await user.click(await screen.findByRole('button', { name: item.page }));
  expect(screen.queryByRole('button', { name: item.button })).not.toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText(item.state), 'deleted');
  await user.click(screen.getByRole('button', { name: item.button }));
  expect(await screen.findByRole('alert')).toHaveTextContent('记录已变化');
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('输入“彻底删除”以确认')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: item.refresh }));
  await user.click(await screen.findByRole('button', { name: item.button }));
  expect(screen.queryByRole('button', { name: item.button })).not.toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith(item.command, { id: 'record', expectedRevision: 8 });
});
