import{render,screen}from'@testing-library/react';import userEvent from'@testing-library/user-event';import{expect,test,vi}from'vitest';import{invoke}from'@tauri-apps/api/core';import App from'./App';
vi.mock('@tauri-apps/api/core',()=>({invoke:vi.fn()}));vi.mock('echarts/core',()=>({use:vi.fn(),init:()=>({setOption:vi.fn(),resize:vi.fn(),dispose:vi.fn()})}));
test('投资表现按日展示，入金不是盈利，缺项不会画成正常零收益',async()=>{
 const user=userEvent.setup();vi.mocked(invoke).mockReset().mockImplementation(async(command,args)=>{if(command==='ledger_snapshot')return{accounts:[{id:'a',name:'已建账',currency:'CNY',balance:'100',archived:false,hasHistory:true}],entries:[],positions:[]};if(command==='get_performance'||command==='refresh_performance')return{startOn:'2026-10-19',endOn:'2026-10-22',currency:(args as {query:{currency:string}}).query.currency,baselineOn:'2026-10-19',totalReturn:null,complete:false,method:'日初入金 / 日末出金；按收盘逐日链接',points:[{date:'2026-10-19',value:'100.00',inflow:'0.00',outflow:'0.00',dailyReturn:null,cumulativeReturn:'0.000000',status:'baseline',reasons:[]},{date:'2026-10-20',value:'200.00',inflow:'100.00',outflow:'0.00',dailyReturn:'0.000000',cumulativeReturn:'0.000000',status:'ok',reasons:[]},{date:'2026-10-21',value:'210.00',inflow:'0.00',outflow:'0.00',dailyReturn:'5.000000',cumulativeReturn:'5.000000',status:'ok',reasons:[]},{date:'2026-10-22',value:null,inflow:'0.00',outflow:'0.00',dailyReturn:null,cumulativeReturn:null,status:'missing',reasons:['缺少当日价格']}],warnings:[],refresh:{pending:1,retryAt:null,error:null,active:false}};throw new Error(String(command));});render(<App/>);await user.click(await screen.findByRole('button',{name:'投资表现曲线'}));expect(await screen.findByText('区间收益率数据不足')).toBeVisible();const table=screen.getByRole('table',{name:'投资表现逐日核对'});expect(table).toHaveTextContent('5.000000%');expect(table).toHaveTextContent('缺少当日价格');expect(table).toHaveTextContent('¥100.00');await user.selectOptions(screen.getByLabelText('表现区间'),'ytd');expect(invoke).toHaveBeenCalledWith('get_performance',expect.objectContaining({query:expect.objectContaining({range:'ytd'})}));
});

test('完整缓存重核期间仍轮询，后台完成后曲线显示新结果',async()=>{
 const user=userEvent.setup();let refreshed=false;let reads=0;
 vi.mocked(invoke).mockReset().mockImplementation(async(command)=>{
  if(command==='ledger_snapshot')return{accounts:[{id:'a',name:'已建账',currency:'CNY',balance:'100',archived:false,hasHistory:true}],entries:[],positions:[]};
  if(command==='stop_performance_refresh')return;
  if(command==='refresh_performance'){refreshed=true;reads=0;}
  if(command==='get_performance'||command==='refresh_performance'){const completed=refreshed&&command==='get_performance'&&++reads>=1;return{startOn:'2026-10-19',endOn:'2026-10-20',currency:'CNY',baselineOn:'2026-10-19',totalReturn:completed?'1.234567':'0.000000',complete:true,method:'日频核对',points:[],warnings:[],refresh:{pending:0,retryAt:null,error:null,active:refreshed&&!completed}};}
  throw new Error(String(command));
 });
 render(<App/>);await user.click(await screen.findByRole('button',{name:'投资表现曲线'}));await screen.findByText('0.000000%');await user.click(screen.getByRole('button',{name:'更新曲线历史数据'}));expect(await screen.findByText('1.234567%',{}, {timeout:7000})).toBeVisible();
},12000);
