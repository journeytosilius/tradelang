use palmscript::exchange::binance::{
    UsdmRiskSnapshot as BinanceUsdmRiskSnapshot, UsdmRiskSource as BinanceUsdmRiskSource,
};
use palmscript::{
    compile as compile_script, run_backtest_with_sources, BacktestConfig, BacktestError, Bar,
    CompileError, CompiledProgram, DiagnosticsDetailMode, Interval, MarkPriceBasis, OrderEndReason,
    OrderKind, OrderStatus, PerpBacktestConfig, PerpBacktestContext, PerpMarginMode, RiskTier,
    SignalRole, SizeMode, SourceFeed, SourceRuntimeConfig, TradeExitClassification,
    VenueRiskSnapshot, VmLimits,
};

#[path = "support/mod.rs"]
mod support;

fn compile(source: &str) -> Result<CompiledProgram, CompileError> {
    compile_script(&support::mirror_execution_decls(source))
}

fn bar(time: i64, open: f64, close: f64) -> Bar {
    Bar {
        open,
        high: open.max(close) + 1.0,
        low: open.min(close) - 1.0,
        close,
        volume: 1_000.0,
        time: time as f64,
        funding_rate: None,
        open_interest: None,
        mark_price: None,
        index_price: None,
        premium_index: None,
        basis: None,
    }
}

fn multi_source_runtime(left_bars: Vec<Bar>, right_bars: Vec<Bar>) -> SourceRuntimeConfig {
    SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![
            SourceFeed {
                source_id: 0,
                interval: Interval::Min1,
                bars: left_bars,
            },
            SourceFeed {
                source_id: 1,
                interval: Interval::Min1,
                bars: right_bars,
            },
        ],
    }
}

fn runtime_with_execution_feeds(
    left_bars: Vec<Bar>,
    right_bars: Vec<Bar>,
    bin_exec_bars: Vec<Bar>,
    gate_exec_bars: Vec<Bar>,
) -> SourceRuntimeConfig {
    SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![
            SourceFeed {
                source_id: 0,
                interval: Interval::Min1,
                bars: left_bars,
            },
            SourceFeed {
                source_id: 1,
                interval: Interval::Min1,
                bars: right_bars,
            },
            SourceFeed {
                source_id: 2,
                interval: Interval::Min1,
                bars: bin_exec_bars,
            },
            SourceFeed {
                source_id: 3,
                interval: Interval::Min1,
                bars: gate_exec_bars,
            },
        ],
    }
}

fn config(alias: &str) -> BacktestConfig {
    BacktestConfig {
        execution_source_alias: alias.to_string(),
        portfolio_execution_aliases: Vec::new(),
        spot_virtual_rebalance: false,
        activation_time_ms: None,
        initial_capital: 1_000.0,
        maker_fee_bps: 0.0,
        taker_fee_bps: 0.0,
        execution_fee_schedules: std::collections::BTreeMap::new(),
        slippage_bps: 0.0,
        diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
        perp: None,
        perp_context: None,
        portfolio_perp_contexts: std::collections::BTreeMap::new(),
    }
}

fn trace_config(alias: &str) -> BacktestConfig {
    let mut config = config(alias);
    config.diagnostics_detail = DiagnosticsDetailMode::FullTrace;
    config
}

#[test]
fn backtest_accepts_reusable_order_templates() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution perp = binance.spot(\"BTCUSDT\")
order_template market_entry = market(venue = perp)
order_template market_exit = market_entry
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
order entry long = market_entry
order exit long = market_exit
plot(spot.close)",
    )
    .expect("script with order templates should compile");

    let bars = vec![
        bar(0, 100.0, 101.0),
        bar(60_000, 101.0, 103.0),
        bar(120_000, 103.0, 102.0),
        bar(180_000, 102.0, 100.0),
    ];
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![
            SourceFeed {
                source_id: 0,
                interval: Interval::Min1,
                bars: bars.clone(),
            },
            SourceFeed {
                source_id: 1,
                interval: Interval::Min1,
                bars,
            },
        ],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("perp"))
        .expect("backtest should run");
    assert_eq!(result.summary.trade_count, 1);
}

#[test]
fn backtest_reports_baseline_and_date_perturbation_diagnostics() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
entry short = false
exit long = spot.close < spot.close[1]
exit short = true
order entry long = market(venue = spot)
order entry short = market(venue = spot)
order exit long = market(venue = spot)
order exit short = market(venue = spot)
plot(spot.close)",
    )
    .expect("script should compile");

    let closes = [
        10.0, 11.0, 12.0, 11.0, 12.0, 13.0, 12.0, 13.0, 14.0, 13.0, 14.0, 15.0,
    ];
    let bars = closes
        .iter()
        .enumerate()
        .map(|(index, close)| bar(index as i64 * 60_000, *close - 0.5, *close))
        .collect::<Vec<_>>();
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![
            SourceFeed {
                source_id: 0,
                interval: Interval::Min1,
                bars: bars.clone(),
            },
            SourceFeed {
                source_id: 1,
                interval: Interval::Min1,
                bars,
            },
        ],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should run");

    approx_eq(
        result
            .diagnostics
            .baseline_comparison
            .execution_asset_return,
        result.diagnostics.capture_summary.execution_asset_return,
    );
    approx_eq(
        result
            .diagnostics
            .baseline_comparison
            .excess_return_vs_execution_asset,
        result.summary.total_return - result.diagnostics.capture_summary.execution_asset_return,
    );
    assert_eq!(result.diagnostics.date_perturbation.offset_bars, 1);
    assert_eq!(result.diagnostics.date_perturbation.scenarios.len(), 3);
    assert!(result
        .diagnostics
        .date_perturbation
        .scenarios
        .iter()
        .any(|scenario| scenario.kind == palmscript::DatePerturbationKind::LateStart));
    assert!(result
        .diagnostics
        .date_perturbation
        .scenarios
        .iter()
        .any(|scenario| scenario.kind == palmscript::DatePerturbationKind::EarlyEnd));
    assert!(result
        .diagnostics
        .date_perturbation
        .scenarios
        .iter()
        .any(|scenario| scenario.kind == palmscript::DatePerturbationKind::TrimmedBoth));
}

#[test]
fn backtest_reports_entry_module_attribution() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution spot = binance.spot(\"BTCUSDT\")
module breakout = entry long
entry long = spot.close > 101
exit long = spot.close < 102
order entry long = market(venue = spot)
order exit long = market(venue = spot)
plot(spot.close)",
    )
    .expect("script should compile");

    let bars = vec![
        bar(0, 100.0, 100.0),
        bar(60_000, 100.0, 102.0),
        bar(120_000, 102.0, 104.0),
        bar(180_000, 104.0, 101.0),
        bar(240_000, 101.0, 100.0),
    ];
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![
            SourceFeed {
                source_id: 0,
                interval: Interval::Min1,
                bars: bars.clone(),
            },
            SourceFeed {
                source_id: 1,
                interval: Interval::Min1,
                bars,
            },
        ],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should run");

    assert_eq!(result.summary.trade_count, 1);
    assert_eq!(result.trades[0].entry_module.as_deref(), Some("breakout"));
    assert_eq!(
        result.diagnostics.trade_diagnostics[0]
            .entry_module
            .as_deref(),
        Some("breakout")
    );
    assert_eq!(result.diagnostics.cohorts.by_entry_module.len(), 1);
    let summary = &result.diagnostics.cohorts.by_entry_module[0];
    assert_eq!(summary.name, "breakout");
    assert_eq!(summary.trade_count, 1);
    assert_eq!(summary.long_trade_count, 1);
    assert_eq!(summary.short_trade_count, 0);
}

#[test]
fn backtest_reports_time_bucket_cohorts() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
order entry long = market(venue = spot)
order exit long = market(venue = spot)
plot(spot.close)",
    )
    .expect("script should compile");

    let bars = vec![
        bar(3 * 60 * 60 * 1000, 100.0, 101.0),
        bar(4 * 60 * 60 * 1000, 101.0, 103.0),
        bar(5 * 60 * 60 * 1000, 103.0, 99.0),
        bar(6 * 60 * 60 * 1000, 99.0, 98.0),
    ];
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![
            SourceFeed {
                source_id: 0,
                interval: Interval::Min1,
                bars: bars.clone(),
            },
            SourceFeed {
                source_id: 1,
                interval: Interval::Min1,
                bars,
            },
        ],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should run");

    assert_eq!(result.summary.trade_count, 1);
    assert_eq!(result.diagnostics.cohorts.by_time_bucket_utc.len(), 1);
    let summary = &result.diagnostics.cohorts.by_time_bucket_utc[0];
    assert_eq!(summary.start_hour_utc, 0);
    assert_eq!(summary.end_hour_utc, 4);
    assert_eq!(summary.trade_count, 1);
    assert_eq!(summary.winning_trade_count, 0);
}

fn binance_perp_config(alias: &str, leverage: f64, mark_bars: Vec<Bar>) -> BacktestConfig {
    BacktestConfig {
        execution_source_alias: alias.to_string(),
        portfolio_execution_aliases: Vec::new(),
        spot_virtual_rebalance: false,
        activation_time_ms: None,
        initial_capital: 1_000.0,
        maker_fee_bps: 0.0,
        taker_fee_bps: 0.0,
        execution_fee_schedules: std::collections::BTreeMap::new(),
        slippage_bps: 0.0,
        diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
        perp: Some(PerpBacktestConfig {
            leverage,
            margin_mode: PerpMarginMode::Isolated,
        }),
        perp_context: Some(PerpBacktestContext {
            mark_price_basis: MarkPriceBasis::BinanceMarkPriceKlines,
            mark_bars,
            risk_snapshot: VenueRiskSnapshot::BinanceUsdm(BinanceUsdmRiskSnapshot {
                symbol: "BTCUSDT".to_string(),
                fetched_at_ms: support::JAN_1_2024_UTC_MS,
                source: BinanceUsdmRiskSource::SignedLeverageBrackets,
                brackets: vec![RiskTier {
                    lower_bound: 0.0,
                    upper_bound: None,
                    max_leverage: 20.0,
                    maintenance_margin_rate: 0.05,
                    maintenance_amount: 0.0,
                }],
            }),
        }),
        portfolio_perp_contexts: std::collections::BTreeMap::new(),
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
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\nentry long = true\norder entry long = market(venue = spot)\nplot(spot.close)",
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
    invalid.maker_fee_bps = -1.0;
    let err = run_backtest_with_sources(&compiled, runtime.clone(), VmLimits::default(), invalid)
        .expect_err("expected invalid maker fee");
    assert!(matches!(err, BacktestError::InvalidMakerFeeBps { .. }));

    let mut invalid = config("spot");
    invalid.taker_fee_bps = -1.0;
    let err = run_backtest_with_sources(&compiled, runtime.clone(), VmLimits::default(), invalid)
        .expect_err("expected invalid taker fee");
    assert!(matches!(err, BacktestError::InvalidTakerFeeBps { .. }));

    let mut invalid = config("spot");
    invalid.execution_fee_schedules.insert(
        "spot".to_string(),
        palmscript::FeeSchedule {
            maker_bps: 0.0,
            taker_bps: -1.0,
        },
    );
    let err = run_backtest_with_sources(&compiled, runtime.clone(), VmLimits::default(), invalid)
        .expect_err("expected invalid execution fee schedule");
    assert!(matches!(
        err,
        BacktestError::InvalidExecutionFeeSchedule { .. }
    ));

    let mut invalid = config("spot");
    invalid.slippage_bps = -1.0;
    let err = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), invalid)
        .expect_err("expected invalid slippage");
    assert!(matches!(err, BacktestError::InvalidSlippageBps { .. }));
}

#[test]
fn rejects_unknown_execution_source_alias() {
    let compiled = compile(
        "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\nentry long = true\norder entry long = market(venue = spot)\nplot(spot.close)",
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
fn rejects_when_signal_roles_are_missing_explicit_order_declarations() {
    let err = compile_script(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
entry short = false
exit long = false
exit short = false
plot(spot.close)",
    )
    .expect_err("expected compile-time missing order diagnostics");
    assert!(err.diagnostics.iter().any(|diag| {
        diag.message.contains(
            "signal declaration for `long_entry` requires a matching `order ...` declaration",
        )
    }));
    assert!(err.diagnostics.iter().any(|diag| {
        diag.message.contains(
            "signal declaration for `long_exit` requires a matching `order ...` declaration",
        )
    }));
}

#[test]
fn long_trade_applies_next_bar_open_and_marks_equity() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
order entry long = market(venue = spot)
order exit long = market(venue = spot)
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
fn portfolio_orders_can_bind_to_a_single_execution_alias() {
    let compiled = compile(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
source right = gate.spot(\"BTC_USDT\")
execution bin_exec = binance.spot(\"BTCUSDT\")
execution gate_exec = gate.spot(\"BTC_USDT\")
entry long = left.close > left.close[1]
entry short = false
exit long = false
exit short = false
order entry long = market(venue = gate_exec)
order entry short = market(venue = gate_exec)
order exit long = market(venue = gate_exec)
order exit short = market(venue = gate_exec)
size entry long = 0.4
plot(left.close)",
    )
    .expect("script should compile");
    let runtime = runtime_with_execution_feeds(
        vec![
            bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
            bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                12.0,
                12.0,
            ),
        ],
        vec![
            bar(support::JAN_1_2024_UTC_MS, 20.0, 20.0),
            bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 21.0, 21.0),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                22.0,
                22.0,
            ),
        ],
        vec![
            bar(support::JAN_1_2024_UTC_MS, 100.0, 100.0),
            bar(
                support::JAN_1_2024_UTC_MS + support::MINUTE_MS,
                101.0,
                101.0,
            ),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                102.0,
                102.0,
            ),
        ],
        vec![
            bar(support::JAN_1_2024_UTC_MS, 200.0, 200.0),
            bar(
                support::JAN_1_2024_UTC_MS + support::MINUTE_MS,
                201.0,
                201.0,
            ),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                202.0,
                202.0,
            ),
        ],
    );
    let mut config = config("bin_exec");
    config.portfolio_execution_aliases = vec!["bin_exec".to_string(), "gate_exec".to_string()];

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config)
        .expect("backtest succeeds");

    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.fills[0].execution_alias, "gate_exec");
    assert_eq!(result.orders.len(), 1);
    assert_eq!(result.orders[0].execution_alias, "gate_exec");
    assert_eq!(result.summary.peak_open_position_count, 1);
}

#[test]
fn attached_exits_can_bind_to_a_single_execution_alias() {
    let compiled = compile(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
source right = gate.spot(\"BTC_USDT\")
execution bin_exec = binance.spot(\"BTCUSDT\")
execution gate_exec = gate.spot(\"BTC_USDT\")
entry long = left.close > left.close[1]
entry short = false
exit long = false
exit short = false
order entry long = market(venue = gate_exec)
order entry short = market(venue = gate_exec)
order exit long = market(venue = gate_exec)
order exit short = market(venue = gate_exec)
protect long = stop_market(trigger_price = position.entry_price - 10, trigger_ref = trigger_ref.last, venue = gate_exec)
target long = take_profit_market(trigger_price = position.entry_price + 1, trigger_ref = trigger_ref.last, venue = gate_exec)
plot(left.close)",
    )
    .expect("script should compile");
    let runtime = runtime_with_execution_feeds(
        vec![
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
        vec![
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
        vec![
            bar(support::JAN_1_2024_UTC_MS, 100.0, 100.0),
            bar(
                support::JAN_1_2024_UTC_MS + support::MINUTE_MS,
                101.0,
                101.0,
            ),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                102.0,
                102.0,
            ),
            bar(
                support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                103.0,
                103.0,
            ),
        ],
        vec![
            bar(support::JAN_1_2024_UTC_MS, 200.0, 200.0),
            bar(
                support::JAN_1_2024_UTC_MS + support::MINUTE_MS,
                201.0,
                201.0,
            ),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                202.0,
                202.0,
            ),
            bar(
                support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                203.0,
                203.0,
            ),
        ],
    );
    let mut config = config("bin_exec");
    config.portfolio_execution_aliases = vec!["bin_exec".to_string(), "gate_exec".to_string()];

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config)
        .expect("backtest succeeds");

    assert!(!result.fills.is_empty());
    assert!(result
        .fills
        .iter()
        .all(|fill| fill.execution_alias == "gate_exec"));
    assert!(result
        .orders
        .iter()
        .all(|order| order.execution_alias == "gate_exec"));
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
order entry long = market(venue = spot)
order exit long = market(venue = spot)
order entry short = market(venue = spot)
order exit short = market(venue = spot)
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
fn cooldown_blocks_same_side_reentry_for_declared_bars() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
cooldown long = 2
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
entry short = false
exit short = false
order entry long = market(venue = spot)
order exit long = market(venue = spot)
order entry short = market(venue = spot)
order exit short = market(venue = spot)
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
                    9.0,
                    9.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                    10.0,
                    10.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 4 * support::MINUTE_MS,
                    11.0,
                    11.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 5 * support::MINUTE_MS,
                    12.0,
                    12.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 6 * support::MINUTE_MS,
                    13.0,
                    13.0,
                ),
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 3);
    assert_eq!(result.trades.len(), 1);
    assert_eq!(result.fills[0].bar_index, 2);
    assert_eq!(result.fills[1].bar_index, 3);
    assert_eq!(result.fills[2].bar_index, 6);
    assert_eq!(
        result
            .diagnostics
            .opportunity_events
            .iter()
            .filter(|event| event.kind == palmscript::OpportunityEventKind::SignalIgnoredCooldown)
            .count(),
        2
    );
}

#[test]
fn activation_time_warms_history_without_creating_pre_session_orders() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
entry short = false
exit long = false
exit short = false
order entry long = market(venue = spot)
order entry short = market(venue = spot)
order exit long = market(venue = spot)
order exit short = market(venue = spot)
plot(spot.close)",
    )
    .expect("script should compile");
    let start_time_ms = support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS;
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
    let mut backtest = config("spot");
    backtest.activation_time_ms = Some(start_time_ms);

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), backtest)
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.fills[0].bar_index, 3);
    assert_eq!(result.equity_curve.len(), 2);
    assert_eq!(result.equity_curve[0].time as i64, start_time_ms);
}

#[test]
fn summary_mode_keeps_per_bar_trace_empty() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
entry short = false
exit long = spot.close < spot.close[1]
exit short = false
order entry long = market(venue = spot)
order entry short = market(venue = spot)
order exit long = market(venue = spot)
order exit short = market(venue = spot)
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
                    9.0,
                    9.0,
                ),
            ],
        }],
    };
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");
    assert!(result.diagnostics.per_bar_trace.is_empty());
}

#[test]
fn portfolio_mode_opens_positions_on_multiple_execution_aliases() {
    let compiled = compile(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
source right = gate.spot(\"BTC_USDT\")
entry long = left.close > left.close[1]
entry short = right.close > right.close[1]
order entry long = market(venue = left)
order entry short = market(venue = right)
size entry long = 0.4
size entry short = 0.4
exit long = false
exit short = false
order exit long = market(venue = left)
order exit short = market(venue = right)
plot(left.close)",
    )
    .expect("script should compile");
    let runtime = multi_source_runtime(
        vec![
            bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
            bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                12.0,
                12.0,
            ),
        ],
        vec![
            bar(support::JAN_1_2024_UTC_MS, 20.0, 20.0),
            bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 21.0, 21.0),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                22.0,
                22.0,
            ),
        ],
    );
    let mut backtest = config("left");
    backtest.portfolio_execution_aliases = vec!["left".to_string(), "right".to_string()];
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), backtest)
        .expect("portfolio backtest should succeed");

    assert!(result.diagnostics.portfolio_mode);
    assert_eq!(result.open_positions.len(), 2);
    assert_eq!(result.summary.peak_open_position_count, 2);
    assert_eq!(
        result
            .equity_curve
            .iter()
            .map(|point| point.open_position_count)
            .max()
            .unwrap_or(0),
        2
    );
    assert_eq!(result.fills.len(), 2);
    assert!(result
        .open_positions
        .iter()
        .any(|position| position.execution_alias == "left"));
    assert!(result
        .open_positions
        .iter()
        .any(|position| position.execution_alias == "right"));
}

#[test]
fn spot_virtual_rebalance_transfers_quote_between_portfolio_aliases() {
    let compiled = compile(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
source right = gate.spot(\"BTC_USDT\")
entry long = left.close > left.close[1]
order entry long = market(venue = left)
size entry long = 1.0
plot(left.close)",
    )
    .expect("script should compile");
    let runtime = multi_source_runtime(
        vec![
            bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
            bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                12.0,
                12.0,
            ),
        ],
        vec![
            bar(support::JAN_1_2024_UTC_MS, 20.0, 20.0),
            bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 20.0, 20.0),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                20.0,
                20.0,
            ),
        ],
    );
    let mut backtest = config("left");
    backtest.portfolio_execution_aliases = vec!["left".to_string(), "right".to_string()];
    backtest.spot_virtual_rebalance = true;
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), backtest)
        .expect("spot virtual rebalance portfolio backtest should succeed");

    assert!(result.diagnostics.portfolio_mode);
    assert!(result.diagnostics.spot_virtual_portfolio);
    assert_eq!(result.diagnostics.spot_quote_transfers.len(), 1);
    let transfer = &result.diagnostics.spot_quote_transfers[0];
    assert_eq!(transfer.from_alias, "right");
    assert_eq!(transfer.to_alias, "left");
    assert!((transfer.amount - 500.0).abs() < 1e-9);
    assert_eq!(result.open_positions.len(), 1);
    assert_eq!(result.open_positions[0].execution_alias, "left");
}

#[test]
fn spot_virtual_rebalance_rejects_short_spot_roles() {
    let compiled = compile(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
source right = gate.spot(\"BTC_USDT\")
entry long = left.close > left.close[1]
entry short = right.close > right.close[1]
order entry long = market(venue = left)
order entry short = market(venue = right)
size entry long = 0.5
size entry short = 0.5
plot(left.close)",
    )
    .expect("script should compile");
    let runtime = multi_source_runtime(
        vec![
            bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
            bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
        ],
        vec![
            bar(support::JAN_1_2024_UTC_MS, 20.0, 20.0),
            bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 21.0, 21.0),
        ],
    );
    let mut backtest = config("left");
    backtest.portfolio_execution_aliases = vec!["left".to_string(), "right".to_string()];
    backtest.spot_virtual_rebalance = true;
    let error = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), backtest)
        .expect_err("short spot roles should be rejected");

    assert_eq!(
        error,
        BacktestError::SpotVirtualRebalanceShortRoleUnsupported {
            alias: "right".to_string(),
            role: SignalRole::ShortEntry,
        }
    );
}

#[test]
fn portfolio_controls_block_second_entry_when_max_positions_is_reached() {
    let compiled = compile(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
source right = gate.spot(\"BTC_USDT\")
max_positions = 1
entry long = left.close > left.close[1]
entry short = right.close > right.close[1]
order entry long = market(venue = left)
order entry short = market(venue = right)
size entry long = 0.4
size entry short = 0.4
exit long = false
exit short = false
order exit long = market(venue = left)
order exit short = market(venue = right)
plot(left.close)",
    )
    .expect("script should compile");
    let runtime = multi_source_runtime(
        vec![
            bar(support::JAN_1_2024_UTC_MS, 10.0, 10.0),
            bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 11.0, 11.0),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                12.0,
                12.0,
            ),
        ],
        vec![
            bar(support::JAN_1_2024_UTC_MS, 20.0, 20.0),
            bar(support::JAN_1_2024_UTC_MS + support::MINUTE_MS, 21.0, 21.0),
            bar(
                support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                22.0,
                22.0,
            ),
        ],
    );
    let mut backtest = trace_config("left");
    backtest.portfolio_execution_aliases = vec!["left".to_string(), "right".to_string()];
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), backtest)
        .expect("portfolio backtest should succeed");

    assert_eq!(result.open_positions.len(), 1);
    assert_eq!(result.summary.peak_open_position_count, 1);
    assert_eq!(result.diagnostics.blocked_portfolio_entries.len(), 1);
    assert_eq!(
        result.diagnostics.blocked_portfolio_entries[0].kind,
        palmscript::backtest::PortfolioControlKind::MaxPositions
    );
    assert!(result.diagnostics.per_bar_trace.iter().any(|trace| {
        trace.signal_decisions.iter().any(|decision| {
            decision.reason == palmscript::DecisionReason::PortfolioMaxPositionsExceeded
        })
    }));
}

#[test]
fn full_trace_records_cooldown_and_forced_exit_reasons() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
cooldown long = 2
max_bars_in_trade long = 1
entry long = spot.close > spot.close[1]
entry short = false
exit long = false
exit short = false
order entry long = market(venue = spot)
order entry short = market(venue = spot)
order exit long = market(venue = spot)
order exit short = market(venue = spot)
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
                bar(
                    support::JAN_1_2024_UTC_MS + 4 * support::MINUTE_MS,
                    14.0,
                    14.0,
                ),
            ],
        }],
    };
    let result = run_backtest_with_sources(
        &compiled,
        runtime,
        VmLimits::default(),
        trace_config("spot"),
    )
    .expect("backtest should succeed");

    assert!(!result.diagnostics.per_bar_trace.is_empty());
    assert!(result
        .diagnostics
        .per_bar_trace
        .iter()
        .flat_map(|trace| trace.signal_decisions.iter())
        .any(|decision| decision.reason == palmscript::DecisionReason::CooldownActive));
    assert!(result
        .diagnostics
        .per_bar_trace
        .iter()
        .flat_map(|trace| trace.order_decisions.iter())
        .any(|decision| decision.reason == palmscript::DecisionReason::ForcedMaxBarsExit));
}

#[test]
fn max_bars_in_trade_forces_next_open_exit() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
cooldown long = 10
max_bars_in_trade long = 2
entry long = spot.close > spot.close[1]
exit long = false
entry short = false
exit short = false
order entry long = market(venue = spot)
order exit long = market(venue = spot)
order entry short = market(venue = spot)
order exit short = market(venue = spot)
export timed_out = last_long_exit.kind == exit_kind.signal
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
                bar(
                    support::JAN_1_2024_UTC_MS + 4 * support::MINUTE_MS,
                    14.0,
                    14.0,
                ),
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 2);
    assert_eq!(result.trades.len(), 1);
    assert_eq!(result.fills[0].bar_index, 2);
    assert_eq!(result.fills[1].bar_index, 4);
    assert_eq!(result.diagnostics.trade_diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics.trade_diagnostics[0].exit_classification,
        TradeExitClassification::Signal
    );
    assert_eq!(result.diagnostics.trade_diagnostics[0].bars_held, 2);
}

#[test]
fn diagnostics_capture_trade_context_and_excursions() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
export trend_state = spot.close > spot.close[1]
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
order entry long = market(venue = spot)
order exit long = market(venue = spot)
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
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                Bar {
                    open: 11.0,
                    high: 11.0,
                    low: 8.0,
                    close: 8.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                Bar {
                    open: 7.0,
                    high: 7.0,
                    low: 6.0,
                    close: 7.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 4 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
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
    assert_eq!(result.diagnostics.capture_summary.flat_bar_count, 3);
    assert_eq!(result.diagnostics.capture_summary.long_bar_count, 2);
    assert_eq!(result.diagnostics.capture_summary.in_market_bar_count, 2);
    let export_summary = result
        .diagnostics
        .export_summaries
        .iter()
        .find_map(|summary| match summary {
            palmscript::ExportDiagnosticSummary::Bool(summary) if summary.name == "trend_state" => {
                Some(summary)
            }
            _ => None,
        })
        .expect("bool export summary should exist");
    assert_eq!(export_summary.true_count, 2);
    assert_eq!(export_summary.false_count, 2);
    assert_eq!(export_summary.rising_edge_count, 1);
    assert_eq!(export_summary.trade_count, 1);
    let activation = result
        .diagnostics
        .opportunity_events
        .iter()
        .find(|event| {
            matches!(
                event.kind,
                palmscript::OpportunityEventKind::ExportActivated
            ) && event.name == "trend_state"
        })
        .expect("export activation event should exist");
    assert_eq!(activation.forward_returns[0].horizon_bars, 1);
}

#[test]
fn fees_and_slippage_adjust_fill_prices_and_fees() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
order entry long = market(venue = spot)
order exit long = market(venue = spot)
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
    cfg.maker_fee_bps = 100.0;
    cfg.taker_fee_bps = 100.0;
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
fn market_orders_use_taker_fee_schedule() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
order entry long = market(venue = spot)
order exit long = market(venue = spot)
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
    cfg.maker_fee_bps = 0.0;
    cfg.taker_fee_bps = 100.0;
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), cfg)
        .expect("backtest should succeed");

    approx_eq(result.fills[0].fee, result.fills[0].notional * 0.01);
    approx_eq(result.fills[1].fee, result.fills[1].notional * 0.01);
}

#[test]
fn resting_limit_orders_use_maker_fee_schedule() {
    let entry_time = support::JAN_1_2024_UTC_MS;
    let exit_time = support::JAN_1_2024_UTC_MS + support::MINUTE_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.time == {entry_time}
exit long = spot.time == {exit_time}
order entry long = limit(price = 9.5, tif = tif.gtc, post_only = true, venue = spot)
order exit long = market(venue = spot)
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
                Bar {
                    open: 10.0,
                    high: 10.0,
                    low: 9.5,
                    close: 10.0,
                    volume: 1.0,
                    time: (support::JAN_1_2024_UTC_MS + support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                Bar {
                    open: 8.0,
                    high: 8.0,
                    low: 8.0,
                    close: 8.0,
                    volume: 1.0,
                    time: (support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
            ],
        }],
    };
    let mut cfg = config("spot");
    cfg.maker_fee_bps = 0.0;
    cfg.taker_fee_bps = 100.0;
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), cfg)
        .expect("backtest should succeed");

    approx_eq(result.fills[0].price, 9.5);
    approx_eq(result.fills[0].fee, 0.0);
    approx_eq(result.fills[1].fee, result.fills[1].notional * 0.01);
}

#[test]
fn short_trade_marks_to_market_and_realizes_on_exit() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry short = spot.close < spot.close[1]
exit short = spot.close > spot.close[1]
order entry short = market(venue = spot)
order exit short = market(venue = spot)
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
entry long = spot.close > spot.close[1]
entry short = spot.close < spot.close[1]
order entry long = market(venue = spot)
order entry short = market(venue = spot)
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
entry long = spot.close > spot.close[1]
order entry long = market(venue = spot)
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
    assert!(result.diagnostics.opportunity_events.iter().any(|event| {
        matches!(
            event.kind,
            palmscript::OpportunityEventKind::SignalIgnoredSameSide
        ) && event.role == Some(SignalRole::LongEntry)
    }));
}

#[test]
fn same_side_reentry_can_scale_in_with_entry_size() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.time == {t0} or spot.time == {}
order entry long = market(venue = spot)
size entry long = 0.5
exit long = false
order exit long = market(venue = spot)
plot(spot.close)",
        t0 + support::MINUTE_MS
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                bar(t0 + support::MINUTE_MS, 12.0, 12.0),
                bar(t0 + 2 * support::MINUTE_MS, 14.0, 14.0),
                bar(t0 + 3 * support::MINUTE_MS, 15.0, 15.0),
            ],
        }],
    };
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 2);
    assert_eq!(result.fills[0].action, palmscript::FillAction::Buy);
    assert_eq!(result.fills[1].action, palmscript::FillAction::Buy);
    approx_eq(result.fills[0].quantity, 500.0 / 12.0);
    approx_eq(result.fills[1].quantity, 250.0 / 14.0);
    let open_position = result.open_position.expect("position should remain open");
    assert_eq!(open_position.side, palmscript::PositionSide::Long);
    approx_eq(
        open_position.quantity,
        result.fills[0].quantity + result.fills[1].quantity,
    );
    approx_eq(
        open_position.entry_price,
        ((result.fills[0].quantity * result.fills[0].price)
            + (result.fills[1].quantity * result.fills[1].price))
            / open_position.quantity,
    );
    assert!(result.diagnostics.opportunity_events.iter().all(|event| {
        !(matches!(
            event.kind,
            palmscript::OpportunityEventKind::SignalIgnoredSameSide
        ) && event.role == Some(SignalRole::LongEntry))
    }));
}

#[test]
fn entry_module_size_uses_existing_entry_sizing_runtime() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
module breakout = entry long
entry long = spot.time == {t0}
order entry long = market(venue = spot)
size module breakout = 0.4
exit long = false
order exit long = market(venue = spot)
plot(spot.close)"
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                bar(t0 + support::MINUTE_MS, 12.0, 12.0),
                bar(t0 + 2 * support::MINUTE_MS, 14.0, 14.0),
            ],
        }],
    };
    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.fills[0].action, palmscript::FillAction::Buy);
    approx_eq(result.fills[0].quantity, 400.0 / 12.0);
    let open_position = result.open_position.expect("position should remain open");
    assert_eq!(open_position.side, palmscript::PositionSide::Long);
    approx_eq(open_position.quantity, result.fills[0].quantity);
}

#[test]
fn conflicting_entries_are_rejected() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = true
entry short = true
order entry long = market(venue = spot)
order entry short = market(venue = spot)
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
source signal = bybit.usdt_perps(\"BTCUSDT\")
entry long = signal.close > signal.close[1]
order entry long = market(venue = exec)
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
entry long = spot.close > spot.close[1]
order entry long = market(venue = spot)
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
order entry long = market(venue = spot)
order exit long = market(venue = spot)
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
order entry long = limit(price = spot.close[1], tif = tif.gtc, post_only = false, venue = spot)
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
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
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
order entry long = stop_market(trigger_price = spot.close + 1, trigger_ref = trigger_ref.last, venue = spot)
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
order entry long = stop_limit(trigger_price = spot.close + 1, limit_price = spot.close, tif = tif.gtc, post_only = false, trigger_ref = trigger_ref.last, expire_time_ms = spot.time + 600000, venue = spot)
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
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                Bar {
                    open: 10.0,
                    high: 10.0,
                    low: 9.0,
                    close: 10.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
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
fn isolated_perp_mode_liquidates_on_mark_price_touch_and_surfaces_exit_state() {
    let compiled = compile(
        "interval 1m
source perp = binance.usdm(\"BTCUSDT\")
entry long = perp.close > perp.close[1]
order entry long = market(venue = perp)
plot(perp.close)
export liquidated = position_event.long_liquidation_fill
export last_kind = last_long_exit.kind == exit_kind.liquidation",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 100.0, 100.0),
                bar(
                    support::JAN_1_2024_UTC_MS + support::MINUTE_MS,
                    100.0,
                    101.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                    101.0,
                    101.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                    101.0,
                    101.0,
                ),
            ],
        }],
    };
    let mark_bars = vec![
        bar(support::JAN_1_2024_UTC_MS, 100.0, 100.0),
        bar(
            support::JAN_1_2024_UTC_MS + support::MINUTE_MS,
            100.0,
            100.0,
        ),
        Bar {
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 0.0,
            time: (support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS) as f64,
            funding_rate: None,
            open_interest: None,
            mark_price: None,
            index_price: None,
            premium_index: None,
            basis: None,
        },
        Bar {
            open: 85.0,
            high: 86.0,
            low: 80.0,
            close: 84.0,
            volume: 0.0,
            time: (support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS) as f64,
            funding_rate: None,
            open_interest: None,
            mark_price: None,
            index_price: None,
            premium_index: None,
            basis: None,
        },
    ];

    let result = run_backtest_with_sources(
        &compiled,
        runtime,
        VmLimits::default(),
        binance_perp_config("perp", 5.0, mark_bars),
    )
    .expect("perp liquidation backtest should succeed");

    assert_eq!(result.trades.len(), 1);
    assert_eq!(
        result.diagnostics.trade_diagnostics[0].exit_classification,
        palmscript::TradeExitClassification::Liquidation
    );
    assert_eq!(result.diagnostics.summary.liquidation_exit_count, 1);
    assert!(result
        .outputs
        .exports
        .iter()
        .find(|series| series.name == "liquidated")
        .expect("liquidated export")
        .points
        .iter()
        .any(|point| matches!(point.value, palmscript::OutputValue::Bool(true))));
    assert!(result
        .outputs
        .exports
        .iter()
        .find(|series| series.name == "last_kind")
        .expect("last_kind export")
        .points
        .iter()
        .any(|point| matches!(point.value, palmscript::OutputValue::Bool(true))));
    assert!(result.perp.is_some());
    approx_eq(
        result.summary.realized_pnl,
        result.summary.ending_equity - result.summary.starting_equity,
    );
    assert!(result.summary.ending_equity >= 0.0);
}

#[test]
fn isolated_perp_does_not_liquidate_on_entry_bar_range() {
    let compiled = compile(
        "interval 1m
source perp = binance.usdm(\"BTCUSDT\")
entry long = perp.close > perp.close[1]
order entry long = market(venue = perp)
plot(perp.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 100.0, 100.0),
                bar(
                    support::JAN_1_2024_UTC_MS + support::MINUTE_MS,
                    100.0,
                    101.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                    101.0,
                    101.0,
                ),
            ],
        }],
    };
    let mark_bars = vec![
        bar(support::JAN_1_2024_UTC_MS, 100.0, 100.0),
        Bar {
            open: 100.0,
            high: 105.0,
            low: 80.0,
            close: 100.0,
            volume: 0.0,
            time: (support::JAN_1_2024_UTC_MS + support::MINUTE_MS) as f64,
            funding_rate: None,
            open_interest: None,
            mark_price: None,
            index_price: None,
            premium_index: None,
            basis: None,
        },
        bar(
            support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
            101.0,
            101.0,
        ),
    ];

    let result = run_backtest_with_sources(
        &compiled,
        runtime,
        VmLimits::default(),
        binance_perp_config("perp", 5.0, mark_bars),
    )
    .expect("perp liquidation backtest should succeed");

    assert_eq!(result.diagnostics.summary.liquidation_exit_count, 0);
    assert!(result.trades.is_empty());
    assert!(result.open_position.is_some());
}

#[test]
fn isolated_perp_liquidation_caps_loss_to_isolated_margin() {
    let compiled = compile(
        "interval 1m
source perp = binance.usdm(\"BTCUSDT\")
entry long = perp.close > perp.close[1]
order entry long = market(venue = perp)
plot(perp.close)",
    )
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(support::JAN_1_2024_UTC_MS, 100.0, 100.0),
                bar(
                    support::JAN_1_2024_UTC_MS + support::MINUTE_MS,
                    100.0,
                    101.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS,
                    101.0,
                    101.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS,
                    101.0,
                    102.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 4 * support::MINUTE_MS,
                    102.0,
                    103.0,
                ),
                bar(
                    support::JAN_1_2024_UTC_MS + 5 * support::MINUTE_MS,
                    103.0,
                    103.0,
                ),
            ],
        }],
    };
    let mark_bars = vec![
        bar(support::JAN_1_2024_UTC_MS, 100.0, 100.0),
        bar(
            support::JAN_1_2024_UTC_MS + support::MINUTE_MS,
            100.0,
            100.0,
        ),
        Bar {
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 0.0,
            time: (support::JAN_1_2024_UTC_MS + 2 * support::MINUTE_MS) as f64,
            funding_rate: None,
            open_interest: None,
            mark_price: None,
            index_price: None,
            premium_index: None,
            basis: None,
        },
        Bar {
            open: 40.0,
            high: 45.0,
            low: 35.0,
            close: 40.0,
            volume: 0.0,
            time: (support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS) as f64,
            funding_rate: None,
            open_interest: None,
            mark_price: None,
            index_price: None,
            premium_index: None,
            basis: None,
        },
        bar(
            support::JAN_1_2024_UTC_MS + 4 * support::MINUTE_MS,
            102.0,
            102.0,
        ),
        bar(
            support::JAN_1_2024_UTC_MS + 5 * support::MINUTE_MS,
            103.0,
            103.0,
        ),
    ];

    let result = run_backtest_with_sources(
        &compiled,
        runtime,
        VmLimits::default(),
        binance_perp_config("perp", 5.0, mark_bars),
    )
    .expect("perp liquidation backtest should succeed");

    assert_eq!(result.trades.len(), 1);
    assert_eq!(result.diagnostics.summary.liquidation_exit_count, 1);
    assert!(result.open_position.is_none());
    assert!(result.summary.ending_equity >= 0.0);
    approx_eq(
        result.summary.realized_pnl,
        result.summary.ending_equity - result.summary.starting_equity,
    );
    assert!(result
        .orders
        .iter()
        .any(|order| order.end_reason == Some(OrderEndReason::InsufficientCollateral)));
}

#[test]
fn ioc_limit_cancels_when_not_filled_on_first_bar() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
order entry long = limit(price = spot.close[1] - 2, tif = tif.ioc, post_only = false, venue = spot)
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
order entry long = stop_limit(trigger_price = spot.close + 1, limit_price = spot.close, tif = tif.gtd, post_only = false, trigger_ref = trigger_ref.last, expire_time_ms = spot.time + 120000, venue = spot)
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
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                Bar {
                    open: 10.0,
                    high: 10.0,
                    low: 9.0,
                    close: 10.0,
                    volume: 1_000.0,
                    time: (support::JAN_1_2024_UTC_MS + 3 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
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
order entry long = limit(price = spot.close[1], tif = tif.gtc, post_only = false, venue = spot)
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
    assert!(result.diagnostics.opportunity_events.iter().any(|event| {
        matches!(
            event.kind,
            palmscript::OpportunityEventKind::SignalReplacedPendingOrder
        ) && event.role == Some(SignalRole::LongEntry)
    }));
}

#[test]
fn post_only_limit_cancels_when_order_would_cross() {
    let compiled = compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
order entry long = limit(price = spot.close + 2, tif = tif.gtc, post_only = true, venue = spot)
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
            "interval 1m\nsource spot = binance.spot(\"BTCUSDT\")\nentry long = spot.close > spot.close[1]\norder entry long = stop_market(trigger_price = spot.close + 1, trigger_ref = trigger_ref.mark, venue = spot)\nplot(spot.close)",
        ),
        (
            "binance usdm",
            "interval 1m\nsource spot = binance.usdm(\"BTCUSDT\")\nentry long = spot.close > spot.close[1]\norder entry long = stop_market(trigger_price = spot.close + 1, trigger_ref = trigger_ref.index, venue = spot)\nplot(spot.close)",
        ),
        (
            "bybit spot",
            "interval 1m\nsource spot = bybit.spot(\"BTCUSDT\")\nentry long = spot.close > spot.close[1]\norder entry long = stop_market(trigger_price = spot.close + 1, trigger_ref = trigger_ref.mark, venue = spot)\nplot(spot.close)",
        ),
        (
            "gate perps",
            "interval 1m\nsource spot = gate.usdt_perps(\"BTC_USDT\")\nentry long = spot.close > spot.close[1]\norder entry long = stop_market(trigger_price = spot.close + 1, trigger_ref = trigger_ref.index, venue = spot)\nplot(spot.close)",
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

#[test]
fn attached_exits_use_position_state_and_protect_wins_same_bar_ambiguity() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.time == {t0}
order entry long = market(venue = spot)
protect long = stop_market(trigger_price = position.entry_price - 1, trigger_ref = trigger_ref.last, venue = spot)
target long = take_profit_market(trigger_price = position.entry_price + 2, trigger_ref = trigger_ref.last, venue = spot)
plot(spot.close)"
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                bar(t0 + support::MINUTE_MS, 11.0, 11.0),
                bar(t0 + 2 * support::MINUTE_MS, 11.0, 11.5),
                Bar {
                    open: 10.5,
                    high: 14.0,
                    low: 9.0,
                    close: 12.0,
                    volume: 1_000.0,
                    time: (t0 + 3 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                bar(t0 + 4 * support::MINUTE_MS, 12.0, 12.0),
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.trades.len(), 1);
    assert_eq!(
        result.diagnostics.trade_diagnostics[0].exit_classification,
        palmscript::TradeExitClassification::Protect
    );
    approx_eq(result.trades[0].exit.price, 10.0);

    let protect_order = result
        .orders
        .iter()
        .rev()
        .find(|order| order.role == SignalRole::ProtectLong)
        .expect("protect order should exist");
    let target_order = result
        .orders
        .iter()
        .rev()
        .find(|order| order.role == SignalRole::TargetLong)
        .expect("target order should exist");
    assert_eq!(protect_order.status, OrderStatus::Filled);
    assert_eq!(target_order.status, OrderStatus::Cancelled);
    assert_eq!(target_order.end_reason, Some(OrderEndReason::OcoCancelled));

    let protect_diag = result
        .diagnostics
        .order_diagnostics
        .iter()
        .find(|diag| diag.role == SignalRole::ProtectLong)
        .expect("protect diagnostic should exist");
    let placed_position = protect_diag
        .placed_position
        .as_ref()
        .expect("attached order should capture placed position");
    approx_eq(placed_position.entry_price, 11.0);
}

#[test]
fn signal_exit_cancels_attached_exits_when_position_closes() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.time == {t0}
exit long = spot.time == {}
order entry long = market(venue = spot)
order exit long = market(venue = spot)
protect long = stop_market(trigger_price = position.entry_price - 5, trigger_ref = trigger_ref.last, venue = spot)
target long = take_profit_market(trigger_price = position.entry_price + 5, trigger_ref = trigger_ref.last, venue = spot)
plot(spot.close)",
        t0 + 2 * support::MINUTE_MS
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                bar(t0 + support::MINUTE_MS, 11.0, 11.0),
                bar(t0 + 2 * support::MINUTE_MS, 11.0, 11.2),
                bar(t0 + 3 * support::MINUTE_MS, 11.5, 11.5),
                bar(t0 + 4 * support::MINUTE_MS, 11.0, 11.0),
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(
        result.diagnostics.trade_diagnostics[0].exit_classification,
        palmscript::TradeExitClassification::Signal
    );
    assert!(result.orders.iter().any(|order| {
        order.role == SignalRole::ProtectLong
            && order.status == OrderStatus::Cancelled
            && order.end_reason == Some(OrderEndReason::PositionClosed)
    }));
    assert!(result.orders.iter().any(|order| {
        order.role == SignalRole::TargetLong
            && order.status == OrderStatus::Cancelled
            && order.end_reason == Some(OrderEndReason::PositionClosed)
    }));
}

#[test]
fn partial_target_exit_closes_a_slice_and_leaves_runner_for_protect() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.time == {t0}
order entry long = market(venue = spot)
protect long = stop_market(trigger_price = position.entry_price - 1, trigger_ref = trigger_ref.last, venue = spot)
target long = take_profit_market(trigger_price = position.entry_price + 2, trigger_ref = trigger_ref.last, venue = spot)
size target long = 0.5
plot(spot.close)"
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                bar(t0 + support::MINUTE_MS, 11.0, 11.0),
                Bar {
                    open: 12.0,
                    high: 13.5,
                    low: 11.5,
                    close: 13.0,
                    volume: 1_000.0,
                    time: (t0 + 2 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                Bar {
                    open: 10.0,
                    high: 10.5,
                    low: 9.0,
                    close: 9.5,
                    volume: 1_000.0,
                    time: (t0 + 3 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.trades.len(), 2);
    approx_eq(result.trades[0].quantity, result.trades[1].quantity);
    assert_eq!(
        result.diagnostics.trade_diagnostics[0].exit_classification,
        palmscript::TradeExitClassification::Target
    );
    assert_eq!(
        result.diagnostics.trade_diagnostics[1].exit_classification,
        palmscript::TradeExitClassification::Protect
    );
    let target_order = result
        .orders
        .iter()
        .find(|order| order.role == SignalRole::TargetLong && order.status == OrderStatus::Filled)
        .expect("target order should fill");
    approx_eq(
        target_order
            .size_fraction
            .expect("size fraction should be recorded"),
        0.5,
    );
    assert!(result
        .orders
        .iter()
        .any(|order| order.role == SignalRole::ProtectLong && order.status == OrderStatus::Filled));
    assert_eq!(result.open_position, None);
}

#[test]
fn staged_entries_and_targets_progress_sequentially() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry1 long = spot.time == {t0}
entry2 long = spot.time == {}
order entry1 long = market(venue = spot)
order entry2 long = market(venue = spot)
size entry1 long = 0.5
size entry2 long = 0.5
protect long = stop_market(trigger_price = position.entry_price - 5, trigger_ref = trigger_ref.last, venue = spot)
protect_after_target1 long = stop_market(trigger_price = position.entry_price + 1, trigger_ref = trigger_ref.last, venue = spot)
target1 long = take_profit_market(trigger_price = position.entry_price + 2, trigger_ref = trigger_ref.last, venue = spot)
target2 long = take_profit_market(trigger_price = position.entry_price + 4, trigger_ref = trigger_ref.last, venue = spot)
size target1 long = 0.5
plot(spot.close)",
        t0 + support::MINUTE_MS
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                Bar {
                    open: 10.0,
                    high: 11.0,
                    low: 9.5,
                    close: 11.0,
                    volume: 1_000.0,
                    time: (t0 + support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                Bar {
                    open: 11.0,
                    high: 11.8,
                    low: 10.5,
                    close: 11.5,
                    volume: 1_000.0,
                    time: (t0 + 2 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                Bar {
                    open: 11.5,
                    high: 13.5,
                    low: 11.0,
                    close: 13.0,
                    volume: 1_000.0,
                    time: (t0 + 3 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                Bar {
                    open: 10.8,
                    high: 11.2,
                    low: 10.0,
                    close: 10.5,
                    volume: 1_000.0,
                    time: (t0 + 4 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert!(result
        .orders
        .iter()
        .any(|order| order.role == SignalRole::LongEntry && order.status == OrderStatus::Filled));
    assert!(result
        .orders
        .iter()
        .any(|order| order.role == SignalRole::LongEntry2 && order.status == OrderStatus::Filled));
    assert!(result
        .orders
        .iter()
        .any(|order| order.role == SignalRole::TargetLong && order.status == OrderStatus::Filled));
    assert!(result.orders.iter().any(|order| {
        order.role == SignalRole::ProtectAfterTarget1Long && order.status == OrderStatus::Filled
    }));
    assert!(result.orders.iter().any(|order| {
        order.role == SignalRole::ProtectLong
            && order.status == OrderStatus::Cancelled
            && order.end_reason == Some(OrderEndReason::Rearmed)
    }));
    assert!(result.orders.iter().any(|order| {
        order.role == SignalRole::TargetLong2
            && order.status == OrderStatus::Cancelled
            && order.end_reason == Some(OrderEndReason::OcoCancelled)
    }));
    assert_eq!(
        result
            .diagnostics
            .trade_diagnostics
            .last()
            .expect("trade diagnostic")
            .exit_role,
        SignalRole::ProtectAfterTarget1Long
    );
    assert_eq!(result.open_position, None);
}

#[test]
fn risk_sized_long_entry_uses_stop_distance() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
let stop_price = 8
entry long = spot.time == {t0}
order entry long = market(venue = spot)
size entry long = risk_pct(0.1, stop_price)
protect long = stop_market(trigger_price = stop_price, trigger_ref = trigger_ref.last, venue = spot)
plot(spot.close)"
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                bar(t0 + support::MINUTE_MS, 10.0, 10.0),
                Bar {
                    open: 8.0,
                    high: 10.0,
                    low: 7.0,
                    close: 8.0,
                    volume: 1_000.0,
                    time: (t0 + 2 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.trades.len(), 1);
    approx_eq(result.trades[0].quantity, 50.0);
    approx_eq(result.trades[0].realized_pnl, -100.0);
    let entry_order = result
        .orders
        .iter()
        .find(|order| order.role == SignalRole::LongEntry && order.status == OrderStatus::Filled)
        .expect("entry order should fill");
    assert_eq!(entry_order.size_mode, Some(SizeMode::RiskPct));
    approx_eq(
        entry_order
            .requested_risk_pct
            .expect("risk pct should be recorded"),
        0.1,
    );
    approx_eq(
        entry_order
            .requested_stop_price
            .expect("stop price should be recorded"),
        8.0,
    );
    approx_eq(
        entry_order
            .effective_risk_per_unit
            .expect("risk per unit should be recorded"),
        2.0,
    );
    assert!(!entry_order.capital_limited);
}

#[test]
fn risk_sized_entry_marks_capital_limited_when_cash_caps_quantity() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
let stop_price = 9.5
entry long = spot.time == {t0}
order entry long = market(venue = spot)
size entry long = risk_pct(0.1, stop_price)
protect long = stop_market(trigger_price = stop_price, trigger_ref = trigger_ref.last, venue = spot)
plot(spot.close)"
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                bar(t0 + support::MINUTE_MS, 10.0, 10.0),
                Bar {
                    open: 9.5,
                    high: 10.0,
                    low: 9.0,
                    close: 9.5,
                    volume: 1_000.0,
                    time: (t0 + 2 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.trades.len(), 1);
    approx_eq(result.trades[0].quantity, 100.0);
    approx_eq(result.trades[0].realized_pnl, -50.0);
    let entry_order = result
        .orders
        .iter()
        .find(|order| order.role == SignalRole::LongEntry && order.status == OrderStatus::Filled)
        .expect("entry order should fill");
    assert!(entry_order.capital_limited);
    approx_eq(
        entry_order
            .effective_risk_per_unit
            .expect("risk per unit should be recorded"),
        0.5,
    );
}

#[test]
fn position_event_anchors_fire_on_fill_bar_and_drive_since_helpers() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.time == {t0}
order entry long = market(venue = spot)
export entry_fill = position_event.long_entry_fill
export trail = highest_since(position_event.long_entry_fill, spot.high)
plot(spot.close)"
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                Bar {
                    open: 11.0,
                    high: 13.0,
                    low: 10.5,
                    close: 12.0,
                    volume: 1_000.0,
                    time: (t0 + support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                Bar {
                    open: 12.0,
                    high: 12.5,
                    low: 11.5,
                    close: 12.2,
                    volume: 1_000.0,
                    time: (t0 + 2 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                Bar {
                    open: 12.0,
                    high: 14.0,
                    low: 11.8,
                    close: 13.5,
                    volume: 1_000.0,
                    time: (t0 + 3 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.outputs.exports.len(), 2);
    let entry_fill_values: Vec<_> = result.outputs.exports[0]
        .points
        .iter()
        .map(|point| &point.value)
        .collect();
    assert_eq!(
        entry_fill_values,
        vec![
            &palmscript::OutputValue::Bool(false),
            &palmscript::OutputValue::Bool(true),
            &palmscript::OutputValue::Bool(false),
            &palmscript::OutputValue::Bool(false)
        ]
    );

    let trail_values: Vec<_> = result.outputs.exports[1]
        .points
        .iter()
        .map(|point| &point.value)
        .collect();
    assert_eq!(
        trail_values,
        vec![
            &palmscript::OutputValue::NA,
            &palmscript::OutputValue::F64(13.0),
            &palmscript::OutputValue::F64(13.0),
            &palmscript::OutputValue::F64(14.0)
        ]
    );
}

#[test]
fn exit_outcome_events_and_last_exit_snapshot_are_visible_on_fill_bar() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.time == {t0}
order entry long = market(venue = spot)
target long = take_profit_market(trigger_price = position.entry_price + 2, trigger_ref = trigger_ref.last, venue = spot)
export exit_fill = position_event.long_exit_fill
export target_fill = position_event.long_target_fill
export signal_fill = position_event.long_signal_exit_fill
export was_target = last_long_exit.kind == exit_kind.target
export global_long = last_exit.side == position_side.long
export exit_price = last_long_exit.price
export realized_return = last_long_exit.realized_return
export bars_held = last_long_exit.bars_held
plot(spot.close)"
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                bar(t0 + support::MINUTE_MS, 11.0, 11.0),
                Bar {
                    open: 12.0,
                    high: 14.0,
                    low: 11.5,
                    close: 13.0,
                    volume: 1_000.0,
                    time: (t0 + 2 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.trades.len(), 1);
    assert_eq!(
        result.diagnostics.trade_diagnostics[0].exit_classification,
        palmscript::TradeExitClassification::Target
    );

    let exports = &result.outputs.exports;
    assert_eq!(
        exports[0].points[2].value,
        palmscript::OutputValue::Bool(true)
    );
    assert_eq!(
        exports[1].points[2].value,
        palmscript::OutputValue::Bool(true)
    );
    assert_eq!(
        exports[2].points[2].value,
        palmscript::OutputValue::Bool(false)
    );
    assert_eq!(
        exports[3].points[2].value,
        palmscript::OutputValue::Bool(true)
    );
    assert_eq!(
        exports[4].points[2].value,
        palmscript::OutputValue::Bool(true)
    );
    assert_eq!(
        exports[5].points[2].value,
        palmscript::OutputValue::F64(13.0)
    );
    match exports[6].points[2].value {
        palmscript::OutputValue::F64(value) => approx_eq(value, 2.0 / 11.0),
        ref other => panic!("expected realized return f64, found {other:?}"),
    }
    assert_eq!(
        exports[7].points[2].value,
        palmscript::OutputValue::F64(1.0)
    );
}

#[test]
fn same_side_reentry_can_branch_on_target_exit_state() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.time == {t0} or (
    last_long_exit.kind == exit_kind.target
    and barssince(position_event.long_target_fill) == 1
)
order entry long = market(venue = spot)
protect long = stop_market(trigger_price = position.entry_price - 5, trigger_ref = trigger_ref.last, venue = spot)
target long = take_profit_market(trigger_price = position.entry_price + 1, trigger_ref = trigger_ref.last, venue = spot)
plot(spot.close)"
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                bar(t0 + support::MINUTE_MS, 11.0, 11.0),
                Bar {
                    open: 11.5,
                    high: 12.5,
                    low: 11.0,
                    close: 12.0,
                    volume: 1_000.0,
                    time: (t0 + 2 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                bar(t0 + 3 * support::MINUTE_MS, 12.0, 12.2),
                bar(t0 + 4 * support::MINUTE_MS, 13.0, 13.0),
                Bar {
                    open: 13.2,
                    high: 14.5,
                    low: 13.0,
                    close: 14.0,
                    volume: 1_000.0,
                    time: (t0 + 5 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.trades.len(), 2);
    assert_eq!(result.trades[0].entry.bar_index, 1);
    assert_eq!(result.trades[1].entry.bar_index, 4);
    assert!(result
        .diagnostics
        .trade_diagnostics
        .iter()
        .all(|diag| diag.exit_classification == palmscript::TradeExitClassification::Target));
}

#[test]
fn last_exit_global_alias_tracks_most_recent_closed_side() {
    let t0 = support::JAN_1_2024_UTC_MS;
    let compiled = compile(&format!(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.time == {t0}
order entry long = market(venue = spot)
target long = take_profit_market(trigger_price = position.entry_price + 1, trigger_ref = trigger_ref.last, venue = spot)
entry short = last_long_exit.kind == exit_kind.target and barssince(position_event.long_target_fill) == 1
order entry short = market(venue = spot)
target short = take_profit_market(trigger_price = position.entry_price - 1, trigger_ref = trigger_ref.last, venue = spot)
export last_exit_is_short = last_exit.side == position_side.short
plot(spot.close)"
    ))
    .expect("script should compile");
    let runtime = SourceRuntimeConfig {
        base_interval: Interval::Min1,
        feeds: vec![SourceFeed {
            source_id: 0,
            interval: Interval::Min1,
            bars: vec![
                bar(t0, 10.0, 10.0),
                bar(t0 + support::MINUTE_MS, 11.0, 11.0),
                Bar {
                    open: 11.5,
                    high: 12.5,
                    low: 11.0,
                    close: 12.0,
                    volume: 1_000.0,
                    time: (t0 + 2 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
                bar(t0 + 3 * support::MINUTE_MS, 11.8, 11.0),
                bar(t0 + 4 * support::MINUTE_MS, 10.5, 9.5),
                Bar {
                    open: 9.6,
                    high: 9.8,
                    low: 8.8,
                    close: 9.0,
                    volume: 1_000.0,
                    time: (t0 + 5 * support::MINUTE_MS) as f64,
                    funding_rate: None,
                    open_interest: None,
                    mark_price: None,
                    index_price: None,
                    premium_index: None,
                    basis: None,
                },
            ],
        }],
    };

    let result = run_backtest_with_sources(&compiled, runtime, VmLimits::default(), config("spot"))
        .expect("backtest should succeed");

    assert_eq!(result.trades.len(), 2);
    assert_eq!(result.trades[1].side, palmscript::PositionSide::Short);
    assert_eq!(
        result.outputs.exports[0]
            .points
            .last()
            .expect("final export point should exist")
            .value,
        palmscript::OutputValue::Bool(true)
    );
}
