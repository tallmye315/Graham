"""Build a hash-locked portable Windows runtime without pip or system installation."""
from pathlib import Path
import argparse,hashlib,json,shutil,tarfile,tempfile,urllib.request,zipfile
ROOT=Path(__file__).resolve().parent.parent

def checked_download(item,cache):
 path=cache/item['filename']
 if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest()!=item['sha256']:
  with urllib.request.urlopen(item['url'],timeout=90) as response: data=response.read()
  if hashlib.sha256(data).hexdigest()!=item['sha256']:raise RuntimeError('Download checksum mismatch: '+item['filename'])
  path.write_bytes(data)
 return path

def unpack_zip(path,target):
 with zipfile.ZipFile(path) as archive:
  for entry in archive.infolist():
   parts=Path(entry.filename).parts
   if '..' in parts or Path(entry.filename).is_absolute():raise RuntimeError('Unsafe archive path')
   relative=Path(entry.filename)
   # Wheel purelib/platlib data are installed at site-packages, never executable scripts.
   if parts and parts[0].endswith('.data'):
    if len(parts)<3 or parts[1] not in ('purelib','platlib'):continue
    relative=Path(*parts[2:])
   output=(target/relative).resolve()
   if not output.is_relative_to(target.resolve()):raise RuntimeError('Archive path escaped target')
   if entry.is_dir():output.mkdir(parents=True,exist_ok=True)
   else:output.parent.mkdir(parents=True,exist_ok=True);output.write_bytes(archive.read(entry))

def main():
 parser=argparse.ArgumentParser();parser.add_argument('--cache',type=Path,default=ROOT/'artifacts/market-packages');args=parser.parse_args();args.cache.mkdir(parents=True,exist_ok=True)
 lock=json.loads((ROOT/'market/runtime-lock.json').read_text(encoding='utf-8'));artifacts=(ROOT/'artifacts').resolve();artifacts.mkdir(exist_ok=True);output=(artifacts/'market-runtime').resolve()
 if not output.is_relative_to(artifacts) or output!=artifacts/'market-runtime':raise RuntimeError('Invalid build target')
 with tempfile.TemporaryDirectory(prefix='market-build-',dir=artifacts) as temp:
  stage=Path(temp)/'runtime';stage.mkdir();unpack_zip(checked_download(lock['python'],args.cache),stage);site=stage/'Lib/site-packages';site.mkdir(parents=True)
  for item in lock['packages']:
   source=checked_download(item,args.cache)
   if item['kind']=='wheel':unpack_zip(source,site)
   elif item['kind']=='jsonpath-source':
    with tarfile.open(source) as archive:
     for entry in archive.getmembers():
      pieces=Path(entry.name).parts
      if not entry.isfile():continue
      if ".." in pieces or Path(entry.name).is_absolute():raise RuntimeError("Unsafe source archive path")
      if len(pieces)==2 and pieces[1]=='jsonpath.py':target=site/'jsonpath.py'
      elif len(pieces)>=3 and pieces[1]=='jsonpath.egg-info':target=site/Path(*pieces[1:])
      elif len(pieces)==2 and pieces[1] in ('README','PKG-INFO','CHANGELOG'):target=stage/'licenses/jsonpath'/pieces[1]
      else:continue
      if not target.resolve().is_relative_to(stage.resolve()):raise RuntimeError("Source archive escaped build stage")
      target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(archive.extractfile(entry).read())
   else:raise RuntimeError('Unknown package format')
  (stage/'python313._pth').write_text('python313.zip\n.\nLib/site-packages\nimport site\n',encoding='utf-8')
  shutil.copyfile(ROOT/'market/runtime-lock.json',stage/'runtime-lock.json')
  if output.exists():shutil.rmtree(output)
  shutil.move(str(stage),str(output))
 print('Portable runtime ready:',output)
if __name__=='__main__':main()
