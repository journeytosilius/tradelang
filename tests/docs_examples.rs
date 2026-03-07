use std::fs;
use std::path::PathBuf;

use palmscript::{
    compile, run_backtest_with_sources, run_with_sources, BacktestConfig, Bar, SignalContract,
    SourceFeed, SourceRuntimeConfig, VmLimits,
};

const JAN_1_2024_UTC_MS: i64 = 1_704_067_200_000;
const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_strategy(relative: &str) -> String {
    fs::read_to_string(repo_path(relative)).expect("strategy example should be readable")
}

fn bars(start_ms: i64, spacing_ms: i64, len: usize, start_close: f64) -> Vec<Bar> {
    (0..len)
        .map(|index| {
            let close = start_close + index as f64;
            Bar {
                open: close - 0.5,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1_000.0 + index as f64,
                time: (start_ms + spacing_ms * index as i64) as f64,
            }
        })
        .collect()
}

#[test]
fn referenced_docs_examples_compile() {
    let examples = [
        "examples/strategies/sma_cross.palm",
        "examples/strategies/volume_breakout.palm",
        "examples/strategies/signal_helpers.palm",
        "examples/strategies/event_memory.palm",
        "examples/strategies/macd_tuple.palm",
        "examples/strategies/weekly_bias.palm",
        "examples/strategies/cross_source_spread.palm",
        "examples/strategies/exchange_backed_sources.palm",
        "examples/strategies/multi_strategy_backtest.palm",
    ];

    for path in examples {
        compile(&read_strategy(path)).unwrap_or_else(|_| panic!("{path} should compile"));
    }
}

#[test]
fn single_source_docs_examples_run_with_local_feeds() {
    let minute_examples = [
        "examples/strategies/sma_cross.palm",
        "examples/strategies/volume_breakout.palm",
        "examples/strategies/signal_helpers.palm",
        "examples/strategies/event_memory.palm",
        "examples/strategies/macd_tuple.palm",
    ];
    let minute_bars = bars(JAN_1_2024_UTC_MS, MINUTE_MS, 80, 100.0);

    for path in minute_examples {
        let compiled =
            compile(&read_strategy(path)).unwrap_or_else(|_| panic!("{path} should compile"));
        let outputs = run_with_sources(
            &compiled,
            SourceRuntimeConfig {
                base_interval: palmscript::Interval::Min1,
                feeds: vec![SourceFeed {
                    source_id: 0,
                    interval: palmscript::Interval::Min1,
                    bars: minute_bars.clone(),
                }],
            },
            VmLimits::default(),
        )
        .unwrap_or_else(|_| panic!("{path} should run"));
        assert!(
            !outputs.plots.is_empty()
                || !outputs.exports.is_empty()
                || !outputs.triggers.is_empty(),
            "{path} should emit outputs"
        );
    }
}

#[test]
fn supplemental_interval_docs_example_runs_with_local_feeds() {
    let path = "examples/strategies/weekly_bias.palm";
    let compiled = compile(&read_strategy(path)).expect("weekly_bias should compile");
    let daily_bars = bars(JAN_1_2024_UTC_MS, DAY_MS, 21, 100.0);
    let outputs = run_with_sources(
        &compiled,
        SourceRuntimeConfig {
            base_interval: palmscript::Interval::Day1,
            feeds: vec![
                SourceFeed {
                    source_id: 0,
                    interval: palmscript::Interval::Day1,
                    bars: daily_bars,
                },
                SourceFeed {
                    source_id: 0,
                    interval: palmscript::Interval::Week1,
                    bars: bars(JAN_1_2024_UTC_MS, 7 * DAY_MS, 3, 90.0),
                },
            ],
        },
        VmLimits::default(),
    )
    .expect("weekly_bias should run");
    assert_eq!(outputs.plots.len(), 1);
}

#[test]
fn multi_strategy_docs_example_runs_with_local_feeds() {
    let path = "examples/strategies/multi_strategy_backtest.palm";
    let compiled = compile(&read_strategy(path)).expect("multi_strategy_backtest should compile");
    let outputs = run_with_sources(
        &compiled,
        SourceRuntimeConfig {
            base_interval: palmscript::Interval::Hour4,
            feeds: vec![
                SourceFeed {
                    source_id: 0,
                    interval: palmscript::Interval::Hour4,
                    bars: bars(JAN_1_2024_UTC_MS, 4 * HOUR_MS, 240, 100.0),
                },
                SourceFeed {
                    source_id: 0,
                    interval: palmscript::Interval::Day1,
                    bars: bars(JAN_1_2024_UTC_MS, DAY_MS, 80, 100.0),
                },
                SourceFeed {
                    source_id: 0,
                    interval: palmscript::Interval::Week1,
                    bars: bars(JAN_1_2024_UTC_MS, 7 * DAY_MS, 40, 100.0),
                },
            ],
        },
        VmLimits::default(),
    )
    .expect("multi_strategy_backtest should run");
    assert!(!outputs.plots.is_empty());

    let result = run_backtest_with_sources(
        &compiled,
        SourceRuntimeConfig {
            base_interval: palmscript::Interval::Hour4,
            feeds: vec![
                SourceFeed {
                    source_id: 0,
                    interval: palmscript::Interval::Hour4,
                    bars: bars(JAN_1_2024_UTC_MS, 4 * HOUR_MS, 240, 100.0),
                },
                SourceFeed {
                    source_id: 0,
                    interval: palmscript::Interval::Day1,
                    bars: bars(JAN_1_2024_UTC_MS, DAY_MS, 80, 100.0),
                },
                SourceFeed {
                    source_id: 0,
                    interval: palmscript::Interval::Week1,
                    bars: bars(JAN_1_2024_UTC_MS, 7 * DAY_MS, 40, 100.0),
                },
            ],
        },
        VmLimits::default(),
        BacktestConfig {
            execution_source_alias: "spot".to_string(),
            initial_capital: 10_000.0,
            fee_bps: 0.0,
            slippage_bps: 0.0,
            signals: SignalContract::default(),
        },
    )
    .expect("multi_strategy_backtest should backtest");
    assert!(!result.equity_curve.is_empty());
}

#[test]
fn source_aware_docs_examples_run_with_local_feeds() {
    let spread = compile(&read_strategy(
        "examples/strategies/cross_source_spread.palm",
    ))
    .expect("cross_source_spread should compile");
    let spread_outputs = run_with_sources(
        &spread,
        SourceRuntimeConfig {
            base_interval: palmscript::Interval::Min1,
            feeds: vec![
                SourceFeed {
                    source_id: 0,
                    interval: palmscript::Interval::Min1,
                    bars: bars(JAN_1_2024_UTC_MS, MINUTE_MS, 4, 100.0),
                },
                SourceFeed {
                    source_id: 1,
                    interval: palmscript::Interval::Min1,
                    bars: bars(JAN_1_2024_UTC_MS, MINUTE_MS, 4, 95.0),
                },
            ],
        },
        VmLimits::default(),
    )
    .expect("cross_source_spread should run");
    assert_eq!(spread_outputs.plots.len(), 1);
    assert_eq!(spread_outputs.plots[0].points.len(), 4);
    assert!(spread_outputs.plots[0]
        .points
        .iter()
        .all(|point| point.value == Some(5.0)));

    let exchange = compile(&read_strategy(
        "examples/strategies/exchange_backed_sources.palm",
    ))
    .expect("exchange_backed_sources should compile");
    let exchange_outputs = run_with_sources(
        &exchange,
        SourceRuntimeConfig {
            base_interval: palmscript::Interval::Min1,
            feeds: vec![
                SourceFeed {
                    source_id: 0,
                    interval: palmscript::Interval::Min1,
                    bars: bars(JAN_1_2024_UTC_MS, MINUTE_MS, 4, 100.0),
                },
                SourceFeed {
                    source_id: 1,
                    interval: palmscript::Interval::Min1,
                    bars: bars(JAN_1_2024_UTC_MS, MINUTE_MS, 4, 95.0),
                },
                SourceFeed {
                    source_id: 1,
                    interval: palmscript::Interval::Hour1,
                    bars: bars(JAN_1_2024_UTC_MS, HOUR_MS, 2, 200.0),
                },
            ],
        },
        VmLimits::default(),
    )
    .expect("exchange_backed_sources should run");
    assert_eq!(exchange_outputs.plots.len(), 1);
    assert!(!exchange_outputs.plots[0].points.is_empty());
}
