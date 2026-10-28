use graham_ledger::market_refresh::MarketProcess;
use std::{io::{Read,Write},path::PathBuf,process::{Command,Stdio},time::{Duration,Instant}};

pub struct BundledMarketProcess { pub resource: PathBuf }
impl MarketProcess for BundledMarketProcess {
 fn request(&self, request:&serde_json::Value, limit:usize)->Result<Vec<u8>,String>{
  let resource = &self.resource;

 let runtime=resource.join("market/runtime/python.exe");let script=resource.join("market/worker.py");if !runtime.is_file()||!script.is_file(){return Err("软件行情组件缺失，请重新安装最新版".into());}
 let mut command=Command::new(&runtime);command.args(["-I","-X","utf8"]).arg(&script).current_dir(runtime.parent().ok_or("行情组件路径无效")?).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
 #[cfg(target_os="windows")]{use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
 let mut child=command.spawn().map_err(|_|"内置行情进程无法启动，请重新安装")?;
 let bytes=serde_json::to_vec(request).map_err(|_|"无法生成行情请求")?;
 if child.stdin.take().ok_or("行情进程输入不可用")?.write_all(&bytes).is_err(){let _=child.kill();let _=child.wait();return Err("行情进程输入失败".into());}
 let stdout=child.stdout.take().ok_or("行情进程输出不可用")?;let stderr=child.stderr.take().ok_or("行情进程日志不可用")?;
 let output=std::thread::spawn(move||{let mut bytes=Vec::new();stdout.take(limit as u64+1).read_to_end(&mut bytes).map(|_|bytes)});
 let errors=std::thread::spawn(move||{let mut sink=std::io::sink();std::io::copy(&mut stderr.take(2*1024*1024),&mut sink)});
 let start=Instant::now();let mut timed_out=false;
 loop{match child.try_wait(){Ok(Some(_))=>break,Ok(None)if start.elapsed()<Duration::from_secs(90)=>std::thread::sleep(Duration::from_millis(100)),_=>{timed_out=true;let _=child.kill();let _=child.wait();break;}}}
 let bytes=output.join().map_err(|_|"行情进程输出中断")?.map_err(|_|"行情进程读取失败")?;let _=errors.join();if timed_out{return Err("行情请求超时，保留最后有效报价".into());}if bytes.len()>limit{return Err("行情响应超出支持范围".into());}
 Ok(bytes)

 }
}
