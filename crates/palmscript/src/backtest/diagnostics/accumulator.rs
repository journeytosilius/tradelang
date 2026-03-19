use std::collections::HashMap;

use crate::backtest::bridge::PreparedExport;
use crate::backtest::{
    ratio, BacktestCaptureSummary, ExportDiagnosticSummary, ExportValueType, FeatureSnapshot,
    ForwardReturnMetric, NumericExportDiagnosticSummary, OpportunityEvent, OpportunityEventKind,
    PositionSide, PositionSnapshot, TradeDiagnostic,
};
use crate::bytecode::SignalRole;
use crate::output::{OutputValue, StepOutput};
use crate::runtime::Bar;

#[derive(Clone, Debug, Default)]
pub(crate) struct OrderDiagnosticContext {
    pub signal_snapshot: Option<FeatureSnapshot>,
    pub placed_snapshot: Option<FeatureSnapshot>,
    pub fill_snapshot: Option<FeatureSnapshot>,
    pub placed_position: Option<PositionSnapshot>,
    pub fill_position: Option<PositionSnapshot>,
}

#[derive(Clone, Debug)]
struct RawOpportunityEvent {
    execution_alias: String,
    kind: OpportunityEventKind,
    name: String,
    role: Option<SignalRole>,
    bar_index: usize,
    time: f64,
    position_snapshot: Option<PositionSnapshot>,
    feature_snapshot: Option<FeatureSnapshot>,
}

#[derive(Clone, Debug)]
struct CaptureAccumulator {
    first_close: Option<f64>,
    last_close: Option<f64>,
    previous_close: Option<f64>,
    flat_bar_count: usize,
    long_bar_count: usize,
    short_bar_count: usize,
    in_market_bar_count: usize,
    flat_return_gross: f64,
    long_return_gross: f64,
    short_return_gross: f64,
}

impl Default for CaptureAccumulator {
    fn default() -> Self {
        Self {
            first_close: None,
            last_close: None,
            previous_close: None,
            flat_bar_count: 0,
            long_bar_count: 0,
            short_bar_count: 0,
            in_market_bar_count: 0,
            flat_return_gross: 1.0,
            long_return_gross: 1.0,
            short_return_gross: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
struct ExportTradeStats {
    trade_count: usize,
    winning_trade_count: usize,
    realized_pnl_sum: f64,
    mae_pct_sum: f64,
    mfe_pct_sum: f64,
}

impl Default for ExportTradeStats {
    fn default() -> Self {
        Self {
            trade_count: 0,
            winning_trade_count: 0,
            realized_pnl_sum: 0.0,
            mae_pct_sum: 0.0,
            mfe_pct_sum: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
struct NumericExportState {
    sample_count: usize,
    na_count: usize,
    value_count: usize,
    sum: f64,
    min: Option<f64>,
    max: Option<f64>,
    entry_sum: f64,
    entry_count: usize,
    exit_sum: f64,
    exit_count: usize,
}

impl Default for NumericExportState {
    fn default() -> Self {
        Self {
            sample_count: 0,
            na_count: 0,
            value_count: 0,
            sum: 0.0,
            min: None,
            max: None,
            entry_sum: 0.0,
            entry_count: 0,
            exit_sum: 0.0,
            exit_count: 0,
        }
    }
}

#[derive(Clone, Debug)]
struct BoolExportState {
    sample_count: usize,
    na_count: usize,
    true_count: usize,
    false_count: usize,
    rising_edge_count: usize,
    falling_edge_count: usize,
    true_while_flat_count: usize,
    true_while_in_market_count: usize,
    true_while_long_count: usize,
    true_while_short_count: usize,
    return_while_true_gross: f64,
    return_while_true_and_flat_gross: f64,
    previous: Option<bool>,
    trade_stats: ExportTradeStats,
}

impl Default for BoolExportState {
    fn default() -> Self {
        Self {
            sample_count: 0,
            na_count: 0,
            true_count: 0,
            false_count: 0,
            rising_edge_count: 0,
            falling_edge_count: 0,
            true_while_flat_count: 0,
            true_while_in_market_count: 0,
            true_while_long_count: 0,
            true_while_short_count: 0,
            return_while_true_gross: 1.0,
            return_while_true_and_flat_gross: 1.0,
            previous: None,
            trade_stats: ExportTradeStats::default(),
        }
    }
}

#[derive(Clone, Debug)]
enum ExportAccumulatorState {
    Numeric(NumericExportState),
    Bool(BoolExportState),
}

#[derive(Clone, Debug)]
struct RegisteredExportState {
    name: String,
    is_regime: bool,
    state: ExportAccumulatorState,
}

pub(crate) struct DiagnosticsAccumulator {
    export_index_by_output: Vec<Option<usize>>,
    export_name_to_index: HashMap<String, usize>,
    exports: Vec<RegisteredExportState>,
    capture: CaptureAccumulator,
    raw_events: Vec<RawOpportunityEvent>,
}

impl DiagnosticsAccumulator {
    pub(crate) fn new(exports: &[PreparedExport]) -> Self {
        let mut export_index_by_output = vec![
            None;
            exports
                .iter()
                .map(|export| export.output_id)
                .max()
                .map(|max| max + 1)
                .unwrap_or(0)
        ];
        let mut export_name_to_index = HashMap::with_capacity(exports.len());
        let mut registered = Vec::with_capacity(exports.len());
        for export in exports {
            let index = registered.len();
            export_index_by_output[export.output_id] = Some(index);
            export_name_to_index.insert(export.name.clone(), index);
            registered.push(RegisteredExportState {
                name: export.name.clone(),
                is_regime: export.is_regime,
                state: match export.value_type {
                    ExportValueType::Numeric => {
                        ExportAccumulatorState::Numeric(NumericExportState::default())
                    }
                    ExportValueType::Bool => {
                        ExportAccumulatorState::Bool(BoolExportState::default())
                    }
                },
            });
        }
        Self {
            export_index_by_output,
            export_name_to_index,
            exports: registered,
            capture: CaptureAccumulator::default(),
            raw_events: Vec::new(),
        }
    }

    pub(crate) fn observe_execution_bar(
        &mut self,
        close: f64,
        position_side: Option<PositionSide>,
    ) -> Option<f64> {
        self.capture.first_close.get_or_insert(close);
        self.capture.last_close = Some(close);
        match position_side {
            Some(PositionSide::Long) => {
                self.capture.long_bar_count += 1;
                self.capture.in_market_bar_count += 1;
            }
            Some(PositionSide::Short) => {
                self.capture.short_bar_count += 1;
                self.capture.in_market_bar_count += 1;
            }
            None => {
                self.capture.flat_bar_count += 1;
            }
        }

        let bar_return = self
            .capture
            .previous_close
            .filter(|previous| previous.is_finite() && previous.abs() > crate::backtest::EPSILON)
            .map(|previous| close / previous - 1.0);
        self.capture.previous_close = Some(close);

        if let Some(bar_return) = bar_return {
            match position_side {
                Some(PositionSide::Long) => self.capture.long_return_gross *= 1.0 + bar_return,
                Some(PositionSide::Short) => self.capture.short_return_gross *= 1.0 + bar_return,
                None => self.capture.flat_return_gross *= 1.0 + bar_return,
            }
        }

        bar_return
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn observe_exports(
        &mut self,
        step: &StepOutput,
        feature_snapshot: Option<&FeatureSnapshot>,
        position_snapshot: Option<&PositionSnapshot>,
        bar_index: usize,
        time: f64,
        bar_return: Option<f64>,
        position_side: Option<PositionSide>,
    ) {
        for sample in &step.exports {
            let Some(index) = self
                .export_index_by_output
                .get(sample.output_id)
                .and_then(|slot| *slot)
            else {
                continue;
            };
            let export = &mut self.exports[index];
            let export_name = export.name.clone();
            let mut emit_activation = false;
            match (&mut export.state, &sample.value) {
                (ExportAccumulatorState::Numeric(state), OutputValue::F64(value)) => {
                    state.sample_count += 1;
                    state.value_count += 1;
                    state.sum += value;
                    state.min = Some(state.min.map_or(*value, |current| current.min(*value)));
                    state.max = Some(state.max.map_or(*value, |current| current.max(*value)));
                }
                (ExportAccumulatorState::Numeric(state), OutputValue::NA) => {
                    state.sample_count += 1;
                    state.na_count += 1;
                }
                (ExportAccumulatorState::Bool(state), OutputValue::Bool(value)) => {
                    state.sample_count += 1;
                    if *value {
                        state.true_count += 1;
                        match position_side {
                            Some(PositionSide::Long) => {
                                state.true_while_long_count += 1;
                                state.true_while_in_market_count += 1;
                            }
                            Some(PositionSide::Short) => {
                                state.true_while_short_count += 1;
                                state.true_while_in_market_count += 1;
                            }
                            None => state.true_while_flat_count += 1,
                        }
                        if let Some(bar_return) = bar_return {
                            state.return_while_true_gross *= 1.0 + bar_return;
                            if position_side.is_none() {
                                state.return_while_true_and_flat_gross *= 1.0 + bar_return;
                            }
                        }
                        if state.previous != Some(true) {
                            state.rising_edge_count += 1;
                            emit_activation = true;
                        }
                    } else {
                        state.false_count += 1;
                        if state.previous == Some(true) {
                            state.falling_edge_count += 1;
                        }
                    }
                    state.previous = Some(*value);
                }
                (ExportAccumulatorState::Bool(state), OutputValue::NA) => {
                    state.sample_count += 1;
                    state.na_count += 1;
                    if state.previous == Some(true) {
                        state.falling_edge_count += 1;
                    }
                    state.previous = None;
                }
                (ExportAccumulatorState::Numeric(state), _) => {
                    state.sample_count += 1;
                    state.na_count += 1;
                }
                (ExportAccumulatorState::Bool(state), _) => {
                    state.sample_count += 1;
                    state.na_count += 1;
                    if state.previous == Some(true) {
                        state.falling_edge_count += 1;
                    }
                    state.previous = None;
                }
            }
            if emit_activation {
                self.raw_events.push(RawOpportunityEvent {
                    execution_alias: position_snapshot
                        .map(|snapshot| snapshot.execution_alias.clone())
                        .unwrap_or_default(),
                    kind: OpportunityEventKind::ExportActivated,
                    name: export_name,
                    role: None,
                    bar_index,
                    time,
                    position_snapshot: position_snapshot.cloned(),
                    feature_snapshot: feature_snapshot.cloned(),
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_signal_event(
        &mut self,
        execution_alias: &str,
        kind: OpportunityEventKind,
        name: &str,
        role: SignalRole,
        bar_index: usize,
        time: f64,
        position_snapshot: Option<&PositionSnapshot>,
        feature_snapshot: Option<&FeatureSnapshot>,
    ) {
        self.raw_events.push(RawOpportunityEvent {
            execution_alias: execution_alias.to_string(),
            kind,
            name: name.to_string(),
            role: Some(role),
            bar_index,
            time,
            position_snapshot: position_snapshot.cloned(),
            feature_snapshot: feature_snapshot.cloned(),
        });
    }

    pub(crate) fn finalize(
        mut self,
        execution_bars: &[Bar],
        trade_diagnostics: &[TradeDiagnostic],
        strategy_total_return: f64,
    ) -> (
        BacktestCaptureSummary,
        Vec<ExportDiagnosticSummary>,
        Vec<OpportunityEvent>,
    ) {
        self.apply_trade_snapshots(trade_diagnostics);
        let capture_summary = self.build_capture_summary(strategy_total_return);
        let export_summaries = self.build_export_summaries();
        let opportunity_events = self.build_opportunity_events(execution_bars);
        (capture_summary, export_summaries, opportunity_events)
    }

    fn apply_trade_snapshots(&mut self, trade_diagnostics: &[TradeDiagnostic]) {
        for diagnostic in trade_diagnostics {
            if let Some(snapshot) = &diagnostic.entry_snapshot {
                for value in &snapshot.values {
                    let Some(index) = self.export_name_to_index.get(&value.name).copied() else {
                        continue;
                    };
                    match (&mut self.exports[index].state, &value.value) {
                        (ExportAccumulatorState::Numeric(state), OutputValue::F64(number)) => {
                            state.entry_sum += number;
                            state.entry_count += 1;
                        }
                        (ExportAccumulatorState::Bool(state), OutputValue::Bool(true)) => {
                            state.trade_stats.trade_count += 1;
                            if diagnostic.realized_pnl > 0.0 {
                                state.trade_stats.winning_trade_count += 1;
                            }
                            state.trade_stats.realized_pnl_sum += diagnostic.realized_pnl;
                            state.trade_stats.mae_pct_sum += diagnostic.mae_pct;
                            state.trade_stats.mfe_pct_sum += diagnostic.mfe_pct;
                        }
                        _ => {}
                    }
                }
            }
            if let Some(snapshot) = &diagnostic.exit_snapshot {
                for value in &snapshot.values {
                    let Some(index) = self.export_name_to_index.get(&value.name).copied() else {
                        continue;
                    };
                    if let (ExportAccumulatorState::Numeric(state), OutputValue::F64(number)) =
                        (&mut self.exports[index].state, &value.value)
                    {
                        state.exit_sum += number;
                        state.exit_count += 1;
                    }
                }
            }
        }
    }

    fn build_capture_summary(&self, strategy_total_return: f64) -> BacktestCaptureSummary {
        let total_bars = self.capture.flat_bar_count
            + self.capture.long_bar_count
            + self.capture.short_bar_count;
        let execution_asset_return = match (self.capture.first_close, self.capture.last_close) {
            (Some(first), Some(last)) if first.abs() > crate::backtest::EPSILON => {
                last / first - 1.0
            }
            _ => 0.0,
        };
        BacktestCaptureSummary {
            execution_asset_return,
            strategy_total_return,
            flat_bar_count: self.capture.flat_bar_count,
            long_bar_count: self.capture.long_bar_count,
            short_bar_count: self.capture.short_bar_count,
            in_market_bar_count: self.capture.in_market_bar_count,
            flat_bar_pct: ratio(self.capture.flat_bar_count, total_bars),
            long_bar_pct: ratio(self.capture.long_bar_count, total_bars),
            short_bar_pct: ratio(self.capture.short_bar_count, total_bars),
            in_market_bar_pct: ratio(self.capture.in_market_bar_count, total_bars),
            execution_return_while_flat: self.capture.flat_return_gross - 1.0,
            execution_return_while_long: self.capture.long_return_gross - 1.0,
            execution_return_while_short: self.capture.short_return_gross - 1.0,
            opportunity_cost_return: self.capture.flat_return_gross - 1.0,
        }
    }

    fn build_export_summaries(&self) -> Vec<ExportDiagnosticSummary> {
        self.exports
            .iter()
            .map(|export| match &export.state {
                ExportAccumulatorState::Numeric(state) => {
                    ExportDiagnosticSummary::Numeric(NumericExportDiagnosticSummary {
                        name: export.name.clone(),
                        sample_count: state.sample_count,
                        na_count: state.na_count,
                        min: state.min,
                        max: state.max,
                        mean: mean_option(state.sum, state.value_count),
                        entry_mean: mean_option(state.entry_sum, state.entry_count),
                        exit_mean: mean_option(state.exit_sum, state.exit_count),
                    })
                }
                ExportAccumulatorState::Bool(state) => {
                    ExportDiagnosticSummary::Bool(crate::backtest::BoolExportDiagnosticSummary {
                        name: export.name.clone(),
                        is_regime: export.is_regime,
                        sample_count: state.sample_count,
                        na_count: state.na_count,
                        true_count: state.true_count,
                        false_count: state.false_count,
                        rising_edge_count: state.rising_edge_count,
                        falling_edge_count: state.falling_edge_count,
                        true_while_flat_count: state.true_while_flat_count,
                        true_while_in_market_count: state.true_while_in_market_count,
                        true_while_long_count: state.true_while_long_count,
                        true_while_short_count: state.true_while_short_count,
                        execution_return_while_true: state.return_while_true_gross - 1.0,
                        execution_return_while_true_and_flat: state
                            .return_while_true_and_flat_gross
                            - 1.0,
                        trade_count: state.trade_stats.trade_count,
                        win_rate: ratio(
                            state.trade_stats.winning_trade_count,
                            state.trade_stats.trade_count,
                        ),
                        average_realized_pnl: mean_zero(
                            state.trade_stats.realized_pnl_sum,
                            state.trade_stats.trade_count,
                        ),
                        average_mae_pct: mean_zero(
                            state.trade_stats.mae_pct_sum,
                            state.trade_stats.trade_count,
                        ),
                        average_mfe_pct: mean_zero(
                            state.trade_stats.mfe_pct_sum,
                            state.trade_stats.trade_count,
                        ),
                    })
                }
            })
            .collect()
    }

    fn build_opportunity_events(&self, execution_bars: &[Bar]) -> Vec<OpportunityEvent> {
        self.raw_events
            .iter()
            .map(|event| OpportunityEvent {
                execution_alias: event.execution_alias.clone(),
                kind: event.kind,
                name: event.name.clone(),
                role: event.role,
                bar_index: event.bar_index,
                time: event.time,
                position_snapshot: event.position_snapshot.clone(),
                feature_snapshot: event.feature_snapshot.clone(),
                forward_returns: build_forward_returns(execution_bars, event.bar_index),
                forward_max_favorable_pct: forward_extreme_pct(
                    execution_bars,
                    event.bar_index,
                    true,
                ),
                forward_max_adverse_pct: forward_extreme_pct(
                    execution_bars,
                    event.bar_index,
                    false,
                ),
            })
            .collect()
    }
}

fn build_forward_returns(execution_bars: &[Bar], bar_index: usize) -> Vec<ForwardReturnMetric> {
    [1usize, 6, 24]
        .into_iter()
        .filter_map(|horizon| {
            let start = execution_bars.get(bar_index)?;
            let end_index = bar_index.saturating_add(horizon);
            let end = execution_bars
                .get(end_index)
                .or_else(|| execution_bars.last())?;
            Some(ForwardReturnMetric {
                horizon_bars: horizon,
                return_pct: if start.close.abs() > crate::backtest::EPSILON {
                    end.close / start.close - 1.0
                } else {
                    0.0
                },
                complete_window: execution_bars.get(end_index).is_some(),
            })
        })
        .collect()
}

fn forward_extreme_pct(execution_bars: &[Bar], bar_index: usize, favorable: bool) -> Option<f64> {
    let start = execution_bars.get(bar_index)?;
    if start.close.abs() <= crate::backtest::EPSILON {
        return Some(0.0);
    }
    let end_index = usize::min(
        bar_index.saturating_add(24),
        execution_bars.len().saturating_sub(1),
    );
    let window = execution_bars.get(bar_index + 1..=end_index)?;
    if favorable {
        window
            .iter()
            .map(|bar| bar.high / start.close - 1.0)
            .reduce(f64::max)
    } else {
        window
            .iter()
            .map(|bar| bar.low / start.close - 1.0)
            .reduce(f64::min)
    }
}

fn mean_option(sum: f64, count: usize) -> Option<f64> {
    if count == 0 {
        None
    } else {
        Some(sum / count as f64)
    }
}

fn mean_zero(sum: f64, count: usize) -> f64 {
    mean_option(sum, count).unwrap_or(0.0)
}
