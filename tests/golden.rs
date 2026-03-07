#[path = "support/mod.rs"]
mod support;

use palmscript::{
    compile, run_with_sources, Bar, CompiledProgram, Interval, Outputs, RuntimeError, SourceFeed,
    SourceRuntimeConfig, VmLimits,
};
use serde_json::json;

fn with_interval(source: &str) -> String {
    support::with_single_source_interval(source)
}

fn with_intervals(base: &str, supplemental: &[&str], source: &str) -> String {
    support::with_single_source_intervals(base, supplemental, source)
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
                time: JAN_1_2024_UTC_MS as f64 + index as f64 * 60_000.0,
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

#[derive(Clone, Debug)]
struct IntervalFeed {
    interval: Interval,
    bars: Vec<Bar>,
}

#[derive(Clone, Debug)]
struct MultiIntervalConfig {
    base_interval: Interval,
    supplemental: Vec<IntervalFeed>,
}

fn run(
    compiled: &CompiledProgram,
    bars: &[Bar],
    limits: VmLimits,
) -> Result<Outputs, RuntimeError> {
    let base_interval = compiled
        .program
        .base_interval
        .expect("compiled strategy should declare a base interval");
    run_with_sources(
        compiled,
        SourceRuntimeConfig {
            base_interval,
            feeds: vec![SourceFeed {
                source_id: 0,
                interval: base_interval,
                bars: bars.to_vec(),
            }],
        },
        limits,
    )
}

fn run_multi_interval(
    compiled: &CompiledProgram,
    base_bars: &[Bar],
    config: MultiIntervalConfig,
    limits: VmLimits,
) -> Result<Outputs, RuntimeError> {
    let mut feeds = Vec::with_capacity(1 + config.supplemental.len());
    feeds.push(SourceFeed {
        source_id: 0,
        interval: config.base_interval,
        bars: base_bars.to_vec(),
    });
    feeds.extend(config.supplemental.into_iter().map(|feed| SourceFeed {
        source_id: 0,
        interval: feed.interval,
        bars: feed.bars,
    }));
    run_with_sources(
        compiled,
        SourceRuntimeConfig {
            base_interval: config.base_interval,
            feeds,
        },
        limits,
    )
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
        "fn is_rising(series) = series > series[1]\nif is_rising(close) { plot(1) } else { plot(0) }",
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
        "fn cross_signal(a, b) = a > b and a[1] <= b[1]\nlet fast = ema(close, 3)\nlet slow = ema(close, 5)\nif cross_signal(fast, slow) { plot(1) } else { plot(0) }",
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
fn golden_crossover_builtin_shape_matches() {
    let compiled = compile(&with_interval(
        "if crossover(close, 104) { plot(1) } else { plot(0) }",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(0.0)
    );
    assert!(json["plots"][0]["points"]
        .as_array()
        .unwrap()
        .iter()
        .any(|point| point["value"] == serde_json::json!(1.0)));
}

#[test]
fn golden_correl_identical_series_matches_one_after_warmup() {
    let compiled =
        compile(&with_interval("plot(correl(close, close, 5))")).expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][4]["value"],
        serde_json::json!(1.0)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(1.0)
    );
}

#[test]
fn golden_beta_identical_series_matches_one_after_warmup() {
    let compiled = compile(&with_interval("plot(beta(close, close, 5))")).expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][4]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][5]["value"],
        serde_json::json!(1.0)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(1.0)
    );
}

#[test]
fn golden_roc_uses_talib_default_window() {
    let compiled = compile(&with_interval("plot(roc(close))")).expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][9]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][10]["value"],
        serde_json::json!(10.0)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(9.174311926605505)
    );
}

#[test]
fn golden_cmo_uses_talib_default_window() {
    let compiled = compile(&with_interval("plot(cmo(close))")).expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][13]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][14]["value"],
        serde_json::json!(100.0)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(100.0)
    );
}

#[test]
fn golden_aroon_tuple_destructuring_matches_talib_order() {
    let compiled = compile(&with_interval(
        "let (down, up) = aroon(high, low, 3)\nexport aroon_down = down\nexport aroon_up = up\nplot(0)",
    ))
    .expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["exports"][0]["points"][2]["value"],
        serde_json::json!("NA")
    );
    assert_eq!(
        json["exports"][1]["points"][2]["value"],
        serde_json::json!("NA")
    );
    assert_eq!(
        json["exports"][0]["points"][3]["value"],
        serde_json::json!({ "F64": 0.0 })
    );
    assert_eq!(
        json["exports"][1]["points"][3]["value"],
        serde_json::json!({ "F64": 100.0 })
    );
}

#[test]
fn golden_aroonosc_matches_up_minus_down() {
    let compiled = compile(&with_interval("plot(aroonosc(high, low, 3))")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][2]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][3]["value"],
        serde_json::json!(100.0)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(100.0)
    );
}

#[test]
fn golden_willr_matches_trailing_high_low_close_window() {
    let compiled = compile(&with_interval("plot(willr(high, low, close, 3))")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][1]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][2]["value"],
        serde_json::json!(-25.0)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(-25.0)
    );
}

#[test]
fn golden_apo_matches_explicit_sma_difference() {
    let compiled =
        compile(&with_interval("plot(apo(close, 3, 5, ma_type.sma))")).expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][3]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][4]["value"],
        serde_json::json!(1.0)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(1.0)
    );
}

#[test]
fn golden_ppo_matches_explicit_sma_percentage() {
    let compiled =
        compile(&with_interval("plot(ppo(close, 3, 5, ma_type.sma))")).expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][3]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][4]["value"],
        serde_json::json!(0.9803921568627451)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(0.8547008547008548)
    );
}

#[test]
fn golden_ma_builtin_with_typed_enum_matches_weighted_window() {
    let compiled =
        compile(&with_interval("plot(ma(close, 3, ma_type.wma))")).expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][2]["value"],
        serde_json::json!(101.33333333333333)
    );
}

#[test]
fn golden_ma_builtin_supports_dema_enum() {
    let compiled = compile(&with_interval(
        "plot(sub(ma(close, 3, ma_type.dema), dema(close, 3)))",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][8]["value"],
        serde_json::json!(0.0)
    );
}

#[test]
fn golden_apo_supports_trima_ma_type() {
    let compiled = compile(&with_interval(
        "plot(sub(apo(close, 3, 5, ma_type.trima), sub(trima(close, 3), trima(close, 5))))",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][6]["value"],
        serde_json::json!(0.0)
    );
}

#[test]
fn golden_macd_tuple_destructuring_shape_matches() {
    let compiled = compile(&with_interval(
        "let (line, signal, hist) = macd(close, 3, 5, 2)\nplot(hist)",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(json["plots"].as_array().unwrap().len(), 1);
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::Value::Null
    );
    assert!(json["plots"][0]["points"]
        .as_array()
        .unwrap()
        .iter()
        .skip(5)
        .any(|point| point["value"].is_number()));
}

#[test]
fn golden_avgprice_matches_expected_value() {
    let compiled =
        compile(&with_interval("plot(avgprice(open, high, low, close))")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(99.875)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(118.875)
    );
}

#[test]
fn golden_bop_matches_expected_ratio() {
    let compiled = compile(&with_interval("plot(bop(open, high, low, close))")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(0.25)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(0.25)
    );
}

#[test]
fn golden_unary_math_transform_compiles_over_series() {
    let compiled = compile(&with_interval("plot(cos(close - close))")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
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
fn golden_midpoint_uses_default_window() {
    let compiled = compile(&with_interval("plot(midpoint(close))")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][12]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][13]["value"],
        serde_json::json!(106.5)
    );
}

#[test]
fn golden_minmax_tuple_destructuring_shape_matches() {
    let compiled = compile(&with_interval(
        "let (lo, hi) = minmax(close, 10)\nplot(hi - lo)",
    ))
    .expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][8]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][9]["value"],
        serde_json::json!(9.0)
    );
}

#[test]
fn golden_stddev_applies_factor() {
    let compiled = compile(&with_interval("plot(stddev(close, 5, 2))")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][3]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][4]["value"],
        serde_json::json!(2.8284271247461903)
    );
}

#[test]
fn golden_cci_uses_talib_default_window() {
    let compiled = compile(&with_interval("plot(cci(high, low, close, 3))")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][1]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][2]["value"],
        serde_json::json!(100.00000000000001)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(100.00000000000001)
    );
}

#[test]
fn golden_linearreg_uses_default_window() {
    let compiled = compile(&with_interval("plot(linearreg(close))")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][12]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][13]["value"],
        serde_json::json!(113.0)
    );
}

#[test]
fn golden_obv_accumulates_with_direction() {
    let compiled = compile(&with_interval("plot(obv(close, volume))")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::json!(1000.0)
    );
    assert_eq!(
        json["plots"][0]["points"][1]["value"],
        serde_json::json!(2001.0)
    );
    assert_eq!(
        json["plots"][0]["points"][2]["value"],
        serde_json::json!(3003.0)
    );
}

#[test]
fn golden_trange_skips_first_bar_and_uses_prior_close() {
    let compiled = compile(&with_interval("plot(trange(high, low, close))")).expect("compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][1]["value"],
        serde_json::json!(2.0)
    );
    assert_eq!(
        json["plots"][0]["points"][19]["value"],
        serde_json::json!(2.0)
    );
}

#[test]
fn golden_extrema_builtin_shape_matches() {
    let compiled = compile(&with_interval("plot(highest(close, 5) - lowest(close, 5))"))
        .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][4]["value"],
        serde_json::json!(4.0)
    );
}

#[test]
fn golden_event_memory_builtin_shape_matches() {
    let compiled = compile(&with_interval(
        "plot(valuewhen(close > close[1], close, 0))",
    ))
    .expect("script compiles");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script runs");
    let json = serde_json::to_value(outputs).expect("json");
    assert_eq!(
        json["plots"][0]["points"][0]["value"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["plots"][0]["points"][1]["value"],
        serde_json::json!(101.0)
    );
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
