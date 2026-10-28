"""One bounded JSON request per isolated process; only public security identifiers enter here."""
import contextlib,json,os,sys,threading,tempfile,re
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime,timezone,timedelta
from decimal import Decimal,InvalidOperation,ROUND_HALF_UP
# A child never outlives a hung upstream indefinitely, including after the UI closes.
_feed=""
_deadline=threading.Timer(85,lambda:os._exit(124));_deadline.daemon=True;_deadline.start()

def load_provider():
 import requests
 request=requests.sessions.Session.request
 def bounded(self,method,url,**kwargs):
  kwargs['timeout']=(5,12)
  return request(self,method,url,**kwargs)
 requests.sessions.Session.request=bounded
 with contextlib.redirect_stdout(sys.stderr):
  import akshare
 return akshare

def positive(value):
 try:price=Decimal(str(value))
 except InvalidOperation:return None
 if not price.is_finite() or price<=0:return None
 return format(price.quantize(Decimal('0.000001'),rounding=ROUND_HALF_UP),'f').rstrip('0').rstrip('.')

def us_quotes(keys):
 import yfinance as yf
 yf.config.debug.hide_exceptions=False
 from zoneinfo import ZoneInfo
 if len(keys)>10:raise ValueError('US batch limit')
 for key in keys:
  if key['market']!='US' or key['kind'] not in ('stock','etf') or not re.fullmatch(r'[A-Z0-9][A-Z0-9.\-]{0,19}',key['code']):raise ValueError('Invalid US identity')
 def fetch(key):
  try:
   symbol=key['code'].replace('.','-');ticker=yf.Ticker(symbol)
   table=ticker.history(period='1mo',interval='1d',auto_adjust=False,back_adjust=False,actions=False,prepost=False,repair=False,timeout=12)
   meta=ticker.history_metadata
   expected='ETF' if key['kind']=='etf' else 'EQUITY'
   if meta.get('symbol')!=symbol or meta.get('currency')!='USD' or meta.get('instrumentType')!=expected or meta.get('exchangeTimezoneName')!='America/New_York':raise ValueError('Source security identity mismatch')
   price=positive(meta.get('regularMarketPrice'));epoch=meta.get('regularMarketTime')
   if not price:raise ValueError('Missing regular quote')
   if isinstance(epoch,datetime) and epoch.tzinfo is not None:time=epoch.astimezone(ZoneInfo('America/New_York')).isoformat()
   elif isinstance(epoch,(int,float)) and not isinstance(epoch,bool):time=datetime.fromtimestamp(epoch,ZoneInfo('America/New_York')).isoformat()
   else:raise ValueError('Missing source timestamp')
   days=[]
   for index,row in table.sort_index().iterrows():
    close=positive(row['Close'])
    if not close:raise ValueError('Invalid daily close')
    days.append({'date':index.tz_convert('America/New_York').strftime('%Y-%m-%d'),'close':close})
   if not days or len(days)>32:raise ValueError('Missing daily history')
   return {'key':key,'price':price,'currency':'USD','sourceTime':time,'quotedAt':time,'history':days},None
  except Exception as error:return None,type(error).__name__
 # Provider cookies/timezone metadata have their own disposable cache, outside the ledger.
 cache=os.path.join(os.environ.get('LOCALAPPDATA',tempfile.gettempdir()),'com.graham.portfolio','market-provider-cache')
 yf.set_tz_cache_location(cache)
 with contextlib.redirect_stdout(sys.stderr),ThreadPoolExecutor(max_workers=4) as pool:results=list(pool.map(fetch,keys))
 quotes=[q for q,error in results if q is not None]
 error=None if quotes else 'yfinance 请求失败（'+','.join(sorted({error for q,error in results if error}))+'）；保留最后有效报价'
 return {'protocol':1,'feed':'us','quotes':quotes,'error':error}

def valuation_points(request,ak):
 class UnverifiedHistory(ValueError):pass
 from zoneinfo import ZoneInfo
 import yfinance as yf
 yf.config.debug.hide_exceptions=False
 yf.set_tz_cache_location(os.path.join(os.environ.get('LOCALAPPDATA',tempfile.gettempdir()),'com.graham.portfolio','market-provider-cache'))
 date=request.get('date');keys=request['keys']
 if date is not None:
  if not isinstance(date,str) or not re.fullmatch(r'\d{4}-\d{2}-\d{2}',date):raise ValueError('Invalid date')
  day=datetime.strptime(date,'%Y-%m-%d').date()
  if day>datetime.now(ZoneInfo('Asia/Shanghai')).date():raise ValueError('Future valuation')
 if not isinstance(keys,list) or not 1<=len(keys)<=10:raise ValueError('Invalid valuation batch')
 for key in keys:
  market,code,kind=key['market'],key['code'],key['kind']
  if kind=='fx':
   if market!='USD' or code not in ('CNY','HKD'):raise ValueError('Invalid FX identity')
  elif market not in ('CN','HK','US') or kind not in ('stock','etf') or date is None:raise ValueError('Invalid historical identity')
  elif market in ('CN','HK') and (not code.isascii() or not code.isdigit() or len(code)!=(6 if market=='CN' else 5)):raise ValueError('Invalid code')
  elif market=='US' and not re.fullmatch(r'[A-Z0-9][A-Z0-9.\-]{0,19}',code):raise ValueError('Invalid code')
 def price(value,digits):
  value=Decimal(str(value))
  if not value.is_finite() or value<=0:raise ValueError('Invalid source price')
  value=value.quantize(Decimal(1).scaleb(-digits),rounding=ROUND_HALF_UP)
  if value<=0:raise ValueError('Source price rounds to zero')
  return format(value,'f').rstrip('0').rstrip('.') if '.' in format(value,'f') else format(value,'f')
 def fetch(key):
  try:
   market,code,kind=key['market'],key['code'],key['kind'];currency={'CN':'CNY','HK':'HKD','US':'USD'}.get(market,code);basis='unadjusted';source='AKShare / 新浪';quoted=None
   if kind=='fx' or market=='US':
    symbol=code+'=X' if kind=='fx' else code.replace('.','-');ticker=yf.Ticker(symbol)
    if date is None:table=ticker.history(period='5d',interval='1d',auto_adjust=False,back_adjust=False,repair=False,actions=False,prepost=False,timeout=12)
    else:
     end=(datetime.now(ZoneInfo('America/New_York')).date()+timedelta(days=1)).isoformat() if market=='US' else (day+timedelta(days=1)).isoformat()
     table=ticker.history(start=date,end=end,interval='1d',auto_adjust=False,back_adjust=False,repair=False,actions=True,prepost=False,timeout=12)
    meta=ticker.history_metadata;expected='CURRENCY' if kind=='fx' else 'ETF' if kind=='etf' else 'EQUITY';zone='Europe/London' if kind=='fx' else 'America/New_York'
    if meta.get('symbol')!=symbol or meta.get('currency')!=currency or meta.get('instrumentType')!=expected or meta.get('exchangeTimezoneName')!=zone:raise ValueError('Source identity mismatch')
    source='yfinance / Yahoo Finance'
    if date is None:
     value=price(meta.get('regularMarketPrice'),8);time=meta.get('regularMarketTime')
     if isinstance(time,datetime) and time.tzinfo is not None:quoted=time.isoformat()
     elif isinstance(time,(int,float)) and not isinstance(time,bool):quoted=datetime.fromtimestamp(time,ZoneInfo(zone)).isoformat()
     else:raise ValueError('Missing FX source timestamp')
     basis='reference_quote';source_time=quoted
    else:
     if market=='US':
      if 'Stock Splits' not in table.columns:raise UnverifiedHistory('Missing split verification')
      later=table[table.index.strftime('%Y-%m-%d')>date]['Stock Splits']
      if not later.notna().all() or (later!=0).any():raise UnverifiedHistory('Historical split-adjusted price needs verification')
     rows=table[table.index.strftime('%Y-%m-%d')==date]
     if len(rows)!=1:raise ValueError('Exact date price unavailable')
     value=price(rows.iloc[0]['Close'],8 if kind=='fx' else 6);basis='daily_close' if kind=='fx' else 'split_checked';source_time=date+' ['+zone+']'
   else:
    zone='Asia/Shanghai' if market=='CN' else 'Asia/Hong_Kong'
    if market=='HK':table=ak.stock_hk_daily(symbol=code,adjust='')
    else:
     prefix='sh' if code[0] in ('5','6','9') and not code.startswith('92') else 'bj' if code[0] in ('4','8') or code.startswith('92') else 'sz'
     if kind=='etf':table=ak.fund_etf_hist_sina(symbol=prefix+code)
     else:table=ak.stock_zh_a_daily(symbol=prefix+code,start_date=date.replace('-',''),end_date=date.replace('-',''),adjust='')
    rows=table[table['date'].astype(str)==date]
    if len(rows)!=1:raise ValueError('Exact date price unavailable')
    value=price(rows.iloc[0]['close'],6);source_time=date+' ['+zone+']'
   return {'key':key,'date':date,'value':value,'currency':currency,'source':source,'basis':basis,'sourceTime':source_time,'quotedAt':quoted},None
  except Exception as error:return None,{'key':key,'error':'历史拆股口径校验失效，停用旧价格缓存' if isinstance(error,UnverifiedHistory) else '来源无有效数据（'+type(error).__name__+'）；请核对日期或网络','invalidates':isinstance(error,UnverifiedHistory)}
 # Sina's embedded V8 decoder must stay on the main thread; parallel native initialization can crash the process.
 with contextlib.redirect_stdout(sys.stderr):
  results=[fetch(key) for key in keys if key['market'] in ('CN','HK')]
  with ThreadPoolExecutor(max_workers=4) as pool:results+=list(pool.map(fetch,[key for key in keys if key['market'] not in ('CN','HK')]))
 return {'protocol':1,'feed':'valuation','points':[point for point,error in results if point is not None],'errors':[error for point,error in results if error is not None],'error':None}

def main():
 global _feed
 if sys.argv[1:]==['--health']:
  ak=load_provider()
  import importlib.metadata
  with contextlib.redirect_stdout(sys.stderr):import yfinance,exchange_calendars
  print(json.dumps({'protocol':1,'python':sys.version.split()[0],'akshare':importlib.metadata.version('akshare'),'yfinance':importlib.metadata.version('yfinance'),'exchangeCalendars':importlib.metadata.version('exchange_calendars'),'isolated':bool(sys.flags.isolated),'packages':len(json.load(open(os.path.join(os.path.dirname(sys.executable),'runtime-lock.json'),encoding='utf-8'))['packages'])}));return
 request=json.loads(sys.stdin.buffer.read(65537))
 if request.get('protocol')!=1:raise ValueError('Unsupported market protocol')
 feed=request['feed'];_feed=feed
 ak=load_provider();keys=request['keys']
 if not isinstance(keys,list) or not 1<=len(keys)<=300:raise ValueError('Invalid key count')
 if feed=='performance':
  import importlib.util
  spec=importlib.util.spec_from_file_location('graham_history',os.path.join(os.path.dirname(__file__),'history.py'));module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
  print(json.dumps(module.fetch(request,ak),ensure_ascii=False,allow_nan=False));return
 if feed=='valuation':
  print(json.dumps(valuation_points(request,ak),ensure_ascii=False,allow_nan=False));return
 if feed=='us':
  print(json.dumps(us_quotes(keys),ensure_ascii=False,allow_nan=False));return
 functions={'cn-stock':ak.stock_zh_a_spot,'hk':ak.stock_hk_spot,'cn-etf':lambda:ak.fund_etf_category_sina(symbol='ETF基金')}
 if feed not in functions:raise ValueError('Unknown feed')
 with contextlib.redirect_stdout(sys.stderr):table=functions[feed]()
 quotes=[]
 for key in keys:
  market,code,kind=key['market'],key['code'],key['kind']
  if (feed=='hk' and (market!='HK' or len(code)!=5)) or (feed!='hk' and (market!='CN' or len(code)!=6)) or not code.isascii() or not code.isdigit() or kind not in ('stock','etf'):raise ValueError('Invalid security identity')
  if (feed=='cn-stock' and kind!='stock') or (feed=='cn-etf' and kind!='etf'):raise ValueError('Wrong feed for security kind')
  codes=table['代码'].astype(str).str.replace(r'^(sh|sz|bj)','',regex=True).str.zfill(5 if market=='HK' else 6)
  rows=table.loc[codes==code]
  if len(rows)!=1:continue
  row=rows.iloc[0]
  try:price=Decimal(str(row['最新价']))
  except InvalidOperation:continue
  if not price.is_finite() or price<=0:continue
  source_time=None;quoted_at=None
  if feed=='hk':
   value=str(row.get('日期时间','')).strip()
   try:quoted_at=datetime.strptime(value,'%Y/%m/%d %H:%M:%S').replace(tzinfo=timezone(timedelta(hours=8))).isoformat();source_time=value
   except ValueError:pass
  elif feed=='cn-stock':
   value=str(row.get('时间戳','')).strip()
   try:datetime.strptime(value,'%H:%M:%S');source_time=value
   except ValueError:pass
  quotes.append({'key':key,'price':format(price,'f'),'currency':'HKD' if market=='HK' else 'CNY','sourceTime':source_time,'quotedAt':quoted_at})
 print(json.dumps({'protocol':1,'feed':feed,'quotes':quotes,'error':None},ensure_ascii=False,allow_nan=False))
if __name__=='__main__':
 try:main()
 except Exception as error:
  print(json.dumps({'protocol':1,'feed':_feed,'quotes':[],'error':'行情源请求失败（'+type(error).__name__+'）'},ensure_ascii=False));sys.exit(1)
 finally:_deadline.cancel()
