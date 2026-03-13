mod support;

use std::collections::BTreeMap;

use palmscript::{
    compile_with_input_overrides, run_optimize_with_source, run_optimize_with_source_resume,
    BacktestConfig, DiagnosticsDetailMode, Interval, OptimizeCandidateSummary, OptimizeConfig,
    OptimizeError, OptimizeEvaluationSummary, OptimizeHoldoutConfig, OptimizeObjective,
    OptimizeParamSpace, OptimizeProgressEvent, OptimizeProgressListener, OptimizeProgressState,
    OptimizeResumeState, OptimizeRunner, OptimizeScheduledBatch, VmLimits, WalkForwardConfig,
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
        portfolio_execution_aliases: Vec::new(),
        activation_time_ms: None,
        initial_capital: 1_000.0,
        fee_bps: 0.0,
        slippage_bps: 0.0,
        diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
        perp: None,
        perp_context: None,
        portfolio_perp_contexts: BTreeMap::new(),
    }
}

fn backtest_optimize_config() -> OptimizeConfig {
    OptimizeConfig {
        runner: OptimizeRunner::Backtest,
        backtest: optimize_backtest_config(),
        walk_forward: None,
        diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
        holdout: None,
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
    }
}

#[derive(Default)]
struct PendingBatchCapture {
    pending_batch: Option<OptimizeScheduledBatch>,
}

impl OptimizeProgressListener for PendingBatchCapture {
    fn on_event(
        &mut self,
        event: OptimizeProgressEvent,
        _state: &OptimizeProgressState,
    ) -> Result<(), String> {
        if let OptimizeProgressEvent::BatchScheduled { batch } = event {
            self.pending_batch = Some(batch);
            return Err("stop after scheduling the first batch".to_string());
        }
        Ok(())
    }
}

#[derive(Default)]
struct CancelAfterCheckpoint {
    completed_candidates: Vec<OptimizeCandidateSummary>,
    checkpoint_count: usize,
    should_cancel: bool,
}

impl OptimizeProgressListener for CancelAfterCheckpoint {
    fn on_event(
        &mut self,
        event: OptimizeProgressEvent,
        _state: &OptimizeProgressState,
    ) -> Result<(), String> {
        match event {
            OptimizeProgressEvent::CandidateCompleted { candidate, .. } => {
                self.completed_candidates.push(candidate);
            }
            OptimizeProgressEvent::CheckpointWritten => {
                self.checkpoint_count += 1;
                self.should_cancel = true;
            }
            _ => {}
        }
        Ok(())
    }

    fn should_cancel(&mut self) -> Result<bool, String> {
        Ok(self.should_cancel)
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
                diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
                train_bars: 2,
                test_bars: 2,
                step_bars: 2,
            }),
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            holdout: None,
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
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            train_bars: 2,
            test_bars: 2,
            step_bars: 2,
        }),
        diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
        holdout: None,
        params: vec![
            OptimizeParamSpace::Choice {
                name: "threshold".to_string(),
                values: vec![0.0, 100.0],
            },
            OptimizeParamSpace::IntegerRange {
                name: "threshold".to_string(),
                low: 0,
                high: 1,
                step: 1,
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
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            holdout: None,
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
fn optimize_respects_stepped_param_spaces() {
    let source = "interval 1m
source spot = binance.spot(\"BTCUSDT\")
input threshold = 0
input offset = 0
entry long = spot.close > spot.close[1] + threshold + offset
entry short = false
exit long = spot.close < spot.close[1]
exit short = true";
    let result = run_optimize_with_source(
        source,
        optimize_runtime(),
        VmLimits::default(),
        OptimizeConfig {
            runner: OptimizeRunner::Backtest,
            backtest: optimize_backtest_config(),
            walk_forward: None,
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            holdout: None,
            params: vec![
                OptimizeParamSpace::IntegerRange {
                    name: "threshold".to_string(),
                    low: 0,
                    high: 4,
                    step: 2,
                },
                OptimizeParamSpace::FloatRange {
                    name: "offset".to_string(),
                    low: 0.0,
                    high: 1.0,
                    step: Some(0.25),
                },
            ],
            objective: OptimizeObjective::EndingEquity,
            trials: 8,
            startup_trials: 8,
            seed: 5,
            workers: 2,
            top_n: 2,
            base_input_overrides: BTreeMap::from([(String::from("offset"), 0.0)]),
        },
    )
    .expect("optimize with stepped param spaces should succeed");

    for candidate in result.top_candidates {
        let threshold = candidate
            .input_overrides
            .get("threshold")
            .copied()
            .expect("threshold override");
        let offset = candidate
            .input_overrides
            .get("offset")
            .copied()
            .expect("offset override");
        assert!(matches!(threshold as i64, 0 | 2 | 4));
        let offset_steps = (offset / 0.25).round();
        assert!((offset - offset_steps * 0.25).abs() < 1.0e-9);
    }
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
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            holdout: None,
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

#[test]
fn optimize_holdout_reserves_tail_bars_and_reports_unseen_summary() {
    let result = run_optimize_with_source(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        OptimizeConfig {
            runner: OptimizeRunner::WalkForward,
            backtest: optimize_backtest_config(),
            walk_forward: Some(WalkForwardConfig {
                backtest: optimize_backtest_config(),
                diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
                train_bars: 2,
                test_bars: 2,
                step_bars: 2,
            }),
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            holdout: Some(OptimizeHoldoutConfig { bars: 2 }),
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
    .expect("optimize with holdout should succeed");

    let holdout = result.holdout.expect("holdout result should be present");
    assert_eq!(holdout.bars, 2);
    assert_eq!(holdout.from, JAN_1_2024_UTC_MS + 6 * MINUTE_MS);
    assert!(holdout.to > holdout.from);
    assert!(result.robustness.holdout_evaluated_count > 0);
    assert!(!result.robustness.parameter_stability.is_empty());
    let OptimizeEvaluationSummary::WalkForward {
        stitched_summary, ..
    } = &result.best_candidate.summary
    else {
        panic!("expected walk-forward summary for best candidate");
    };
    assert_eq!(
        holdout.summary.starting_equity,
        stitched_summary.ending_equity
    );
    assert!(holdout.from < holdout.to);
    assert!(holdout.drift.trade_count_delta <= 0);
}

#[test]
fn optimize_resume_from_pending_batch_matches_fresh_run() {
    let config = backtest_optimize_config();
    let baseline = run_optimize_with_source(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        config.clone(),
    )
    .expect("fresh optimize should succeed");

    let mut capture = PendingBatchCapture::default();
    let err = run_optimize_with_source_resume(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        config.clone(),
        OptimizeResumeState::default(),
        Some(&mut capture),
    )
    .expect_err("captured run should stop after scheduling the first batch");
    assert!(matches!(err, OptimizeError::ProgressCallback { .. }));

    let resumed = run_optimize_with_source_resume(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        config,
        OptimizeResumeState {
            completed_candidates: Vec::new(),
            pending_batch: capture.pending_batch.clone(),
        },
        None,
    )
    .expect("resume from pending batch should succeed");

    assert!(capture.pending_batch.is_some());
    assert_eq!(resumed.best_candidate, baseline.best_candidate);
    assert_eq!(resumed.top_candidates, baseline.top_candidates);
}

#[test]
fn optimize_resume_from_completed_candidates_matches_fresh_run() {
    let config = backtest_optimize_config();
    let baseline = run_optimize_with_source(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        config.clone(),
    )
    .expect("fresh optimize should succeed");

    let mut capture = CancelAfterCheckpoint::default();
    let err = run_optimize_with_source_resume(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        config.clone(),
        OptimizeResumeState::default(),
        Some(&mut capture),
    )
    .expect_err("captured run should cancel after the first checkpoint");
    assert!(matches!(err, OptimizeError::Canceled));
    assert!(!capture.completed_candidates.is_empty());
    assert_eq!(capture.checkpoint_count, 1);

    let resumed = run_optimize_with_source_resume(
        optimize_source(),
        optimize_runtime(),
        VmLimits::default(),
        config,
        OptimizeResumeState {
            completed_candidates: capture.completed_candidates.clone(),
            pending_batch: None,
        },
        None,
    )
    .expect("resume from completed candidates should succeed");

    assert_eq!(resumed.best_candidate, baseline.best_candidate);
    assert_eq!(resumed.top_candidates, baseline.top_candidates);
}
