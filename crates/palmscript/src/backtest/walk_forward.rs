use serde::{Deserialize, Serialize};

use crate::backtest::bridge::PreparedExport;
use crate::backtest::diagnostics::{
    build_backtest_hints, build_cohort_diagnostics, build_diagnostics_summary,
    build_drawdown_diagnostics, snapshot_from_step, DiagnosticsAccumulator,
};
use crate::backtest::overfitting::build_walk_forward_overfitting_risk;
use crate::backtest::{
    average, execution_bars, run_backtest_with_sources, BacktestCaptureSummary, BacktestConfig,
    BacktestDiagnosticSummary, BacktestError, CohortDiagnostics, DiagnosticsDetailMode,
    DrawdownDiagnostics, ExportDiagnosticSummary, ImprovementHint, OverfittingRiskSummary,
};
use crate::compiler::CompiledProgram;
use crate::output::{OutputSample, StepOutput};
use crate::runtime::{slice_runtime_window, SourceRuntimeConfig, VmLimits};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WalkForwardConfig {
    pub backtest: BacktestConfig,
    #[serde(default)]
    pub diagnostics_detail: DiagnosticsDetailMode,
    pub train_bars: usize,
    pub test_bars: usize,
    pub step_bars: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WalkForwardWindowSummary {
    pub starting_equity: f64,
    pub ending_equity: f64,
    pub total_return: f64,
    pub trade_count: usize,
    pub winning_trade_count: usize,
    pub losing_trade_count: usize,
    pub win_rate: f64,
    pub max_drawdown: f64,
    pub execution_asset_return: f64,
    pub flat_bar_pct: f64,
    pub long_bar_pct: f64,
    pub short_bar_pct: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WalkForwardSegmentResult {
    pub segment_index: usize,
    pub train_from: i64,
    pub train_to: i64,
    pub test_from: i64,
    pub test_to: i64,
    pub in_sample: WalkForwardWindowSummary,
    pub out_of_sample: WalkForwardWindowSummary,
    pub out_of_sample_diagnostics: WalkForwardSegmentDiagnostics,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WalkForwardSegmentDiagnostics {
    pub summary: BacktestDiagnosticSummary,
    pub capture_summary: BacktestCaptureSummary,
    pub export_summaries: Vec<ExportDiagnosticSummary>,
    pub opportunity_event_count: usize,
    #[serde(default)]
    pub cohorts: CohortDiagnostics,
    #[serde(default)]
    pub drawdown: DrawdownDiagnostics,
    #[serde(default)]
    pub drift_flags: Vec<SegmentDriftFlag>,
    #[serde(default)]
    pub hints: Vec<ImprovementHint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentDriftFlag {
    NegativeOutOfSampleReturn,
    ZeroTradeSegment,
    HighDrawdown,
    WinRateCollapse,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WalkForwardEquityPoint {
    pub segment_index: usize,
    pub segment_bar_index: usize,
    pub time: f64,
    pub equity: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WalkForwardStitchedSummary {
    pub segment_count: usize,
    pub starting_equity: f64,
    pub ending_equity: f64,
    pub total_return: f64,
    pub max_drawdown: f64,
    pub average_execution_asset_return: f64,
    pub trade_count: usize,
    pub winning_trade_count: usize,
    pub losing_trade_count: usize,
    pub win_rate: f64,
    pub positive_segment_count: usize,
    pub negative_segment_count: usize,
    pub average_segment_return: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WalkForwardResult {
    pub config: WalkForwardConfig,
    pub segments: Vec<WalkForwardSegmentResult>,
    pub stitched_equity_curve: Vec<WalkForwardEquityPoint>,
    pub stitched_summary: WalkForwardStitchedSummary,
    #[serde(default)]
    pub overfitting_risk: OverfittingRiskSummary,
}

pub fn run_walk_forward_with_sources(
    compiled: &CompiledProgram,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: WalkForwardConfig,
) -> Result<WalkForwardResult, BacktestError> {
    validate_walk_forward_config(&config)?;
    let execution = crate::backtest::bridge::resolve_execution_source(
        compiled,
        &config.backtest.execution_source_alias,
    )?;
    let prepared = crate::backtest::bridge::prepare_backtest(
        compiled,
        &config.backtest.execution_source_alias,
        execution.template,
    )?;
    let full_execution_bars = execution_bars(
        &runtime,
        execution.source_id,
        &config.backtest.execution_source_alias,
    )?;
    let required_bars = config.train_bars + config.test_bars;
    if full_execution_bars.len() < required_bars {
        return Err(BacktestError::InsufficientWalkForwardBars {
            available: full_execution_bars.len(),
            required: required_bars,
        });
    }

    let segment_ranges = build_segment_ranges(
        full_execution_bars.len(),
        config.train_bars,
        config.test_bars,
        config.step_bars,
    );
    let mut segments = Vec::with_capacity(segment_ranges.len());
    let mut stitched_equity_curve = Vec::new();
    let mut segment_starting_equity = config.backtest.initial_capital;

    for (segment_index, segment) in segment_ranges.iter().enumerate() {
        let train_from = full_execution_bars[segment.start_index].time as i64;
        let train_to = full_execution_bars[segment.train_end_index].time as i64;
        let test_from = train_to;
        let test_to = exclusive_end_time(&full_execution_bars, segment.end_index);
        let runtime_slice = slice_runtime_window(&runtime, train_from, test_to);
        let mut backtest = config.backtest.clone();
        backtest.diagnostics_detail = config.diagnostics_detail;
        let result = run_backtest_with_sources(compiled, runtime_slice, vm_limits, backtest)?;

        let in_sample = summarize_window(
            &result.equity_curve,
            &result.trades,
            0,
            config.train_bars,
            config.backtest.initial_capital,
        );
        let out_of_sample = summarize_window(
            &result.equity_curve,
            &result.trades,
            config.train_bars,
            config.train_bars + config.test_bars,
            in_sample.ending_equity,
        );
        let out_of_sample_total_return = out_of_sample.total_return;
        stitch_window(
            &mut stitched_equity_curve,
            &result.equity_curve,
            config.train_bars,
            config.train_bars + config.test_bars,
            segment_index,
            segment_starting_equity,
            out_of_sample.starting_equity,
        );
        segment_starting_equity = stitched_equity_curve
            .last()
            .map(|point| point.equity)
            .unwrap_or(segment_starting_equity);

        segments.push(WalkForwardSegmentResult {
            segment_index,
            train_from,
            train_to,
            test_from,
            test_to,
            in_sample,
            out_of_sample,
            out_of_sample_diagnostics: summarize_segment_diagnostics(
                &prepared.exports,
                &result,
                &full_execution_bars[segment.train_end_index..segment.end_index],
                config.train_bars,
                config.train_bars + config.test_bars,
                out_of_sample_total_return,
            ),
        });
    }

    let stitched_summary = summarize_stitched_curve(
        config.backtest.initial_capital,
        &segments,
        &stitched_equity_curve,
    );
    apply_segment_drift_flags(&mut segments, &stitched_summary);

    let overfitting_risk = build_walk_forward_overfitting_risk(&segments, &stitched_summary);

    Ok(WalkForwardResult {
        config,
        segments,
        stitched_equity_curve,
        stitched_summary,
        overfitting_risk,
    })
}

#[derive(Clone, Copy)]
struct SegmentRange {
    start_index: usize,
    train_end_index: usize,
    end_index: usize,
}

fn validate_walk_forward_config(config: &WalkForwardConfig) -> Result<(), BacktestError> {
    if config.train_bars == 0 {
        return Err(BacktestError::InvalidWalkForwardTrainBars {
            value: config.train_bars,
        });
    }
    if config.test_bars == 0 {
        return Err(BacktestError::InvalidWalkForwardTestBars {
            value: config.test_bars,
        });
    }
    if config.step_bars == 0 {
        return Err(BacktestError::InvalidWalkForwardStepBars {
            value: config.step_bars,
        });
    }
    crate::backtest::validate_config(&config.backtest)
}

fn build_segment_ranges(
    total_bars: usize,
    train_bars: usize,
    test_bars: usize,
    step_bars: usize,
) -> Vec<SegmentRange> {
    let mut ranges = Vec::new();
    let mut start_index = 0usize;
    while start_index + train_bars + test_bars <= total_bars {
        ranges.push(SegmentRange {
            start_index,
            train_end_index: start_index + train_bars,
            end_index: start_index + train_bars + test_bars,
        });
        start_index += step_bars;
    }
    ranges
}

fn exclusive_end_time(execution_bars: &[crate::runtime::Bar], end_index: usize) -> i64 {
    execution_bars
        .get(end_index)
        .map(|bar| bar.time as i64)
        .unwrap_or_else(|| {
            execution_bars
                .last()
                .map(|bar| bar.time as i64 + 1)
                .unwrap_or_default()
        })
}

pub(crate) fn summarize_window(
    equity_curve: &[crate::backtest::EquityPoint],
    trades: &[crate::backtest::Trade],
    start_index: usize,
    end_index: usize,
    starting_equity: f64,
) -> WalkForwardWindowSummary {
    let points = &equity_curve[start_index..end_index];
    let ending_equity = points
        .last()
        .map(|point| point.equity)
        .unwrap_or(starting_equity);
    let total_return = if starting_equity.abs() > crate::backtest::EPSILON {
        ending_equity / starting_equity - 1.0
    } else {
        0.0
    };
    let mut peak = starting_equity;
    let mut max_drawdown: f64 = 0.0;
    let mut flat_count = 0usize;
    let mut long_count = 0usize;
    let mut short_count = 0usize;
    for point in points {
        peak = peak.max(point.equity);
        max_drawdown = max_drawdown.max(peak - point.equity);
        match point.position_side {
            Some(crate::position::PositionSide::Long) => long_count += 1,
            Some(crate::position::PositionSide::Short) => short_count += 1,
            None => flat_count += 1,
        }
    }
    let trade_slice: Vec<_> = trades
        .iter()
        .filter(|trade| trade.exit.bar_index >= start_index && trade.exit.bar_index < end_index)
        .collect();
    let winning_trade_count = trade_slice
        .iter()
        .filter(|trade| trade.realized_pnl > crate::backtest::EPSILON)
        .count();
    let losing_trade_count = trade_slice
        .iter()
        .filter(|trade| trade.realized_pnl < -crate::backtest::EPSILON)
        .count();
    let trade_count = trade_slice.len();
    let win_rate = if trade_count == 0 {
        0.0
    } else {
        winning_trade_count as f64 / trade_count as f64
    };
    let execution_asset_return = execution_asset_return(equity_curve, start_index, end_index);
    let point_count = points.len() as f64;
    WalkForwardWindowSummary {
        starting_equity,
        ending_equity,
        total_return,
        trade_count,
        winning_trade_count,
        losing_trade_count,
        win_rate,
        max_drawdown,
        execution_asset_return,
        flat_bar_pct: ratio(flat_count as f64, point_count),
        long_bar_pct: ratio(long_count as f64, point_count),
        short_bar_pct: ratio(short_count as f64, point_count),
    }
}

fn execution_asset_return(
    equity_curve: &[crate::backtest::EquityPoint],
    start_index: usize,
    end_index: usize,
) -> f64 {
    let Some(end_point) = equity_curve.get(end_index.saturating_sub(1)) else {
        return 0.0;
    };
    let start_price = if start_index == 0 {
        equity_curve
            .first()
            .map(|point| point.mark_price)
            .unwrap_or(end_point.mark_price)
    } else {
        equity_curve
            .get(start_index - 1)
            .map(|point| point.mark_price)
            .unwrap_or(end_point.mark_price)
    };
    if start_price.abs() <= crate::backtest::EPSILON {
        0.0
    } else {
        end_point.mark_price / start_price - 1.0
    }
}

fn stitch_window(
    stitched_curve: &mut Vec<WalkForwardEquityPoint>,
    equity_curve: &[crate::backtest::EquityPoint],
    start_index: usize,
    end_index: usize,
    segment_index: usize,
    segment_starting_equity: f64,
    window_starting_equity: f64,
) {
    if window_starting_equity.abs() <= crate::backtest::EPSILON {
        return;
    }
    for (offset, point) in equity_curve[start_index..end_index].iter().enumerate() {
        stitched_curve.push(WalkForwardEquityPoint {
            segment_index,
            segment_bar_index: offset,
            time: point.time,
            equity: segment_starting_equity * (point.equity / window_starting_equity),
        });
    }
}

fn summarize_stitched_curve(
    starting_equity: f64,
    segments: &[WalkForwardSegmentResult],
    stitched_curve: &[WalkForwardEquityPoint],
) -> WalkForwardStitchedSummary {
    let ending_equity = stitched_curve
        .last()
        .map(|point| point.equity)
        .unwrap_or(starting_equity);
    let total_return = if starting_equity.abs() > crate::backtest::EPSILON {
        ending_equity / starting_equity - 1.0
    } else {
        0.0
    };
    let mut peak = starting_equity;
    let mut max_drawdown: f64 = 0.0;
    for point in stitched_curve {
        peak = peak.max(point.equity);
        max_drawdown = max_drawdown.max(peak - point.equity);
    }
    let positive_segment_count = segments
        .iter()
        .filter(|segment| segment.out_of_sample.total_return > crate::backtest::EPSILON)
        .count();
    let negative_segment_count = segments
        .iter()
        .filter(|segment| segment.out_of_sample.total_return < -crate::backtest::EPSILON)
        .count();
    let trade_count = segments
        .iter()
        .map(|segment| segment.out_of_sample.trade_count)
        .sum();
    let winning_trade_count = segments
        .iter()
        .map(|segment| segment.out_of_sample.winning_trade_count)
        .sum();
    let losing_trade_count = segments
        .iter()
        .map(|segment| segment.out_of_sample.losing_trade_count)
        .sum();
    WalkForwardStitchedSummary {
        segment_count: segments.len(),
        starting_equity,
        ending_equity,
        total_return,
        max_drawdown,
        average_execution_asset_return: average(
            segments
                .iter()
                .map(|segment| segment.out_of_sample.execution_asset_return),
        ),
        trade_count,
        winning_trade_count,
        losing_trade_count,
        win_rate: if trade_count == 0 {
            0.0
        } else {
            winning_trade_count as f64 / trade_count as f64
        },
        positive_segment_count,
        negative_segment_count,
        average_segment_return: average(
            segments
                .iter()
                .map(|segment| segment.out_of_sample.total_return),
        ),
    }
}

pub(crate) fn summarize_segment_diagnostics(
    exports: &[PreparedExport],
    result: &crate::backtest::BacktestResult,
    execution_bars: &[crate::runtime::Bar],
    start_index: usize,
    end_index: usize,
    strategy_total_return: f64,
) -> WalkForwardSegmentDiagnostics {
    let mut accumulator = DiagnosticsAccumulator::new(exports);
    let export_steps = export_steps_by_bar(&result.outputs.exports, start_index, end_index);

    for (offset, step_exports) in export_steps.into_iter().enumerate() {
        let full_bar_index = start_index + offset;
        let Some(execution_bar) = execution_bars.get(offset) else {
            continue;
        };
        let position_side = result
            .equity_curve
            .get(full_bar_index)
            .and_then(|point| point.position_side);
        let bar_return = accumulator.observe_execution_bar(execution_bar.close, position_side);
        let step = StepOutput {
            exports: step_exports,
            ..StepOutput::default()
        };
        let feature_snapshot = snapshot_from_step(&step, execution_bar.time);
        accumulator.observe_exports(
            &step,
            feature_snapshot.as_ref(),
            None,
            full_bar_index,
            execution_bar.time,
            bar_return,
            position_side,
        );
    }

    let entered_trade_diagnostics = result
        .diagnostics
        .trade_diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .entry_snapshot
                .as_ref()
                .is_some_and(|snapshot| in_bar_range(snapshot.bar_index, start_index, end_index))
        })
        .cloned()
        .collect::<Vec<_>>();
    let exited_trade_diagnostics = result
        .diagnostics
        .trade_diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .exit_snapshot
                .as_ref()
                .is_some_and(|snapshot| in_bar_range(snapshot.bar_index, start_index, end_index))
        })
        .cloned()
        .collect::<Vec<_>>();
    let placed_order_diagnostics = result
        .diagnostics
        .order_diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .placed_snapshot
                .as_ref()
                .is_some_and(|snapshot| in_bar_range(snapshot.bar_index, start_index, end_index))
        })
        .cloned()
        .collect::<Vec<_>>();

    let summary = build_diagnostics_summary(&placed_order_diagnostics, &exited_trade_diagnostics);
    let (capture_summary, export_summaries, _) = accumulator.finalize(
        execution_bars,
        &entered_trade_diagnostics,
        strategy_total_return,
    );
    let opportunity_event_count = result
        .diagnostics
        .opportunity_events
        .iter()
        .filter(|event| in_bar_range(event.bar_index, start_index, end_index))
        .count();
    let cohorts = build_cohort_diagnostics(&exited_trade_diagnostics, &export_summaries);
    let drawdown = build_drawdown_diagnostics(&result.equity_curve[start_index..end_index]);
    let segment_summary = summarize_window(
        &result.equity_curve,
        &result.trades,
        start_index,
        end_index,
        result
            .equity_curve
            .get(start_index.saturating_sub(1))
            .map(|point| point.equity)
            .unwrap_or(result.summary.starting_equity),
    );
    let hints = build_backtest_hints(
        &crate::backtest::BacktestSummary {
            starting_equity: segment_summary.starting_equity,
            ending_equity: segment_summary.ending_equity,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            total_return: segment_summary.total_return,
            trade_count: segment_summary.trade_count,
            winning_trade_count: segment_summary.winning_trade_count,
            losing_trade_count: segment_summary.losing_trade_count,
            win_rate: segment_summary.win_rate,
            max_drawdown: segment_summary.max_drawdown,
            max_gross_exposure: 0.0,
            max_net_exposure: 0.0,
            peak_open_position_count: 0,
        },
        &summary,
        &cohorts,
        &drawdown,
    );

    WalkForwardSegmentDiagnostics {
        summary,
        capture_summary,
        export_summaries,
        opportunity_event_count,
        cohorts,
        drawdown,
        drift_flags: Vec::new(),
        hints,
    }
}

fn export_steps_by_bar(
    exports: &[crate::output::OutputSeries],
    start_index: usize,
    end_index: usize,
) -> Vec<Vec<OutputSample>> {
    let mut steps = vec![Vec::new(); end_index.saturating_sub(start_index)];
    for series in exports {
        for sample in &series.points {
            if in_bar_range(sample.bar_index, start_index, end_index) {
                steps[sample.bar_index - start_index].push(sample.clone());
            }
        }
    }
    steps
}

fn in_bar_range(bar_index: usize, start_index: usize, end_index: usize) -> bool {
    bar_index >= start_index && bar_index < end_index
}

fn ratio(numerator: f64, denominator: f64) -> f64 {
    if denominator <= 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

fn apply_segment_drift_flags(
    segments: &mut [WalkForwardSegmentResult],
    stitched_summary: &WalkForwardStitchedSummary,
) {
    for segment in segments {
        if segment.out_of_sample.trade_count == 0 {
            segment
                .out_of_sample_diagnostics
                .drift_flags
                .push(SegmentDriftFlag::ZeroTradeSegment);
        }
        if segment.out_of_sample.total_return < -crate::backtest::EPSILON {
            segment
                .out_of_sample_diagnostics
                .drift_flags
                .push(SegmentDriftFlag::NegativeOutOfSampleReturn);
        }
        if segment.out_of_sample.max_drawdown > stitched_summary.max_drawdown.max(1.0) {
            segment
                .out_of_sample_diagnostics
                .drift_flags
                .push(SegmentDriftFlag::HighDrawdown);
        }
        if stitched_summary.trade_count > 0
            && segment.out_of_sample.trade_count > 0
            && segment.out_of_sample.win_rate + 0.15 < stitched_summary.win_rate
        {
            segment
                .out_of_sample_diagnostics
                .drift_flags
                .push(SegmentDriftFlag::WinRateCollapse);
        }
    }
}
