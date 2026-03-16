use std::path::PathBuf;
use std::sync::Mutex;

use mockito::{Matcher, Server};
use palmscript::{
    load_paper_session_export, serve_execution_daemon, stop_paper_session, submit_paper_session,
    DiagnosticsDetailMode, ExchangeEndpoints, ExecutionDaemonConfig, ExecutionSessionStatus,
    PaperSessionConfig, SubmitPaperSession, VmLimits,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn binance_klines(rows: &[serde_json::Value]) -> String {
    serde_json::Value::Array(rows.to_vec()).to_string()
}

fn mock_binance_interval(server: &mut Server, interval: &str, rows: &[serde_json::Value]) {
    server
        .mock("GET", "/api/v3/klines")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            Matcher::UrlEncoded("interval".into(), interval.into()),
        ]))
        .with_status(200)
        .with_body(binance_klines(rows))
        .create();
}

fn mock_binance_book_ticker(server: &mut Server) {
    server
        .mock("GET", "/api/v3/ticker/bookTicker")
        .match_query(Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()))
        .with_status(200)
        .with_body(
            serde_json::json!({
                "symbol": "BTCUSDT",
                "bidPrice": "12.50",
                "askPrice": "13.50",
            })
            .to_string(),
        )
        .create();
}

fn mock_binance_last_price(server: &mut Server) {
    server
        .mock("GET", "/api/v3/ticker/price")
        .match_query(Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()))
        .with_status(200)
        .with_body(
            serde_json::json!({
                "symbol": "BTCUSDT",
                "price": "13.00",
            })
            .to_string(),
        )
        .create();
}

fn source() -> &'static str {
    "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
entry short = false
exit long = false
exit short = false
order entry long = market(venue = spot)
order entry short = market(venue = spot)
order exit long = market(venue = spot)
order exit short = market(venue = spot)
plot(spot.close)"
}

fn perp_source() -> &'static str {
    "interval 1m
source perp = binance.usdm(\"BTCUSDT\")
execution exec = binance.usdm(\"BTCUSDT\")
entry long = perp.close > perp.close[1]
entry short = false
exit long = false
exit short = false
order entry long = market(venue = exec)
order entry short = market(venue = exec)
order exit long = market(venue = exec)
order exit short = market(venue = exec)
plot(perp.close)"
}

fn perp_aux_source() -> &'static str {
    "interval 1m
source perp = binance.usdm(\"BTCUSDT\")
execution exec = binance.usdm(\"BTCUSDT\")
entry long = false
entry short = false
exit long = false
exit short = false
order entry long = market(venue = exec)
order entry short = market(venue = exec)
order exit long = market(venue = exec)
order exit short = market(venue = exec)
plot(nz(perp.funding_rate, 0))"
}

fn mock_binance_usdm_interval(server: &mut Server, interval: &str, rows: &[serde_json::Value]) {
    server
        .mock("GET", "/fapi/v1/klines")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            Matcher::UrlEncoded("interval".into(), interval.into()),
        ]))
        .with_status(200)
        .with_body(binance_klines(rows))
        .create();
}

fn mock_binance_usdm_mark_interval(
    server: &mut Server,
    interval: &str,
    rows: &[serde_json::Value],
) {
    server
        .mock("GET", "/fapi/v1/markPriceKlines")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            Matcher::UrlEncoded("interval".into(), interval.into()),
        ]))
        .with_status(200)
        .with_body(binance_klines(rows))
        .create();
}

fn mock_binance_usdm_exchange_info(server: &mut Server) {
    server
        .mock("GET", "/fapi/v1/exchangeInfo")
        .with_status(200)
        .with_body(
            serde_json::json!({
                "symbols": [{
                    "symbol": "BTCUSDT",
                    "maintMarginPercent": "2.5",
                    "requiredMarginPercent": "5.0"
                }]
            })
            .to_string(),
        )
        .create();
}

fn mock_binance_usdm_book_ticker(server: &mut Server) {
    server
        .mock("GET", "/fapi/v1/ticker/bookTicker")
        .match_query(Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()))
        .with_status(200)
        .with_body(
            serde_json::json!({
                "symbol": "BTCUSDT",
                "bidPrice": "12.50",
                "askPrice": "13.50",
            })
            .to_string(),
        )
        .create();
}

fn mock_binance_usdm_last_price(server: &mut Server) {
    server
        .mock("GET", "/fapi/v1/ticker/price")
        .match_query(Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()))
        .with_status(200)
        .with_body(
            serde_json::json!({
                "symbol": "BTCUSDT",
                "price": "13.00",
            })
            .to_string(),
        )
        .create();
}

fn mock_binance_usdm_premium_index(server: &mut Server) {
    server
        .mock("GET", "/fapi/v1/premiumIndex")
        .match_query(Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()))
        .with_status(200)
        .with_body(
            serde_json::json!({
                "symbol": "BTCUSDT",
                "markPrice": "13.00",
            })
            .to_string(),
        )
        .create();
}

fn mock_binance_usdm_empty_funding(server: &mut Server) {
    server
        .mock("GET", "/fapi/v1/fundingRate")
        .match_query(Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()))
        .with_status(200)
        .with_body("[]")
        .create();
}

#[test]
fn paper_daemon_processes_a_submitted_session_against_mocked_exchange_bars() {
    let _guard = ENV_LOCK.lock().expect("lock env");
    let state_dir = tempfile::tempdir().expect("tempdir");
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10", "10", "10", "10", "1000"]),
            serde_json::json!([1704067260000_i64, "11", "11", "11", "11", "1000"]),
            serde_json::json!([1704067320000_i64, "12", "12", "12", "12", "1000"]),
            serde_json::json!([1704067380000_i64, "13", "13", "13", "13", "1000"]),
        ],
    );
    mock_binance_book_ticker(&mut server);
    mock_binance_last_price(&mut server);

    std::env::set_var("PALMSCRIPT_EXECUTION_STATE_DIR", state_dir.path());
    std::env::set_var("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url());

    let manifest = submit_paper_session(SubmitPaperSession {
        source: source().to_string(),
        script_path: Some(PathBuf::from("strategy.ps")),
        config: PaperSessionConfig {
            execution_source_aliases: vec!["spot".to_string()],
            initial_capital: 1_000.0,
            maker_fee_bps: 0.0,
            taker_fee_bps: 0.0,
            execution_fee_schedules: std::collections::BTreeMap::new(),
            slippage_bps: 0.0,
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            leverage: None,
            margin_mode: None,
            vm_limits: VmLimits::default(),
        },
        start_time_ms: 1704067320000_i64,
        endpoints: ExchangeEndpoints::from_env(),
    })
    .expect("paper session submission should succeed");

    let status = serve_execution_daemon(ExecutionDaemonConfig {
        poll_interval_ms: 1,
        once: true,
    })
    .expect("daemon should process queued paper sessions");
    assert!(!status.running);

    let export = load_paper_session_export(&manifest.session_id).expect("paper export should load");
    assert_eq!(export.manifest.status, ExecutionSessionStatus::Live);
    assert_eq!(status.subscription_count, 1);
    let result = export
        .latest_result
        .expect("paper session should persist a latest result");
    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.fills[0].bar_index, 3);
    assert_eq!(result.open_positions.len(), 1);
    assert_eq!(
        export
            .snapshot
            .as_ref()
            .expect("paper snapshot should exist")
            .latest_closed_bar_time_ms,
        Some(1704067380000_i64)
    );
    let snapshot = export.snapshot.expect("paper snapshot should exist");
    assert_eq!(snapshot.feed_snapshots.len(), 1);
    let feed = &snapshot.feed_snapshots[0];
    assert_eq!(feed.execution_alias, "spot");
    assert_eq!(
        feed.top_of_book
            .as_ref()
            .expect("top of book should be present")
            .mid_price,
        13.0
    );
    assert_eq!(
        snapshot.open_positions[0].market_price, 13.0,
        "open position valuation should use top-of-book mid"
    );

    std::env::remove_var("PALMSCRIPT_EXECUTION_STATE_DIR");
    std::env::remove_var("PALMSCRIPT_BINANCE_SPOT_BASE_URL");
}

#[test]
fn queued_paper_session_can_be_stopped_before_the_daemon_picks_it_up() {
    let _guard = ENV_LOCK.lock().expect("lock env");
    let state_dir = tempfile::tempdir().expect("tempdir");
    std::env::set_var("PALMSCRIPT_EXECUTION_STATE_DIR", state_dir.path());

    let manifest = submit_paper_session(SubmitPaperSession {
        source: source().to_string(),
        script_path: Some(PathBuf::from("strategy.ps")),
        config: PaperSessionConfig {
            execution_source_aliases: vec!["spot".to_string()],
            initial_capital: 1_000.0,
            maker_fee_bps: 0.0,
            taker_fee_bps: 0.0,
            execution_fee_schedules: std::collections::BTreeMap::new(),
            slippage_bps: 0.0,
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            leverage: None,
            margin_mode: None,
            vm_limits: VmLimits::default(),
        },
        start_time_ms: 1704067320000_i64,
        endpoints: ExchangeEndpoints::from_env(),
    })
    .expect("paper session submission should succeed");

    let stopped = stop_paper_session(&manifest.session_id).expect("stop should succeed");
    assert_eq!(stopped.status, ExecutionSessionStatus::Stopped);

    std::env::remove_var("PALMSCRIPT_EXECUTION_STATE_DIR");
}

#[test]
fn paper_daemon_processes_a_perp_session_without_async_blocking_panics() {
    let _guard = ENV_LOCK.lock().expect("lock env");
    let state_dir = tempfile::tempdir().expect("tempdir");
    let mut server = Server::new();
    mock_binance_usdm_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10", "10", "10", "10", "1000"]),
            serde_json::json!([1704067260000_i64, "11", "11", "11", "11", "1000"]),
            serde_json::json!([1704067320000_i64, "12", "12", "12", "12", "1000"]),
            serde_json::json!([1704067380000_i64, "13", "13", "13", "13", "1000"]),
        ],
    );
    mock_binance_usdm_mark_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10", "10", "10", "10", "0"]),
            serde_json::json!([1704067260000_i64, "11", "11", "11", "11", "0"]),
            serde_json::json!([1704067320000_i64, "12", "12", "12", "12", "0"]),
            serde_json::json!([1704067380000_i64, "13", "13", "13", "13", "0"]),
        ],
    );
    mock_binance_usdm_exchange_info(&mut server);
    mock_binance_usdm_book_ticker(&mut server);
    mock_binance_usdm_last_price(&mut server);
    mock_binance_usdm_premium_index(&mut server);

    std::env::set_var("PALMSCRIPT_EXECUTION_STATE_DIR", state_dir.path());
    std::env::set_var("PALMSCRIPT_BINANCE_USDM_BASE_URL", server.url());

    let manifest = submit_paper_session(SubmitPaperSession {
        source: perp_source().to_string(),
        script_path: Some(PathBuf::from("perp_strategy.ps")),
        config: PaperSessionConfig {
            execution_source_aliases: vec!["exec".to_string()],
            initial_capital: 1_000.0,
            maker_fee_bps: 0.0,
            taker_fee_bps: 0.0,
            execution_fee_schedules: std::collections::BTreeMap::new(),
            slippage_bps: 0.0,
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            leverage: Some(5.0),
            margin_mode: Some(palmscript::PerpMarginMode::Isolated),
            vm_limits: VmLimits::default(),
        },
        start_time_ms: 1704067320000_i64,
        endpoints: ExchangeEndpoints::from_env(),
    })
    .expect("perp paper session submission should succeed");

    let status = serve_execution_daemon(ExecutionDaemonConfig {
        poll_interval_ms: 1,
        once: true,
    })
    .expect("daemon should process queued perp paper sessions");
    assert!(!status.running);

    let export = load_paper_session_export(&manifest.session_id).expect("perp paper export");
    assert_eq!(export.manifest.status, ExecutionSessionStatus::Live);
    assert_eq!(status.subscription_count, 1);
    let result = export
        .latest_result
        .expect("perp latest result should exist");
    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.open_positions.len(), 1);
    assert_eq!(
        export
            .snapshot
            .as_ref()
            .expect("perp snapshot should exist")
            .open_positions[0]
            .market_price,
        13.0
    );

    std::env::remove_var("PALMSCRIPT_EXECUTION_STATE_DIR");
    std::env::remove_var("PALMSCRIPT_BINANCE_USDM_BASE_URL");
}

#[test]
fn paper_daemon_keeps_binance_usdm_session_live_when_funding_feed_is_empty() {
    let _guard = ENV_LOCK.lock().expect("lock env");
    let state_dir = tempfile::tempdir().expect("tempdir");
    let mut server = Server::new();
    mock_binance_usdm_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10", "10", "10", "10", "1000"]),
            serde_json::json!([1704067260000_i64, "11", "11", "11", "11", "1000"]),
            serde_json::json!([1704067320000_i64, "12", "12", "12", "12", "1000"]),
            serde_json::json!([1704067380000_i64, "13", "13", "13", "13", "1000"]),
        ],
    );
    mock_binance_usdm_mark_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10", "10", "10", "10", "0"]),
            serde_json::json!([1704067260000_i64, "11", "11", "11", "11", "0"]),
            serde_json::json!([1704067320000_i64, "12", "12", "12", "12", "0"]),
            serde_json::json!([1704067380000_i64, "13", "13", "13", "13", "0"]),
        ],
    );
    mock_binance_usdm_exchange_info(&mut server);
    mock_binance_usdm_book_ticker(&mut server);
    mock_binance_usdm_last_price(&mut server);
    mock_binance_usdm_premium_index(&mut server);
    mock_binance_usdm_empty_funding(&mut server);

    std::env::set_var("PALMSCRIPT_EXECUTION_STATE_DIR", state_dir.path());
    std::env::set_var("PALMSCRIPT_BINANCE_USDM_BASE_URL", server.url());

    let manifest = submit_paper_session(SubmitPaperSession {
        source: perp_aux_source().to_string(),
        script_path: Some(PathBuf::from("perp_aux_strategy.ps")),
        config: PaperSessionConfig {
            execution_source_aliases: vec!["exec".to_string()],
            initial_capital: 1_000.0,
            maker_fee_bps: 0.0,
            taker_fee_bps: 0.0,
            execution_fee_schedules: std::collections::BTreeMap::new(),
            slippage_bps: 0.0,
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            leverage: Some(5.0),
            margin_mode: Some(palmscript::PerpMarginMode::Isolated),
            vm_limits: VmLimits::default(),
        },
        start_time_ms: 1704067320000_i64,
        endpoints: ExchangeEndpoints::from_env(),
    })
    .expect("perp aux paper session submission should succeed");

    let status = serve_execution_daemon(ExecutionDaemonConfig {
        poll_interval_ms: 1,
        once: true,
    })
    .expect("daemon should tolerate empty funding rows");
    assert!(!status.running);

    let export = load_paper_session_export(&manifest.session_id).expect("perp aux paper export");
    assert_eq!(export.manifest.status, ExecutionSessionStatus::Live);
    assert_eq!(export.manifest.feed_summary.total_feeds, 2);
    assert_eq!(export.manifest.feed_summary.failed_feeds, 0);
    assert!(!export
        .latest_result
        .expect("perp aux latest result should exist")
        .equity_curve
        .is_empty());

    std::env::remove_var("PALMSCRIPT_EXECUTION_STATE_DIR");
    std::env::remove_var("PALMSCRIPT_BINANCE_USDM_BASE_URL");
}
