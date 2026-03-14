use crate::backtest::{
    BacktestSummary, OptimizationRobustnessSummary, OptimizeCandidateSummary, OptimizeConfig,
    OptimizeEvaluationSummary, OptimizeHoldoutResult, OverfittingRiskLevel, OverfittingRiskReason,
    OverfittingRiskReasonKind, OverfittingRiskSummary, WalkForwardSegmentResult,
    WalkForwardStitchedSummary,
};

const SEGMENT_RETURN_RANGE_BASELINE: f64 = 0.01;

pub(crate) fn build_backtest_overfitting_risk(summary: &BacktestSummary) -> OverfittingRiskSummary {
    let mut reasons = vec![OverfittingRiskReason {
        kind: OverfittingRiskReasonKind::NoOutOfSampleValidation,
        metric: None,
        value: None,
    }];
    if summary.trade_count < 30 {
        reasons.push(OverfittingRiskReason {
            kind: OverfittingRiskReasonKind::TooFewTrades,
            metric: Some("trade_count".to_string()),
            value: Some(summary.trade_count as f64),
        });
    }
    OverfittingRiskSummary {
        level: OverfittingRiskLevel::Unknown,
        score: 0.0,
        reasons,
    }
}

pub(crate) fn build_walk_forward_overfitting_risk(
    segments: &[WalkForwardSegmentResult],
    stitched_summary: &WalkForwardStitchedSummary,
) -> OverfittingRiskSummary {
    if segments.is_empty() {
        return OverfittingRiskSummary {
            level: OverfittingRiskLevel::Unknown,
            score: 0.0,
            reasons: vec![OverfittingRiskReason {
                kind: OverfittingRiskReasonKind::NoOutOfSampleValidation,
                metric: None,
                value: None,
            }],
        };
    }

    let mut score = 0.0;
    let mut reasons = Vec::new();
    let segment_count = segments.len() as f64;
    let zero_trade_segments = segments
        .iter()
        .filter(|segment| segment.out_of_sample.trade_count == 0)
        .count();
    if zero_trade_segments > 0 {
        let ratio = zero_trade_segments as f64 / segment_count;
        score += (ratio * 0.30).min(0.30);
        reasons.push(OverfittingRiskReason {
            kind: OverfittingRiskReasonKind::ZeroTradeSegments,
            metric: Some("segment_ratio".to_string()),
            value: Some(ratio),
        });
    }

    let negative_segments = segments
        .iter()
        .filter(|segment| segment.out_of_sample.total_return < -crate::backtest::EPSILON)
        .count();
    if negative_segments > 0 {
        let ratio = negative_segments as f64 / segment_count;
        score += if ratio >= 0.50 { 0.30 } else { 0.15 };
        reasons.push(OverfittingRiskReason {
            kind: OverfittingRiskReasonKind::NegativeOutOfSampleSegments,
            metric: Some("segment_ratio".to_string()),
            value: Some(ratio),
        });
    }

    let mut min_return = f64::INFINITY;
    let mut max_return = f64::NEG_INFINITY;
    for segment in segments {
        min_return = min_return.min(segment.out_of_sample.total_return);
        max_return = max_return.max(segment.out_of_sample.total_return);
    }
    let instability = (max_return - min_return)
        / stitched_summary
            .average_segment_return
            .abs()
            .max(SEGMENT_RETURN_RANGE_BASELINE);
    if instability >= 2.0 {
        score += if instability >= 4.0 { 0.25 } else { 0.15 };
        reasons.push(OverfittingRiskReason {
            kind: OverfittingRiskReasonKind::SegmentReturnInstability,
            metric: Some("return_range_ratio".to_string()),
            value: Some(instability),
        });
    }

    if stitched_summary.trade_count < 20 {
        score += 0.15;
        reasons.push(OverfittingRiskReason {
            kind: OverfittingRiskReasonKind::TooFewTrades,
            metric: Some("trade_count".to_string()),
            value: Some(stitched_summary.trade_count as f64),
        });
    }

    finalize_scored_risk(score, reasons)
}

pub(crate) fn build_optimize_overfitting_risk(
    config: &OptimizeConfig,
    best_candidate: &OptimizeCandidateSummary,
    holdout: Option<&OptimizeHoldoutResult>,
    robustness: &OptimizationRobustnessSummary,
) -> OverfittingRiskSummary {
    let mut score = 0.0;
    let mut reasons = Vec::new();

    let trade_count = match &best_candidate.summary {
        OptimizeEvaluationSummary::WalkForward {
            trade_count,
            zero_trade_segment_count,
            ..
        } => {
            if *zero_trade_segment_count > 0 {
                score += 0.15;
                reasons.push(OverfittingRiskReason {
                    kind: OverfittingRiskReasonKind::ZeroTradeSegments,
                    metric: Some("segment_count".to_string()),
                    value: Some(*zero_trade_segment_count as f64),
                });
            }
            *trade_count
        }
        OptimizeEvaluationSummary::Backtest { summary, .. } => {
            reasons.push(OverfittingRiskReason {
                kind: OverfittingRiskReasonKind::NoOutOfSampleValidation,
                metric: None,
                value: None,
            });
            summary.trade_count
        }
    };
    if trade_count < 20 {
        score += 0.10;
        reasons.push(OverfittingRiskReason {
            kind: OverfittingRiskReasonKind::TooFewTrades,
            metric: Some("trade_count".to_string()),
            value: Some(trade_count as f64),
        });
    }

    if let Some(holdout) = holdout {
        if holdout.summary.trade_count == 0 || holdout.summary.total_return <= 0.0 {
            score += 0.35;
            reasons.push(OverfittingRiskReason {
                kind: OverfittingRiskReasonKind::HoldoutReturnCollapse,
                metric: Some("holdout_total_return".to_string()),
                value: Some(holdout.summary.total_return),
            });
        }
        if holdout.drift.total_return_delta <= -0.10 {
            score += 0.20;
            reasons.push(OverfittingRiskReason {
                kind: OverfittingRiskReasonKind::LargeHoldoutReturnDrop,
                metric: Some("holdout_total_return_delta".to_string()),
                value: Some(holdout.drift.total_return_delta),
            });
        }
    }

    if robustness.holdout_evaluated_count > 0 {
        let pass_rate =
            robustness.holdout_pass_count as f64 / robustness.holdout_evaluated_count as f64;
        if pass_rate < 0.34 {
            score += 0.25;
            reasons.push(OverfittingRiskReason {
                kind: OverfittingRiskReasonKind::WeakHoldoutPassRate,
                metric: Some("holdout_pass_rate".to_string()),
                value: Some(pass_rate),
            });
        }
        if robustness.best_candidate_holdout_rank.is_none()
            || robustness
                .best_candidate_holdout_rank
                .is_some_and(|rank| rank > 3)
        {
            score += 0.15;
            reasons.push(OverfittingRiskReason {
                kind: OverfittingRiskReasonKind::BestCandidateNotRobust,
                metric: Some("best_candidate_holdout_rank".to_string()),
                value: robustness
                    .best_candidate_holdout_rank
                    .map(|rank| rank as f64),
            });
        }
    }

    let narrow_parameter_count = robustness
        .parameter_stability
        .iter()
        .filter(|summary| parameter_range_is_narrow(config, summary))
        .count();
    if narrow_parameter_count > 0 {
        score += 0.15;
        reasons.push(OverfittingRiskReason {
            kind: OverfittingRiskReasonKind::NarrowParameterStability,
            metric: Some("narrow_parameter_count".to_string()),
            value: Some(narrow_parameter_count as f64),
        });
    }

    if matches!(
        best_candidate.summary,
        OptimizeEvaluationSummary::Backtest { .. }
    ) && holdout.is_none()
        && robustness.holdout_evaluated_count == 0
    {
        return OverfittingRiskSummary {
            level: OverfittingRiskLevel::Unknown,
            score: 0.0,
            reasons,
        };
    }

    finalize_scored_risk(score, reasons)
}

fn parameter_range_is_narrow(
    config: &OptimizeConfig,
    summary: &crate::backtest::ParameterRobustnessSummary,
) -> bool {
    let Some(param) = config.params.iter().find(|param| match param {
        crate::backtest::OptimizeParamSpace::IntegerRange { name, .. }
        | crate::backtest::OptimizeParamSpace::FloatRange { name, .. }
        | crate::backtest::OptimizeParamSpace::Choice { name, .. } => name == &summary.name,
    }) else {
        return false;
    };
    let (Some(low), Some(high)) = (summary.holdout_passing_min, summary.holdout_passing_max) else {
        return false;
    };
    if summary.distinct_sampled_value_count < 4 {
        return false;
    }
    let span = match param {
        crate::backtest::OptimizeParamSpace::IntegerRange {
            low, high, step, ..
        } => ((*high - *low).max(*step).max(1)) as f64,
        crate::backtest::OptimizeParamSpace::FloatRange {
            low, high, step, ..
        } => match step {
            Some(step) => (*high - *low)
                .abs()
                .max(*step)
                .max(crate::backtest::EPSILON),
            None => (*high - *low).abs().max(crate::backtest::EPSILON),
        },
        crate::backtest::OptimizeParamSpace::Choice { values, .. } => {
            if values.len() <= 1 {
                return false;
            }
            let min = values.iter().copied().fold(f64::INFINITY, f64::min);
            let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            (max - min).abs().max(crate::backtest::EPSILON)
        }
    };
    (high - low).abs() / span < 0.10
}

fn finalize_scored_risk(score: f64, reasons: Vec<OverfittingRiskReason>) -> OverfittingRiskSummary {
    let score = score.clamp(0.0, 1.0);
    let level = if reasons.is_empty() {
        OverfittingRiskLevel::Low
    } else if score >= 0.60 {
        OverfittingRiskLevel::High
    } else if score >= 0.30 {
        OverfittingRiskLevel::Moderate
    } else {
        OverfittingRiskLevel::Low
    };
    OverfittingRiskSummary {
        level,
        score,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        build_backtest_overfitting_risk, build_optimize_overfitting_risk,
        build_walk_forward_overfitting_risk,
    };
    use crate::backtest::{
        BacktestSummary, DiagnosticsDetailMode, FeeSchedule, OptimizationRobustnessSummary,
        OptimizeCandidateSummary, OptimizeConfig, OptimizeEvaluationSummary, OptimizeHoldoutConfig,
        OptimizeHoldoutResult, OptimizeObjective, OptimizeParamSpace, OptimizeRunner,
        OverfittingRiskLevel, OverfittingRiskReasonKind, ParameterRobustnessSummary,
        WalkForwardConfig, WalkForwardSegmentDiagnostics, WalkForwardSegmentResult,
        WalkForwardStitchedSummary, WalkForwardWindowSummary,
    };

    fn window(
        total_return: f64,
        trade_count: usize,
        win_rate: f64,
        max_drawdown: f64,
    ) -> WalkForwardWindowSummary {
        WalkForwardWindowSummary {
            starting_equity: 1_000.0,
            ending_equity: 1_000.0 * (1.0 + total_return),
            total_return,
            trade_count,
            winning_trade_count: ((trade_count as f64) * win_rate).round() as usize,
            losing_trade_count: trade_count
                .saturating_sub(((trade_count as f64) * win_rate).round() as usize),
            win_rate,
            max_drawdown,
            execution_asset_return: 0.0,
            flat_bar_pct: 0.5,
            long_bar_pct: 0.5,
            short_bar_pct: 0.0,
        }
    }

    #[test]
    fn plain_backtest_risk_is_unknown_without_oos_validation() {
        let risk = build_backtest_overfitting_risk(&BacktestSummary {
            starting_equity: 1_000.0,
            ending_equity: 1_050.0,
            realized_pnl: 50.0,
            unrealized_pnl: 0.0,
            total_return: 0.05,
            trade_count: 3,
            winning_trade_count: 2,
            losing_trade_count: 1,
            win_rate: 0.66,
            max_drawdown: 10.0,
            max_gross_exposure: 1.0,
            max_net_exposure: 1.0,
            peak_open_position_count: 1,
        });

        assert_eq!(risk.level, OverfittingRiskLevel::Unknown);
        assert!(risk
            .reasons
            .iter()
            .any(|reason| reason.kind == OverfittingRiskReasonKind::NoOutOfSampleValidation));
        assert!(risk
            .reasons
            .iter()
            .any(|reason| reason.kind == OverfittingRiskReasonKind::TooFewTrades));
    }

    #[test]
    fn walk_forward_risk_scores_segment_fragility() {
        let segments = vec![
            WalkForwardSegmentResult {
                segment_index: 0,
                train_from: 0,
                train_to: 1,
                test_from: 1,
                test_to: 2,
                in_sample: window(0.10, 5, 0.60, 20.0),
                out_of_sample: window(-0.08, 0, 0.0, 35.0),
                out_of_sample_diagnostics: WalkForwardSegmentDiagnostics {
                    summary: crate::backtest::BacktestDiagnosticSummary::default(),
                    capture_summary: crate::backtest::BacktestCaptureSummary::default(),
                    export_summaries: Vec::new(),
                    opportunity_event_count: 0,
                    cohorts: crate::backtest::CohortDiagnostics::default(),
                    drawdown: crate::backtest::DrawdownDiagnostics::default(),
                    drift_flags: Vec::new(),
                    hints: Vec::new(),
                },
            },
            WalkForwardSegmentResult {
                segment_index: 1,
                train_from: 2,
                train_to: 3,
                test_from: 3,
                test_to: 4,
                in_sample: window(0.12, 5, 0.60, 18.0),
                out_of_sample: window(0.02, 1, 1.0, 8.0),
                out_of_sample_diagnostics: WalkForwardSegmentDiagnostics {
                    summary: crate::backtest::BacktestDiagnosticSummary::default(),
                    capture_summary: crate::backtest::BacktestCaptureSummary::default(),
                    export_summaries: Vec::new(),
                    opportunity_event_count: 0,
                    cohorts: crate::backtest::CohortDiagnostics::default(),
                    drawdown: crate::backtest::DrawdownDiagnostics::default(),
                    drift_flags: Vec::new(),
                    hints: Vec::new(),
                },
            },
        ];
        let risk = build_walk_forward_overfitting_risk(
            &segments,
            &WalkForwardStitchedSummary {
                segment_count: 2,
                starting_equity: 1_000.0,
                ending_equity: 940.0,
                total_return: -0.06,
                max_drawdown: 35.0,
                average_execution_asset_return: 0.0,
                trade_count: 1,
                winning_trade_count: 1,
                losing_trade_count: 0,
                win_rate: 1.0,
                positive_segment_count: 1,
                negative_segment_count: 1,
                average_segment_return: -0.03,
            },
        );

        assert_eq!(risk.level, OverfittingRiskLevel::High);
        assert!(risk
            .reasons
            .iter()
            .any(|reason| reason.kind == OverfittingRiskReasonKind::ZeroTradeSegments));
        assert!(risk
            .reasons
            .iter()
            .any(|reason| reason.kind == OverfittingRiskReasonKind::NegativeOutOfSampleSegments));
    }

    #[test]
    fn optimize_risk_uses_holdout_and_parameter_fragility() {
        let config = OptimizeConfig {
            runner: OptimizeRunner::WalkForward,
            backtest: crate::backtest::BacktestConfig {
                execution_source_alias: "spot".to_string(),
                portfolio_execution_aliases: Vec::new(),
                activation_time_ms: None,
                initial_capital: 1_000.0,
                maker_fee_bps: 0.0,
                taker_fee_bps: 0.0,
                execution_fee_schedules: BTreeMap::from([(
                    "spot".to_string(),
                    FeeSchedule {
                        maker_bps: 0.0,
                        taker_bps: 0.0,
                    },
                )]),
                slippage_bps: 0.0,
                diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
                perp: None,
                perp_context: None,
                portfolio_perp_contexts: BTreeMap::new(),
            },
            walk_forward: Some(WalkForwardConfig {
                backtest: crate::backtest::BacktestConfig {
                    execution_source_alias: "spot".to_string(),
                    portfolio_execution_aliases: Vec::new(),
                    activation_time_ms: None,
                    initial_capital: 1_000.0,
                    maker_fee_bps: 0.0,
                    taker_fee_bps: 0.0,
                    execution_fee_schedules: BTreeMap::new(),
                    slippage_bps: 0.0,
                    diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
                    perp: None,
                    perp_context: None,
                    portfolio_perp_contexts: BTreeMap::new(),
                },
                diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
                train_bars: 10,
                test_bars: 5,
                step_bars: 5,
            }),
            diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
            holdout: Some(OptimizeHoldoutConfig { bars: 5 }),
            params: vec![OptimizeParamSpace::FloatRange {
                name: "threshold".to_string(),
                low: 0.0,
                high: 10.0,
                step: Some(0.5),
            }],
            objective: OptimizeObjective::RobustReturn,
            trials: 20,
            startup_trials: 5,
            seed: 0,
            workers: 1,
            top_n: 5,
            base_input_overrides: BTreeMap::new(),
        };
        let risk = build_optimize_overfitting_risk(
            &config,
            &OptimizeCandidateSummary {
                trial_id: 1,
                input_overrides: BTreeMap::from([("threshold".to_string(), 4.0)]),
                objective_score: 1.0,
                summary: OptimizeEvaluationSummary::WalkForward {
                    stitched_summary: WalkForwardStitchedSummary {
                        segment_count: 3,
                        starting_equity: 1_000.0,
                        ending_equity: 1_200.0,
                        total_return: 0.20,
                        max_drawdown: 30.0,
                        average_execution_asset_return: 0.0,
                        trade_count: 6,
                        winning_trade_count: 4,
                        losing_trade_count: 2,
                        win_rate: 0.66,
                        positive_segment_count: 2,
                        negative_segment_count: 1,
                        average_segment_return: 0.05,
                    },
                    zero_trade_segment_count: 1,
                    trade_count: 6,
                    winning_trade_count: 4,
                    losing_trade_count: 2,
                    win_rate: 0.66,
                },
            },
            Some(&OptimizeHoldoutResult {
                bars: 5,
                from: 10,
                to: 20,
                summary: window(-0.05, 1, 0.0, 40.0),
                diagnostics: WalkForwardSegmentDiagnostics {
                    summary: crate::backtest::BacktestDiagnosticSummary::default(),
                    capture_summary: crate::backtest::BacktestCaptureSummary::default(),
                    export_summaries: Vec::new(),
                    opportunity_event_count: 0,
                    cohorts: crate::backtest::CohortDiagnostics::default(),
                    drawdown: crate::backtest::DrawdownDiagnostics::default(),
                    drift_flags: Vec::new(),
                    hints: Vec::new(),
                },
                drift: crate::backtest::HoldoutDriftSummary {
                    total_return_delta: -0.25,
                    execution_asset_return_delta: 0.0,
                    trade_count_delta: -5,
                    win_rate_delta: -0.66,
                    max_drawdown_delta: 10.0,
                },
            }),
            &OptimizationRobustnessSummary {
                top_candidate_count: 5,
                holdout_evaluated_count: 5,
                holdout_pass_count: 1,
                holdout_fail_count: 4,
                best_candidate_holdout_rank: None,
                holdout_return_min: Some(-0.10),
                holdout_return_max: Some(0.02),
                holdout_return_mean: Some(-0.03),
                evaluations: Vec::new(),
                parameter_stability: vec![ParameterRobustnessSummary {
                    name: "threshold".to_string(),
                    best_value: Some(4.0),
                    top_ranked_min: Some(3.5),
                    top_ranked_max: Some(4.5),
                    holdout_passing_min: Some(4.0),
                    holdout_passing_max: Some(4.2),
                    distinct_sampled_value_count: 8,
                }],
            },
        );

        assert_eq!(risk.level, OverfittingRiskLevel::High);
        assert!(risk
            .reasons
            .iter()
            .any(|reason| reason.kind == OverfittingRiskReasonKind::HoldoutReturnCollapse));
        assert!(risk
            .reasons
            .iter()
            .any(|reason| reason.kind == OverfittingRiskReasonKind::WeakHoldoutPassRate));
        assert!(risk
            .reasons
            .iter()
            .any(|reason| reason.kind == OverfittingRiskReasonKind::NarrowParameterStability));
    }
}
