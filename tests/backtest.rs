use palmscript::{
    compile, run_backtest_with_sources, BacktestConfig, BacktestError, Bar, Interval, SourceFeed,
    SourceRuntimeConfig, VmLimits,
};

#[path = "support/mod.rs"]
mod support;

fn bar(time: i64, open: f64, close: f64) -> Bar {
    Bar {
        open,
        high: open.max(close) + 1.0,
        low: open.min(close) - 1.0,
        close,
        volume: 1_000.0,
        time: time as f64,
    }
}

fn config(alias: &str) -> BacktestConfig {
    BacktestConfig {
        execution_source_alias: alias.to_string(),
        initial_capital: 1_000.0,
        fee_bps: 0.0,
        slippage_bps: 0.0,
    }
}

fn approx_eq(left: f64, right: f64) {
    let delta = (left - right).abs();
    assert!(
        delta < 1e-6,
        "expected {left} to be within tolerance of {right}, delta={delta}"
    );
}

#[test]
fn rejects_invalid_backtest_config() {
    let compiled = compile(
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ntrigger long_entry = true\nplot(spot.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0)],
        }],
    };

    let mut invalid = config("spot");
    invalid.initial_capital = 0.0;
    let err = run_backtest_with_sources(&compiled, runtime.clone(), VmLimits::default(), invalid)
        .expect_err("expected invalid capital");
    assert!(matches!(err, BacktestError::InvalidInitialCapital { .. }));

    let mut invalid = config("spot");
    invalid.fee_bps = -1.0;
    let err = run_backtest_with_sources(&compiled, runtime.clone(), VmLimits::default(), invalid)
        .expect_err("expected invalid fee");
    assert!(matches!(err, BacktestError::InvalidFeeBps { .. }));

    let mut invalid = config("spot");
    invalid.slippage_bps = -1.0;
    let err = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), invalid)
        .expect_err("expected invalid slippage");
    assert!(matches!(err, BacktestError::InvalidSlippageBps { .. }));
}

#[test]
fn rejects_unknown_execution_source_alias() {
    let compiled = compile(
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ntrigger long_entry = true\nplot(spot.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0)],
        }],
    };
    let err = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("other"))
        .expect_err("expected unknown alias error");
    assert!(matches!(err, BacktestError::UnknownExecutionSource { .. }));
}

#[test]
fn rejects_when_required_backtest_signals_are_missing() {
    let compiled = compile(
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\ntrigger continuation = true\nplot(spot.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0)],
        }],
    };
    let err = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect_err("expected missing signals error");
    assert!(matches!(err, BacktestError::MissingSignalRoles { .. }));
}

#[test]
fn long_trade_applies_next_bar_open_and_marks_equity() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
trigger long_entry = spot.close > spot.close[1]
trigger long_exit = spot.close < spot.close[1]
plot(spot.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
                bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
                bar(
                    support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                    12.0,
                    9.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                    8.0,
                    8.0,
                ),
            ],
        }],
    };
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 2);
    assert_eq!(result.trades.len(), 1);
    assert_eq!(result.open_position, None);
    assert_eq!(result.fills[0].bar_index, 2);
    assert_eq!(result.fills[1].bar_index, 3);
    approx_eq(result.fills[0].price, 12.0);
    approx_eq(result.fills[1].price, 8.0);
    approx_eq(result.trades[0].quantity, 83.33333333333333);
    approx_eq(result.trades[0].realized_pnl, -333.3333333333333);
    approx_eq(result.summary.ending_equity, 666.6666666666667);
    approx_eq(result.summary.realized_pnl, -333.3333333333333);
    approx_eq(result.summary.unrealized_pnl, 0.0);
    approx_eq(result.summary.total_return, -0.3333333333333333);
    approx_eq(result.summary.max_drawdown, 333.33333333333326);
    approx_eq(result.summary.max_gross_exposure, 750.0);
    approx_eq(result.equity_curve[2].equity, 750.0);
}

#[test]
fn first_class_signal_declarations_drive_backtests() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
entry short = false
exit short = false
plot(spot.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
                bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
                bar(
                    support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                    12.0,
                    9.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                    8.0,
                    8.0,
                ),
            ],
        }],
    };
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 2);
    assert_eq!(result.trades.len(), 1);
}

#[test]
fn fees_and_slippage_adjust_fill_prices_and_fees() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
trigger long_entry = spot.close > spot.close[1]
trigger long_exit = spot.close < spot.close[1]
plot(spot.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 9.0, 9.0),
                bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 10.0, 10.0),
                bar(
                    support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                    10.0,
                    8.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                    12.0,
                    12.0,
                ),
            ],
        }],
    };
    let mut cfg = config("spot");
    cfg.fee_bps = 100.0;
    cfg.slippage_bps = 100.0;
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), cfg)
        .expect("backtest should succeed");

    approx_eq(result.fills[0].price, 10.1);
    approx_eq(result.fills[1].price, 11.88);
    approx_eq(result.fills[0].fee, result.fills[0].notional * 0.01);
    approx_eq(result.fills[1].fee, result.fills[1].notional * 0.01);
    assert!(result.summary.realized_pnl > 150.0);
    assert!(result.summary.realized_pnl < 160.0);
}

#[test]
fn short_trade_marks_to_market_and_realizes_on_exit() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
trigger short_entry = spot.close < spot.close[1]
trigger short_exit = spot.close > spot.close[1]
plot(spot.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
                bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 9.0, 9.0),
                bar(
                    support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                    8.0,
                    11.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                    12.0,
                    12.0,
                ),
            ],
        }],
    };
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 2);
    assert_eq!(result.trades.len(), 1);
    approx_eq(result.fills[0].price, 8.0);
    approx_eq(result.fills[1].price, 12.0);
    approx_eq(result.trades[0].quantity, 125.0);
    approx_eq(result.trades[0].realized_pnl, -500.0);
    approx_eq(result.equity_curve[2].equity, 625.0);
    approx_eq(result.summary.ending_equity, 500.0);
    approx_eq(result.summary.max_drawdown, 500.0);
}

#[test]
fn reversal_closes_and_reopens_on_same_execution_bar() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
trigger long_entry = spot.close > spot.close[1]
trigger short_entry = spot.close < spot.close[1]
plot(spot.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
                bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
                bar(
                    support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                    12.0,
                    9.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                    8.0,
                    7.0,
                ),
            ],
        }],
    };
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 3);
    assert_eq!(result.fills[0].action, palmscript::FillAction::Buy);
    assert_eq!(result.fills[1].action, palmscript::FillAction::Sell);
    assert_eq!(result.fills[2].action, palmscript::FillAction::Sell);
    assert_eq!(result.fills[1].bar_index, 3);
    assert_eq!(result.fills[2].bar_index, 3);
    assert!(result.open_position.is_some());
    assert_eq!(result.summary.trade_count, 1);
}

#[test]
fn same_side_reentry_is_ignored() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
trigger long_entry = spot.close > spot.close[1]
plot(spot.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
                bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
                bar(
                    support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                    12.0,
                    12.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                    13.0,
                    13.0,
                ),
            ],
        }],
    };
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.trades.len(), 0);
    assert!(result.open_position.is_some());
}

#[test]
fn conflicting_entries_are_rejected() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
trigger long_entry = true
trigger short_entry = true
plot(spot.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
                bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
            ],
        }],
    };
    let err = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect_err("expected conflicting entries");
    assert!(matches!(err, BacktestError::ConflictingSignals { .. }));
}

#[test]
fn multi_source_signal_fills_on_next_execution_bar() {
    let compiled = compile(
        "interval 1m
source exec = binance.spot(\"BTCUSDT\")
source signal = hyperliquid.perps(\"BTC\")
trigger long_entry = signal.close > signal.close[1]
plot(exec.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![
            SourceFeed {
                source_id: 0,
                interval: Interval::Min1,
                bars: vec![
                    bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
                    bar(
                        support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                        12.0,
                        12.0,
                    ),
                    bar(
                        support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                        13.0,
                        13.0,
                    ),
                ],
            },
            SourceFeed {
                source_id: 1,
                interval: Interval::Min1,
                bars: vec![
                    bar(support::JAN_1_2024_UTC_MS, 20.0, 20.0),
                    bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 21.0, 21.0),
                    bar(
                        support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                        22.0,
                        22.0,
                    ),
                    bar(
                        support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                        23.0,
                        23.0,
                    ),
                ],
            },
        ],
    };
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("exec"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.fills[0].bar_index, 1);
    approx_eq(
        result.fills[0].time,
        (support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS) as f64,
    );
}

#[test]
fn open_position_is_reported_without_synthetic_close() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
trigger long_entry = spot.close > spot.close[1]
plot(spot.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
                bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
                bar(
                    support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                    12.0,
                    13.0,
                ),
            ],
        }],
    };
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.trades.len(), 0);
    let open_position = result.open_position.expect("position should remain open");
    approx_eq(open_position.entry_price, 12.0);
    approx_eq(open_position.market_price, 13.0);
    approx_eq(open_position.unrealized_pnl, 83.33333333333333);
    approx_eq(result.summary.unrealized_pnl, 83.33333333333326);
}
