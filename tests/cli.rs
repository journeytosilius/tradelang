use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::tempdir;

fn write_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).expect("writes test file");
    path
}

fn bars_csv(rows: &[&str]) -> String {
    let mut csv = String::from("time,open,high,low,close,volume\n");
    for row in rows {
        csv.push_str(row);
        csv.push('\n');
    }
    csv
}

fn tradelang_cmd() -> std::process::Command {
    std::process::Command::new(assert_cmd::cargo::cargo_bin!("tradelang"))
}

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn help_prints_usage() {
    let mut cmd = tradelang_cmd();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("check"))
        .stdout(predicate::str::contains("dump-bytecode"));
}

#[test]
fn run_requires_bars_argument() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "plot(close)");
    let mut cmd = tradelang_cmd();
    cmd.args(["run", script.to_str().unwrap()]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--bars"));
}

#[test]
fn run_rejects_invalid_base_interval() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "plot(close)");
    let bars = write_file(
        dir.path(),
        "bars.csv",
        &bars_csv(&["1704067200000,1,2,0.5,1.5,10"]),
    );
    let mut cmd = tradelang_cmd();
    cmd.args([
        "run",
        script.to_str().unwrap(),
        "--bars",
        bars.to_str().unwrap(),
        "--base-interval",
        "1W",
    ]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid interval `1W`"));
}

#[test]
fn run_rejects_malformed_feed_argument() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "plot(1w.close)");
    let bars = write_file(
        dir.path(),
        "bars.csv",
        &bars_csv(&["1704067200000,1,2,0.5,1.5,10"]),
    );
    let mut cmd = tradelang_cmd();
    cmd.args([
        "run",
        script.to_str().unwrap(),
        "--bars",
        bars.to_str().unwrap(),
        "--base-interval",
        "1d",
        "--feed",
        "1w",
    ]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("feed must use <interval=path>"));
}

#[test]
fn check_reports_success_for_valid_script() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "valid.trl", "plot(sma(close, 3))");
    let mut cmd = tradelang_cmd();
    cmd.args(["check", script.to_str().unwrap()]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("valid.trl: ok"));
}

#[test]
fn check_reports_compile_diagnostics() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "invalid.trl", "if true { plot(1) }");
    let mut cmd = tradelang_cmd();
    cmd.args(["check", script.to_str().unwrap()]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("expected `else` after `if` block"));
}

#[test]
fn check_supports_compile_environment_files() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "consumer.trl",
        "if trend { plot(1) } else { plot(0) }",
    );
    let env = write_file(
        dir.path(),
        "env.json",
        r#"{"external_inputs":[{"name":"trend","ty":"SeriesBool","kind":"ExportSeries"}]}"#,
    );
    let mut cmd = tradelang_cmd();
    cmd.args([
        "check",
        script.to_str().unwrap(),
        "--env",
        env.to_str().unwrap(),
    ]);
    cmd.assert().success();
}

#[test]
fn run_executes_single_interval_script_and_prints_json_by_default() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "plot(close[1])");
    let bars = write_file(
        dir.path(),
        "bars.csv",
        &bars_csv(&[
            "1704067200000,1,2,0.5,1.5,10",
            "1704067260000,2,3,1.5,2.5,11",
        ]),
    );
    let output = tradelang_cmd()
        .args([
            "run",
            script.to_str().unwrap(),
            "--bars",
            bars.to_str().unwrap(),
            "--base-interval",
            "1m",
        ])
        .output()
        .expect("run command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["plots"][0]["points"][0]["value"], Value::Null);
    assert_eq!(json["plots"][0]["points"][1]["value"], Value::from(1.5));
}

#[test]
fn run_executes_multi_interval_script() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "plot(1w.close)");
    let base = write_file(
        dir.path(),
        "base.csv",
        &bars_csv(&[
            "1704067200000,1,2,0.5,1.5,10",
            "1704153600000,1,2,0.5,1.5,10",
            "1704240000000,1,2,0.5,1.5,10",
            "1704326400000,1,2,0.5,1.5,10",
            "1704412800000,1,2,0.5,1.5,10",
            "1704499200000,1,2,0.5,1.5,10",
            "1704585600000,1,2,0.5,1.5,10",
        ]),
    );
    let weekly = write_file(
        dir.path(),
        "weekly.csv",
        &bars_csv(&["1704067200000,9,11,8,10,100"]),
    );
    let output = tradelang_cmd()
        .args([
            "run",
            script.to_str().unwrap(),
            "--bars",
            base.to_str().unwrap(),
            "--base-interval",
            "1d",
            "--feed",
            &format!("1w={}", weekly.display()),
        ])
        .output()
        .expect("run command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["plots"][0]["points"][5]["value"], Value::Null);
    assert_eq!(json["plots"][0]["points"][6]["value"], Value::from(10.0));
}

#[test]
fn run_reports_missing_supplemental_feed_errors() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "plot(1w.close)");
    let base = write_file(
        dir.path(),
        "base.csv",
        &bars_csv(&["1704067200000,1,2,0.5,1.5,10"]),
    );
    let mut cmd = tradelang_cmd();
    cmd.args([
        "run",
        script.to_str().unwrap(),
        "--bars",
        base.to_str().unwrap(),
        "--base-interval",
        "1d",
    ]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("missing interval feed"));
}

#[test]
fn run_rejects_invalid_csv_rows() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "plot(close)");
    let bars = write_file(
        dir.path(),
        "bars.csv",
        "time,open,high,low,close,volume\n1704067200000,1,2,0.5,1.5\n",
    );
    let mut cmd = tradelang_cmd();
    cmd.args([
        "run",
        script.to_str().unwrap(),
        "--bars",
        bars.to_str().unwrap(),
        "--base-interval",
        "1m",
    ]);
    cmd.assert().failure().stderr(predicate::str::contains(
        "must contain 6 comma-separated fields",
    ));
}

#[test]
fn run_rejects_invalid_timestamps() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "plot(close)");
    let bars = write_file(
        dir.path(),
        "bars.csv",
        "time,open,high,low,close,volume\n1704067200000.5,1,2,0.5,1.5,10\n",
    );
    let mut cmd = tradelang_cmd();
    cmd.args([
        "run",
        script.to_str().unwrap(),
        "--bars",
        bars.to_str().unwrap(),
        "--base-interval",
        "1m",
    ]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid `time` value"));
}

#[test]
fn run_supports_text_output() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "script.trl",
        "export rising = close > close[1]\ntrigger long = close > open\nplot(close)",
    );
    let bars = write_file(
        dir.path(),
        "bars.csv",
        &bars_csv(&[
            "1704067200000,1,2,0.5,1.5,10",
            "1704067260000,2,3,1.5,2.5,11",
        ]),
    );
    let mut cmd = tradelang_cmd();
    cmd.args([
        "run",
        script.to_str().unwrap(),
        "--bars",
        bars.to_str().unwrap(),
        "--base-interval",
        "1m",
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
fn dump_bytecode_text_contains_sections() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "plot(sma(close, 3))");
    let mut cmd = tradelang_cmd();
    cmd.args(["dump-bytecode", script.to_str().unwrap()]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Constants"))
        .stdout(predicate::str::contains("Locals"))
        .stdout(predicate::str::contains("Instructions"));
}

#[test]
fn dump_bytecode_json_serializes_compiled_program() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "plot(close)");
    let output = tradelang_cmd()
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
}

#[test]
fn dump_bytecode_supports_compile_environment_files() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "consumer.trl",
        "if trend { plot(1) } else { plot(0) }",
    );
    let env = write_file(
        dir.path(),
        "env.json",
        r#"{"external_inputs":[{"name":"trend","ty":"SeriesBool","kind":"ExportSeries"}]}"#,
    );
    let output = tradelang_cmd()
        .args([
            "dump-bytecode",
            script.to_str().unwrap(),
            "--env",
            env.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("dump-bytecode executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(
        json["program"]["external_inputs"][0]["name"],
        Value::from("trend")
    );
}

#[test]
fn checked_in_single_interval_example_runs_via_cli() {
    let output = tradelang_cmd()
        .args([
            "run",
            repo_path("examples/strategies/sma_cross.trl")
                .to_str()
                .unwrap(),
            "--bars",
            repo_path("examples/data/minute_bars.csv").to_str().unwrap(),
            "--base-interval",
            "1m",
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
    let output = tradelang_cmd()
        .args([
            "run",
            repo_path("examples/strategies/weekly_bias.trl")
                .to_str()
                .unwrap(),
            "--bars",
            repo_path("examples/data/daily_bars.csv").to_str().unwrap(),
            "--base-interval",
            "1d",
            "--feed",
            &format!(
                "1w={}",
                repo_path("examples/data/weekly_bars.csv").display()
            ),
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
