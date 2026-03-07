//! Library backtesting layer built on top of the source-aware runtime.
//!
//! The backtester reuses the existing VM execution model to produce trigger
//! outputs, then deterministically translates those trigger events into fills,
//! trades, and an equity curve for one configured execution source.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::compiler::CompiledProgram;
use crate::diagnostic::RuntimeError;
use crate::output::Outputs;
use crate::runtime::{run_with_sources, Bar, SourceRuntimeConfig, VmLimits};

const BPS_SCALE: f64 = 10_000.0;
const EPSILON: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BacktestConfig {
    pub execution_source_alias: String,
    pub initial_capital: f64,
    pub fee_bps: f64,
    pub slippage_bps: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    Long,
    Short,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillAction {
    Buy,
    Sell,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub bar_index: usize,
    pub time: f64,
    pub action: FillAction,
    pub quantity: f64,
    pub raw_price: f64,
    pub price: f64,
    pub notional: f64,
    pub fee: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub side: PositionSide,
    pub quantity: f64,
    pub entry: Fill,
    pub exit: Fill,
    pub realized_pnl: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PositionSnapshot {
    pub side: PositionSide,
    pub quantity: f64,
    pub entry_bar_index: usize,
    pub entry_time: f64,
    pub entry_price: f64,
    pub market_price: f64,
    pub market_time: f64,
    pub unrealized_pnl: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EquityPoint {
    pub bar_index: usize,
    pub time: f64,
    pub cash: f64,
    pub equity: f64,
    pub position_side: Option<PositionSide>,
    pub quantity: f64,
    pub mark_price: f64,
    pub gross_exposure: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BacktestSummary {
    pub starting_equity: f64,
    pub ending_equity: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub total_return: f64,
    pub trade_count: usize,
    pub winning_trade_count: usize,
    pub losing_trade_count: usize,
    pub win_rate: f64,
    pub max_drawdown: f64,
    pub max_gross_exposure: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BacktestResult {
    pub outputs: Outputs,
    pub fills: Vec<Fill>,
    pub trades: Vec<Trade>,
    pub equity_curve: Vec<EquityPoint>,
    pub summary: BacktestSummary,
    pub open_position: Option<PositionSnapshot>,
}

#[derive(Debug, Error, PartialEq)]
pub enum BacktestError {
    #[error("runtime failed during backtest: {0}")]
    Runtime(#[from] RuntimeError),
    #[error("execution source `{alias}` is not declared in the compiled program")]
    UnknownExecutionSource { alias: String },
    #[error("missing base feed for execution source `{alias}`")]
    MissingExecutionBaseFeed { alias: String },
    #[error("backtest initial capital must be finite and > 0, found {value}")]
    InvalidInitialCapital { value: f64 },
    #[error("backtest fee_bps must be finite and >= 0, found {value}")]
    InvalidFeeBps { value: f64 },
    #[error("backtest slippage_bps must be finite and >= 0, found {value}")]
    InvalidSlippageBps { value: f64 },
    #[error("backtest requires entry/exit signals for long and short; missing={missing:?}, available={available:?}")]
    MissingSignalRoles {
        missing: Vec<String>,
        available: Vec<String>,
    },
    #[error("conflicting long and short entry signals before execution bar at {time}")]
    ConflictingSignals { time: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SignalKind {
    LongEntry,
    LongExit,
    ShortEntry,
    ShortExit,
}

#[derive(Clone, Copy, Debug, Default)]
struct PendingSignals {
    long_entry: bool,
    long_exit: bool,
    short_entry: bool,
    short_exit: bool,
}

#[derive(Clone, Debug)]
struct SignalBatch {
    time: f64,
    pending: PendingSignals,
}

#[derive(Clone, Debug)]
struct ResolvedSignals {
    kinds_by_output_id: HashMap<usize, SignalKind>,
}

#[derive(Clone, Debug)]
struct PositionState {
    side: PositionSide,
    quantity: f64,
    entry_bar_index: usize,
    entry_time: f64,
    entry_price: f64,
}

#[derive(Clone, Debug)]
struct OpenTrade {
    side: PositionSide,
    quantity: f64,
    entry: Fill,
}

pub fn run_backtest_with_sources(
    compiled: &CompiledProgram,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: BacktestConfig,
) -> Result<BacktestResult, BacktestError> {
    validate_config(&config)?;
    let execution_source_id = execution_source_id(compiled, &config.execution_source_alias)?;
    let resolved_signals = resolve_signals(compiled)?;

    let execution_bars = execution_bars(
        &runtime,
        execution_source_id,
        &config.execution_source_alias,
    )?;
    let outputs = run_with_sources(compiled, runtime, vm_limits)?;
    let signal_batches = collect_signal_batches(&outputs, &resolved_signals);
    simulate_backtest(outputs, execution_bars, &config, signal_batches)
}

fn validate_config(config: &BacktestConfig) -> Result<(), BacktestError> {
    if !config.initial_capital.is_finite() || config.initial_capital <= 0.0 {
        return Err(BacktestError::InvalidInitialCapital {
            value: config.initial_capital,
        });
    }
    if !config.fee_bps.is_finite() || config.fee_bps < 0.0 {
        return Err(BacktestError::InvalidFeeBps {
            value: config.fee_bps,
        });
    }
    if !config.slippage_bps.is_finite() || config.slippage_bps < 0.0 {
        return Err(BacktestError::InvalidSlippageBps {
            value: config.slippage_bps,
        });
    }
    Ok(())
}

fn execution_source_id(compiled: &CompiledProgram, alias: &str) -> Result<u16, BacktestError> {
    compiled
        .program
        .declared_sources
        .iter()
        .find(|source| source.alias == alias)
        .map(|source| source.id)
        .ok_or_else(|| BacktestError::UnknownExecutionSource {
            alias: alias.to_string(),
        })
}

fn available_trigger_names(compiled: &CompiledProgram) -> Vec<String> {
    compiled
        .program
        .outputs
        .iter()
        .filter(|decl| matches!(decl.kind, crate::bytecode::OutputKind::Trigger))
        .map(|decl| decl.name.clone())
        .collect()
}

fn resolve_signals(compiled: &CompiledProgram) -> Result<ResolvedSignals, BacktestError> {
    let has_first_class = compiled
        .program
        .outputs
        .iter()
        .any(|decl| decl.signal_role.is_some());
    let mut kinds_by_output_id = HashMap::new();

    for (output_id, decl) in compiled.program.outputs.iter().enumerate() {
        if !matches!(decl.kind, crate::bytecode::OutputKind::Trigger) {
            continue;
        }
        let kind = if has_first_class {
            decl.signal_role.map(signal_kind_for_role)
        } else {
            legacy_signal_kind(&decl.name)
        };
        if let Some(kind) = kind {
            kinds_by_output_id.insert(output_id, kind);
        }
    }

    let missing = missing_signal_roles(&kinds_by_output_id);
    if !missing.is_empty() {
        return Err(BacktestError::MissingSignalRoles {
            missing,
            available: available_trigger_names(compiled),
        });
    }

    Ok(ResolvedSignals { kinds_by_output_id })
}

fn execution_bars(
    runtime: &SourceRuntimeConfig,
    execution_source_id: u16,
    alias: &str,
) -> Result<Vec<Bar>, BacktestError> {
    runtime
        .feeds
        .iter()
        .find(|feed| {
            feed.source_id == execution_source_id && feed.interval == runtime.base_interval
        })
        .map(|feed| feed.bars.clone())
        .ok_or_else(|| BacktestError::MissingExecutionBaseFeed {
            alias: alias.to_string(),
        })
}

fn collect_signal_batches(outputs: &Outputs, signals: &ResolvedSignals) -> Vec<SignalBatch> {
    let mut grouped = BTreeMap::<i64, PendingSignals>::new();
    for event in &outputs.trigger_events {
        let Some(kind) = signals.kinds_by_output_id.get(&event.output_id).copied() else {
            continue;
        };
        let time_key = event.time.and_then(time_key);
        let Some(time_key) = time_key else {
            continue;
        };
        let pending = grouped.entry(time_key).or_default();
        match kind {
            SignalKind::LongEntry => pending.long_entry = true,
            SignalKind::LongExit => pending.long_exit = true,
            SignalKind::ShortEntry => pending.short_entry = true,
            SignalKind::ShortExit => pending.short_exit = true,
        }
    }
    grouped
        .into_iter()
        .map(|(time, pending)| SignalBatch {
            time: time as f64,
            pending,
        })
        .collect()
}

fn signal_kind_for_role(role: crate::bytecode::SignalRole) -> SignalKind {
    match role {
        crate::bytecode::SignalRole::LongEntry => SignalKind::LongEntry,
        crate::bytecode::SignalRole::LongExit => SignalKind::LongExit,
        crate::bytecode::SignalRole::ShortEntry => SignalKind::ShortEntry,
        crate::bytecode::SignalRole::ShortExit => SignalKind::ShortExit,
    }
}

fn legacy_signal_kind(name: &str) -> Option<SignalKind> {
    match name {
        "long_entry" => Some(SignalKind::LongEntry),
        "long_exit" => Some(SignalKind::LongExit),
        "short_entry" => Some(SignalKind::ShortEntry),
        "short_exit" => Some(SignalKind::ShortExit),
        _ => None,
    }
}

fn missing_signal_roles(kinds_by_output_id: &HashMap<usize, SignalKind>) -> Vec<String> {
    let mut present = [false; 4];
    for kind in kinds_by_output_id.values() {
        match kind {
            SignalKind::LongEntry => present[0] = true,
            SignalKind::LongExit => present[1] = true,
            SignalKind::ShortEntry => present[2] = true,
            SignalKind::ShortExit => present[3] = true,
        }
    }
    if present[0] || present[2] {
        Vec::new()
    } else {
        vec!["long_entry".to_string(), "short_entry".to_string()]
    }
}

fn time_key(time: f64) -> Option<i64> {
    if time.is_finite() && time.fract() == 0.0 {
        Some(time as i64)
    } else {
        None
    }
}

fn simulate_backtest(
    outputs: Outputs,
    execution_bars: Vec<Bar>,
    config: &BacktestConfig,
    signal_batches: Vec<SignalBatch>,
) -> Result<BacktestResult, BacktestError> {
    let fee_rate = config.fee_bps / BPS_SCALE;
    let slippage_rate = config.slippage_bps / BPS_SCALE;
    let mut cash = config.initial_capital;
    let mut position = None::<PositionState>;
    let mut open_trade = None::<OpenTrade>;
    let mut fills = Vec::new();
    let mut trades = Vec::new();
    let mut equity_curve = Vec::with_capacity(execution_bars.len());
    let mut pending = PendingSignals::default();
    let mut batch_cursor = 0usize;
    let mut total_realized_pnl = 0.0;
    let mut max_gross_exposure = 0.0_f64;
    let mut peak_equity = config.initial_capital;
    let mut max_drawdown = 0.0_f64;

    for (bar_index, bar) in execution_bars.iter().copied().enumerate() {
        while batch_cursor < signal_batches.len() && signal_batches[batch_cursor].time < bar.time {
            pending.merge(signal_batches[batch_cursor].pending);
            batch_cursor += 1;
        }

        if let Some(action) = resolve_pending(pending, position.as_ref(), bar.time)? {
            if action.close_current {
                let closed_position = position
                    .take()
                    .expect("close action requires an open position");
                let fill = close_position(
                    bar_index,
                    bar,
                    fee_rate,
                    slippage_rate,
                    &mut cash,
                    &closed_position,
                );
                let trade = close_trade(
                    open_trade
                        .take()
                        .expect("close action requires an open trade"),
                    fill.clone(),
                );
                total_realized_pnl += trade.realized_pnl;
                fills.push(fill);
                trades.push(trade);
            }
            if let Some(side) = action.open_side {
                let (next_position, next_trade, fill) =
                    open_position(bar_index, bar, side, fee_rate, slippage_rate, &mut cash);
                fills.push(fill);
                position = Some(next_position);
                open_trade = Some(next_trade);
            }
            pending = PendingSignals::default();
        }

        let quantity = position.as_ref().map_or(0.0, |state| state.quantity);
        let gross_exposure = quantity.abs() * bar.close;
        max_gross_exposure = max_gross_exposure.max(gross_exposure);
        let equity = cash + quantity * bar.close;
        peak_equity = peak_equity.max(equity);
        max_drawdown = max_drawdown.max(peak_equity - equity);
        equity_curve.push(EquityPoint {
            bar_index,
            time: bar.time,
            cash,
            equity,
            position_side: position.as_ref().map(|state| state.side),
            quantity,
            mark_price: bar.close,
            gross_exposure,
        });
    }

    let ending_equity = equity_curve
        .last()
        .map_or(config.initial_capital, |point| point.equity);
    let unrealized_pnl = ending_equity - config.initial_capital - total_realized_pnl;
    let winning_trade_count = trades
        .iter()
        .filter(|trade| trade.realized_pnl > 0.0)
        .count();
    let losing_trade_count = trades
        .iter()
        .filter(|trade| trade.realized_pnl < 0.0)
        .count();
    let trade_count = trades.len();
    let win_rate = if trade_count == 0 {
        0.0
    } else {
        winning_trade_count as f64 / trade_count as f64
    };

    let open_position = match (position, equity_curve.last()) {
        (Some(position), Some(last_point)) => Some(PositionSnapshot {
            side: position.side,
            quantity: position.quantity.abs(),
            entry_bar_index: position.entry_bar_index,
            entry_time: position.entry_time,
            entry_price: position.entry_price,
            market_price: last_point.mark_price,
            market_time: last_point.time,
            unrealized_pnl: unrealized_pnl_for_position(&position, last_point.mark_price),
        }),
        _ => None,
    };

    Ok(BacktestResult {
        outputs,
        fills,
        trades,
        equity_curve,
        summary: BacktestSummary {
            starting_equity: config.initial_capital,
            ending_equity,
            realized_pnl: total_realized_pnl,
            unrealized_pnl,
            total_return: (ending_equity - config.initial_capital) / config.initial_capital,
            trade_count,
            winning_trade_count,
            losing_trade_count,
            win_rate,
            max_drawdown,
            max_gross_exposure,
        },
        open_position,
    })
}

fn unrealized_pnl_for_position(position: &PositionState, mark_price: f64) -> f64 {
    match position.side {
        PositionSide::Long => (mark_price - position.entry_price) * position.quantity.abs(),
        PositionSide::Short => (position.entry_price - mark_price) * position.quantity.abs(),
    }
}

#[derive(Clone, Copy)]
struct ResolvedAction {
    close_current: bool,
    open_side: Option<PositionSide>,
}

fn resolve_pending(
    pending: PendingSignals,
    position: Option<&PositionState>,
    execution_time: f64,
) -> Result<Option<ResolvedAction>, BacktestError> {
    match position.map(|state| state.side) {
        None => {
            if pending.long_entry && pending.short_entry {
                return Err(BacktestError::ConflictingSignals {
                    time: execution_time,
                });
            }
            if pending.long_entry {
                return Ok(Some(ResolvedAction {
                    close_current: false,
                    open_side: Some(PositionSide::Long),
                }));
            }
            if pending.short_entry {
                return Ok(Some(ResolvedAction {
                    close_current: false,
                    open_side: Some(PositionSide::Short),
                }));
            }
            Ok(None)
        }
        Some(PositionSide::Long) => {
            if pending.short_entry {
                return Ok(Some(ResolvedAction {
                    close_current: true,
                    open_side: Some(PositionSide::Short),
                }));
            }
            if pending.long_exit {
                return Ok(Some(ResolvedAction {
                    close_current: true,
                    open_side: None,
                }));
            }
            Ok(None)
        }
        Some(PositionSide::Short) => {
            if pending.long_entry {
                return Ok(Some(ResolvedAction {
                    close_current: true,
                    open_side: Some(PositionSide::Long),
                }));
            }
            if pending.short_exit {
                return Ok(Some(ResolvedAction {
                    close_current: true,
                    open_side: None,
                }));
            }
            Ok(None)
        }
    }
}

fn open_position(
    bar_index: usize,
    bar: Bar,
    side: PositionSide,
    fee_rate: f64,
    slippage_rate: f64,
    cash: &mut f64,
) -> (PositionState, OpenTrade, Fill) {
    let action = match side {
        PositionSide::Long => FillAction::Buy,
        PositionSide::Short => FillAction::Sell,
    };
    let price = adjusted_price(bar.open, action, slippage_rate);
    let quantity = *cash / (price * (1.0 + fee_rate));
    let notional = quantity * price;
    let fee = notional * fee_rate;
    match side {
        PositionSide::Long => {
            *cash -= notional + fee;
        }
        PositionSide::Short => {
            *cash += notional - fee;
        }
    }
    zero_small_cash(cash);
    let signed_quantity = match side {
        PositionSide::Long => quantity,
        PositionSide::Short => -quantity,
    };
    let fill = Fill {
        bar_index,
        time: bar.time,
        action,
        quantity,
        raw_price: bar.open,
        price,
        notional,
        fee,
    };
    let position = PositionState {
        side,
        quantity: signed_quantity,
        entry_bar_index: bar_index,
        entry_time: bar.time,
        entry_price: price,
    };
    let trade = OpenTrade {
        side,
        quantity,
        entry: fill.clone(),
    };
    (position, trade, fill)
}

fn close_position(
    bar_index: usize,
    bar: Bar,
    fee_rate: f64,
    slippage_rate: f64,
    cash: &mut f64,
    position: &PositionState,
) -> Fill {
    let action = match position.side {
        PositionSide::Long => FillAction::Sell,
        PositionSide::Short => FillAction::Buy,
    };
    let price = adjusted_price(bar.open, action, slippage_rate);
    let quantity = position.quantity.abs();
    let notional = quantity * price;
    let fee = notional * fee_rate;
    match position.side {
        PositionSide::Long => {
            *cash += notional - fee;
        }
        PositionSide::Short => {
            *cash -= notional + fee;
        }
    }
    zero_small_cash(cash);
    Fill {
        bar_index,
        time: bar.time,
        action,
        quantity,
        raw_price: bar.open,
        price,
        notional,
        fee,
    }
}

fn close_trade(open_trade: OpenTrade, exit: Fill) -> Trade {
    let signed_entry_price = match open_trade.side {
        PositionSide::Long => -open_trade.entry.price,
        PositionSide::Short => open_trade.entry.price,
    };
    let signed_exit_price = match open_trade.side {
        PositionSide::Long => exit.price,
        PositionSide::Short => -exit.price,
    };
    let realized_pnl = (signed_entry_price + signed_exit_price) * open_trade.quantity
        - open_trade.entry.fee
        - exit.fee;
    Trade {
        side: open_trade.side,
        quantity: open_trade.quantity,
        entry: open_trade.entry,
        exit,
        realized_pnl,
    }
}

fn adjusted_price(raw_open: f64, action: FillAction, slippage_rate: f64) -> f64 {
    match action {
        FillAction::Buy => raw_open * (1.0 + slippage_rate),
        FillAction::Sell => raw_open * (1.0 - slippage_rate),
    }
}

fn zero_small_cash(cash: &mut f64) {
    if cash.abs() < EPSILON {
        *cash = 0.0;
    }
}

impl PendingSignals {
    fn merge(&mut self, other: Self) {
        self.long_entry |= other.long_entry;
        self.long_exit |= other.long_exit;
        self.short_entry |= other.short_entry;
        self.short_exit |= other.short_exit;
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_pending, PendingSignals, PositionSide, PositionState};

    #[test]
    fn resolve_pending_prefers_reversal_for_opposite_entry() {
        let position = PositionState {
            side: PositionSide::Long,
            quantity: 2.0,
            entry_bar_index: 0,
            entry_time: 0.0,
            entry_price: 10.0,
        };
        let action = resolve_pending(
            PendingSignals {
                long_exit: true,
                short_entry: true,
                ..PendingSignals::default()
            },
            Some(&position),
            1.0,
        )
        .expect("pending resolution should succeed")
        .expect("action should be present");
        assert!(action.close_current);
        assert_eq!(action.open_side, Some(PositionSide::Short));
    }

    #[test]
    fn resolve_pending_ignores_same_side_entry() {
        let position = PositionState {
            side: PositionSide::Short,
            quantity: -2.0,
            entry_bar_index: 0,
            entry_time: 0.0,
            entry_price: 10.0,
        };
        let action = resolve_pending(
            PendingSignals {
                short_entry: true,
                ..PendingSignals::default()
            },
            Some(&position),
            1.0,
        )
        .expect("pending resolution should succeed");
        assert!(action.is_none());
    }
}
