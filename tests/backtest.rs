use palmscript::{
    compile, run_backtest_with_sources, BacktestConfig, BacktestError, Bar, Interval,
    OrderEndReason, OrderKind, OrderStatus, SourceFeed, SourceRuntimeConfig, VmLimits,
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
fn diagnostics_capture_trade_context_and_excursions() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
export trend_state = spot.close > spot.close[1]
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
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
                Bar {
                    open: 12.0,
                    high: 14.0,
                    low: 11.0,
                    close: 13.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS) as f64,
                },
                Bar {
                    open: 11.0,
                    high: 11.0,
                    low: 8.0,
                    close: 8.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS) as f64,
                },
                Bar {
                    open: 7.0,
                    high: 7.0,
                    low: 6.0,
                    close: 7.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 4 * support::MINUTE_MS) as f64,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.diagnostics.order_diagnostics.len(), 2);
    assert_eq!(result.diagnostics.trade_diagnostics.len(), 1);
    assert_eq!(result.diagnostics.summary.signal_exit_count, 1);
    let order_diag = &result.diagnostics.order_diagnostics[0];
    let signal_snapshot = order_diag
        .signal_snapshot
        .as_ref()
        .expect("signal snapshot should exist");
    assert_eq!(signal_snapshot.values[0].name, "trend_state");
    assert_eq!(
        signal_snapshot.values[0].value,
        palmscript::OutputValue::Bool(true)
    );

    let trade_diag = &result.diagnostics.trade_diagnostics[0];
    assert!(trade_diag.mfe_pct > 0.0);
    assert!(trade_diag.mae_pct < 0.0);
    assert_eq!(trade_diag.entry_snapshot, order_diag.fill_snapshot);
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

#[test]
fn explicit_market_orders_preserve_market_fill_behavior() {
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
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
order entry long = market()
order exit long = market()
plot(spot.close)",
    )
    .expect("script should compile");

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.orders.len(), 2);
    assert!(result
        .orders
        .iter()
        .all(|order| order.kind == OrderKind::Market && order.status == OrderStatus::Filled));
    approx_eq(result.fills[0].price, 12.0);
    approx_eq(result.fills[1].price, 8.0);
}

#[test]
fn limit_entry_fills_at_better_of_open_and_limit() {
    let signal_time = support::JAN_1_2024_UTC_MS + support::MINUTE_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.time == {signal_time}
order entry long = limit(spot.close[1], tif.gtc, false)
plot(spot.close)",
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
                bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
                Bar {
                    open: 12.0,
                    high: 13.0,
                    low: 9.0,
                    close: 12.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS) as f64,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.orders.len(), 1);
    assert_eq!(result.orders[0].kind, OrderKind::Limit);
    assert_eq!(result.orders[0].status, OrderStatus::Filled);
    approx_eq(result.orders[0].limit_price.expect("limit"), 10.0);
    approx_eq(result.orders[0].fill_price.expect("fill"), 10.0);
    approx_eq(result.fills[0].price, 10.0);
}

#[test]
fn stop_market_entry_fills_at_worse_of_open_and_trigger() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
order entry long = stop_market(spot.close + 1, trigger_ref.last)
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
                    11.0,
                    11.0,
                ),
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.orders[0].kind, OrderKind::StopMarket);
    assert_eq!(result.orders[0].status, OrderStatus::Filled);
    approx_eq(result.orders[0].trigger_price.expect("trigger"), 12.0);
    approx_eq(result.orders[0].fill_price.expect("fill"), 12.0);
    approx_eq(result.fills[0].price, 12.0);
}

#[test]
fn stop_limit_waits_until_next_bar_after_trigger() {
    let signal_time = support::JAN_1_2024_UTC_MS + support::MINUTE_MS;
    let compiled = compile(
        &format!(
            "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.time == {signal_time}
order entry long = stop_limit(spot.close + 1, spot.close, tif.gtc, false, trigger_ref.last, spot.time + 600000)
plot(spot.close)",
        ),
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
                Bar {
                    open: 13.0,
                    high: 13.0,
                    low: 10.0,
                    close: 13.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS) as f64,
                },
                Bar {
                    open: 10.0,
                    high: 10.0,
                    low: 9.0,
                    close: 10.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS) as f64,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.orders[0].kind, OrderKind::StopLimit);
    assert_eq!(result.orders[0].status, OrderStatus::Filled);
    approx_eq(
        result.orders[0].trigger_time.expect("trigger"),
        (support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS) as f64,
    );
    assert_eq!(result.orders[0].fill_bar_index, Some(3));
    approx_eq(result.orders[0].fill_price.expect("fill"), 10.0);
    approx_eq(result.fills[0].price, 10.0);
}

#[test]
fn ioc_limit_cancels_when_not_filled_on_first_bar() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
order entry long = limit(spot.close[1] - 2, tif.ioc, false)
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
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 0);
    assert_eq!(result.orders[0].status, OrderStatus::Cancelled);
    assert_eq!(
        result.orders[0].end_reason,
        Some(OrderEndReason::IocUnfilled)
    );
}

#[test]
fn gtd_stop_limit_expires_before_late_touch() {
    let signal_time = support::JAN_1_2024_UTC_MS + support::MINUTE_MS;
    let compiled = compile(
        &format!(
            "interval 1m
source spot = binance.usdm(\"BTCUSDT\")
entry long = spot.time == {signal_time}
order entry long = stop_limit(spot.close + 1, spot.close, tif.gtd, false, trigger_ref.last, spot.time + 120000)
plot(spot.close)",
        ),
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
                Bar {
                    open: 13.0,
                    high: 13.0,
                    low: 10.0,
                    close: 13.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS) as f64,
                },
                Bar {
                    open: 10.0,
                    high: 10.0,
                    low: 9.0,
                    close: 10.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS) as f64,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 0);
    assert_eq!(result.orders[0].status, OrderStatus::Expired);
}

#[test]
fn same_role_orders_replace_active_resting_orders() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
order entry long = limit(spot.close[1], tif.gtc, false)
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
                    11.0,
                    11.0,
                ),
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.orders.len(), 2);
    assert_eq!(result.orders[0].status, OrderStatus::Cancelled);
    assert_eq!(result.orders[0].end_reason, Some(OrderEndReason::Replaced));
    assert_eq!(result.orders[1].status, OrderStatus::Filled);
    approx_eq(result.orders[1].limit_price.expect("limit"), 11.0);
    approx_eq(result.orders[1].fill_price.expect("fill"), 11.0);
}

#[test]
fn post_only_limit_cancels_when_order_would_cross() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
order entry long = limit(spot.close + 2, tif.gtc, true)
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
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 0);
    assert_eq!(result.orders[0].status, OrderStatus::Cancelled);
    assert_eq!(
        result.orders[0].end_reason,
        Some(OrderEndReason::PostOnlyWouldCross)
    );
}

#[test]
fn venue_profiles_reject_unsupported_order_configurations() {
    let cases = [
        (
            "binance spot",
            "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\nentry long = spot.close > spot.close[1]\norder entry long = stop_market(spot.close + 1, trigger_ref.mark)\nplot(spot.close)",
        ),
        (
            "binance usdm",
            "interval 1m\nsource spot = binance.usdm(\"BTCUSDT\")\nentry long = spot.close > spot.close[1]\norder entry long = stop_market(spot.close + 1, trigger_ref.index)\nplot(spot.close)",
        ),
        (
            "hyperliquid spot",
            "interval 1m\nsource spot = hyperliquid.spot(\"BTC\")\nentry long = spot.close > spot.close[1]\norder entry long = limit(spot.close[1], tif.fok, false)\nplot(spot.close)",
        ),
        (
            "hyperliquid perps",
            "interval 1m\nsource spot = hyperliquid.perps(\"BTC\")\nentry long = spot.close > spot.close[1]\norder entry long = stop_market(spot.close + 1, trigger_ref.last)\nplot(spot.close)",
        ),
    ];
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

    for (name, source) in cases {
        let compiled = compile(source).expect("script should compile");
        let err = run_backtest_with_sources(
            &compiled,
            runtime.clone(),
            VmLimits::default(),
            config("spot"),
        )
        .expect_err("expected venue validation error");
        assert!(
            matches!(err, BacktestError::UnsupportedOrderForVenue { .. }),
            "{name}"
        );
    }
}
