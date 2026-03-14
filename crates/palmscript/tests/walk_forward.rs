mod support;

use palmscript::{
    compile as compile_script, run_walk_forward_sweep_with_source, run_walk_forward_with_sources,
    BacktestConfig, BacktestError, CompileError, CompiledProgram, DiagnosticsDetailMode,
    InputSweepDefinition, Interval, ValidationConstraintConfig, VmLimits, WalkForwardConfig,
    WalkForwardSweepConfig, WalkForwardSweepObjective,
};

use crate::support::{flat_bars, source_runtime_config, JAN_1_2024_UTC_MS, MINUTE_MS};

fn compile(source: &str) -> Result<CompiledProgram, CompileError> {
    compile_script(&support::mirror_execution_decls(source))
}

#[test]
fn walk_forward_builds_rolling_out_of_sample_segments() {
    let source = "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
entry short = false
exit long = spot.close < spot.close[1]
exit short = true
order entry long = market(venue = spot)
order entry short = market(venue = spot)
order exit long = market(venue = spot)
order exit short = market(venue = spot)
plot(spot.close)";
    let compiled = compile(source).expect("script compiles");
    let runtime = source_runtime_config(
        Interval::Min1,
        flat_bars(
            JAN_1_2024_UTC_MS,
            MINUTE_MS,
            &[10.0, 11.0, 12.0, 11.0, 12.0, 13.0, 12.0, 13.0],
        ),
        vec![],
    );

    let result = run_walk_forward_with_sources(
        &compiled,
        runtime,
        VmLimits::default(),
        WalkForwardConfig {
            backtest: BacktestConfig {
                execution_source_alias: "spot".to_string(),
                portfolio_execution_aliases: Vec::new(),
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
            },
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            train_bars: 2,
            test_bars: 2,
            step_bars: 2,
            constraints: ValidationConstraintConfig::default(),
        },
    )
    .expect("walk-forward should succeed");

    assert_eq!(result.segments.len(), 3);
    assert_eq!(result.stitched_summary.segment_count, 3);
    assert_eq!(result.segments[0].train_from, JAN_1_2024_UTC_MS);
    assert_eq!(
        result.segments[0].test_from,
        JAN_1_2024_UTC_MS + 2 * MINUTE_MS
    );
    assert!(!result.stitched_equity_curve.is_empty());
    assert!(result
        .segments
        .iter()
        .all(|segment| segment.out_of_sample.trade_count <= 1));
    assert!(result.segments.iter().all(|segment| segment
        .out_of_sample_diagnostics
        .capture_summary
        .in_market_bar_count
        <= 2));
    assert!(result.segments.iter().all(|segment| segment
        .out_of_sample_diagnostics
        .summary
        .order_fill_rate
        >= 0.0));
    assert!(result.segments.iter().all(|segment| {
        (segment
            .out_of_sample_diagnostics
            .baseline_comparison
            .execution_asset_return
            - segment
                .out_of_sample_diagnostics
                .capture_summary
                .execution_asset_return)
            .abs()
            < 1e-9
    }));
}

#[test]
fn walk_forward_rejects_zero_windows() {
    let source = "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = spot.close > spot.close[1]
entry short = false
exit long = spot.close < spot.close[1]
exit short = true
order entry long = market(venue = spot)
order entry short = market(venue = spot)
order exit long = market(venue = spot)
order exit short = market(venue = spot)";
    let compiled = compile(source).expect("script compiles");
    let runtime = source_runtime_config(
        Interval::Min1,
        flat_bars(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 11.0, 12.0, 11.0]),
        vec![],
    );

    let err = run_walk_forward_with_sources(
        &compiled,
        runtime,
        VmLimits::default(),
        WalkForwardConfig {
            backtest: BacktestConfig {
                execution_source_alias: "spot".to_string(),
                portfolio_execution_aliases: Vec::new(),
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
            },
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            train_bars: 0,
            test_bars: 2,
            step_bars: 2,
            constraints: ValidationConstraintConfig::default(),
        },
    )
    .expect_err("zero train_bars should fail");

    assert_eq!(err, BacktestError::InvalidWalkForwardTrainBars { value: 0 });
}

#[test]
fn walk_forward_sweep_ranks_input_candidates() {
    let source = support::mirror_execution_decls(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
input threshold = 0
entry long = spot.close > spot.close[1] + threshold
entry short = false
exit long = spot.close < spot.close[1]
exit short = true
order entry long = market(venue = spot)
order entry short = market(venue = spot)
order exit long = market(venue = spot)
order exit short = market(venue = spot)",
    );
    let runtime = source_runtime_config(
        Interval::Min1,
        flat_bars(
            JAN_1_2024_UTC_MS,
            MINUTE_MS,
            &[10.0, 11.0, 12.0, 11.0, 12.0, 13.0, 12.0, 13.0],
        ),
        vec![],
    );

    let result = run_walk_forward_sweep_with_source(
        &source,
        runtime,
        VmLimits::default(),
        WalkForwardSweepConfig {
            walk_forward: WalkForwardConfig {
                backtest: BacktestConfig {
                    execution_source_alias: "spot".to_string(),
                    portfolio_execution_aliases: Vec::new(),
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
                },
                diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
                train_bars: 2,
                test_bars: 2,
                step_bars: 2,
                constraints: ValidationConstraintConfig::default(),
            },
            inputs: vec![InputSweepDefinition {
                name: "threshold".to_string(),
                values: vec![0.0, 100.0],
            }],
            objective: WalkForwardSweepObjective::TotalReturn,
            top_n: 2,
            base_input_overrides: std::collections::BTreeMap::new(),
        },
    )
    .expect("walk-forward sweep should succeed");

    assert_eq!(result.candidate_count, 2);
    assert_eq!(result.top_candidates.len(), 2);
    assert_eq!(
        result.best_candidate.input_overrides.get("threshold"),
        Some(&0.0)
    );
    assert!(
        result.top_candidates[0].stitched_summary.total_return
            >= result.top_candidates[1].stitched_summary.total_return
    );
}

#[test]
fn walk_forward_reports_constraint_failures() {
    let source = "interval 1m
source spot = binance.spot(\"BTCUSDT\")
entry long = false
entry short = false
exit long = false
exit short = false
order entry long = market(venue = spot)
order entry short = market(venue = spot)
order exit long = market(venue = spot)
order exit short = market(venue = spot)";
    let compiled = compile(source).expect("script compiles");
    let runtime = source_runtime_config(
        Interval::Min1,
        flat_bars(
            JAN_1_2024_UTC_MS,
            MINUTE_MS,
            &[10.0, 11.0, 12.0, 11.0, 12.0, 13.0, 12.0, 13.0],
        ),
        vec![],
    );

    let result = run_walk_forward_with_sources(
        &compiled,
        runtime,
        VmLimits::default(),
        WalkForwardConfig {
            backtest: BacktestConfig {
                execution_source_alias: "spot".to_string(),
                portfolio_execution_aliases: Vec::new(),
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
            },
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            train_bars: 2,
            test_bars: 2,
            step_bars: 2,
            constraints: ValidationConstraintConfig {
                min_trade_count: Some(1),
                min_holdout_trade_count: None,
                require_positive_holdout: false,
                max_zero_trade_segments: Some(0),
                min_holdout_pass_rate: None,
            },
        },
    )
    .expect("walk-forward should succeed");

    assert!(!result.constraints.passed);
    assert_eq!(result.constraints.violations.len(), 2);
}
