import { nativeReplies, withIncome } from './test-native';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, test, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import App from './App';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

test('录入买入后显示真实现金和持仓，失败保留交易输入', async () => {
  const user = userEvent.setup();
  const account = { id: 'a', name: '验收账户', currency: 'CNY', balance: '5000.00', archived: false, hasHistory: true };
  vi.mocked(invoke).mockReset().mockImplementation(nativeReplies([{value:{ accounts: [account], entries: [], trades: [], positions: [] }},{value:'账户现金不足',reject:true},{value:{ accounts: [{ ...account, balance: '3995.00' }], entries: [], trades: [], positions: [{ accountId: 'a', market: 'CN', code: '600036', kind: 'stock', name: '招商银行', currency: 'CNY', quantity: '100', cost: '1005.00', averageCost: '10.05', realized: '0.00' }] }}],{quotes:[],holdings:[],cacheWarning:null,automatic:true}));
  render(<App />);
  await user.click(await screen.findByRole('button', { name: '买卖与持仓' }));
  await user.click(screen.getByRole('button', { name: '录入买卖' }));
  await user.type(screen.getByLabelText('证券代码'), '600036');
  await user.type(screen.getByLabelText('证券名称'), '招商银行');
  await user.type(screen.getByLabelText('数量 / 股或份'), '100');
  await user.type(screen.getByLabelText('成交单价 / 原币'), '10');
  await user.clear(screen.getByLabelText('实际费用 / 原币'));
  await user.type(screen.getByLabelText('实际费用 / 原币'), '5');
  await user.click(screen.getByRole('button', { name: '保存交易' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('账户现金不足');
  expect(screen.getByLabelText('证券代码')).toHaveValue('600036');
  await user.click(screen.getByRole('button', { name: '保存交易' }));
  expect(await screen.findByText('¥1,005.00')).toBeVisible();
  expect(screen.getByText('¥3,995.00')).toBeVisible();
  expect(invoke).toHaveBeenCalledWith('record_trade', { input: expect.objectContaining({ market: 'CN', code: '600036', quantity: '100', price: '10', fee: '5', tax: '0', currency: 'CNY' }) });
});


test('交易彻底删除只在已删除列表出现，直接点击执行，失败可刷新且成功清除撤回', async () => {
  const user = userEvent.setup();
  const account = { id: 'a', name: '验收账户', currency: 'CNY', balance: '90.00', archived: false, hasHistory: true };
  const trade = { id: 't', input: { accountId: 'a', occurredOn: '2026-10-25', market: 'CN', code: '600036', kind: 'stock', name: '招商银行', currency: 'CNY', side: 'buy', quantity: '1', price: '10', fee: '0', tax: '0', note: '' }, source: '手动录入', deleted: false, revisionId: 1 };
  const active = { accounts: [account], entries: [], trades: [trade], positions: [] };
  const deleted = { accounts: [{ ...account, balance: '100.00' }], entries: [], trades: [{ ...trade, deleted: true, revisionId: 2 }], positions: [] };
  let purges = 0;
  vi.mocked(invoke).mockReset().mockImplementation(withIncome(async command => {
    if (command === 'ledger_snapshot') return purges ? deleted : active;
    if (command === 'set_trade_deleted') return deleted;
    if (command === 'permanently_delete_trade') { purges++; if (purges === 1) throw '交易已变化，请刷新后重试'; return { ...deleted, trades: [] }; }
    throw new Error(`unexpected command ${command}`);
  }));
  render(<App />);
  await user.click(await screen.findByRole('button', { name: '买卖与持仓' }));
  expect(screen.queryByRole('button', { name: '彻底删除交易' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '删除交易' }));
  await user.click(screen.getByRole('button', { name: '确认删除交易' }));
  expect(await screen.findByRole('button', { name: '撤回交易修改' })).toBeVisible();
  await user.selectOptions(screen.getByLabelText('买卖记录状态'), 'deleted');
  await user.click(screen.getByRole('button', { name: '彻底删除交易' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('交易已变化');
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('输入“彻底删除”以确认')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '刷新记录' }));
  await user.click(await screen.findByRole('button', { name: '彻底删除交易' }));
  expect(await screen.findByText('暂无已删除买卖记录')).toBeVisible();
  expect(screen.queryByRole('button', { name: '撤回交易修改' })).not.toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith('permanently_delete_trade', { id: 't', expectedRevision: 2 });
});
