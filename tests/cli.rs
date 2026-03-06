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

fn palmscript_cmd() -> std::process::Command {
    std::process::Command::new(assert_cmd::cargo::cargo_bin!("palmscript"))
}

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn help_prints_usage() {
    let mut cmd = palmscript_cmd();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("check"))
        .stdout(predicate::str::contains("dump-bytecode"));
}

#[test]
fn run_help_mentions_csv_mode() {
    let mut cmd = palmscript_cmd();
    cmd.args(["run", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("csv"));
}

#[test]
fn run_requires_bars_argument() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "interval 1m\nplot(close)");
    let mut cmd = palmscript_cmd();
    cmd.args(["run", "csv", script.to_str().unwrap()]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--bars"));
}

#[test]
fn run_rejects_missing_interval_directive() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "plot(close)");
    let bars = write_file(
        dir.path(),
        "bars.csv",
        &bars_csv(&["1704067200000,1,2,0.5,1.5,10"]),
    );
    let mut cmd = palmscript_cmd();
    cmd.args([
        "run",
        "csv",
        script.to_str().unwrap(),
        "--bars",
        bars.to_str().unwrap(),
    ]);
    cmd.assert().failure().stderr(predicate::str::contains(
        "strategy must declare exactly one `interval <...>` directive",
    ));
}

#[test]
fn run_rejects_removed_feed_argument() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "script.trl",
        "interval 1d\nuse 1w\nplot(1w.close)",
    );
    let bars = write_file(
        dir.path(),
        "bars.csv",
        &bars_csv(&["1704067200000,1,2,0.5,1.5,10"]),
    );
    let mut cmd = palmscript_cmd();
    cmd.args([
        "run",
        "csv",
        script.to_str().unwrap(),
        "--bars",
        bars.to_str().unwrap(),
        "--feed",
        "1w=weekly.csv",
    ]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument '--feed'"));
}

#[test]
fn check_reports_success_for_valid_script() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "valid.trl", "interval 1m\nplot(sma(close, 3))");
    let mut cmd = palmscript_cmd();
    cmd.args(["check", script.to_str().unwrap()]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("valid.trl: ok"));
}

#[test]
fn check_reports_compile_diagnostics() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "invalid.trl",
        "interval 1m\nif true { plot(1) }",
    );
    let mut cmd = palmscript_cmd();
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
        "interval 1m\nif trend { plot(1) } else { plot(0) }",
    );
    let env = write_file(
        dir.path(),
        "env.json",
        r#"{"external_inputs":[{"name":"trend","ty":"SeriesBool","kind":"ExportSeries"}]}"#,
    );
    let mut cmd = palmscript_cmd();
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
    let script = write_file(dir.path(), "script.trl", "interval 1m\nplot(close[1])");
    let bars = write_file(
        dir.path(),
        "bars.csv",
        &bars_csv(&[
            "1704067200000,1,2,0.5,1.5,10",
            "1704067260000,2,3,1.5,2.5,11",
        ]),
    );
    let output = palmscript_cmd()
        .args([
            "run",
            "csv",
            script.to_str().unwrap(),
            "--bars",
            bars.to_str().unwrap(),
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
    let script = write_file(
        dir.path(),
        "script.trl",
        "interval 1d\nuse 1w\nplot(1w.close)",
    );
    let base = write_file(
        dir.path(),
        "base.csv",
        &bars_csv(&[
            "1704067200000,1,2,0.5,1.0,10",
            "1704153600000,1,2,0.5,2.0,10",
            "1704240000000,1,2,0.5,3.0,10",
            "1704326400000,1,2,0.5,4.0,10",
            "1704412800000,1,2,0.5,5.0,10",
            "1704499200000,1,2,0.5,6.0,10",
            "1704585600000,1,2,0.5,10.0,10",
        ]),
    );
    let output = palmscript_cmd()
        .args([
            "run",
            "csv",
            script.to_str().unwrap(),
            "--bars",
            base.to_str().unwrap(),
        ])
        .output()
        .expect("run command executes");
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout is json");
    assert_eq!(json["plots"][0]["points"][5]["value"], Value::Null);
    assert_eq!(json["plots"][0]["points"][6]["value"], Value::from(10.0));
}

#[test]
fn run_rejects_incomplete_rollup_bucket() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "script.trl",
        "interval 1d\nuse 1w\nplot(1w.close)",
    );
    let base = write_file(
        dir.path(),
        "base.csv",
        &bars_csv(&[
            "1704067200000,1,2,0.5,1.0,10",
            "1704153600000,1,2,0.5,2.0,10",
            "1704240000000,1,2,0.5,3.0,10",
            "1704326400000,1,2,0.5,4.0,10",
            "1704412800000,1,2,0.5,5.0,10",
            "1704499200000,1,2,0.5,6.0,10",
        ]),
    );
    let mut cmd = palmscript_cmd();
    cmd.args([
        "run",
        "csv",
        script.to_str().unwrap(),
        "--bars",
        base.to_str().unwrap(),
    ]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("CSV mode error"))
        .stderr(predicate::str::contains("incomplete rollup bucket"));
}

#[test]
fn run_rejects_raw_input_that_is_too_coarse() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "interval 1m\nplot(close)");
    let bars = write_file(
        dir.path(),
        "bars.csv",
        &bars_csv(&[
            "1704067200000,1,2,0.5,1.5,10",
            "1704153600000,2,3,1.5,2.5,11",
        ]),
    );
    let mut cmd = palmscript_cmd();
    cmd.args([
        "run",
        "csv",
        script.to_str().unwrap(),
        "--bars",
        bars.to_str().unwrap(),
    ]);
    cmd.assert().failure().stderr(predicate::str::contains(
        "raw input interval Day1 is too coarse",
    ));
}

#[test]
fn run_rejects_invalid_csv_rows() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "interval 1m\nplot(close)");
    let bars = write_file(
        dir.path(),
        "bars.csv",
        "time,open,high,low,close,volume\n1704067200000,1,2,0.5,1.5\n",
    );
    let mut cmd = palmscript_cmd();
    cmd.args([
        "run",
        "csv",
        script.to_str().unwrap(),
        "--bars",
        bars.to_str().unwrap(),
    ]);
    cmd.assert().failure().stderr(predicate::str::contains(
        "must contain 6 comma-separated fields",
    ));
}

#[test]
fn run_rejects_invalid_timestamps() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(dir.path(), "script.trl", "interval 1m\nplot(close)");
    let bars = write_file(
        dir.path(),
        "bars.csv",
        "time,open,high,low,close,volume\n1704067200000.5,1,2,0.5,1.5,10\n",
    );
    let mut cmd = palmscript_cmd();
    cmd.args([
        "run",
        "csv",
        script.to_str().unwrap(),
        "--bars",
        bars.to_str().unwrap(),
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
        "interval 1m\nexport rising = close > close[1]\ntrigger long = close > open\nplot(close)",
    );
    let bars = write_file(
        dir.path(),
        "bars.csv",
        &bars_csv(&[
            "1704067200000,1,2,0.5,1.5,10",
            "1704067260000,2,3,1.5,2.5,11",
        ]),
    );
    let mut cmd = palmscript_cmd();
    cmd.args([
        "run",
        "csv",
        script.to_str().unwrap(),
        "--bars",
        bars.to_str().unwrap(),
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
    let script = write_file(dir.path(), "script.trl", "interval 1m\nplot(sma(close, 3))");
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
    let script = write_file(dir.path(), "script.trl", "interval 1m\nplot(close)");
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
}

#[test]
fn dump_bytecode_supports_compile_environment_files() {
    let dir = tempdir().expect("tempdir");
    let script = write_file(
        dir.path(),
        "consumer.trl",
        "interval 1m\nif trend { plot(1) } else { plot(0) }",
    );
    let env = write_file(
        dir.path(),
        "env.json",
        r#"{"external_inputs":[{"name":"trend","ty":"SeriesBool","kind":"ExportSeries"}]}"#,
    );
    let output = palmscript_cmd()
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
    let output = palmscript_cmd()
        .args([
            "run",
            "csv",
            repo_path("examples/strategies/sma_cross.trl")
                .to_str()
                .unwrap(),
            "--bars",
            repo_path("examples/data/minute_bars.csv").to_str().unwrap(),
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
    let dir = tempdir().expect("tempdir");
    let bars = write_file(
        dir.path(),
        "daily_bars.csv",
        "time,open,high,low,close,volume\n\
1704067200000,100.0,101.0,99.0,100.5,1000.0\n\
1704153600000,100.5,101.5,100.0,101.0,1010.0\n\
1704240000000,101.0,102.0,100.5,101.5,1020.0\n\
1704326400000,101.5,102.5,101.0,102.0,1030.0\n\
1704412800000,102.0,103.0,101.5,102.5,1040.0\n\
1704499200000,102.5,103.5,102.0,103.0,1050.0\n\
1704585600000,103.0,104.0,102.5,103.5,1060.0\n\
1704672000000,103.5,104.5,103.0,104.0,1070.0\n\
1704758400000,104.0,105.0,103.5,104.5,1080.0\n\
1704844800000,104.5,105.5,104.0,105.0,1090.0\n\
1704931200000,105.0,106.0,104.5,105.5,1100.0\n\
1705017600000,105.5,106.5,105.0,106.0,1110.0\n\
1705104000000,106.0,107.0,105.5,106.5,1120.0\n\
1705190400000,106.5,107.5,106.0,107.0,1130.0\n",
    );
    let output = palmscript_cmd()
        .args([
            "run",
            "csv",
            repo_path("examples/strategies/weekly_bias.trl")
                .to_str()
                .unwrap(),
            "--bars",
            bars.to_str().unwrap(),
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
