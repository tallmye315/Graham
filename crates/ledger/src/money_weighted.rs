use super::{PerformanceRefresh,trades};
use chrono::NaiveDate;
use std::collections::BTreeMap;
use serde::Serialize;
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct MoneyWeightedFlow{pub date:String,pub kind:String,pub amount:Option<String>,pub reasons:Vec<String>}
#[derive(Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct MoneyWeightedState{pub start_on:String,pub end_on:String,pub baseline_on:Option<String>,pub currency:String,pub elapsed_days:u32,pub annualized_return:Option<String>,pub period_return:Option<String>,pub status:String,pub reason:Option<String>,pub flows:Vec<MoneyWeightedFlow>,pub warnings:Vec<String>,pub refresh:PerformanceRefresh}

// XIRR root is evaluated in log-period growth, avoiding a hard-coded rate grid.
pub(super) fn calculate(mut state:MoneyWeightedState)->Result<MoneyWeightedState,String>{
 if state.flows.iter().any(|f|f.amount.is_none()){state.reason=Some("起点、期末估值或外部资金流汇率不足，请核对资金流明细".into());return Ok(state);}
 if state.elapsed_days==0{state.status="insufficient_period".into();state.reason=Some("只有收盘基准，至少需要两个不同日期的可核对资产".into());return Ok(state);}
 if state.flows.first().and_then(|f|f.amount.as_ref()).is_none_or(|v|!v.starts_with('-')||v=="-0.00"){state.status="no_initial_capital".into();state.reason=Some("起点无有效资产，请选择已建立本金的收盘起点".into());return Ok(state);}
 let base=NaiveDate::parse_from_str(&state.flows[0].date,"%Y-%m-%d").map_err(|_|"资金流日期无效")?;let mut grouped:BTreeMap<i64,i128>=BTreeMap::new();
 for flow in &state.flows{let date=NaiveDate::parse_from_str(&flow.date,"%Y-%m-%d").map_err(|_|"资金流日期无效")?;let text=flow.amount.as_deref().ok_or("缺少资金流")?;let (negative,text)=text.strip_prefix('-').map(|v|(true,v)).unwrap_or((false,text));let amount=trades::scaled(text,2,true)? as i128;*grouped.entry((date-base).num_days()).or_default()+=if negative{-amount}else{amount};}
 let terms=grouped.into_iter().filter(|(_,v)|*v!=0).map(|(d,v)|(d as f64/state.elapsed_days as f64,v as f64)).collect::<Vec<_>>();
 if terms.len()<2||!terms.iter().any(|t|t.1>0.0)||!terms.iter().any(|t|t.1<0.0){state.status="no_solution".into();state.reason=Some("资金流没有跨日期的正负金额，无法计算有限年化回报".into());return Ok(state);}
 let mut budget=2_000_000;
 let roots=match isolate_roots(&terms,0,&mut budget){Ok(roots)=>roots,Err(reason)=>{state.status="uncertain".into();state.reason=Some(reason.into());return Ok(state);}};
 if roots.len()!=1{state.status=if roots.is_empty(){"no_solution"}else{"multiple_roots"}.into();state.reason=Some(if roots.is_empty(){"这些日期和金额没有有限年化解".into()}else{format!("存在 {} 个年化解，无法给出唯一资金回报",roots.len())});return Ok(state);}
 let root=roots[0];let annual=(root*365.0/state.elapsed_days as f64).exp_m1();let period=root.exp_m1();
 if !annual.is_finite()||!period.is_finite()||annual<=-1.0||period<=-1.0||annual.abs()>1e10||period.abs()>1e10{state.status="out_of_range".into();state.reason=Some("年化或区间回报超出可靠展示范围".into());return Ok(state);}
 let text=|value:f64|{let value=value*100.0;format!("{:.6}",if value.abs()<0.0000005{0.0}else{value})};state.annualized_return=Some(text(annual));state.period_return=Some(text(period));state.status="ok".into();state.reason=None;Ok(state)
}

// Removing the first exponential leaves a constant term. The derivative then has
// one fewer term; all its roots split the original function into monotone spans.
// Sign changes give a shortcut for conventional flows (Descartes' rule).
fn isolate_roots(input:&[(f64,f64)],depth:usize,budget:&mut usize)->Result<Vec<f64>,&'static str>{
 if input.len()<2{return Ok(vec![]);}
 let offset=input[0].0;let scale=input.iter().map(|v|v.1.abs()).fold(0.0,f64::max);
 let terms=input.iter().map(|(t,c)|(t-offset,c/scale)).collect::<Vec<_>>();
 if terms.iter().any(|(_,c)|*c==0.0||!c.is_finite()){return Err("数值精度不足，无法确认唯一解");}
 let changes=terms.windows(2).filter(|w|w[0].1.is_sign_positive()!=w[1].1.is_sign_positive()).count();
 if changes==0{return Ok(vec![]);}
 let critical=if changes==1{vec![]}else{
  if depth>=128||terms.len()>128{return Err("资金流过于复杂，无法在可靠预算内确认唯一解");}
  let derivative=terms.iter().skip(1).map(|(t,c)|(*t,-t*c)).collect::<Vec<_>>();isolate_roots(&derivative,depth+1,budget)?
 };
 let mut edges=vec![(None,terms.last().ok_or("资金流不足")?.1.signum())];
 for root in critical{let(value,norm)=evaluate(&terms,root,budget)?;if value.abs()<=norm*1e-10{return Err("资金流接近重根或多个相近解，无法可靠确认唯一回报");}edges.push((Some(root),value.signum()));}
 edges.push((None,terms[0].1.signum()));let mut roots=Vec::new();
 for pair in edges.windows(2){let(lower,left_sign)=pair[0];let(upper,right_sign)=pair[1];if left_sign==right_sign{continue;}
  let mut lo=lower.unwrap_or(upper.unwrap_or(0.0)-1.0);let mut hi=upper.unwrap_or(lower.unwrap_or(0.0)+1.0);let mut step=1.0;
  while evaluate(&terms,lo,budget)?.0.signum()!=left_sign{lo-=step;step*=2.0;if lo.abs()>1_048_576.0{return Err("收益率超出可靠求解范围，无法确认唯一解");}}
  step=1.0;while evaluate(&terms,hi,budget)?.0.signum()!=right_sign{hi+=step;step*=2.0;if hi.abs()>1_048_576.0{return Err("收益率超出可靠求解范围，无法确认唯一解");}}
  for _ in 0..160{let mid=(lo+hi)/2.0;if mid==lo||mid==hi{break;}let value=evaluate(&terms,mid,budget)?.0;if value==0.0{lo=mid;hi=mid;break;}if value.signum()==left_sign{lo=mid;}else{hi=mid;}}
  roots.push((lo+hi)/2.0);
 }
 Ok(roots)
}
fn evaluate(terms:&[(f64,f64)],x:f64,budget:&mut usize)->Result<(f64,f64),&'static str>{
 *budget=budget.checked_sub(terms.len()).ok_or("数值求解达到可靠预算，无法确认唯一解")?;
 let scale=terms.iter().map(|(t,c)|c.abs().ln()-t*x).fold(f64::NEG_INFINITY,f64::max);let mut sum=0.0;let mut correction=0.0;let mut norm=0.0;
 for(t,c)in terms{let value=c.signum()*(c.abs().ln()-t*x-scale).exp();norm+=value.abs();let adjusted=value-correction;let next=sum+adjusted;correction=(next-sum)-adjusted;sum=next;}
 if !sum.is_finite()||!norm.is_finite(){return Err("数值条件异常，无法确认唯一解");}Ok((sum,norm))
}
