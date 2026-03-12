use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::prelude::*;
use mockito::{Matcher, Server};
use predicates::prelude::*;
use serde_json::Value;
use tempfile::tempdir;

fn write_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).expect("writes test file");
    path
}

fn palmscript_cmd() -> std::process::Command {
    std::process::Command::new(assert_cmd::cargo::cargo_bin!("palmscript"))
}

fn stdout_string(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is utf-8")
}

fn extract_key_value(output: &std::process::Output, key: &str) -> Option<String> {
    stdout_string(output)
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")).map(str::to_string))
}

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/palmscript")
        .join(relative)
}

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

fn bybit_envelope(rows: &[serde_json::Value]) -> String {
    serde_json::json!({
        "retCode": 0,
        "retMsg": "OK",
        "result": { "list": rows },
        "time": 1704067200000_i64
    })
    .to_string()
}

fn mock_bybit_kline(
    server: &mut Server,
    path: &str,
    category: &str,
    symbol: &str,
    interval: &str,
    rows: &[serde_json::Value],
) {
    server
        .mock("GET", path)
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("category".into(), category.into()),
            Matcher::UrlEncoded("symbol".into(), symbol.into()),
            Matcher::UrlEncoded("interval".into(), interval.into()),
        ]))
        .with_status(200)
        .with_body(bybit_envelope(rows))
        .create();
}

fn mock_gate_spot_interval(
    server: &mut Server,
    symbol: &str,
    interval: &str,
    rows: &[serde_json::Value],
) {
    server
        .mock("GET", "/spot/candlesticks")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("currency_pair".into(), symbol.into()),
            Matcher::UrlEncoded("interval".into(), interval.into()),
        ]))
        .with_status(200)
        .with_body(binance_klines(rows))
        .create();
}

fn mock_gate_futures_interval(
    server: &mut Server,
    contract: &str,
    interval: &str,
    rows: &[serde_json::Value],
) {
    server
        .mock("GET", "/futures/usdt/candlesticks")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("contract".into(), contract.into()),
            Matcher::UrlEncoded("interval".into(), interval.into()),
        ]))
        .with_status(200)
        .with_body(binance_klines(rows))
        .create();
}

fn optimize_script() -> &'static str {
    "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ninput threshold = 0\nentry long = spot.close > spot.close[1] + threshold\nentry short = false\nexit long = spot.close < spot.close[1]\nexit short = true"
}

#[test]
fn help_prints_usage() {
    let mut cmd = palmscript_cmd();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("docs"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("runs"))
        .stdout(predicate::str::contains("check"))
        .stdout(predicate::str::contains("dump-bytecode"));
}

#[test]
fn docs_list_exposes_embedded_topics() {
    let mut cmd = palmscript_cmd();
    cmd.args(["docs", "--list"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("doc_count="))
        .stdout(predicate::str::contains("topic=tooling/cli"))
        .stdout(predicate::str::contains("topic=reference/cli"));
}

#[test]
fn docs_topic_renders_one_embedded_page() {
    let mut cmd = palmscript_cmd();
    cmd.args(["docs", "tooling/cli"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("topic=tooling/cli"))
        .stdout(predicate::str::contains("# CLI"))
        .stdout(predicate::str::contains("palmscript docs --all"));
}

#[test]
fn docs_all_renders_full_embedded_snapshot() {
    let mut cmd = palmscript_cmd();
    cmd.args(["docs", "--all"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("PalmScript embedded English docs"))
        .stdout(predicate::str::contains(
            "===== BEGIN DOC tooling/cli =====",
        ))
        .stdout(predicate::str::contains(
            "===== BEGIN DOC reference/cli =====",
        ))
        .stdout(predicate::str::contains(
            "===== END DOC reference/cli =====",
        ));
}

#[test]
fn run_help_mentions_market_mode() {
    let mut cmd = palmscript_cmd();
    cmd.args(["run", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("market"))
        .stdout(predicate::str::contains("backtest"))
        .stdout(predicate::str::contains("optimize"))
        .stdout(predicate::str::contains("csv").not());
}

#[test]
fn run_rejects_removed_csv_subcommand() {
    let mut cmd = palmscript_cmd();
    cmd.args(["run", "csv"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand 'csv'"));
}

#[test]
fn check_reports_success_for_valid_script() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "valid.ps",
        "interval 1m\nsource bn = binance.spot(\"BTCUSDT\")\nplot(sma(bn.close, 3))",
    );
    let mut cmd = palmscript_cmd();
    cmd.args(["check", script.to_str().unwrap()]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("valid.ps: ok"));
}

#[test]
fn check_reports_compile_diagnostics() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "invalid.ps",
        "interval 1m\nsource bn = binance.spot(\"BTCUSDT\")\nif true { plot(1) }",
    );
    let mut cmd = palmscript_cmd();
    cmd.args(["check", script.to_str().unwrap()]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("expected `else` after `if` block"));
}

#[test]
fn check_reports_multiple_compile_diagnostics() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "invalid.ps",
        "interval 1m\nsource bn = binance.spot(\"BTCUSDT\")\nlet x = bn.close\nlet x = bn.close[1]\nplot(true + 1)",
    );
    let mut cmd = palmscript_cmd();
    cmd.args(["check", script.to_str().unwrap()]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains(
            "duplicate binding `x` in the same scope",
        ))
        .stderr(predicate::str::contains(
            "arithmetic operators require numeric operands",
        ));
}

#[test]
fn dump_bytecode_text_contains_sections() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "script.ps",
        "interval 1m\nsource bn = binance.spot(\"BTCUSDT\")\nplot(sma(bn.close, 3))",
    );
    let mut cmd = palmscript_cmd();
    cmd.args(["dump-bytecode", script.to_str().unwrap()]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Strategy Intervals"))
        .stdout(predicate::str::contains("Constants"))
        .stdout(predicate::str::contains("Locals"))
        .stdout(predicate::str::contains("Instructions"));
}

#[test]
fn dump_bytecode_json_serializes_compiled_program() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "script.ps",
        "interval 1m\nsource bn = binance.spot(\"BTCUSDT\")\nplot(bn.close)",
    );
    let output = palmscript_cmd()
        .args([
            "dump-bytecode",
            script.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("dump-bytecode executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert!(json["program"]["instructions"].is_array());
    assert!(json["program"]["locals"].is_array());
    assert_eq!(json["program"]["base_interval"], Value::from("Min1"));
    assert_eq!(
        json["program"]["declared_sources"][0]["alias"],
        Value::from("bn")
    );
}

#[test]
fn run_market_executes_source_aware_script() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "1.0", "2.0", "0.5", "1.5", "10.0"]),
            serde_json::json!([1704067260000_i64, "2.0", "3.0", "1.5", "2.5", "11.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "market.ps",
        "interval 1m\nsource bn = binance.spot(\"BTCUSDT\")\nplot(bn.close)",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "market",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067320000",
        ])
        .output()
        .expect("run command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["plots"][0]["points"][0]["value"], Value::from(1.5));
    assert_eq!(json["plots"][0]["points"][1]["value"], Value::from(2.5));
}

#[test]
fn run_market_supports_text_output() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "1.0", "2.0", "0.5", "1.5", "10.0"]),
            serde_json::json!([1704067260000_i64, "2.0", "3.0", "1.5", "2.5", "11.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "market.ps",
        "interval 1m\nsource bn = binance.spot(\"BTCUSDT\")\nexport rising = bn.close > bn.close[1]\ntrigger bullish = bn.close > bn.open\nplot(bn.close)",
    );

    let mut cmd = palmscript_cmd();
    cmd.env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "market",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067320000",
            "--format",
            "text",
        ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Plots"))
        .stdout(predicate::str::contains("Exports"))
        .stdout(predicate::str::contains("Triggers"))
        .stdout(predicate::str::contains("Trigger Events"));
}

#[test]
fn run_market_executes_bybit_source_aware_script() {
    let mut server = Server::new();
    mock_bybit_kline(
        &mut server,
        "/v5/market/kline",
        "spot",
        "BTCUSDT",
        "1",
        &[
            serde_json::json!([1704067260000_i64, "2.0", "3.0", "1.5", "2.5", "11.0", "0"]),
            serde_json::json!([1704067200000_i64, "1.0", "2.0", "0.5", "1.5", "10.0", "0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "market_bybit.ps",
        "interval 1m\nsource bb = bybit.spot(\"BTCUSDT\")\nplot(bb.close)",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BYBIT_BASE_URL", server.url())
        .args([
            "run",
            "market",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067320000",
        ])
        .output()
        .expect("run command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["plots"][0]["points"][0]["value"], Value::from(1.5));
    assert_eq!(json["plots"][0]["points"][1]["value"], Value::from(2.5));
}

#[test]
fn run_market_executes_gate_source_aware_script() {
    let mut server = Server::new();
    mock_gate_spot_interval(
        &mut server,
        "BTC_USDT",
        "1m",
        &[
            serde_json::json!([
                1704067200_i64,
                "15.0",
                "1.5",
                "2.0",
                "0.5",
                "1.0",
                "10.0",
                true
            ]),
            serde_json::json!([
                1704067260_i64,
                "16.0",
                "2.5",
                "3.0",
                "1.5",
                "2.0",
                "11.0",
                true
            ]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "market_gate.ps",
        "interval 1m\nsource gt = gate.spot(\"BTC_USDT\")\nplot(gt.close)",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_GATE_BASE_URL", server.url())
        .args([
            "run",
            "market",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067320000",
        ])
        .output()
        .expect("run command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["plots"][0]["points"][0]["value"], Value::from(1.5));
    assert_eq!(json["plots"][0]["points"][1]["value"], Value::from(2.5));
}

#[test]
fn checked_in_single_interval_example_runs_via_cli() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([
                1704067200000_i64,
                "100.0",
                "101.0",
                "99.0",
                "100.5",
                "1000.0"
            ]),
            serde_json::json!([
                1704067260000_i64,
                "100.5",
                "101.5",
                "100.0",
                "101.0",
                "1010.0"
            ]),
            serde_json::json!([
                1704067320000_i64,
                "101.0",
                "102.0",
                "100.5",
                "101.5",
                "1020.0"
            ]),
            serde_json::json!([
                1704067380000_i64,
                "101.5",
                "102.5",
                "101.0",
                "102.0",
                "1030.0"
            ]),
            serde_json::json!([
                1704067440000_i64,
                "102.0",
                "103.0",
                "101.5",
                "102.5",
                "1040.0"
            ]),
            serde_json::json!([
                1704067500000_i64,
                "102.5",
                "103.5",
                "102.0",
                "103.0",
                "1050.0"
            ]),
        ],
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "market",
            repo_path("examples/strategies/sma_cross.ps")
                .to_str()
                .unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067560000",
        ])
        .output()
        .expect("run command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["exports"][0]["name"], Value::from("bullish"));
    assert_eq!(json["triggers"][0]["name"], Value::from("cross_up"));
}

#[test]
fn checked_in_multi_interval_example_runs_via_cli() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1d",
        &[
            serde_json::json!([
                1704067200000_i64,
                "100.0",
                "101.0",
                "99.0",
                "100.5",
                "1000.0"
            ]),
            serde_json::json!([
                1704153600000_i64,
                "100.5",
                "101.5",
                "100.0",
                "101.0",
                "1010.0"
            ]),
            serde_json::json!([
                1704240000000_i64,
                "101.0",
                "102.0",
                "100.5",
                "101.5",
                "1020.0"
            ]),
            serde_json::json!([
                1704326400000_i64,
                "101.5",
                "102.5",
                "101.0",
                "102.0",
                "1030.0"
            ]),
            serde_json::json!([
                1704412800000_i64,
                "102.0",
                "103.0",
                "101.5",
                "102.5",
                "1040.0"
            ]),
            serde_json::json!([
                1704499200000_i64,
                "102.5",
                "103.5",
                "102.0",
                "103.0",
                "1050.0"
            ]),
            serde_json::json!([
                1704585600000_i64,
                "103.0",
                "104.0",
                "102.5",
                "103.5",
                "1060.0"
            ]),
            serde_json::json!([
                1704672000000_i64,
                "103.5",
                "104.5",
                "103.0",
                "104.0",
                "1070.0"
            ]),
            serde_json::json!([
                1704758400000_i64,
                "104.0",
                "105.0",
                "103.5",
                "104.5",
                "1080.0"
            ]),
            serde_json::json!([
                1704844800000_i64,
                "104.5",
                "105.5",
                "104.0",
                "105.0",
                "1090.0"
            ]),
            serde_json::json!([
                1704931200000_i64,
                "105.0",
                "106.0",
                "104.5",
                "105.5",
                "1100.0"
            ]),
            serde_json::json!([
                1705017600000_i64,
                "105.5",
                "106.5",
                "105.0",
                "106.0",
                "1110.0"
            ]),
            serde_json::json!([
                1705104000000_i64,
                "106.0",
                "107.0",
                "105.5",
                "106.5",
                "1120.0"
            ]),
            serde_json::json!([
                1705190400000_i64,
                "106.5",
                "107.5",
                "106.0",
                "107.0",
                "1130.0"
            ]),
        ],
    );
    mock_binance_interval(
        &mut server,
        "1w",
        &[
            serde_json::json!([1704067200000_i64, "90.0", "91.0", "89.0", "90.5", "5000.0"]),
            serde_json::json!([1704672000000_i64, "95.0", "96.0", "94.0", "95.5", "5100.0"]),
            serde_json::json!([
                1705276800000_i64,
                "105.0",
                "106.0",
                "104.0",
                "105.5",
                "5200.0"
            ]),
        ],
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "market",
            repo_path("examples/strategies/weekly_bias.ps")
                .to_str()
                .unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1705276800000",
        ])
        .output()
        .expect("run command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(
        json["exports"][0]["name"],
        Value::from("above_weekly_basis")
    );
    assert_eq!(json["triggers"][0]["name"], Value::from("continuation"));
}

#[test]
fn run_market_reports_fetch_failures_with_cli_prefix() {
    let mut server = Server::new();
    server
        .mock("GET", "/api/v3/klines")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            Matcher::UrlEncoded("interval".into(), "1m".into()),
        ]))
        .with_status(200)
        .with_body(
            serde_json::json!([[1704067200000_i64, "bad", "2.0", "0.5", "1.5", "10.0"]])
                .to_string(),
        )
        .create();

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "market.ps",
        "interval 1m\nsource bn = binance.spot(\"BTCUSDT\")\nplot(bn.close)",
    );

    let mut cmd = palmscript_cmd();
    cmd.env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "market",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067260000",
        ]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("market mode error:"))
        .stderr(predicate::str::contains("malformed response"))
        .stderr(predicate::str::contains("invalid `open` value"));
}

#[test]
fn run_backtest_executes_single_source_script_with_default_execution_source() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "12.0", "12.5", "8.0", "9.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "8.0", "8.5", "7.5", "8.0", "13.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "backtest.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ntrigger long_entry = spot.close > spot.close[1]\ntrigger long_exit = spot.close < spot.close[1]\nplot(spot.close)",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "backtest",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067440000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
        ])
        .output()
        .expect("backtest command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["summary"]["trade_count"], Value::from(1));
    assert!(json["orders"].is_array());
    assert!(json["diagnostics"]["order_diagnostics"].is_array());
    assert!(json["diagnostics"]["trade_diagnostics"].is_array());
    assert!(json["diagnostics"]["capture_summary"].is_object());
    assert!(json["diagnostics"]["export_summaries"].is_array());
    assert!(json["diagnostics"]["opportunity_events"].is_array());
    assert_eq!(json["orders"][0]["kind"], Value::from("Market"));
    let ending_equity = json["summary"]["ending_equity"]
        .as_f64()
        .expect("ending equity should be numeric");
    assert!((ending_equity - 666.6666666666667).abs() < 1e-9);
}

#[test]
fn run_backtest_supports_text_output() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "12.0", "12.5", "8.0", "9.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "8.0", "8.5", "7.5", "8.0", "13.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "backtest.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ntrigger long_entry = spot.close > spot.close[1]\ntrigger long_exit = spot.close < spot.close[1]\nplot(spot.close)",
    );

    let mut cmd = palmscript_cmd();
    cmd.env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "backtest",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067440000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--format",
            "text",
        ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Backtest Summary"))
        .stdout(predicate::str::contains("Order Summary"))
        .stdout(predicate::str::contains("Diagnostics Summary"))
        .stdout(predicate::str::contains("execution_asset_return_pct"))
        .stdout(predicate::str::contains("Recent Opportunity Events"))
        .stdout(predicate::str::contains("Recent Orders"))
        .stdout(predicate::str::contains("Recent Trades"))
        .stdout(predicate::str::contains("Open Position"));
}

#[test]
fn run_backtest_supports_bybit_usdt_perps_execution_source() {
    let mut server = Server::new();
    mock_bybit_kline(
        &mut server,
        "/v5/market/kline",
        "linear",
        "BTCUSDT",
        "1",
        &[
            serde_json::json!([1704067380000_i64, "8.0", "8.5", "7.5", "8.0", "13.0", "0"]),
            serde_json::json!([1704067320000_i64, "12.0", "12.5", "8.0", "9.0", "12.0", "0"]),
            serde_json::json!([
                1704067260000_i64,
                "10.0",
                "12.0",
                "9.0",
                "11.0",
                "11.0",
                "0"
            ]),
            serde_json::json!([
                1704067200000_i64,
                "10.0",
                "11.0",
                "9.0",
                "10.0",
                "10.0",
                "0"
            ]),
        ],
    );
    mock_bybit_kline(
        &mut server,
        "/v5/market/mark-price-kline",
        "linear",
        "BTCUSDT",
        "1",
        &[
            serde_json::json!([1704067380000_i64, "8.0", "8.5", "7.5", "8.0"]),
            serde_json::json!([1704067320000_i64, "12.0", "12.5", "8.0", "9.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0"]),
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0"]),
        ],
    );
    server
        .mock("GET", "/v5/market/risk-limit")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("category".into(), "linear".into()),
            Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
        ]))
        .with_status(200)
        .with_body(
            serde_json::json!({
                "retCode": 0,
                "retMsg": "OK",
                "result": {
                    "list": [{
                        "symbol": "BTCUSDT",
                        "riskLimitValue": "100000",
                        "maintenanceMargin": "0.5",
                        "initialMargin": "1.0",
                        "maxLeverage": "100",
                        "mmDeduction": "0"
                    }],
                    "nextPageCursor": ""
                },
                "time": 1704067200000_i64
            })
            .to_string(),
        )
        .create();

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "backtest_bybit.ps",
        "interval 1m\nsource perp = bybit.usdt_perps(\"BTCUSDT\")\ntrigger long_entry = perp.close > perp.close[1]\ntrigger long_exit = perp.close < perp.close[1]\nplot(perp.close)",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BYBIT_BASE_URL", server.url())
        .args([
            "run",
            "backtest",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067440000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--leverage",
            "2",
        ])
        .output()
        .expect("backtest command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["summary"]["trade_count"], Value::from(1));
    assert!(json["orders"].is_array());
}

#[test]
fn run_backtest_rejects_leverage_for_spot_sources() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "spot_backtest.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ntrigger long_entry = spot.close > spot.close[1]\nplot(spot.close)",
    );

    palmscript_cmd()
        .args([
            "run",
            "backtest",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067260000",
            "--leverage",
            "2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "does not accept --leverage or --margin-mode",
        ));
}

#[test]
fn run_backtest_requires_execution_source_for_multi_source_scripts() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "backtest.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\nsource perp = binance.usdm(\"BTCUSDT\")\ntrigger long_entry = spot.close > perp.close\nplot(spot.close - perp.close)",
    );

    let mut cmd = palmscript_cmd();
    cmd.args([
        "run",
        "backtest",
        script.to_str().unwrap(),
        "--from",
        "1704067200000",
        "--to",
        "1704067440000",
    ]);
    cmd.assert().failure().stderr(predicate::str::contains(
        "this mode requires --execution-source when the script declares multiple `source`s",
    ));
}

#[test]
fn run_walk_forward_emits_segmented_json() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0", "14.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0", "15.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0", "16.0"]),
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0", "17.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "walk_forward.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\nentry long = spot.close > spot.close[1]\nentry short = false\nexit long = spot.close < spot.close[1]\nexit short = true\nplot(spot.close)",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "walk-forward",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--train-bars",
            "2",
            "--test-bars",
            "2",
            "--step-bars",
            "2",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
        ])
        .output()
        .expect("walk-forward command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["stitched_summary"]["segment_count"], Value::from(3));
    assert!(json["segments"].is_array());
    assert!(json["segments"][0]["out_of_sample"].is_object());
    assert!(json["segments"][0]["out_of_sample_diagnostics"].is_object());
    assert!(json["segments"][0]["out_of_sample_diagnostics"]["summary"].is_object());
    assert!(json["segments"][0]["out_of_sample_diagnostics"]["capture_summary"].is_object());
    assert!(json["segments"][0]["out_of_sample_diagnostics"]["export_summaries"].is_array());
}

#[test]
fn run_walk_forward_supports_text_output() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0", "14.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0", "15.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0", "16.0"]),
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0", "17.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "walk_forward.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\nentry long = spot.close > spot.close[1]\nentry short = false\nexit long = spot.close < spot.close[1]\nexit short = true\nplot(spot.close)",
    );

    let mut cmd = palmscript_cmd();
    cmd.env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "walk-forward",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--train-bars",
            "2",
            "--test-bars",
            "2",
            "--step-bars",
            "2",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--format",
            "text",
        ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Walk-Forward Summary"))
        .stdout(predicate::str::contains("Walk-Forward Config"))
        .stdout(predicate::str::contains("Recent Segments"))
        .stdout(predicate::str::contains("Worst Segments"));
}

#[test]
fn run_walk_forward_supports_gate_usdt_perps_execution_source() {
    let mut server = Server::new();
    mock_gate_futures_interval(
        &mut server,
        "BTC_USDT",
        "1m",
        &[
            serde_json::json!({"t": 1704067200_i64, "o": "10.0", "h": "11.0", "l": "9.0", "c": "10.0", "v": "10.0", "sum": "100.0"}),
            serde_json::json!({"t": 1704067260_i64, "o": "10.0", "h": "12.0", "l": "9.0", "c": "11.0", "v": "11.0", "sum": "110.0"}),
            serde_json::json!({"t": 1704067320_i64, "o": "11.0", "h": "13.0", "l": "10.0", "c": "12.0", "v": "12.0", "sum": "120.0"}),
            serde_json::json!({"t": 1704067380_i64, "o": "12.0", "h": "12.5", "l": "10.0", "c": "11.0", "v": "13.0", "sum": "130.0"}),
            serde_json::json!({"t": 1704067440_i64, "o": "11.0", "h": "13.0", "l": "10.5", "c": "12.0", "v": "14.0", "sum": "140.0"}),
            serde_json::json!({"t": 1704067500_i64, "o": "12.0", "h": "14.0", "l": "11.5", "c": "13.0", "v": "15.0", "sum": "150.0"}),
            serde_json::json!({"t": 1704067560_i64, "o": "13.0", "h": "13.5", "l": "11.0", "c": "12.0", "v": "16.0", "sum": "160.0"}),
            serde_json::json!({"t": 1704067620_i64, "o": "12.0", "h": "14.0", "l": "11.5", "c": "13.0", "v": "17.0", "sum": "170.0"}),
        ],
    );
    mock_gate_futures_interval(
        &mut server,
        "mark_BTC_USDT",
        "1m",
        &[
            serde_json::json!({"t": 1704067200_i64, "o": "10.0", "h": "11.0", "l": "9.0", "c": "10.0"}),
            serde_json::json!({"t": 1704067260_i64, "o": "10.0", "h": "12.0", "l": "9.0", "c": "11.0"}),
            serde_json::json!({"t": 1704067320_i64, "o": "11.0", "h": "13.0", "l": "10.0", "c": "12.0"}),
            serde_json::json!({"t": 1704067380_i64, "o": "12.0", "h": "12.5", "l": "10.0", "c": "11.0"}),
            serde_json::json!({"t": 1704067440_i64, "o": "11.0", "h": "13.0", "l": "10.5", "c": "12.0"}),
            serde_json::json!({"t": 1704067500_i64, "o": "12.0", "h": "14.0", "l": "11.5", "c": "13.0"}),
            serde_json::json!({"t": 1704067560_i64, "o": "13.0", "h": "13.5", "l": "11.0", "c": "12.0"}),
            serde_json::json!({"t": 1704067620_i64, "o": "12.0", "h": "14.0", "l": "11.5", "c": "13.0"}),
        ],
    );
    server
        .mock("GET", "/futures/usdt/risk_limit_tiers")
        .match_query(Matcher::UrlEncoded("contract".into(), "BTC_USDT".into()))
        .with_status(200)
        .with_body(
            serde_json::json!([{
                "contract": "BTC_USDT",
                "risk_limit": "100000",
                "initial_rate": "0.01",
                "maintenance_rate": "0.005",
                "leverage_max": "100",
                "deduction": "0"
            }])
            .to_string(),
        )
        .create();

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "walk_forward_gate_perp.ps",
        "interval 1m\nsource perp = gate.usdt_perps(\"BTC_USDT\")\nentry long = perp.close > perp.close[1]\nentry short = false\nexit long = perp.close < perp.close[1]\nexit short = true\nplot(perp.close)",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_GATE_BASE_URL", server.url())
        .args([
            "run",
            "walk-forward",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--train-bars",
            "2",
            "--test-bars",
            "2",
            "--step-bars",
            "2",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--leverage",
            "2",
        ])
        .output()
        .expect("walk-forward command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["stitched_summary"]["segment_count"], Value::from(3));
}

#[test]
fn run_walk_forward_sweep_emits_ranked_json() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0", "14.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0", "15.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0", "16.0"]),
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0", "17.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "walk_forward_sweep.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ninput threshold = 0\nentry long = spot.close > spot.close[1] + threshold\nentry short = false\nexit long = spot.close < spot.close[1]\nexit short = true",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "walk-forward-sweep",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--train-bars",
            "2",
            "--test-bars",
            "2",
            "--step-bars",
            "2",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--set",
            "threshold=0,100",
        ])
        .output()
        .expect("walk-forward sweep command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["candidate_count"], Value::from(2));
    assert!(json["best_candidate"].is_object());
    assert!(json["top_candidates"].is_array());
    let best_threshold = json["best_candidate"]["input_overrides"]["threshold"]
        .as_f64()
        .expect("threshold is numeric");
    assert!(best_threshold == 0.0 || best_threshold == 100.0);
}

#[test]
fn run_walk_forward_sweep_supports_text_output() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0", "14.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0", "15.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0", "16.0"]),
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0", "17.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "walk_forward_sweep.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ninput threshold = 0\nentry long = spot.close > spot.close[1] + threshold\nentry short = false\nexit long = spot.close < spot.close[1]\nexit short = true",
    );

    let mut cmd = palmscript_cmd();
    cmd.env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "walk-forward-sweep",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--train-bars",
            "2",
            "--test-bars",
            "2",
            "--step-bars",
            "2",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--set",
            "threshold=0,100",
            "--format",
            "text",
        ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Walk-Forward Sweep Summary"))
        .stdout(predicate::str::contains("Best Candidate"))
        .stdout(predicate::str::contains("Top Candidates"))
        .stdout(predicate::str::contains("threshold=0"));
}

#[test]
fn run_walk_forward_sweep_supports_gate_usdt_perps_execution_source() {
    let mut server = Server::new();
    mock_gate_futures_interval(
        &mut server,
        "BTC_USDT",
        "1m",
        &[
            serde_json::json!({"t": 1704067200_i64, "o": "10.0", "h": "11.0", "l": "9.0", "c": "10.0", "v": "10.0", "sum": "100.0"}),
            serde_json::json!({"t": 1704067260_i64, "o": "10.0", "h": "12.0", "l": "9.0", "c": "11.0", "v": "11.0", "sum": "110.0"}),
            serde_json::json!({"t": 1704067320_i64, "o": "11.0", "h": "13.0", "l": "10.0", "c": "12.0", "v": "12.0", "sum": "120.0"}),
            serde_json::json!({"t": 1704067380_i64, "o": "12.0", "h": "12.5", "l": "10.0", "c": "11.0", "v": "13.0", "sum": "130.0"}),
            serde_json::json!({"t": 1704067440_i64, "o": "11.0", "h": "13.0", "l": "10.5", "c": "12.0", "v": "14.0", "sum": "140.0"}),
            serde_json::json!({"t": 1704067500_i64, "o": "12.0", "h": "14.0", "l": "11.5", "c": "13.0", "v": "15.0", "sum": "150.0"}),
            serde_json::json!({"t": 1704067560_i64, "o": "13.0", "h": "13.5", "l": "11.0", "c": "12.0", "v": "16.0", "sum": "160.0"}),
            serde_json::json!({"t": 1704067620_i64, "o": "12.0", "h": "14.0", "l": "11.5", "c": "13.0", "v": "17.0", "sum": "170.0"}),
        ],
    );
    mock_gate_futures_interval(
        &mut server,
        "mark_BTC_USDT",
        "1m",
        &[
            serde_json::json!({"t": 1704067200_i64, "o": "10.0", "h": "11.0", "l": "9.0", "c": "10.0"}),
            serde_json::json!({"t": 1704067260_i64, "o": "10.0", "h": "12.0", "l": "9.0", "c": "11.0"}),
            serde_json::json!({"t": 1704067320_i64, "o": "11.0", "h": "13.0", "l": "10.0", "c": "12.0"}),
            serde_json::json!({"t": 1704067380_i64, "o": "12.0", "h": "12.5", "l": "10.0", "c": "11.0"}),
            serde_json::json!({"t": 1704067440_i64, "o": "11.0", "h": "13.0", "l": "10.5", "c": "12.0"}),
            serde_json::json!({"t": 1704067500_i64, "o": "12.0", "h": "14.0", "l": "11.5", "c": "13.0"}),
            serde_json::json!({"t": 1704067560_i64, "o": "13.0", "h": "13.5", "l": "11.0", "c": "12.0"}),
            serde_json::json!({"t": 1704067620_i64, "o": "12.0", "h": "14.0", "l": "11.5", "c": "13.0"}),
        ],
    );
    server
        .mock("GET", "/futures/usdt/risk_limit_tiers")
        .match_query(Matcher::UrlEncoded("contract".into(), "BTC_USDT".into()))
        .with_status(200)
        .with_body(
            serde_json::json!([{
                "contract": "BTC_USDT",
                "risk_limit": "100000",
                "initial_rate": "0.01",
                "maintenance_rate": "0.005",
                "leverage_max": "100",
                "deduction": "0"
            }])
            .to_string(),
        )
        .create();

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "walk_forward_sweep_gate_perp.ps",
        "interval 1m\nsource perp = gate.usdt_perps(\"BTC_USDT\")\ninput threshold = 0\nentry long = perp.close > perp.close[1] + threshold\nentry short = false\nexit long = perp.close < perp.close[1]\nexit short = true",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_GATE_BASE_URL", server.url())
        .args([
            "run",
            "walk-forward-sweep",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--train-bars",
            "2",
            "--test-bars",
            "2",
            "--step-bars",
            "2",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--leverage",
            "2",
            "--set",
            "threshold=0,100",
        ])
        .output()
        .expect("walk-forward sweep command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["candidate_count"], Value::from(2));
}

#[test]
fn run_optimize_emits_ranked_json() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0", "14.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0", "15.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0", "16.0"]),
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0", "17.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "optimize.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ninput threshold = 0\nentry long = spot.close > spot.close[1] + threshold\nentry short = false\nexit long = spot.close < spot.close[1]\nexit short = true",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "optimize",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--runner",
            "backtest",
            "--param",
            "choice:threshold=0,100",
            "--trials",
            "8",
            "--startup-trials",
            "8",
            "--seed",
            "7",
            "--workers",
            "2",
        ])
        .output()
        .expect("optimize command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["candidate_count"], Value::from(8));
    assert_eq!(json["completed_trials"], Value::from(8));
    assert!(json["best_candidate"].is_object());
    assert!(json["top_candidates"].is_array());
    let best_threshold = json["best_candidate"]["input_overrides"]["threshold"]
        .as_f64()
        .expect("threshold is numeric");
    assert!(best_threshold == 0.0 || best_threshold == 100.0);
}

#[test]
fn run_optimize_infers_params_from_input_metadata() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0", "14.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0", "15.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0", "16.0"]),
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0", "17.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "optimize_metadata.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ninput threshold = 0 optimize(choice, 0, 100)\nentry long = spot.close > spot.close[1] + threshold\nentry short = false\nexit long = spot.close < spot.close[1]\nexit short = true",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "optimize",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--runner",
            "backtest",
            "--trials",
            "8",
            "--startup-trials",
            "8",
            "--seed",
            "7",
            "--workers",
            "2",
        ])
        .output()
        .expect("optimize command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(
        json["config"]["params"][0]["space_kind"],
        Value::from("choice")
    );
    assert_eq!(
        json["config"]["params"][0]["name"],
        Value::from("threshold")
    );
}

#[test]
fn run_optimize_accepts_step_syntax_in_param_ranges() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0", "14.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0", "15.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0", "16.0"]),
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0", "17.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "optimize_step.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ninput threshold = 0\nentry long = spot.close > spot.close[1] + threshold\nentry short = false\nexit long = spot.close < spot.close[1]\nexit short = true",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "optimize",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--runner",
            "backtest",
            "--param",
            "int:threshold=0:4:2",
            "--trials",
            "8",
            "--startup-trials",
            "8",
            "--seed",
            "7",
            "--workers",
            "2",
        ])
        .output()
        .expect("optimize command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["config"]["params"][0]["step"], Value::from(2));
}

#[test]
fn run_optimize_walk_forward_defaults_to_final_holdout() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0", "14.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0", "15.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0", "16.0"]),
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0", "17.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "optimize.ps", optimize_script());

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "optimize",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--train-bars",
            "2",
            "--test-bars",
            "2",
            "--step-bars",
            "2",
            "--param",
            "choice:threshold=0,100",
            "--trials",
            "8",
            "--startup-trials",
            "8",
            "--seed",
            "7",
            "--workers",
            "2",
        ])
        .output()
        .expect("optimize command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["config"]["holdout"]["bars"], Value::from(2));
    assert_eq!(json["holdout"]["bars"], Value::from(2));
    assert!(json["holdout"]["summary"]["trade_count"].is_number());
}

#[test]
fn run_optimize_supports_text_output_and_presets() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0", "14.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0", "15.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0", "16.0"]),
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0", "17.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "optimize.ps",
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ninput threshold = 0\nentry long = spot.close > spot.close[1] + threshold\nentry short = false\nexit long = spot.close < spot.close[1]\nexit short = true",
    );
    let preset = dir.path().join("best.json");

    let mut optimize = palmscript_cmd();
    optimize
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "optimize",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--runner",
            "backtest",
            "--param",
            "choice:threshold=0,100",
            "--trials",
            "8",
            "--startup-trials",
            "8",
            "--seed",
            "7",
            "--workers",
            "2",
            "--preset-out",
            preset.to_str().unwrap(),
            "--format",
            "text",
        ]);
    optimize
        .assert()
        .success()
        .stdout(predicate::str::contains("Optimization Summary"))
        .stdout(predicate::str::contains("Best Candidate"))
        .stdout(predicate::str::contains("Top Candidates"))
        .stdout(predicate::str::contains("Holdout").not())
        .stdout(predicate::str::contains("preset_out="));

    let preset_json: Value =
        serde_json::from_str(&fs::read_to_string(&preset).expect("preset readable"))
            .expect("preset is json");
    let best_threshold = preset_json["best_input_overrides"]["threshold"]
        .as_f64()
        .expect("threshold is numeric");
    assert!(best_threshold == 0.0 || best_threshold == 100.0);

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .args([
            "run",
            "backtest",
            script.to_str().unwrap(),
            "--preset",
            preset.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
        ])
        .output()
        .expect("backtest with preset executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert!(
        json["summary"]["ending_equity"]
            .as_f64()
            .unwrap_or_default()
            >= 1000.0
    );
}

#[test]
fn run_optimize_supports_bybit_usdt_perps_execution_source() {
    let mut server = Server::new();
    mock_bybit_kline(
        &mut server,
        "/v5/market/kline",
        "linear",
        "BTCUSDT",
        "1",
        &[
            serde_json::json!([
                1704067620000_i64,
                "12.0",
                "14.0",
                "11.5",
                "13.0",
                "17.0",
                "0"
            ]),
            serde_json::json!([
                1704067560000_i64,
                "13.0",
                "13.5",
                "11.0",
                "12.0",
                "16.0",
                "0"
            ]),
            serde_json::json!([
                1704067500000_i64,
                "12.0",
                "14.0",
                "11.5",
                "13.0",
                "15.0",
                "0"
            ]),
            serde_json::json!([
                1704067440000_i64,
                "11.0",
                "13.0",
                "10.5",
                "12.0",
                "14.0",
                "0"
            ]),
            serde_json::json!([
                1704067380000_i64,
                "12.0",
                "12.5",
                "10.0",
                "11.0",
                "13.0",
                "0"
            ]),
            serde_json::json!([
                1704067320000_i64,
                "11.0",
                "13.0",
                "10.0",
                "12.0",
                "12.0",
                "0"
            ]),
            serde_json::json!([
                1704067260000_i64,
                "10.0",
                "12.0",
                "9.0",
                "11.0",
                "11.0",
                "0"
            ]),
            serde_json::json!([
                1704067200000_i64,
                "10.0",
                "11.0",
                "9.0",
                "10.0",
                "10.0",
                "0"
            ]),
        ],
    );
    mock_bybit_kline(
        &mut server,
        "/v5/market/mark-price-kline",
        "linear",
        "BTCUSDT",
        "1",
        &[
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0"]),
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0"]),
        ],
    );
    server
        .mock("GET", "/v5/market/risk-limit")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("category".into(), "linear".into()),
            Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
        ]))
        .with_status(200)
        .with_body(
            serde_json::json!({
                "retCode": 0,
                "retMsg": "OK",
                "result": {
                    "list": [{
                        "symbol": "BTCUSDT",
                        "riskLimitValue": "100000",
                        "maintenanceMargin": "0.5",
                        "initialMargin": "1.0",
                        "maxLeverage": "100",
                        "mmDeduction": "0"
                    }],
                    "nextPageCursor": ""
                },
                "time": 1704067200000_i64
            })
            .to_string(),
        )
        .create();

    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "optimize_bybit_perp.ps",
        "interval 1m\nsource perp = bybit.usdt_perps(\"BTCUSDT\")\ninput threshold = 0\nentry long = perp.close > perp.close[1] + threshold\nentry short = false\nexit long = perp.close < perp.close[1]\nexit short = true",
    );

    let output = palmscript_cmd()
        .env("PALMSCRIPT_BYBIT_BASE_URL", server.url())
        .args([
            "run",
            "optimize",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--runner",
            "backtest",
            "--leverage",
            "2",
            "--param",
            "choice:threshold=0,100",
            "--trials",
            "8",
            "--startup-trials",
            "8",
            "--seed",
            "7",
            "--workers",
            "2",
        ])
        .output()
        .expect("optimize command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["candidate_count"], Value::from(8));
}

#[test]
fn runs_submit_status_and_serve_persist_optimize_job() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0", "14.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0", "15.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0", "16.0"]),
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0", "17.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "optimize.ps", optimize_script());
    let state_dir = dir.path().join("runs-state");
    let preset_out = dir.path().join("best.json");

    let submit = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .env("PALMSCRIPT_RUNS_STATE_DIR", &state_dir)
        .env("PALMSCRIPT_RUNS_NO_AUTOSTART", "1")
        .args([
            "runs",
            "submit",
            "optimize",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--runner",
            "backtest",
            "--param",
            "choice:threshold=0,100",
            "--trials",
            "8",
            "--startup-trials",
            "8",
            "--seed",
            "7",
            "--workers",
            "2",
        ])
        .output()
        .expect("runs submit executes");
    assert!(submit.status.success());
    let run_id = extract_key_value(&submit, "run_id").expect("run id");
    let artifact_dir = extract_key_value(&submit, "artifact_dir").expect("artifact dir");

    let status = palmscript_cmd()
        .env("PALMSCRIPT_RUNS_STATE_DIR", &state_dir)
        .args(["runs", "status", &run_id])
        .output()
        .expect("runs status executes");
    assert!(status.status.success());
    let status_stdout = stdout_string(&status);
    assert!(status_stdout.contains("status=queued"));
    assert!(status_stdout.contains("progress=0/8"));

    let serve = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .env("PALMSCRIPT_RUNS_STATE_DIR", &state_dir)
        .env("PALMSCRIPT_RUNS_NO_AUTOSTART", "1")
        .args(["runs", "serve", "--once"])
        .output()
        .expect("runs serve executes");
    assert!(serve.status.success());

    let show = palmscript_cmd()
        .env("PALMSCRIPT_RUNS_STATE_DIR", &state_dir)
        .args(["runs", "show", &run_id])
        .output()
        .expect("runs show executes");
    assert!(show.status.success());
    let show_stdout = stdout_string(&show);
    assert!(show_stdout.contains("status=completed"));
    assert!(show_stdout.contains("completed_trials=8"));

    let best = palmscript_cmd()
        .env("PALMSCRIPT_RUNS_STATE_DIR", &state_dir)
        .args([
            "runs",
            "best",
            &run_id,
            "--preset-out",
            preset_out.to_str().unwrap(),
        ])
        .output()
        .expect("runs best executes");
    assert!(best.status.success());
    assert!(preset_out.exists());
    let preset_json: Value =
        serde_json::from_str(&fs::read_to_string(&preset_out).expect("preset readable"))
            .expect("preset is json");
    assert!(preset_json["best_input_overrides"]["threshold"].is_number());

    let artifact_path = PathBuf::from(&artifact_dir);
    assert!(artifact_path.join("manifest.json").exists());
    assert!(artifact_path.join("events.jsonl").exists());
    assert!(artifact_path.join("result.json").exists());
    assert!(artifact_path.join("best_preset.json").exists());
}

#[test]
fn runs_cancel_and_resume_requeue_optimize_job() {
    let mut server = Server::new();
    mock_binance_interval(
        &mut server,
        "1m",
        &[
            serde_json::json!([1704067200000_i64, "10.0", "11.0", "9.0", "10.0", "10.0"]),
            serde_json::json!([1704067260000_i64, "10.0", "12.0", "9.0", "11.0", "11.0"]),
            serde_json::json!([1704067320000_i64, "11.0", "13.0", "10.0", "12.0", "12.0"]),
            serde_json::json!([1704067380000_i64, "12.0", "12.5", "10.0", "11.0", "13.0"]),
            serde_json::json!([1704067440000_i64, "11.0", "13.0", "10.5", "12.0", "14.0"]),
            serde_json::json!([1704067500000_i64, "12.0", "14.0", "11.5", "13.0", "15.0"]),
            serde_json::json!([1704067560000_i64, "13.0", "13.5", "11.0", "12.0", "16.0"]),
            serde_json::json!([1704067620000_i64, "12.0", "14.0", "11.5", "13.0", "17.0"]),
        ],
    );

    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "optimize.ps", optimize_script());
    let state_dir = dir.path().join("runs-state");

    let submit = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .env("PALMSCRIPT_RUNS_STATE_DIR", &state_dir)
        .env("PALMSCRIPT_RUNS_NO_AUTOSTART", "1")
        .args([
            "runs",
            "submit",
            "optimize",
            script.to_str().unwrap(),
            "--from",
            "1704067200000",
            "--to",
            "1704067680000",
            "--initial-capital",
            "1000",
            "--fee-bps",
            "0",
            "--slippage-bps",
            "0",
            "--runner",
            "backtest",
            "--param",
            "choice:threshold=0,100",
            "--trials",
            "8",
            "--startup-trials",
            "8",
            "--seed",
            "7",
            "--workers",
            "2",
        ])
        .output()
        .expect("runs submit executes");
    assert!(submit.status.success());
    let run_id = extract_key_value(&submit, "run_id").expect("run id");

    let cancel = palmscript_cmd()
        .env("PALMSCRIPT_RUNS_STATE_DIR", &state_dir)
        .args(["runs", "cancel", &run_id])
        .output()
        .expect("runs cancel executes");
    assert!(cancel.status.success());
    assert!(stdout_string(&cancel).contains("status=canceled"));

    let resume = palmscript_cmd()
        .env("PALMSCRIPT_RUNS_STATE_DIR", &state_dir)
        .env("PALMSCRIPT_RUNS_NO_AUTOSTART", "1")
        .args(["runs", "resume", &run_id])
        .output()
        .expect("runs resume executes");
    assert!(resume.status.success());
    assert!(stdout_string(&resume).contains("status=queued"));

    let serve = palmscript_cmd()
        .env("PALMSCRIPT_BINANCE_SPOT_BASE_URL", server.url())
        .env("PALMSCRIPT_RUNS_STATE_DIR", &state_dir)
        .env("PALMSCRIPT_RUNS_NO_AUTOSTART", "1")
        .args(["runs", "serve", "--once"])
        .output()
        .expect("runs serve executes");
    assert!(serve.status.success());

    let status = palmscript_cmd()
        .env("PALMSCRIPT_RUNS_STATE_DIR", &state_dir)
        .args(["runs", "status", &run_id])
        .output()
        .expect("runs status executes");
    assert!(status.status.success());
    assert!(stdout_string(&status).contains("status=completed"));
}
