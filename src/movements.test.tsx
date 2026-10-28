import{render,screen}from'@testing-library/react';import userEvent from'@testing-library/user-event';import{test,expect,vi}from'vitest';import{invoke}from'@tauri-apps/api/core';import App from'./App';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn()}));
test('关联转账保留失败输入、展示两端余额，单账户资金流可核对',async()=>{
 const user=userEvent.setup();const a={id:'a',name:'转出验收',currency:'CNY',balance:'100000.00',archived:false,hasHistory:true};const b={...a,id:'b',name:'转入验收',balance:'0.00',hasHistory:false};const initial={accounts:[a,b],entries:[],trades:[],positions:[],openings:[],movements:[]};let saves=0;
 vi.mocked(invoke).mockReset().mockImplementation(async(command,args)=>{
  if(command==='ledger_snapshot')return initial;
  if(command==='record_movement'){saves++;if(saves===1)throw'账户现金不足';return{...initial,accounts:[{...a,balance:'89990.00'},{...b,balance:'10000.00',hasHistory:true}],movements:[{id:'m',input:(args as {input:unknown}).input,source:'手动录入',deleted:false,revisionId:1,eventOrder:1}]};}
  if(command==='cash_flows')return{startOn:'2026-01-01',endOn:'2026-12-31',unavailableReason:null,internalCount:0,totals:[{currency:'CNY',inflow:'10000.00',outflow:'0.00',netCapital:'10000.00',fees:'0.00'}]};
  throw new Error(String(command));
 });render(<App/>);await user.click(await screen.findByRole('button',{name:'转账与换汇'}));await user.click(screen.getByRole('button',{name:'录入转账 / 换汇'}));
 await user.type(screen.getByLabelText('转出本金 / 原币'),'10000');await user.clear(screen.getByLabelText('手续费 / 所选币种'));await user.type(screen.getByLabelText('手续费 / 所选币种'),'10');
 await user.click(screen.getByRole('button',{name:'保存关联记录'}));expect(await screen.findByRole('alert')).toHaveTextContent('账户现金不足');expect(screen.getByLabelText('转出本金 / 原币')).toHaveValue('10000');
 await user.click(screen.getByRole('button',{name:'保存关联记录'}));expect(await screen.findByText('¥89,990.00')).toBeVisible();expect(screen.getByText('¥10,000.00')).toBeVisible();
 await user.selectOptions(screen.getByLabelText('转账账户范围'),'b');await user.click(screen.getByRole('button',{name:'核对资金流'}));expect(await screen.findByText('本范围新增本金 / CNY')).toBeVisible();
 expect(invoke).toHaveBeenCalledWith('record_movement',{input:expect.objectContaining({fromAccountId:'a',toAccountId:'b',fromAmount:'10000',toAmount:'10000',actualRate:'1',fee:'10',feeSide:'from'})});
 expect(invoke).toHaveBeenCalledWith('cash_flows',expect.objectContaining({accountIds:['b']}));
});
