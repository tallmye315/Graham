use crate::{AnalysisState, HistoryResponse, Ledger, MoneyWeightedState, PerformanceQuery,
    PerformanceRefresh, PerformanceState, QuoteInput, QuoteResponse, QuoteState,
    ValuationError, ValuationPoint, ValuationQuery, ValuationResponse, ValuationState};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex, MutexGuard},
    thread::{self, JoinHandle}, time::{Duration, SystemTime, UNIX_EPOCH}};

/// The shipped Python process and controlled test processes use the same protocol.
pub trait MarketProcess: Send + Sync {
    fn request(&self, request: &Value, limit: usize) -> Result<Vec<u8>, String>;
}

pub trait RefreshClock: Send + Sync {
    fn now(&self) -> Result<i64, String>;
    fn wait(&self, duration: Duration);
}

pub struct SystemClock;
impl RefreshClock for SystemClock {
    fn now(&self) -> Result<i64, String> {
        i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)
            .map_err(|_| "系统时间无效")?.as_secs()).map_err(|_| "系统时间无效".into())
    }
    fn wait(&self, duration: Duration) { thread::sleep(duration); }
}

#[derive(Default)]
struct HistoryQueue { epoch: u64, query: Option<String>, active: bool, error: Option<String> }

pub struct MarketRefresh {
    ledger: Arc<Mutex<Ledger>>,
    process: Arc<dyn MarketProcess>,
    clock: Arc<dyn RefreshClock>,
    automatic: AtomicBool,
    history: Mutex<HistoryQueue>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QuoteWire { protocol: u8, feed: String, quotes: Vec<QuoteInput>, error: Option<String> }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ValuationWire { protocol: u8, feed: String, points: Vec<ValuationPoint>, errors: Vec<ValuationError>, error: Option<String> }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryWire { protocol: u8, feed: String, series: Vec<crate::HistorySeries>, calendars: Vec<crate::HistoryCalendar>, error: Option<String> }

impl MarketRefresh {
    pub fn new(ledger: Arc<Mutex<Ledger>>, process: Arc<dyn MarketProcess>, clock: Arc<dyn RefreshClock>) -> Self {
        Self { ledger, process, clock, automatic: AtomicBool::new(true), history: Mutex::default() }
    }
    fn book(&self) -> Result<MutexGuard<'_, Ledger>, String> {
        self.ledger.lock().map_err(|_| "账本不可用".into())
    }
    fn decode<T: DeserializeOwned>(&self, request: Value, limit: usize, error: &str) -> Result<T, String> {
        let bytes = self.process.request(&request, limit)?;
        if bytes.len() > limit { return Err("行情响应超出支持范围".into()); }
        serde_json::from_slice(&bytes).map_err(|_| error.into())
    }
    pub fn quotes(&self, probe: bool) -> Result<QuoteState, String> {
        let mut state = self.book()?.quote_state(self.clock.now()?, probe)?;
        state.automatic = self.automatic.load(Ordering::Relaxed);
        Ok(state)
    }
    pub fn set_automatic(&self, enabled: bool) -> Result<QuoteState, String> {
        self.automatic.store(enabled, Ordering::Relaxed);
        self.quotes(false)
    }
    pub fn refresh_quotes(&self, probe: bool) -> Result<QuoteState, String> { self.update_quotes(probe, false) }
    fn update_quotes(&self, probe: bool, automatic: bool) -> Result<QuoteState, String> {
        let jobs = self.book()?.begin_quote_refresh(self.clock.now()?, probe, automatic)?.jobs;
        thread::scope(|scope| {
            for job in jobs {
                scope.spawn(move || {
                    let response = (|| -> Result<QuoteResponse, String> {
                        let wire: QuoteWire = self.decode(json!({"protocol":1,"feed":job.feed,"keys":job.keys}), 1024*1024,
                            "行情进程未返回有效数据，保留最后有效报价")?;
                        if wire.protocol != 1 || wire.feed != job.feed { return Err("行情进程协议或来源不匹配".into()); }
                        Ok(QuoteResponse { feed: wire.feed, quotes: wire.quotes, error: wire.error })
                    })().unwrap_or_else(|error| QuoteResponse { feed: job.feed.clone(), quotes: vec![], error: Some(error) });
                    if let Ok(now) = self.clock.now() {
                        if let Ok(mut ledger) = self.book() { let _ = ledger.finish_quote_job(&job, &response, now); }
                    }
                });
            }
        });
        self.quotes(probe)
    }
    pub fn valuation(&self, query: &ValuationQuery) -> Result<ValuationState, String> {
        self.book()?.valuation(query, self.clock.now()?, false)
    }
    fn update_valuation(&self, query: &ValuationQuery, probe: bool, analysis: bool) -> Result<ValuationState, String> {
        let job = {
            let mut ledger = self.book()?;
            if analysis { ledger.begin_analysis_refresh(query, self.clock.now()?)? }
            else { ledger.begin_valuation_refresh(query, self.clock.now()?, probe)? }
        };
        if let Some(job) = job {
            let response = (|| -> Result<ValuationResponse, String> {
                let wire: ValuationWire = self.decode(json!({"protocol":1,"feed":"valuation","date":job.date,"keys":job.keys}),
                    1024*1024, "估值来源响应无效；保留有效缓存")?;
                if wire.protocol != 1 || wire.feed != "valuation" { return Err("估值来源协议不符".into()); }
                Ok(ValuationResponse { feed: wire.feed, points: wire.points, errors: wire.errors, error: wire.error })
            })().unwrap_or_else(|error| ValuationResponse { feed: "valuation".into(), points: vec![], errors: vec![], error: Some(error) });
            self.book()?.finish_valuation_refresh(&job, &response, self.clock.now()?)?;
        }
        self.book()?.valuation(query, self.clock.now()?, probe)
    }
    pub fn refresh_valuation(&self, query: &ValuationQuery, probe: bool) -> Result<ValuationState, String> {
        if query.date.is_none() && !probe { let _ = self.refresh_quotes(false); }
        self.update_valuation(query, probe, false)
    }
    pub fn analysis(&self, query: &ValuationQuery) -> Result<AnalysisState, String> {
        self.book()?.analysis(query, self.clock.now()?)
    }
    pub fn refresh_analysis(&self, query: &ValuationQuery) -> Result<AnalysisState, String> {
        if query.date.is_none() { let _ = self.refresh_quotes(false); }
        self.update_valuation(query, false, true)?;
        self.analysis(query)
    }
    pub fn income(&self, query: &crate::IncomeQuery) -> Result<crate::IncomeState, String> {
        self.book()?.income(query, self.clock.now()?)
    }
    fn history_response(&self, job: &crate::PerformanceJob) -> HistoryResponse {
        (|| -> Result<HistoryResponse, String> {
                let wire: HistoryWire = self.decode(json!({"protocol":1,"feed":"performance","startOn":job.start_on,
                    "endOn":job.end_on,"keys":job.keys}), 8*1024*1024, "历史行情响应无效，保留有效缓存")?;
                if wire.protocol != 1 || wire.feed != "performance" { return Err("历史行情协议不符".into()); }
                Ok(HistoryResponse { feed: wire.feed, series: wire.series, calendars: wire.calendars, error: wire.error })
        })().unwrap_or_else(|error| HistoryResponse { feed: "performance".into(), series: vec![], calendars: vec![], error: Some(error) })
    }
    fn update_income_history(&self, query: &crate::IncomeQuery) -> Result<(), String> {
        let job = self.book()?.begin_income_refresh(query, self.clock.now()?)?;
        if let Some(job) = job {
            let response = self.history_response(&job);
            self.book()?.finish_performance_refresh(&job, &response, self.clock.now()?)?;
        }
        Ok(())
    }
    pub fn refresh_income(&self, query: &crate::IncomeQuery) -> Result<crate::IncomeState, String> {
        self.refresh_quotes(false)?;
        self.update_income_history(query)?;
        self.income(query)
    }
    fn decorate(&self, query: &PerformanceQuery, money: bool, refresh: &mut PerformanceRefresh) -> Result<(), String> {
        let queue = self.history.lock().map_err(|_| "历史更新队列不可用")?;
        let key = serde_json::to_string(&(money, query)).map_err(|_| "区间无效")?;
        let matches = queue.query.as_ref() == Some(&key);
        refresh.active = matches && queue.active;
        if matches { refresh.error = queue.error.clone(); }
        Ok(())
    }
    pub fn performance(&self, query: &PerformanceQuery) -> Result<PerformanceState, String> {
        let mut state = self.book()?.performance(query, self.clock.now()?)?;
        self.decorate(query, false, &mut state.refresh)?;
        Ok(state)
    }
    pub fn money_weighted(&self, query: &PerformanceQuery) -> Result<MoneyWeightedState, String> {
        let mut state = self.book()?.money_weighted(query, self.clock.now()?)?;
        self.decorate(query, true, &mut state.refresh)?;
        Ok(state)
    }
    fn pending(&self, query: &PerformanceQuery, money: bool) -> Result<usize, String> {
        Ok(if money { self.money_weighted(query)?.refresh.pending } else { self.performance(query)?.refresh.pending })
    }
    fn run_history(&self, query: &PerformanceQuery, epoch: u64, money: bool) -> Result<(), String> {
        let mut first = true;
        loop {
            if self.history.lock().map_err(|_| "历史更新队列不可用")?.epoch != epoch { return Ok(()); }
            let remaining = self.pending(query, money)?;
            if !first && remaining == 0 { return Ok(()); }
            let job = {
                let mut ledger = self.book()?;
                let now = self.clock.now()?;
                // Admit a task atomically with stop/replacement, after any clock or book wait.
                // Once admitted it is in flight; the process still runs outside both locks.
                let queue = self.history.lock().map_err(|_| "历史更新队列不可用")?;
                if queue.epoch != epoch { return Ok(()); }
                if money { ledger.begin_money_weighted_refresh(query, now)? }
                else { ledger.begin_performance_refresh(query, now)? }
            };
            if let Some(job) = job {
                first = false;
                let response = self.history_response(&job);
                let error = response.error.clone().or_else(|| response.series.iter().find_map(|series| series.error.clone()));
                self.book()?.finish_performance_refresh(&job, &response, self.clock.now()?)?;
                let mut queue = self.history.lock().map_err(|_| "历史更新队列不可用")?;
                if queue.epoch != epoch { return Ok(()); }
                queue.error = error;
            } else if remaining == 0 { return Ok(()); }
            self.clock.wait(Duration::from_secs(5));
        }
    }
    fn start_history(self: &Arc<Self>, query: PerformanceQuery, money: bool) -> Result<JoinHandle<()>, String> {
        self.pending(&query, money)?;
        let epoch = {
            let mut queue = self.history.lock().map_err(|_| "历史更新队列不可用")?;
            queue.epoch = queue.epoch.wrapping_add(1);
            queue.query = Some(serde_json::to_string(&(money, &query)).map_err(|_| "区间无效")?);
            queue.error = None; queue.active = true; queue.epoch
        };
        let refresh = Arc::clone(self);
        Ok(thread::spawn(move || {
            let result = refresh.run_history(&query, epoch, money);
            if let Ok(mut queue) = refresh.history.lock() {
                if queue.epoch == epoch {
                    queue.active = false;
                    if let Err(error) = result { queue.error = Some(error); }
                }
            };
        }))
    }
    pub fn start_performance(self: &Arc<Self>, query: PerformanceQuery) -> Result<JoinHandle<()>, String> { self.start_history(query, false) }
    pub fn start_money_weighted(self: &Arc<Self>, query: PerformanceQuery) -> Result<JoinHandle<()>, String> { self.start_history(query, true) }
    pub fn stop_history(&self) -> Result<(), String> {
        let mut queue = self.history.lock().map_err(|_| "历史更新队列不可用")?;
        queue.epoch = queue.epoch.wrapping_add(1); queue.active = false;
        Ok(())
    }
    pub fn start_background(self: &Arc<Self>) {
        let refresh = Arc::clone(self);
        thread::spawn(move || loop {
            if refresh.automatic.load(Ordering::Relaxed) {
                let _ = refresh.update_quotes(false, true);
                let _ = refresh.update_valuation(&ValuationQuery { account_ids: vec![], currency: "CNY".into(), date: None }, false, false);
                let _ = refresh.update_income_history(&crate::IncomeQuery { account_ids: vec![] });
            }
            refresh.clock.wait(Duration::from_secs(5));
        });
    }
}
