import {render,screen} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {test,expect,vi} from 'vitest';
import {invoke} from '@tauri-apps/api/core';
import App from './App';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn()}));
test('期初快照须确认覆盖范围，保存后展示起点，历史不足明确说明',async()=>{
 const user=userEvent.setup(); const account={id:'a',name:'验收账户',currency:'CNY',balance:'5000.00',archived:false,hasHistory:true};
 const original={accounts:[account],entries:[{id:'d',accountId:'a',occurredOn:'2026-01-01',amount:'5000.00',note:'',deleted:false,revisionId:1,source:'手动录入'}],trades:[],positions:[],openings:[]};
 vi.mocked(invoke).mockReset().mockImplementation(async(command,args)=>{
  if(command==='ledger_snapshot')return original;
  if(command==='save_opening')return {...original,accounts:[{...account,balance:'1000.00'}],openings:[{id:'o',input:(args as {input:unknown}).input,source:'手动核对',deleted:false,revisionId:1}]};
  if(command==='snapshot_on')return {date:'2026-01-01',accounts:[{accountId:'a',name:'验收账户',currency:'CNY',balance:null,positions:[],unavailableReason:'期初之前数据不足'}]};
  throw new Error(String(command));
 });
 render(<App/>); await user.click(await screen.findByRole('button',{name:'期初快照'})); await user.click(screen.getByRole('button',{name:'建立期初快照'}));
 await user.clear(screen.getByLabelText('核对日期（收盘后）')); await user.type(screen.getByLabelText('核对日期（收盘后）'),'2026-01-31');
 await user.clear(screen.getByLabelText('期初现金 / 原币')); await user.type(screen.getByLabelText('期初现金 / 原币'),'1000');
 expect(screen.getByRole('button',{name:'保存期初快照'})).toBeDisabled();
 await user.click(screen.getByLabelText('我已核对收盘状态，并确认该日及以前记录不重复计入'));
 await user.click(screen.getByRole('button',{name:'保存期初快照'}));
 expect(await screen.findByText('2026-01-31 收盘后')).toBeVisible();
 await user.clear(screen.getByLabelText('历史核对日期')); await user.type(screen.getByLabelText('历史核对日期'),'2026-01-01');
 await user.click(screen.getByRole('button',{name:'核对当日账本'})); expect(await screen.findByText('期初之前数据不足')).toBeVisible();
 expect(invoke).toHaveBeenCalledWith('save_opening',{input:expect.objectContaining({accountId:'a',occurredOn:'2026-01-31',cash:'1000',positions:[]}),expectedRevision:null});
});
