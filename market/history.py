"""Bounded historical batches; no account IDs, quantities or transactions enter here."""
import contextlib,os,re,sys,tempfile
from concurrent.futures import ThreadPoolExecutor
from datetime import date,datetime,timedelta,timezone
from decimal import Decimal,ROUND_HALF_UP
from zoneinfo import ZoneInfo

def fetch(request,ak):
 import pandas as pd
 import exchange_calendars as calendars
 import yfinance as yf
 start=date.fromisoformat(request['startOn']);end=date.fromisoformat(request['endOn']);keys=request['keys']
 if start>end or (end-start).days>3660 or end>datetime.now(ZoneInfo('Asia/Shanghai')).date() or not 1<=len(keys)<=10:raise ValueError('Invalid history window')
 for key in keys:
  m,c,k=key['market'],key['code'],key['kind']
  if k=='fx':
   if m!='USD' or c not in ('CNY','HKD'):raise ValueError('Invalid FX key')
  elif m not in ('CN','HK','US') or k not in ('stock','etf'):raise ValueError('Invalid market')
  elif m in ('CN','HK') and (len(c)!=(6 if m=='CN' else 5) or not c.isascii() or not c.isdigit()):raise ValueError('Invalid security code')
  elif m=='US' and not re.fullmatch(r'[A-Z0-9][A-Z0-9.\-]{0,19}',c):raise ValueError('Invalid US code')
 days=[start+timedelta(days=i) for i in range((end-start).days+1)]
 schedule={};out=[]
 for market in sorted({'FX' if k['kind']=='fx' else k['market'] for k in keys}):
  name={'CN':'XSHG','HK':'XHKG','US':'XNYS','FX':'FX_WEEKDAYS'}[market];mapping={};failure=None
  try:
   if market=='FX':
    for day in days:
     session=day
     while session.weekday()>4:session-=timedelta(days=1)
     mapping[day.isoformat()]=session.isoformat()
   else:
    cal=calendars.get_calendar(name,start=(start-timedelta(days=35)).isoformat(),end=end.isoformat());sessions=cal.sessions
    for day in days:
     position=sessions.searchsorted(pd.Timestamp(day),side='right')-1
     if position<0:raise ValueError('Calendar has no prior session')
     mapping[day.isoformat()]=sessions[position].date().isoformat()
  except Exception as error:failure='包内交易日历无法核对该区间（'+type(error).__name__+'）'
  schedule[market]=mapping
  out.append({'market':market,'name':name,'version':'1' if market=='FX' else calendars.__version__,'sessions':[{'date':d.isoformat(),'session':mapping.get(d.isoformat()),'reason':failure} for d in days]})
 yf.config.debug.hide_exceptions=False;yf.set_tz_cache_location(os.path.join(os.environ.get('LOCALAPPDATA',tempfile.gettempdir()),'com.graham.portfolio','market-provider-cache'))
 def closed(day,market):
  now=datetime.now(timezone.utc)
  if market=='FX':return day<now.date()
  local=now.astimezone(ZoneInfo({'CN':'Asia/Shanghai','HK':'Asia/Hong_Kong','US':'America/New_York'}[market]));cutoff=915 if market=='CN' else 990
  return day<local.date() or day==local.date() and local.hour*60+local.minute>=cutoff
 def series(key):
  market='FX' if key['kind']=='fx' else key['market'];code=key['code'];source='AKShare / 新浪' if market in ('CN','HK') else 'yfinance / Yahoo Finance';basis={'CN':'unadjusted','HK':'unadjusted','US':'split_checked','FX':'daily_close'}[market];zone={'CN':'Asia/Shanghai','HK':'Asia/Hong_Kong','US':'America/New_York','FX':'Europe/London'}[market]
  result={'key':key,'source':source,'basis':basis,'timezone':zone,'prices':[],'invalidatesBefore':None,'error':None}
  try:
   needed={d for d in schedule[market].values() if closed(date.fromisoformat(d),market)}
   if not needed:return result
   first=min(needed)
   if market in ('US','FX'):
    symbol=code+'=X' if market=='FX' else code.replace('.','-');ticker=yf.Ticker(symbol);finish=(datetime.now(ZoneInfo('America/New_York')).date()+timedelta(days=1)).isoformat() if market=='US' else (end+timedelta(days=1)).isoformat()
    table=ticker.history(start=first,end=finish,interval='1d',auto_adjust=False,back_adjust=False,repair=False,prepost=False,actions=True,timeout=12);meta=ticker.history_metadata;currency={'US':'USD','FX':code}[market];expected='CURRENCY' if market=='FX' else 'ETF' if key['kind']=='etf' else 'EQUITY'
    if meta.get('symbol')!=symbol or meta.get('currency')!=currency or meta.get('instrumentType')!=expected or meta.get('exchangeTimezoneName')!=zone:raise ValueError('History identity mismatch')
    if market=='US':
     if 'Stock Splits' not in table.columns or not table['Stock Splits'].notna().all():
      result['invalidatesBefore']=(end+timedelta(days=1)).isoformat();raise ValueError('Missing split verification')
     splits=table[table['Stock Splits']!=0]
     if len(splits):
      cutoff=max(splits.index.strftime('%Y-%m-%d'));result['invalidatesBefore']=cutoff;needed={d for d in needed if d>=cutoff}
    values=[(index.strftime('%Y-%m-%d'),row['Close']) for index,row in table.iterrows() if index.strftime('%Y-%m-%d') in needed]
   else:
    if market=='HK':table=ak.stock_hk_daily(symbol=code,adjust='')
    else:
     prefix='sh' if code[0] in ('5','6','9') and not code.startswith('92') else 'bj' if code[0] in ('4','8') or code.startswith('92') else 'sz'
     table=ak.fund_etf_hist_sina(symbol=prefix+code) if key['kind']=='etf' else ak.stock_zh_a_daily(symbol=prefix+code,start_date=first.replace('-',''),end_date=end.isoformat().replace('-',''),adjust='')
    values=[(str(row['date']),row['close']) for _,row in table.iterrows() if str(row['date']) in needed]
   seen=set();prices=[]
   for day,value in sorted(values):
    value=Decimal(str(value))
    if day in seen or not value.is_finite() or value<=0:raise ValueError('Invalid or duplicated daily close')
    seen.add(day);value=value.quantize(Decimal('0.00000001' if market=='FX' else '0.000001'),rounding=ROUND_HALF_UP)
    if value<=0:raise ValueError('Price rounds to zero')
    prices.append({'date':day,'close':format(value,'f').rstrip('0').rstrip('.')})
   result['prices']=prices
   if len(seen)<len(needed):result['error']='部分交易日缺少已核对收盘价，不使用其他日期补齐'
  except Exception as error:result['prices']=[];result['error']='历史来源失败（'+type(error).__name__+'），保留仍有效缓存'
  return result
 with contextlib.redirect_stdout(sys.stderr):
  results=[series(k) for k in keys if k['market'] in ('CN','HK')]
  with ThreadPoolExecutor(max_workers=4) as pool:results+=list(pool.map(series,[k for k in keys if k['market'] not in ('CN','HK')]))
 return {'protocol':1,'feed':'performance','series':results,'calendars':out,'error':None}
