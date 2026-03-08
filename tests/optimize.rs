mod support;

use std::collections::BTreeMap;

use palmscript::{
    compile_with_input_overrides, run_optimize_with_source, BacktestConfig, Interval,
    OptimizeConfig, OptimizeError, OptimizeObjective, OptimizeParamSpace, OptimizeRunner, VmLimits,
    WalkForwardConfig,
};

use crate::support::{flat_bars, source_runtime_config, JAN_1_2024_UTC_MS, MINUTE_MS};

fn optimize_source() -> &'static str {
    "interval 1m
source spot = binance.spot(\"BTCUSDT\")
input threshold = 0
entry long = spot.close > spot.close[1] + threshold
entry short = false
exit long = spot.close < spot.close[1]
exit short = true"
}

fn optimize_runtime() -> palmscript::SourceRuntimeConfig {
    source_runtime_config(
        Interval::Min1,
        flat_bars(
            JAN_1_2024_UTC_MS,
            MINUTE_MS,
            &[10.0, 11.0, 12.0, 11.0, 12.0, 13.0, 12.0, 13.0],
        ),
        vec![],
    )
}

fn optimize_backtest_config() -> BacktestConfig {
    BacktestConfig {
        execution_source_alias: "spot".to_string(),
        initial_capital: 1_000.0,
        fee_bps: 0.0,
        slippage_bps: 0.0,
        perp: None,
        perp_context: None,
    }
}

#[test]
fn optimize_walk_forward_ranks_candidates() {
    let result = run_optimize_with_source(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        OptimizeConfig {
            runner: OptimizeRunner::WalkForward,
            backtest: optimize_backtest_config(),
            walk_forward: Some(WalkForwardConfig {
                backtest: optimize_backtest_config(),
                train_bars: 2,
                test_bars: 2,
                step_bars: 2,
            }),
            params: vec![OptimizeParamSpace::Choice {
                name: "threshold".to_string(),
                values: vec![0.0, 100.0],
            }],
            objective: OptimizeObjective::TotalReturn,
            trials: 8,
            startup_trials: 8,
            seed: 7,
            workers: 2,
            top_n: 3,
            base_input_overrides: BTreeMap::new(),
        },
    )
    .expect("optimize should succeed");

    assert_eq!(result.candidate_count, 8);
    assert_eq!(result.completed_trials, 8);
    assert_eq!(
        result.best_candidate.input_overrides.get("threshold"),
        Some(&0.0)
    );
    assert_eq!(result.top_candidates.len(), 3);
}

#[test]
fn optimize_is_seed_stable_across_worker_counts() {
    let config = OptimizeConfig {
        runner: OptimizeRunner::WalkForward,
        backtest: optimize_backtest_config(),
        walk_forward: Some(WalkForwardConfig {
            backtest: optimize_backtest_config(),
            train_bars: 2,
            test_bars: 2,
            step_bars: 2,
        }),
        params: vec![
            OptimizeParamSpace::Choice {
                name: "threshold".to_string(),
                values: vec![0.0, 100.0],
            },
            OptimizeParamSpace::IntegerRange {
                name: "threshold".to_string(),
                low: 0,
                high: 1,
            },
        ],
        objective: OptimizeObjective::RobustReturn,
        trials: 8,
        startup_trials: 8,
        seed: 99,
        workers: 1,
        top_n: 4,
        base_input_overrides: BTreeMap::new(),
    };
    let err = run_optimize_with_source(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        config.clone(),
    )
    .expect_err("duplicate params should fail");
    assert!(matches!(
        err,
        OptimizeError::DuplicateParam { name } if name == "threshold"
    ));

    let params = vec![OptimizeParamSpace::Choice {
        name: "threshold".to_string(),
        values: vec![0.0, 100.0],
    }];
    let result_one = run_optimize_with_source(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        OptimizeConfig {
            params: params.clone(),
            workers: 1,
            ..config.clone()
        },
    )
    .expect("optimize should succeed");
    let result_many = run_optimize_with_source(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        OptimizeConfig {
            params,
            workers: 3,
            ..config
        },
    )
    .expect("optimize should succeed");

    assert_eq!(result_one.best_candidate, result_many.best_candidate);
    assert_eq!(result_one.top_candidates, result_many.top_candidates);
}

#[test]
fn optimize_best_candidate_round_trips_into_input_overrides() {
    let result = run_optimize_with_source(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        OptimizeConfig {
            runner: OptimizeRunner::Backtest,
            backtest: optimize_backtest_config(),
            walk_forward: None,
            params: vec![OptimizeParamSpace::Choice {
                name: "threshold".to_string(),
                values: vec![0.0, 100.0],
            }],
            objective: OptimizeObjective::EndingEquity,
            trials: 8,
            startup_trials: 8,
            seed: 11,
            workers: 2,
            top_n: 2,
            base_input_overrides: BTreeMap::new(),
        },
    )
    .expect("optimize should succeed");

    let compiled =
        compile_with_input_overrides(optimize_source(), &result.best_candidate.input_overrides)
            .expect("best candidate overrides should compile");
    assert_eq!(compiled.program.declared_sources.len(), 1);
}

#[test]
fn optimize_rejects_missing_walk_forward_config() {
    let err = run_optimize_with_source(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        OptimizeConfig {
            runner: OptimizeRunner::WalkForward,
            backtest: optimize_backtest_config(),
            walk_forward: None,
            params: vec![OptimizeParamSpace::Choice {
                name: "threshold".to_string(),
                values: vec![0.0, 100.0],
            }],
            objective: OptimizeObjective::TotalReturn,
            trials: 4,
            startup_trials: 4,
            seed: 1,
            workers: 1,
            top_n: 1,
            base_input_overrides: BTreeMap::new(),
        },
    )
    .expect_err("missing walk-forward config should fail");
    assert!(matches!(err, OptimizeError::MissingParams));
}
