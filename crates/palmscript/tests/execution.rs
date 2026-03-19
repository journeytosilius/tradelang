use std::path::PathBuf;
use std::sync::Mutex;

use mockito::{Matcher, Server};
use palmscript::{
    load_paper_session_export, load_paper_session_logs, load_paper_session_manifest,
    serve_execution_daemon, stop_paper_session, submit_paper_session, DiagnosticsDetailMode,
    ExchangeEndpoints, ExecutionDaemonConfig, ExecutionSessionHealth, ExecutionSessionStatus,
    PaperSessionConfig, PaperSessionManifest, SubmitPaperSession, VmLimits,
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
            max_volume_fill_pct: None,
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
    let logs = load_paper_session_logs(&manifest.session_id).expect("paper logs should load");
    assert_eq!(export.manifest.status, ExecutionSessionStatus::Live);
    assert_eq!(status.subscription_count, 1);
    assert_eq!(
        logs.iter()
            .filter(|event| event.message.starts_with("paper session updated:"))
            .count(),
        1
    );
    assert!(
        logs.iter()
            .any(|event| event.message.contains("runtime_to=1704067440000")),
        "{logs:#?}"
    );
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
            max_volume_fill_pct: None,
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
fn paper_session_submission_rejects_unknown_execution_aliases() {
    let _guard = ENV_LOCK.lock().expect("lock env");
    let state_dir = tempfile::tempdir().expect("tempdir");
    std::env::set_var("PALMSCRIPT_EXECUTION_STATE_DIR", state_dir.path());

    let err = submit_paper_session(SubmitPaperSession {
        source: source().to_string(),
        script_path: Some(PathBuf::from("strategy.ps")),
        config: PaperSessionConfig {
            execution_source_aliases: vec!["missing".to_string()],
            initial_capital: 1_000.0,
            maker_fee_bps: 0.0,
            taker_fee_bps: 0.0,
            execution_fee_schedules: std::collections::BTreeMap::new(),
            slippage_bps: 0.0,
            max_volume_fill_pct: None,
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            leverage: None,
            margin_mode: None,
            vm_limits: VmLimits::default(),
        },
        start_time_ms: 1704067320000_i64,
        endpoints: ExchangeEndpoints::from_env(),
    })
    .expect_err("unknown execution alias should be rejected at submission time");

    assert!(err
        .to_string()
        .contains("unknown execution source `missing`"));
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
            max_volume_fill_pct: None,
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
fn paper_daemon_retries_gappy_perp_mark_cache_until_session_goes_live() {
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
            max_volume_fill_pct: None,
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
    .expect("daemon should retry the gappy perp mark cache");
    assert!(!status.running);

    let export = load_paper_session_export(&manifest.session_id).expect("perp paper export");
    let logs = load_paper_session_logs(&manifest.session_id).expect("paper logs should load");
    assert_eq!(export.manifest.status, ExecutionSessionStatus::Live);
    assert_eq!(export.manifest.health, ExecutionSessionHealth::Live);
    assert_eq!(export.manifest.failure_message, None);
    assert!(export.latest_result.is_some());
    assert!(
        logs.iter()
            .any(|event| event.message.contains("paper session updated")),
        "{logs:#?}"
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
            max_volume_fill_pct: None,
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

#[test]
fn paper_daemon_reports_feed_context_when_history_bootstrap_is_empty() {
    let _guard = ENV_LOCK.lock().expect("lock env");
    let state_dir = tempfile::tempdir().expect("tempdir");
    let mut server = Server::new();
    server
        .mock("GET", "/api/v3/klines")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            Matcher::UrlEncoded("interval".into(), "1m".into()),
        ]))
        .with_status(200)
        .with_body("[]")
        .create();

    std::env::set_var("PALMSCRIPT_EXECUTION_STATE_DIR", state_dir.path());
    std::env::set_var("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url());

    submit_paper_session(SubmitPaperSession {
        source: source().to_string(),
        script_path: Some(PathBuf::from("spot_strategy.ps")),
        config: PaperSessionConfig {
            execution_source_aliases: vec!["spot".to_string()],
            initial_capital: 1_000.0,
            maker_fee_bps: 0.0,
            taker_fee_bps: 0.0,
            execution_fee_schedules: std::collections::BTreeMap::new(),
            slippage_bps: 0.0,
            max_volume_fill_pct: None,
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            leverage: None,
            margin_mode: None,
            vm_limits: VmLimits::default(),
        },
        start_time_ms: 1704067320000_i64,
        endpoints: ExchangeEndpoints::from_env(),
    })
    .expect("spot paper session submission should succeed");

    let err = serve_execution_daemon(ExecutionDaemonConfig {
        poll_interval_ms: 1,
        once: true,
    })
    .expect_err("daemon should report empty history bootstrap as a fetch failure");
    let rendered = err.to_string();
    assert!(rendered.contains("feed hub sync failed"), "{rendered}");
    assert!(
        rendered.contains("feed bootstrap failed for source `spot` (binance.spot) `BTCUSDT` 1m"),
        "{rendered}"
    );
    assert!(rendered.contains("required_fields=["), "{rendered}");
    assert!(rendered.contains("requested_window=["), "{rendered}");

    std::env::remove_var("PALMSCRIPT_EXECUTION_STATE_DIR");
    std::env::remove_var("PALMSCRIPT_BINANCE_SPOT_BASE_URL");
}

#[test]
fn paper_daemon_marks_only_broken_sessions_failed_when_load_validation_fails() {
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

    let broken = submit_paper_session(SubmitPaperSession {
        source: source().to_string(),
        script_path: Some(PathBuf::from("broken_strategy.ps")),
        config: PaperSessionConfig {
            execution_source_aliases: vec!["spot".to_string()],
            initial_capital: 1_000.0,
            maker_fee_bps: 0.0,
            taker_fee_bps: 0.0,
            execution_fee_schedules: std::collections::BTreeMap::new(),
            slippage_bps: 0.0,
            max_volume_fill_pct: None,
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            leverage: None,
            margin_mode: None,
            vm_limits: VmLimits::default(),
        },
        start_time_ms: 1704067320000_i64,
        endpoints: ExchangeEndpoints::from_env(),
    })
    .expect("broken session submission should succeed before manifest corruption");
    let healthy = submit_paper_session(SubmitPaperSession {
        source: source().to_string(),
        script_path: Some(PathBuf::from("healthy_strategy.ps")),
        config: PaperSessionConfig {
            execution_source_aliases: vec!["spot".to_string()],
            initial_capital: 1_000.0,
            maker_fee_bps: 0.0,
            taker_fee_bps: 0.0,
            execution_fee_schedules: std::collections::BTreeMap::new(),
            slippage_bps: 0.0,
            max_volume_fill_pct: None,
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            leverage: None,
            margin_mode: None,
            vm_limits: VmLimits::default(),
        },
        start_time_ms: 1704067320000_i64,
        endpoints: ExchangeEndpoints::from_env(),
    })
    .expect("healthy session submission should succeed");

    let manifest_path = state_dir
        .path()
        .join("sessions")
        .join(&broken.session_id)
        .join("manifest.json");
    let mut broken_manifest: PaperSessionManifest =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("read manifest"))
            .expect("parse manifest");
    broken_manifest.config.execution_source_aliases = vec!["missing".to_string()];
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&broken_manifest).expect("serialize manifest"),
    )
    .expect("rewrite manifest");

    let status = serve_execution_daemon(ExecutionDaemonConfig {
        poll_interval_ms: 1,
        once: true,
    })
    .expect("daemon should isolate broken sessions instead of aborting");

    let broken_manifest =
        load_paper_session_manifest(&broken.session_id).expect("broken manifest should load");
    let broken_logs =
        load_paper_session_logs(&broken.session_id).expect("broken session logs should load");
    let healthy_export =
        load_paper_session_export(&healthy.session_id).expect("healthy export should load");

    assert_eq!(broken_manifest.status, ExecutionSessionStatus::Failed);
    assert_eq!(broken_manifest.health, ExecutionSessionHealth::Failed);
    assert!(broken_manifest
        .failure_message
        .as_deref()
        .is_some_and(|message| message.contains("unknown execution source `missing`")));
    assert!(broken_logs.iter().any(|event| {
        event.status == ExecutionSessionStatus::Failed
            && event.message.contains("unknown execution source `missing`")
    }));
    assert_eq!(healthy_export.manifest.status, ExecutionSessionStatus::Live);
    assert_eq!(status.subscription_count, 1);
    assert_eq!(status.active_sessions, vec![healthy.session_id]);

    std::env::remove_var("PALMSCRIPT_EXECUTION_STATE_DIR");
    std::env::remove_var("PALMSCRIPT_BINANCE_SPOT_BASE_URL");
}
