import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, test, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import App from './App';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('echarts/core', () => ({ use: vi.fn() }));

test('总览按市场原币分开展示今日与累计分类，已清仓证券在独立窗口查看', async () => {
  const user = userEvent.setup();
  const amounts = { realized: '58.00', unrealized: '72.00', netDividend: '8.00', pendingDividend: '8.00', paidDividend: '0.00', total: '138.00', fee: '2.00', tax: '0.00', dividendTax: '2.00', today: '38.00', todayDividend: '8.00', todayFee: '2.00', todayTax: '0.00', todayDividendTax: '2.00', dailyReturn: '3.454545' };
  const row = { accountId: 'a', accountName: '美元样例', key: { market: 'US', code: 'AAPL', kind: 'stock' }, name: '持有样例', quantity: '6', cost: '600.00', closed: false, amounts, sourceTime: '2026-10-27T11:00:00-04:00', receivedAt: '2026-10-27T15:00:00Z', reasons: [] };
  const closed = { ...row, name: '已清仓样例', key: { market: 'US', code: 'MSFT', kind: 'stock' }, closed: true, quantity: '0', cost: '0.00', amounts: { ...amounts, realized: '20.00', unrealized: '0.00', netDividend: '0.00', total: '20.00', today: '0.00', dailyReturn: null } };
  const state = { markets: [
    { market: 'US', currency: 'USD', tradingDate: '2026-10-27', previousDate: '2026-10-26', status: 'intraday', amounts: { ...amounts, realized: '78.00', total: '158.00' }, rows: [row, closed] },
    { market: 'CN', currency: 'CNY', tradingDate: '2026-10-28', previousDate: '2026-10-27', status: 'intraday', amounts: { ...amounts, today: '-12.00', total: '-30.00', dailyReturn: '-1.000000' }, rows: [] },
  ], cacheWarning: null, method: '证券原币口径，闲置现金单列，费用不重复扣除。' };
  vi.mocked(invoke).mockReset().mockImplementation(async command => {
    if (command === 'ledger_snapshot') return { accounts: [{ id: 'a', name: '美元样例', currency: 'USD', balance: '1000', archived: false, hasHistory: true }], positions: [], entries: [], trades: [], openings: [], movements: [], dividends: [] };
    if (command === 'get_income' || command === 'refresh_income') return state;
    throw new Error(String(command));
  });
  render(<App />);
  const us = await screen.findByRole('region', { name: '美股收益 · USD' });
  expect(us).toHaveTextContent('今日盈亏'); expect(us).toHaveTextContent('总盈亏');
  expect(us).toHaveTextContent('158.00'); expect(us).toHaveTextContent('38.00');
  expect(us).toHaveTextContent('已实现盈亏'); expect(us).toHaveTextContent('未实现盈亏');
  expect(us).toHaveTextContent('税后分红'); expect(us).toHaveTextContent('盘中估算');
  expect(await screen.findByRole('region', { name: 'A 股收益 · CNY' })).toHaveTextContent('12.00');
  expect(within(us).queryByText('已清仓样例')).not.toBeInTheDocument();
  await user.click(within(us).getByRole('button', { name: '查看美股已清仓证券' }));
  const dialog = screen.getByRole('dialog', { name: '已清仓证券 · 美股 · USD' });
  expect(dialog).toHaveTextContent('已清仓样例'); expect(dialog).toHaveTextContent('MSFT');
  expect(dialog).toHaveTextContent('20.00');
  await user.click(within(dialog).getByRole('button', { name: '关闭已清仓窗口' }));
  await user.click(screen.getByRole('button', { name: '刷新今日收益' }));
  expect(await screen.findByRole('region', { name: '美股收益 · USD' })).toHaveTextContent('2026-10-27');
});
