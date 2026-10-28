use graham_ledger::{market_refresh::{MarketProcess, MarketRefresh, RefreshClock}, Ledger, OpeningInput, OpeningPosition};
use serde_json::{json, Value};
use std::{sync::{atomic::{AtomicBool, AtomicI64, Ordering}, mpsc, Arc, Mutex}, time::Duration};

struct FixedClock(AtomicI64);
impl Default for FixedClock {
    fn default() -> Self { Self(AtomicI64::new(chrono::DateTime::parse_from_rfc3339("2026-10-26T13:40:00Z").unwrap().timestamp())) }
}
impl RefreshClock for FixedClock {
    fn now(&self) -> Result<i64, String> {
        Ok(self.0.load(Ordering::SeqCst))
    }
    fn wait(&self, duration: Duration) {
        self.0.fetch_add(duration.as_secs() as i64, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[derive(Default)]
struct Quotes { offline: AtomicBool }
impl MarketProcess for Quotes {
    fn request(&self, request: &Value, _: usize) -> Result<Vec<u8>, String> {
        if self.offline.load(Ordering::SeqCst) { return Err("受控断线，保留缓存".into()); }
        Ok(serde_json::to_vec(&json!({"protocol":1,"feed":request["feed"],"quotes":[{
            "key":{"market":"CN","code":"600036","kind":"stock"},
            "price":"12","currency":"CNY","sourceTime":"10:00:00","quotedAt":null,"history":[]
        }],"error":null})).unwrap())
    }
}

fn book() -> (tempfile::TempDir, Arc<Mutex<Ledger>>) {
    let directory = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::open(&directory.path().join("ledger.sqlite3")).unwrap();
    let account = ledger.create_account("刷新核对", "CNY").unwrap();
    ledger.save_opening(&OpeningInput {
        account_id: account, occurred_on: "2026-10-23".into(), cash: "0".into(), note: "".into(),
        positions: vec![OpeningPosition { market: "CN".into(), code: "600036".into(), kind: "stock".into(),
            name: "招商银行".into(), currency: "CNY".into(), quantity: "10".into(), cost: "100".into(), reference_price: None }],
    }, None).unwrap();
    (directory, Arc::new(Mutex::new(ledger)))
}

#[test]
fn refreshed_quotes_value_the_real_holdings_without_changing_account_records() {
    let (_directory, ledger) = book();
    let before = serde_json::to_value(ledger.lock().unwrap().snapshot().unwrap()).unwrap();
    let refresh = Arc::new(MarketRefresh::new(ledger.clone(), Arc::new(Quotes::default()), Arc::new(FixedClock::default())));
    let state = refresh.refresh_quotes(false).unwrap();
    assert_eq!(state.quotes[0].price.as_deref(), Some("12"));
    assert_eq!(state.holdings[0].market_value.as_deref(), Some("120.00"));
    assert_eq!(serde_json::to_value(ledger.lock().unwrap().snapshot().unwrap()).unwrap(), before);
}

#[test]
fn a_failed_refresh_keeps_the_last_quote_and_all_account_records() {
    let (_directory, ledger) = book();
    let before = serde_json::to_value(ledger.lock().unwrap().snapshot().unwrap()).unwrap();
    let process = Arc::new(Quotes::default());
    let clock = Arc::new(FixedClock::default());
    let refresh = MarketRefresh::new(ledger.clone(), process.clone(), clock.clone());
    refresh.refresh_quotes(false).unwrap();
    process.offline.store(true, Ordering::SeqCst);
    clock.0.fetch_add(3600, Ordering::SeqCst);
    let state = refresh.refresh_quotes(false).unwrap();
    assert_eq!(state.quotes[0].price.as_deref(), Some("12"));
    assert!(state.quotes[0].error.as_deref().unwrap().contains("受控断线"));
    assert_eq!(serde_json::to_value(ledger.lock().unwrap().snapshot().unwrap()).unwrap(), before);
}

fn query(account: String) -> graham_ledger::PerformanceQuery {
    graham_ledger::PerformanceQuery { account_ids: vec![account], currency: "CNY".into(),
        range: "custom".into(), start_on: Some("2026-10-23".into()), end_on: Some("2026-10-26".into()) }
}

fn history_response(key: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"protocol":1,"feed":"performance","series":[{
        "key":key,"source":"AKShare / 新浪","basis":"unadjusted","timezone":"Asia/Shanghai",
        "prices":[{"date":"2026-10-23","close":"10"},{"date":"2026-10-26","close":"11"}],"invalidatesBefore":null,"error":null
    }],"calendars":[{"market":"CN","name":"XSHG","version":"4.13.2","sessions":[
        {"date":"2026-10-23","session":"2026-10-23","reason":null},
        {"date":"2026-10-24","session":"2026-10-23","reason":null},
        {"date":"2026-10-25","session":"2026-10-23","reason":null},
        {"date":"2026-10-26","session":"2026-10-26","reason":null}
    ]}],"error":null})).unwrap()
}

struct GatedHistory {
    started: mpsc::Sender<String>,
    first: Mutex<mpsc::Receiver<Result<(), String>>>,
    second: Mutex<mpsc::Receiver<Result<(), String>>>,
}
impl MarketProcess for GatedHistory {
    fn request(&self, request: &Value, _: usize) -> Result<Vec<u8>, String> {
        let key = request["keys"][0].clone();
        let code = key["code"].as_str().unwrap().to_string();
        self.started.send(code.clone()).map_err(|_| "测试读取端已关闭")?;
        let release = if code == "600036" { &self.first } else { &self.second };
        release.lock().unwrap().recv_timeout(Duration::from_secs(10)).map_err(|_| "受控请求未释放")??;
        Ok(history_response(key))
    }
}

#[test]
fn stopping_in_flight_history_keeps_reads_available_and_accepts_valid_cache_without_reopening_the_queue() {
    let (_directory, ledger) = book();
    let account = ledger.lock().unwrap().snapshot().unwrap().accounts[0].id.clone();
    let before = serde_json::to_value(ledger.lock().unwrap().snapshot().unwrap()).unwrap();
    let (entered, started) = mpsc::channel();
    let (release, first) = mpsc::channel();
    let (_unused, second) = mpsc::channel();
    let refresh = Arc::new(MarketRefresh::new(ledger.clone(), Arc::new(GatedHistory {
        started: entered, first: Mutex::new(first), second: Mutex::new(second),
    }), Arc::new(FixedClock::default())));
    let query = query(account);
    let task = refresh.start_performance(query.clone()).unwrap();
    assert_eq!(started.recv_timeout(Duration::from_secs(5)).unwrap(), "600036");
    let reader = refresh.clone();
    let (result, read) = mpsc::channel();
    let read_task = std::thread::spawn(move || { result.send(reader.quotes(false)).unwrap(); });
    read.recv_timeout(Duration::from_secs(2)).expect("行情进程等待不应阻塞账本读取").unwrap();
    read_task.join().unwrap();
    refresh.stop_history().unwrap();
    release.send(Ok(())).unwrap();
    task.join().unwrap();
    let state = refresh.performance(&query).unwrap();
    assert!(!state.refresh.active);
    assert_eq!(state.total_return.as_deref(), Some("10.000000"));
    assert_eq!(serde_json::to_value(ledger.lock().unwrap().snapshot().unwrap()).unwrap(), before);
}

#[test]
fn an_old_history_failure_cannot_finish_or_overwrite_the_new_query() {
    let (_directory, ledger) = book();
    let first_account = ledger.lock().unwrap().snapshot().unwrap().accounts[0].id.clone();
    let second_account = {
        let mut book = ledger.lock().unwrap();
        let account = book.create_account("第二范围", "CNY").unwrap();
        book.save_opening(&OpeningInput { account_id: account.clone(), occurred_on: "2026-10-23".into(), cash: "0".into(), note: "".into(),
            positions: vec![OpeningPosition { market: "CN".into(), code: "600519".into(), kind: "stock".into(), name: "贵州茅台".into(),
                currency: "CNY".into(), quantity: "10".into(), cost: "100".into(), reference_price: None }],
        }, None).unwrap();
        account
    };
    let (entered, started) = mpsc::channel();
    let (old_release, first) = mpsc::channel();
    let (new_release, second) = mpsc::channel();
    let refresh = Arc::new(MarketRefresh::new(ledger, Arc::new(GatedHistory {
        started: entered, first: Mutex::new(first), second: Mutex::new(second),
    }), Arc::new(FixedClock::default())));
    let old_task = refresh.start_performance(query(first_account)).unwrap();
    assert_eq!(started.recv_timeout(Duration::from_secs(5)).unwrap(), "600036");
    let new_query = query(second_account);
    let new_task = refresh.start_performance(new_query.clone()).unwrap();
    assert_eq!(started.recv_timeout(Duration::from_secs(5)).unwrap(), "600519");
    old_release.send(Err("旧请求失败".into())).unwrap();
    old_task.join().unwrap();
    let state = refresh.performance(&new_query).unwrap();
    assert!(state.refresh.active);
    assert!(state.refresh.error.is_none());
    refresh.stop_history().unwrap();
    new_release.send(Ok(())).unwrap();
    new_task.join().unwrap();
    assert!(!refresh.performance(&new_query).unwrap().refresh.active);
}

struct AdmissionClock {
    owner: std::thread::ThreadId,
    blocked: AtomicBool,
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl RefreshClock for AdmissionClock {
    fn now(&self) -> Result<i64, String> {
        if std::thread::current().id() != self.owner && self.blocked.swap(false, Ordering::SeqCst) {
            self.entered.send(()).map_err(|_| "受控时钟接收端已关闭")?;
            self.release.lock().unwrap().recv_timeout(Duration::from_secs(10)).map_err(|_| "受控时钟未释放")?;
        }
        FixedClock::default().now()
    }
    fn wait(&self, _: Duration) { std::thread::yield_now(); }
}

#[derive(Default)]
struct CountedHistory { requests: std::sync::atomic::AtomicUsize }
impl MarketProcess for CountedHistory {
    fn request(&self, request: &Value, _: usize) -> Result<Vec<u8>, String> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        Ok(history_response(request["keys"][0].clone()))
    }
}

#[test]
fn stopping_before_admission_does_not_send_a_new_request_after_clock_wait() {
    let (_directory, ledger) = book();
    let account = ledger.lock().unwrap().snapshot().unwrap().accounts[0].id.clone();
    let (entered, waiting) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let process = Arc::new(CountedHistory::default());
    let refresh = Arc::new(MarketRefresh::new(ledger, process.clone(), Arc::new(AdmissionClock {
        owner: std::thread::current().id(), blocked: AtomicBool::new(true), entered, release: Mutex::new(gate),
    })));
    let query = query(account);
    let task = refresh.start_performance(query.clone()).unwrap();
    waiting.recv_timeout(Duration::from_secs(5)).unwrap();
    refresh.stop_history().unwrap();
    release.send(()).unwrap();
    task.join().unwrap();
    assert_eq!(process.requests.load(Ordering::SeqCst), 0);
    assert!(!refresh.performance(&query).unwrap().refresh.active);
}
