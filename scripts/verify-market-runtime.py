"""Verify the exact shipped Python with no system Python path or working directory."""
from pathlib import Path
import argparse,json,os,subprocess,tempfile
ROOT=Path(__file__).resolve().parent.parent
p=argparse.ArgumentParser();p.add_argument('--installed-dir',type=Path);args=p.parse_args()
base=args.installed_dir.resolve() if args.installed_dir else ROOT
runtime=base/'market/runtime' if args.installed_dir else ROOT/'artifacts/market-runtime'
worker=base/'market/worker.py';exe=runtime/'python.exe';lock=json.loads((runtime/'runtime-lock.json').read_text(encoding='utf-8'))
assert exe.is_file() and worker.is_file() and (worker.parent/'history.py').is_file()
with tempfile.TemporaryDirectory(prefix='graham-offline-runtime-') as cwd:
 env=os.environ.copy();env['PATH']='';env['PYTHONHOME']=str(Path(cwd)/'no-system-python');env['PYTHONPATH']=str(Path(cwd)/'no-system-packages')
 r=subprocess.run([str(exe),'-I','-X','utf8',str(worker),'--health'],env=env,cwd=cwd,capture_output=True,text=True,encoding='utf-8',timeout=45,check=True)
 health=json.loads(r.stdout);assert health['protocol']==1 and health['isolated'] and health['python']==lock['python']['version'];assert health['akshare']==next(p['version'] for p in lock['packages'] if p['name'].lower()=='akshare')
 assert health['yfinance']==next(p['version'] for p in lock['packages'] if p['name'].lower()=='yfinance')
 assert health['exchangeCalendars']==next(p['version'] for p in lock['packages'] if p['name'].lower().replace('_','-')=='exchange-calendars')
 r=subprocess.run([str(exe),'-I','-X','utf8',str(worker)],input='{"protocol":0}',env=env,cwd=cwd,capture_output=True,text=True,encoding='utf-8',timeout=45)
 assert r.returncode!=0 and json.loads(r.stdout)['error']
# Exercise the shipped JSON boundary with a malformed upstream row, without HTTP.
fixture=r"""
import io,json,runpy,sys
from types import SimpleNamespace
from unittest.mock import patch
import pandas as pd
worker=sys.argv[1];sys.argv=[worker]
table=pd.DataFrame({'Close':[100.0,0.0,102.0]},index=pd.date_range('2026-10-21',periods=3,tz='America/New_York'))
ticker=SimpleNamespace(history=lambda **kwargs:table,history_metadata={'symbol':'AAPL','currency':'USD','instrumentType':'EQUITY','exchangeTimezoneName':'America/New_York','regularMarketPrice':102,'regularMarketTime':pd.Timestamp('2026-10-23T16:00:00-04:00')})
sys.stdin=io.TextIOWrapper(io.BytesIO(json.dumps({'protocol':1,'feed':'us','keys':[{'market':'US','code':'AAPL','kind':'stock'}]}).encode()))
with patch('yfinance.Ticker',return_value=ticker):runpy.run_path(worker,run_name='__main__')
"""
r=subprocess.run([str(exe),'-I','-X','utf8','-c',fixture,str(worker)],capture_output=True,text=True,encoding='utf-8',timeout=45)
response=json.loads(r.stdout);assert response['feed']=='us' and response['quotes']==[] and response['error'],'Invalid daily row must fail the security, never silently replace cached history'
# Sina's V8 decoder must be invoked from the main thread, even for multiple holdings.
valuation_fixture=r"""
import io,json,runpy,sys,threading
from unittest.mock import patch
import pandas as pd
worker=sys.argv[1];sys.argv=[worker]
def daily(**kwargs):
 if threading.current_thread() is not threading.main_thread():raise AssertionError('Sina decoder moved to a worker thread')
 return pd.DataFrame({'date':['2026-10-23'],'close':[41.77]})
sys.stdin=io.TextIOWrapper(io.BytesIO(json.dumps({'protocol':1,'feed':'valuation','date':'2026-10-23','keys':[{'market':'CN','code':'600036','kind':'stock'},{'market':'HK','code':'00700','kind':'stock'}]}).encode()))
with patch('akshare.stock_zh_a_daily',daily),patch('akshare.stock_hk_daily',daily):runpy.run_path(worker,run_name='__main__')
"""
r=subprocess.run([str(exe),'-I','-X','utf8','-c',valuation_fixture,str(worker)],capture_output=True,text=True,encoding='utf-8',timeout=45)
response=json.loads(r.stdout);assert response['feed']=='valuation' and len(response['points'])==2 and response['errors']==[] and response['error'] is None,'Historical provider protocol or main-thread isolation failed'
# Never multiply a pre-split holding by Yahoo's subsequently split-adjusted Close.
split_fixture=fixture.replace("{'Close':[100.0,0.0,102.0]}","{'Close':[100.0,25.0,26.0],'Stock Splits':[0.0,4.0,0.0]}").replace("'feed':'us'","'feed':'valuation','date':'2026-10-21'")
r=subprocess.run([str(exe),'-I','-X','utf8','-c',split_fixture,str(worker)],capture_output=True,text=True,encoding='utf-8',timeout=45)
response=json.loads(r.stdout);assert response['feed']=='valuation' and response['points']==[] and len(response['errors'])==1 and response['errors'][0]['invalidates'] is True,'Later split must make the unverified historical price unavailable'
print(json.dumps({'portableRuntime':str(runtime),'health':health,'independentOfPathAndCwd':True}))

# Real bundled exchange calendar, mocked external quotes, full shipped JSON boundary.
history_fixture=valuation_fixture.replace("'feed':'valuation','date':'2026-10-23'", "'feed':'performance','startOn':'2026-10-23','endOn':'2026-10-25'")
r=subprocess.run([str(exe),'-I','-X','utf8','-c',history_fixture,str(worker)],capture_output=True,text=True,encoding='utf-8',timeout=45)
response=json.loads(r.stdout);assert r.returncode==0 and response['feed']=='performance' and len(response['series'])==2 and response['error'] is None
assert all(s['prices']==[{'date':'2026-10-23','close':'41.77'}] and s['basis']=='unadjusted' and s['error'] is None for s in response['series'])
assert all(c['sessions'][-1]=={'date':'2026-10-25','session':'2026-10-23','reason':None} for c in response['calendars']),'Weekend requires an explicitly verified prior exchange session'
print('historicalProtocolAndExchangeCalendar: passed')
