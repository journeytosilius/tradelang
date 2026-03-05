use serde_json::json;
use tradelang::{compile, run, Bar, VmLimits};

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

#[test]
fn golden_sma_shape_matches() {
    let compiled = compile("plot(sma(close, 14))").expect("script compiles");
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
    let compiled = compile("plot(close[1])").expect("script compiles");
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
    let compiled =
        compile("if close > sma(close, 14) { plot(1) } else { plot(0) }").expect("script compiles");
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
    let compiled =
        compile("if close > ema(close, 3) and rsi(close, 3) > 50 { plot(1) } else { plot(0) }")
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
    let compiled = compile(
        "if close < ema(close, 3) { plot(-1) } else if close > ema(close, 3) or close > close[1] { plot(1) } else { plot(0) }",
    )
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
    let compiled = compile(
        "let rising = close > close[1]\nif rising[1] or false { plot(1) } else { plot(0) }",
    )
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
