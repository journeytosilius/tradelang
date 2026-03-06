use palmscript::{
    compile, compile_with_env, run, run_multi_interval, Bar, CompileEnvironment, ExternalInputDecl,
    ExternalInputKind, Interval, IntervalFeed, MultiIntervalConfig, PipelineEdge, PipelineEngine,
    PipelineNodeSpec, PipelineSpec, Type, VmLimits,
};
use serde_json::json;

fn with_interval(source: &str) -> String {
    format!("interval 1m\n{source}")
}

fn with_intervals(base: &str, supplemental: &[&str], source: &str) -> String {
    let mut script = format!("interval {base}\n");
    for interval in supplemental {
        script.push_str("use ");
        script.push_str(interval);
        script.push('\n');
    }
    script.push_str(source);
    script
}

fn fixture_bars() -> Vec<Bar> {
    (0..20)
        .map(|index| {
            let close = 100.0 + index as f64;
            Bar {
                open: close - 0.5,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1_000.0 + index as f64,
                time: 1_700_000_000_000.0 + index as f64 * 60_000.0,
            }
        })
        .collect()
}

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;
const WEEK_MS: i64 = 7 * DAY_MS;
const JAN_1_2024_UTC_MS: i64 = 1_704_067_200_000;
const FEB_1_2024_UTC_MS: i64 = 1_706_745_600_000;
const MAR_1_2024_UTC_MS: i64 = 1_709_251_200_000;

fn bars_with_spacing(start_ms: i64, spacing_ms: i64, closes: &[f64]) -> Vec<Bar> {
    closes
        .iter()
        .enumerate()
        .map(|(index, close)| Bar {
            open: *close - 0.5,
            high: *close + 1.0,
            low: *close - 1.0,
            close: *close,
            volume: 1_000.0 + index as f64,
            time: (start_ms + spacing_ms * index as i64) as f64,
        })
        .collect()
}

#[test]
fn golden_sma_shape_matches() {
    let compiled = compile(&with_interval("plot(sma(close, 14))")).expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(json["plots"][0]["id"], json!(0));
    assert_eq!(json["plots"][0]["points"].as_array().unwrap().len(), 20);
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::Value::Null
    );
    assert!(json["plots"][0]["points"][13]["value"].is_number());
}

#[test]
fn golden_close_index_shape_matches() {
    let compiled = compile(&with_interval("plot(close[1])")).expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::Value::Null
    );
    assert!(json["plots"][0]["points"][1]["value"].is_number());
}

#[test]
fn golden_if_else_shape_matches() {
    let compiled = compile(&with_interval(
        "if close > sma(close, 14) { plot(1) } else { plot(0) }",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(json["plots"][0]["points"].as_array().unwrap().len(), 20);
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(0.0)
    );
    assert!(json["plots"][0]["points"][14]["value"].is_number());
}

#[test]
fn golden_logical_and_shape_matches() {
    let compiled = compile(&with_interval(
        "if close > ema(close, 3) and rsi(close, 3) > 50 { plot(1) } else { plot(0) }",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        json["plots"][0]["points"][1]["value"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        json["plots"][0]["points"][3]["value"],
        serde_json::json!(1.0)
    );
}

#[test]
fn golden_else_if_chain_shape_matches() {
    let compiled = compile(&with_interval(
        "if close < ema(close, 3) { plot(-1) } else if close > ema(close, 3) or close > close[1] { plot(1) } else { plot(0) }",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        json["plots"][0]["points"][1]["value"],
        serde_json::json!(1.0)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(1.0)
    );
}

#[test]
fn golden_boolean_series_reuse_shape_matches() {
    let compiled = compile(&with_interval(
        "let rising = close > close[1]\nif rising[1] or false { plot(1) } else { plot(0) }",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        json["plots"][0]["points"][1]["value"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        json["plots"][0]["points"][2]["value"],
        serde_json::json!(1.0)
    );
}

#[test]
fn golden_zero_argument_function_shape_matches() {
    let compiled = compile(&with_interval(
        "fn bullish_bar() = close > open\nif bullish_bar() { plot(1) } else { plot(0) }",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(1.0)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(1.0)
    );
}

#[test]
fn golden_function_indexing_over_series_shape_matches() {
    let compiled = compile(&with_interval(
        "fn rising(series) = series > series[1]\nif rising(close) { plot(1) } else { plot(0) }",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        json["plots"][0]["points"][1]["value"],
        serde_json::json!(1.0)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(1.0)
    );
}

#[test]
fn golden_nested_function_helpers_shape_matches() {
    let compiled = compile(&with_interval(
        "fn bullish_bar() = close > open\nfn signal() = bullish_bar() and close > close[1]\nif signal() { plot(1) } else { plot(0) }",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        json["plots"][0]["points"][1]["value"],
        serde_json::json!(1.0)
    );
}

#[test]
fn golden_indicator_helper_shape_matches() {
    let compiled = compile(&with_interval(
        "fn crossover(a, b) = a > b and a[1] <= b[1]\nlet fast = ema(close, 3)\nlet slow = ema(close, 5)\nif crossover(fast, slow) { plot(1) } else { plot(0) }",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(0.0)
    );
    assert_eq!(json["plots"][0]["points"].as_array().unwrap().len(), 20);
}

#[test]
fn golden_minute_execution_with_weekly_signal_shape_matches() {
    let compiled = compile(&with_intervals(
        "1d",
        &["1w"],
        "if close > ema(1w.close, 2) { plot(1) } else { plot(0) }",
    ))
    .expect("compiles");
    let base = bars_with_spacing(JAN_1_2024_UTC_MS, DAY_MS, &[100.0; 21]);
    let weekly = bars_with_spacing(JAN_1_2024_UTC_MS, WEEK_MS, &[90.0, 95.0, 105.0]);
    let outputs = run_multi_interval(
        &compiled,
        &base,
        MultiIntervalConfig {
            base_interval: Interval::Day1,
            supplemental: vec![IntervalFeed {
                interval: Interval::Week1,
                bars: weekly,
            }],
        },
        VmLimits::default(),
    )
    .expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][5]["value"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        json["plots"][0]["points"][6]["value"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        json["plots"][0]["points"][13]["value"],
        serde_json::json!(1.0)
    );
}

#[test]
fn golden_monthly_reference_shape_matches() {
    let compiled = compile(&with_intervals(
        "1w",
        &["1M"],
        "if 1M.close > 1M.close[1] { plot(1) } else { plot(0) }",
    ))
    .expect("compiles");
    let base = bars_with_spacing(
        JAN_1_2024_UTC_MS,
        WEEK_MS,
        &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    );
    let monthly = vec![
        bars_with_spacing(JAN_1_2024_UTC_MS, DAY_MS, &[100.0])[0],
        bars_with_spacing(FEB_1_2024_UTC_MS, DAY_MS, &[120.0])[0],
        bars_with_spacing(MAR_1_2024_UTC_MS, DAY_MS, &[110.0])[0],
    ];
    let outputs = run_multi_interval(
        &compiled,
        &base,
        MultiIntervalConfig {
            base_interval: Interval::Week1,
            supplemental: vec![IntervalFeed {
                interval: Interval::Month1,
                bars: monthly,
            }],
        },
        VmLimits::default(),
    )
    .expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][3]["value"],
        serde_json::json!(0.0)
    );
    assert_eq!(
        json["plots"][0]["points"][4]["value"],
        serde_json::json!(0.0)
    );
}

#[test]
fn golden_export_series_shape_matches() {
    let compiled =
        compile(&with_interval("export trend = close > close[1]\nplot(0)")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(json["exports"][0]["name"], json!("trend"));
    assert_eq!(json["exports"][0]["points"].as_array().unwrap().len(), 20);
    assert_eq!(json["exports"][0]["points"][0]["value"], json!("NA"));
}

#[test]
fn golden_pipeline_shape_matches() {
    let producer =
        compile(&with_interval("export trend = close > close[1]\nplot(0)")).expect("producer");
    let consumer = compile_with_env(
        &with_interval("if trend { plot(1) } else { plot(0) }"),
        &CompileEnvironment {
            external_inputs: vec![ExternalInputDecl {
                name: "trend".into(),
                ty: Type::SeriesBool,
                kind: ExternalInputKind::ExportSeries,
            }],
        },
    )
    .expect("consumer");
    let outputs = PipelineEngine::new(
        PipelineSpec {
            nodes: vec![
                PipelineNodeSpec {
                    name: "producer".into(),
                    compiled: producer,
                    base_interval: Interval::Min1,
                    data_config: None,
                },
                PipelineNodeSpec {
                    name: "consumer".into(),
                    compiled: consumer,
                    base_interval: Interval::Min1,
                    data_config: None,
                },
            ],
            edges: vec![PipelineEdge {
                from_node: "producer".into(),
                output: "trend".into(),
                to_node: "consumer".into(),
                input: "trend".into(),
            }],
        },
        VmLimits::default(),
    )
    .expect("pipeline")
    .run(&fixture_bars())
    .expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["nodes"][0]["outputs"]["exports"][0]["name"],
        json!("trend")
    );
    assert_eq!(
        json["nodes"][1]["outputs"]["plots"][0]["points"][0]["value"],
        json!(0.0)
    );
    assert_eq!(
        json["nodes"][1]["outputs"]["plots"][0]["points"][1]["value"],
        json!(1.0)
    );
}
