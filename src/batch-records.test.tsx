import { nativeReplies, withIncome } from './test-native';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, test, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import App from './App';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const account = { id: 'a', name: '验收账户', currency: 'CNY', balance: '0.00', archived: false, hasHistory: true };
const entry = { id: 'e', accountId: 'a', amount: '12.30', occurredOn: '2026-10-25', note: '待清除', source: '手动录入', deleted: true, revisionId: 2 };

test('已删除记录点击彻底删除立即清除，不输入文字或再次确认', async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:{ accounts: [account], entries: [entry] }},{value:{ accounts: [{ ...account, hasHistory: false }], entries: [] }}]));
  render(<App />);
  await user.selectOptions(await screen.findByLabelText('记录状态'), 'deleted');
  await user.click(screen.getByRole('button', { name: '彻底删除记录' }));
  expect(await screen.findByText('暂无已删除记录')).toBeVisible();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('输入“彻底删除”以确认')).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '恢复记录' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '撤回' })).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: '删除空账户' })).toBeVisible();
  expect(invoke).toHaveBeenCalledWith('permanently_delete_deposits', { selection: [{ id: 'e', revisionId: 2 }] });
});

test('筛选切换清空选择，批量删除仅处理当前选择且可整批撤回', async () => {
  const user = userEvent.setup();
  const active = { ...entry, deleted: false, revisionId: 1 };
  const other = { ...account, id: 'b', name: '另一个账户', balance: '12.30' };
  const original = { accounts: [{ ...account, balance: '12.30' }, other], entries: [active, { ...active, id: 'hidden', accountId: 'b' }] };
  const undo = [{ current: { id: 'e', revisionId: 3 }, restoreRevisionId: 1 }];
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:original},{value:{ snapshot: { accounts: [account, other], entries: [{ ...entry, revisionId: 3 }, original.entries[1]] }, undo }},{value:original}]));
  render(<App />);
  await user.click(await screen.findByRole('checkbox', { name: '全选当前筛选记录' }));
  expect(screen.getByText('已选择 2 条')).toBeVisible();
  await user.selectOptions(screen.getByLabelText('选择账户'), 'a');
  expect(screen.getByText('已选择 0 条')).toBeVisible();
  expect(screen.getByRole('button', { name: '批量删除' })).toBeDisabled();
  await user.click(screen.getByRole('checkbox', { name: '全选当前筛选记录' }));
  await user.click(screen.getByRole('button', { name: '批量删除' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: '确认删除' }));
  expect(await screen.findByText('暂无资金记录')).toBeVisible();
  expect(invoke).toHaveBeenCalledWith('set_deposits_deleted', { selection: [{ id: 'e', revisionId: 1 }], deleted: true });
  await user.click(screen.getByRole('button', { name: '撤回本次操作' }));
  expect(await screen.findByText('+¥12.30')).toBeVisible();
  expect(invoke).toHaveBeenCalledWith('undo_deposit_batch', { restore: undo });
  expect(screen.getByText('已选择 0 条')).toBeVisible();
});

test('切换记录状态清空选择，已删除记录可批量恢复并整批撤回', async () => {
  const user = userEvent.setup();
  const second = { ...entry, id: 'second', amount: '7.70', revisionId: 3 };
  const original = { accounts: [account], entries: [entry, second] };
  const undo = [{ current: { id: 'e', revisionId: 4 }, restoreRevisionId: 2 }, { current: { id: 'second', revisionId: 5 }, restoreRevisionId: 3 }];
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:original},{value:{ snapshot: { accounts: [{ ...account, balance: '20.00' }], entries: [{ ...entry, deleted: false, revisionId: 4 }, { ...second, deleted: false, revisionId: 5 }] }, undo }},{value:original}]));
  render(<App />);
  await user.selectOptions(await screen.findByLabelText('记录状态'), 'deleted');
  await user.click(screen.getByRole('checkbox', { name: '全选当前筛选记录' }));
  await user.selectOptions(screen.getByLabelText('记录状态'), 'active');
  expect(screen.queryByRole('button', { name: '批量彻底删除' })).not.toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText('记录状态'), 'deleted');
  expect(screen.getByText('已选择 0 条')).toBeVisible();
  await user.click(screen.getByRole('checkbox', { name: '全选当前筛选记录' }));
  await user.click(screen.getByRole('button', { name: '批量恢复' }));
  expect(await screen.findByText('暂无已删除记录')).toBeVisible();
  await user.click(screen.getByRole('button', { name: '撤回本次操作' }));
  expect(await screen.findByText('已选择 0 条')).toBeVisible();
  expect(screen.getAllByRole('button', { name: '恢复记录' })).toHaveLength(2);
  expect(invoke).toHaveBeenCalledWith('undo_deposit_batch', { restore: undo });
});

test('批量彻底删除失败保留范围，刷新后重新选择且不显示旧撤回入口', async () => {
  const user = userEvent.setup();
  const second = { ...entry, id: 'second', revisionId: 3 };
  const original = { accounts: [account], entries: [entry, second] };
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:original},{value:'记录已变化或不存在，请刷新记录后重新选择',reject:true},{value:{ accounts: [{ ...account, balance: '12.30' }], entries: [entry, { ...second, deleted: false, revisionId: 4 }] }}]));
  render(<App />);
  await user.selectOptions(await screen.findByLabelText('记录状态'), 'deleted');
  await user.click(screen.getByRole('checkbox', { name: '全选当前筛选记录' }));
  await user.click(screen.getByRole('button', { name: '批量彻底删除' }));
  const alert = await screen.findByRole('alert');
  expect(alert).toHaveTextContent('记录已变化');
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(screen.getByText('已选择 2 条')).toBeVisible();
  expect(invoke).toHaveBeenCalledWith('permanently_delete_deposits', { selection: [{ id: 'e', revisionId: 2 }, { id: 'second', revisionId: 3 }] });
  await user.click(within(alert).getByRole('button', { name: '刷新记录' }));
  expect(await screen.findByText('已选择 0 条')).toBeVisible();
  expect(screen.getAllByRole('button', { name: '恢复记录' })).toHaveLength(1);
  expect(screen.queryByRole('button', { name: '撤回本次操作' })).not.toBeInTheDocument();
});


test('批量彻底删除直接处理当前选择，等待时禁用重复操作并保留未选记录', async () => {
  const user = userEvent.setup();
  const second = { ...entry, id: 'second', revisionId: 3 };
  const hidden = { ...entry, id: 'hidden', accountId: 'b', revisionId: 4 };
  const accounts = [account, { ...account, id: 'b', name: '未选账户' }];
  let finish!: (value: unknown) => void;
  const request = new Promise(resolve => { finish = resolve; });
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:{ accounts, entries: [entry, second, hidden] }},{value:request}]));
  render(<App />);
  await user.selectOptions(await screen.findByLabelText('记录状态'), 'deleted');
  await user.selectOptions(screen.getByLabelText('选择账户'), 'a');
  await user.click(screen.getByRole('checkbox', { name: '全选当前筛选记录' }));
  const button = screen.getByRole('button', { name: '批量彻底删除' });
  await user.click(button);
  expect(button).toBeDisabled();
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  await user.click(button);
  expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === 'permanently_delete_deposits')).toHaveLength(1);
  expect(invoke).toHaveBeenCalledWith('permanently_delete_deposits', { selection: [{ id: 'e', revisionId: 2 }, { id: 'second', revisionId: 3 }] });
  await act(async () => finish({ accounts, entries: [hidden] }));
  expect(await screen.findByText('暂无已删除记录')).toBeVisible();
  await user.selectOptions(screen.getByLabelText('选择账户'), 'all');
  expect(screen.getAllByRole('button', { name: '恢复记录' })).toHaveLength(1);
  expect(screen.queryByRole('button', { name: '撤回本次操作' })).not.toBeInTheDocument();
});
