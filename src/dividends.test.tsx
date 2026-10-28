import{render,screen,within}from'@testing-library/react';import userEvent from'@testing-library/user-event';import{test,expect,vi}from'vitest';import{invoke}from'@tauri-apps/api/core';import App from'./App';import type{Account,Snapshot,DividendInput}from'./ledger';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn()}));
test('分红录入失败保留输入，待到账转已到账并可从汇总追溯原始记录',async()=>{
 const user=userEvent.setup();const a:Account={id:'a',name:'分红验收',currency:'CNY',balance:'0.00',archived:false,hasHistory:false};let current:Snapshot={accounts:[a],entries:[],dividends:[],dividendTotals:[]};let saves=0;
 vi.mocked(invoke).mockReset().mockImplementation(async(command,args)=>{
  if(command==='ledger_snapshot')return current;
  if(command==='record_dividend'||command==='revise_dividend'){
   if(command==='record_dividend'&&++saves===1)throw'税前、扣税与净额不符';
   const input=(args as {input:DividendInput}).input;const paid=input.status==='paid';const record={id:'d1',input,source:'手动录入',deleted:false,revisionId:paid?2:1,eventOrder:1};
   current={...current,accounts:[{...a,balance:paid?'90.00':'0.00',hasHistory:true}],dividends:[record],dividendTotals:[{accountId:'a',market:'CN',code:'600036',kind:'stock',name:'招商银行',currency:'CNY',gross:'100.00',tax:'10.00',net:'90.00',pending:paid?'0.00':'90.00',paid:paid?'90.00':'0.00',recordIds:['d1']}]};return current;
  }
  if(command==='set_dividend_deleted'){current={...current,accounts:[{...a,balance:'0.00',hasHistory:true}],dividends:current.dividends!.map(d=>({...d,deleted:true,revisionId:3})),dividendTotals:[]};return current;}
  if(command==='permanently_delete_dividend'){current={...current,dividends:[]};return current;}
  throw new Error(String(command));
 });render(<App/>);await user.click(await screen.findByRole('button',{name:'分红记录'}));await user.click(screen.getByRole('button',{name:'录入分红'}));
 await user.type(screen.getByLabelText('证券代码'),'600036');await user.type(screen.getByLabelText('证券名称'),'招商银行');await user.type(screen.getByLabelText('税前金额'),'100');await user.clear(screen.getByLabelText('扣税'));await user.type(screen.getByLabelText('扣税'),'10');await user.type(screen.getByLabelText('净额'),'90');
 await user.click(screen.getByRole('button',{name:'保存分红'}));expect(await screen.findByRole('alert')).toHaveTextContent('净额不符');expect(screen.getByLabelText('净额')).toHaveValue('90');
 await user.click(screen.getByRole('button',{name:'保存分红'}));expect(within(await screen.findByRole('table',{name:'分红原始记录'})).getByText('¥100.00 / ¥10.00')).toBeVisible();await user.click(screen.getByRole('button',{name:'修改分红'}));await user.selectOptions(screen.getByLabelText('到账状态'),'paid');await user.click(screen.getByRole('button',{name:'保存分红'}));
 expect(await screen.findByText('现金 ¥90.00')).toBeVisible();await user.click(screen.getByRole('button',{name:'查看来源 · 招商银行'}));expect(screen.getByText('来源记录 d1')).toBeVisible();expect(within(screen.getByRole('table',{name:'分红原始记录'})).getByText('已到账')).toBeVisible();
 expect(invoke).toHaveBeenCalledWith('revise_dividend',expect.objectContaining({id:'d1',expectedRevision:1,input:expect.objectContaining({gross:'100',tax:'10',net:'90',status:'paid',paidOn:expect.any(String)})}));
 await user.selectOptions(screen.getByLabelText('分红汇总维度'),'account');expect(screen.getByRole('button',{name:'查看来源 · 分红验收'})).toBeVisible();
 await user.click(screen.getByRole('button',{name:'删除分红'}));expect(within(screen.getByRole('dialog')).getByText('净额 ¥90.00')).toBeVisible();await user.click(screen.getByRole('button',{name:'确认删除'}));
 await user.selectOptions(screen.getByLabelText('分红记录状态'),'deleted');await user.click(await screen.findByRole('button',{name:'彻底删除分红'}));expect(screen.queryByRole('dialog')).not.toBeInTheDocument();expect(await screen.findByText('暂无对应分红记录')).toBeVisible();expect(screen.queryByRole('button',{name:'撤回分红修改'})).not.toBeInTheDocument();
 expect(invoke).toHaveBeenCalledWith('permanently_delete_dividend',{id:'d1',expectedRevision:3});
});
