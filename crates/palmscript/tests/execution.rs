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

fn source() -> &'static str {
    "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
entry short = false
exit long = false
exit short = false
plot(spot.close)"
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

    std::env::set_var("PALMSCRIPT_EXECUTION_STATE_DIR", state_dir.path());
    std::env::set_var("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url());

    let manifest = submit_paper_session(SubmitPaperSession {
        source: source().to_string(),
        script_path: Some(PathBuf::from("strategy.ps")),
        config: PaperSessionConfig {
            execution_source_aliases: vec!["spot".to_string()],
            initial_capital: 1_000.0,
            fee_bps: 0.0,
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
    let result = export
        .latest_result
        .expect("paper session should persist a latest result");
    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.fills[0].bar_index, 3);
    assert_eq!(result.open_positions.len(), 1);
    assert_eq!(
        export
            .snapshot
            .expect("paper snapshot should exist")
            .latest_closed_bar_time_ms,
        Some(1704067380000_i64)
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
            fee_bps: 0.0,
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
