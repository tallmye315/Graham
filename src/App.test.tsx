import { nativeReplies, withIncome } from './test-native';
import { act, render, screen } from '@testing-library/react';
import { vi, test, expect } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import App from './App';
import userEvent from '@testing-library/user-event';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

test('入金编辑过期时保留输入，可在弹窗内刷新后重新选择最新记录', async () => {
  const user = userEvent.setup();
  const account = { id: 'a', name: '版本核对', currency: 'CNY', balance: '100.00', archived: false, hasHistory: true };
  const entry = { id: 'e', accountId: 'a', amount: '100.00', occurredOn: '2026-10-25', note: '', source: '手动录入', deleted: false, revisionId: 1 };
  let reads = 0;
  vi.mocked(invoke).mockReset().mockImplementation(withIncome(async (command, args) => {
    if (command === 'ledger_snapshot') { reads++; return { accounts: [{ ...account, balance: reads === 1 ? '100.00' : '200.00' }], entries: [{ ...entry, amount: reads === 1 ? '100.00' : '200.00', revisionId: reads === 1 ? 1 : 2 }] }; }
    if (command === 'revise_deposit') {
      if ((args as { expectedRevision: number }).expectedRevision !== 2) throw '记录已变化或不存在，请刷新记录后重新选择';
      return { accounts: [{ ...account, balance: '300.00' }], entries: [{ ...entry, amount: '300.00', revisionId: 3 }] };
    }
    throw new Error(String(command));
  }));
  render(<App />);
  await user.click(await screen.findByRole('button', { name: '修改记录' }));
  await user.clear(screen.getByLabelText('入金金额 / 原币'));
  await user.type(screen.getByLabelText('入金金额 / 原币'), '300');
  await user.click(screen.getByRole('button', { name: '保存' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('请刷新记录后重新选择');
  expect(screen.getByLabelText('入金金额 / 原币')).toHaveValue('300');
  await user.click(screen.getByRole('button', { name: '刷新记录' }));
  expect(await screen.findByText('+¥200.00')).toBeVisible();
  await user.click(screen.getByRole('button', { name: '修改记录' }));
  await user.clear(screen.getByLabelText('入金金额 / 原币'));
  await user.type(screen.getByLabelText('入金金额 / 原币'), '300');
  await user.click(screen.getByRole('button', { name: '保存' }));
  expect(await screen.findByText('+¥300.00')).toBeVisible();
});

test('用户可查看版本及当前发布限制，证券标识和更新入口已移除', async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([],{ accounts: [], entries: [] }));
  render(<App />);
  await user.click(await screen.findByRole('button', { name: '版本与使用说明' }));
  expect(screen.getByRole('heading', { name: '版本与使用说明' })).toBeVisible();
  expect(screen.getByText(/华龙证券、兴业证券专用文件格式尚未适配/)).toBeVisible();
  expect(screen.getByText(/行情刷新不会同步券商交易/)).toBeVisible();
  expect(screen.queryByRole('img', { name: /公司标识|基金标识/ })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /更新.*Logo/ })).not.toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: '离线内置标识' })).not.toBeInTheDocument();
  expect(screen.getByRole('img', { name: 'Graham' })).toBeVisible();
  expect(screen.queryByRole('button', { name: '录入入金' })).not.toBeInTheDocument();
});

test('切换记录后忽略旧的历史查询结果', async () => {
  const user = userEvent.setup();
  let finishFirst!: (value: unknown) => void;
  const firstRequest = new Promise(resolve => { finishFirst = resolve; });
  const account = { id: 'a', name: '账户', currency: 'CNY', balance: '3.00', archived: false, hasHistory: true };
  const entry = { id: 'one', accountId: 'a', amount: '1.00', occurredOn: '2026-10-25', note: '', source: '手动录入', deleted: false, revisionId: 1 };
  const revision = { id: 2, amount: '2.00', occurredOn: entry.occurredOn, note: '第二条的历史', deleted: false, action: '录入', changedAt: '2026-10-25T00:00:00Z' };
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:{ accounts: [account], entries: [entry, { ...entry, id: 'two', amount: '2.00', revisionId: 2 }] }},{value:firstRequest},{value:[revision]}]));
  render(<App />);
  await user.click((await screen.findAllByRole('button', { name: '记录历史' }))[0]);
  await user.keyboard('{Escape}');
  await user.click(screen.getAllByRole('button', { name: '记录历史' })[1]);
  expect(await screen.findByText('第二条的历史')).toBeVisible();
  await act(async () => { finishFirst([{ ...revision, id: 1, note: '第一条迟到的历史' }]); });
  expect(screen.getByText('第二条的历史')).toBeVisible();
  expect(screen.queryByText('第一条迟到的历史')).not.toBeInTheDocument();
});

test('首次打开空账本时显示建账入口，不显示虚构资产', async () => {
  vi.mocked(invoke).mockImplementation(nativeReplies([],{ accounts: [], entries: [] }));
  render(<App />);
  expect(await screen.findByText('建立第一个投资账户')).toBeVisible();
  expect(screen.getByRole('button', { name: '创建账户' })).toBeEnabled();
  expect(screen.queryByText('¥894,654.32')).not.toBeInTheDocument();
});

test('已有记录账户修改时锁定币种，归档后仍显示原币余额', async () => {
  const user = userEvent.setup();
  const account = { id: 'a', name: '原名称', currency: 'CNY', balance: '20.00', archived: false, hasHistory: true };
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:{ accounts: [account], entries: [] }},{value:{ accounts: [{ ...account, name: '兴业证券' }], entries: [] }},{value:{ accounts: [{ ...account, name: '兴业证券', archived: true }], entries: [] }}]));
  render(<App />);
  await user.click(await screen.findByRole('button', { name: '修改账户' }));
  expect(screen.getByLabelText('记账币种')).toBeDisabled();
  await user.clear(screen.getByLabelText('账户名称'));
  await user.type(screen.getByLabelText('账户名称'), '兴业证券');
  await user.click(screen.getByRole('button', { name: '保存' }));
  await user.click(await screen.findByRole('button', { name: '归档账户' }));
  await user.click(screen.getByRole('button', { name: '确认归档' }));
  expect(await screen.findByRole('button', { name: '取消归档' })).toBeVisible();
  expect(screen.getAllByText('¥20.00')).toHaveLength(2);
  expect(screen.getByRole('button', { name: '录入入金' })).toBeDisabled();
});

test('删除记录需要确认，并能在已删除列表恢复', async () => {
  const user = userEvent.setup();
  const account = { id: 'a', name: '华龙证券', currency: 'CNY', balance: '12.30', archived: false, hasHistory: true };
  const entry = { id: 'e', accountId: 'a', amount: '12.30', occurredOn: '2026-10-25', note: '待修正', source: '手动录入', deleted: false, revisionId: 1 };
  const original = { accounts: [account], entries: [entry] };
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:original},{value:{ accounts: [{ ...account, balance: '0.00' }], entries: [{ ...entry, deleted: true, revisionId: 2 }] }},{value:original}]));
  render(<App />);
  await user.click(await screen.findByRole('button', { name: '删除记录' }));
  expect(screen.getByRole('heading', { name: '删除这条记录？' })).toBeVisible();
  expect(invoke).not.toHaveBeenCalledWith('set_deposit_deleted', expect.anything());
  await user.click(screen.getByRole('button', { name: '确认删除' }));
  expect(await screen.findByText('暂无资金记录')).toBeVisible();
  await user.selectOptions(screen.getByLabelText('记录状态'), 'deleted');
  await user.click(await screen.findByRole('button', { name: '恢复记录' }));
  expect(await screen.findByText('暂无已删除记录')).toBeVisible();
  await user.selectOptions(screen.getByLabelText('记录状态'), 'active');
  expect(screen.getByText('+¥12.30')).toBeVisible();
});

test('通过桌面命令创建账户并录入入金后显示原币记录', async () => {
  const user = userEvent.setup();
  const account = { id: 'account-1', name: '华龙证券', currency: 'CNY', balance: '0.00' };
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:{ accounts: [], entries: [] }},{value:{ accounts: [account], entries: [] }},{value:{ accounts: [{ ...account, balance: '100000.01' }], entries: [{ id: 'entry-1', accountId: 'account-1', amount: '100000.01', occurredOn: '2026-10-24', note: '首次入金', source: '手动录入' }] }}]));
  render(<App />);
  await user.click(await screen.findByRole('button', { name: '创建账户' }));
  await user.type(screen.getByLabelText('账户名称'), '华龙证券');
  await user.click(screen.getByRole('button', { name: '保存' }));
  expect(await screen.findByText('人民币账户')).toBeVisible();
  await user.click(screen.getByRole('button', { name: '录入入金' }));
  await user.type(screen.getByLabelText('入金金额 / 原币'), '100000.01');
  await user.click(screen.getByRole('button', { name: '保存' }));
  expect(await screen.findByText('+¥100,000.01')).toBeVisible();
  expect(screen.getByText('手动录入')).toBeVisible();
});

test('保存失败时显示原因并保留输入，允许修正', async () => {
  const user = userEvent.setup();
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:{ accounts: [], entries: [] }},{value:'账户名称不能为空',reject:true}]));
  render(<App />);
  await user.click(await screen.findByRole('button', { name: '创建账户' }));
  await user.type(screen.getByLabelText('账户名称'), '待核对账户');
  await user.click(screen.getByRole('button', { name: '保存' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('账户名称不能为空');
  expect(screen.getByLabelText('账户名称')).toHaveValue('待核对账户');
  expect(screen.getByRole('button', { name: '保存' })).toBeEnabled();
});

test('修正错误入金后可以立即撤回并打开持久化历史', async () => {
  const user = userEvent.setup();
  const account = { id: 'a', name: '华龙证券', currency: 'CNY', balance: '1200.00', archived: false, hasHistory: true };
  const entry = { id: 'e', accountId: 'a', amount: '1200.00', occurredOn: '2026-10-24', note: '误填', source: '手动录入', deleted: false, revisionId: 1 };
  const original = { accounts: [account], entries: [entry] };
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:original},{value:{ accounts: [{ ...account, balance: '125.30' }], entries: [{ ...entry, amount: '125.30', revisionId: 2 }] }},{value:original},{value:[{ id: 1, amount: '1200.00', occurredOn: entry.occurredOn, note: '误填', deleted: false, action: '录入', changedAt: '2026-10-25T00:00:00Z' }]}]));
  render(<App />);
  await user.click(await screen.findByRole('button', { name: '修改记录' }));
  await user.clear(screen.getByLabelText('入金金额 / 原币'));
  await user.type(screen.getByLabelText('入金金额 / 原币'), '125.30');
  await user.click(screen.getByRole('button', { name: '保存' }));
  expect(await screen.findByText('+¥125.30')).toBeVisible();
  expect(invoke).toHaveBeenCalledWith('revise_deposit', expect.objectContaining({ id: 'e', expectedRevision: 1 }));
  await user.click(screen.getByRole('button', { name: '撤回' }));
  expect(await screen.findByText('+¥1,200.00')).toBeVisible();
  expect(invoke).toHaveBeenCalledWith('restore_deposit_revision', { id: 'e', expectedRevision: 2, revisionId: 1 });
  await user.click(screen.getByRole('button', { name: '记录历史' }));
  expect(await screen.findByRole('heading', { name: '记录历史' })).toBeVisible();
  expect(screen.getByRole('button', { name: '恢复此版本' })).toBeVisible();
});
