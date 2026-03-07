//! Library backtesting layer built on top of the source-aware runtime.
//!
//! The backtester reuses the existing VM execution model to produce trigger
//! outputs, internal order-field series, and a deterministic order simulation
//! for one configured execution source.

mod bridge;
mod engine;
mod orders;
mod venue;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::bytecode::SignalRole;
use crate::compiler::CompiledProgram;
use crate::diagnostic::RuntimeError;
use crate::order::{OrderKind, TimeInForce, TriggerReference};
use crate::output::{OutputValue, Outputs};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    Open,
    Filled,
    Cancelled,
    Rejected,
    Expired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderEndReason {
    Replaced,
    RoleInvalidated,
    MissingPrice,
    MissingTriggerPrice,
    MissingExpireTime,
    IocUnfilled,
    FokUnfilled,
    PostOnlyWouldCross,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderRecord {
    pub id: usize,
    pub role: SignalRole,
    pub kind: OrderKind,
    pub action: FillAction,
    pub tif: Option<TimeInForce>,
    pub post_only: bool,
    pub trigger_ref: Option<TriggerReference>,
    pub signal_time: f64,
    pub placed_bar_index: usize,
    pub placed_time: f64,
    pub trigger_time: Option<f64>,
    pub fill_bar_index: Option<usize>,
    pub fill_time: Option<f64>,
    pub raw_price: Option<f64>,
    pub fill_price: Option<f64>,
    pub limit_price: Option<f64>,
    pub trigger_price: Option<f64>,
    pub expire_time: Option<f64>,
    pub status: OrderStatus,
    pub end_reason: Option<OrderEndReason>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureValue {
    pub name: String,
    pub value: OutputValue,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FeatureSnapshot {
    pub bar_index: usize,
    pub time: f64,
    pub values: Vec<FeatureValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeExitClassification {
    Signal,
    StopLoss,
    TakeProfit,
    Reversal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderDiagnostic {
    pub order_id: usize,
    pub role: SignalRole,
    pub kind: OrderKind,
    pub status: OrderStatus,
    pub end_reason: Option<OrderEndReason>,
    pub signal_snapshot: Option<FeatureSnapshot>,
    pub placed_snapshot: Option<FeatureSnapshot>,
    pub fill_snapshot: Option<FeatureSnapshot>,
    pub bars_to_fill: Option<usize>,
    pub time_to_fill_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TradeDiagnostic {
    pub trade_id: usize,
    pub side: PositionSide,
    pub entry_order_id: usize,
    pub exit_order_id: usize,
    pub entry_role: SignalRole,
    pub exit_role: SignalRole,
    pub entry_kind: OrderKind,
    pub exit_kind: OrderKind,
    pub exit_classification: TradeExitClassification,
    pub entry_snapshot: Option<FeatureSnapshot>,
    pub exit_snapshot: Option<FeatureSnapshot>,
    pub bars_held: usize,
    pub duration_ms: f64,
    pub realized_pnl: f64,
    pub mae_price_delta: f64,
    pub mfe_price_delta: f64,
    pub mae_pct: f64,
    pub mfe_pct: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderKindDiagnosticSummary {
    pub kind: OrderKind,
    pub placed_count: usize,
    pub filled_count: usize,
    pub cancelled_count: usize,
    pub rejected_count: usize,
    pub expired_count: usize,
    pub fill_rate: f64,
    pub average_bars_to_fill: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SideDiagnosticSummary {
    pub side: PositionSide,
    pub trade_count: usize,
    pub win_rate: f64,
    pub average_realized_pnl: f64,
    pub average_bars_held: f64,
    pub average_mae_pct: f64,
    pub average_mfe_pct: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BacktestDiagnosticSummary {
    pub order_fill_rate: f64,
    pub average_bars_to_fill: f64,
    pub average_bars_held: f64,
    pub average_mae_pct: f64,
    pub average_mfe_pct: f64,
    pub signal_exit_count: usize,
    pub stop_loss_exit_count: usize,
    pub take_profit_exit_count: usize,
    pub reversal_exit_count: usize,
    pub by_order_kind: Vec<OrderKindDiagnosticSummary>,
    pub by_side: Vec<SideDiagnosticSummary>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BacktestDiagnostics {
    pub order_diagnostics: Vec<OrderDiagnostic>,
    pub trade_diagnostics: Vec<TradeDiagnostic>,
    pub summary: BacktestDiagnosticSummary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BacktestResult {
    pub outputs: Outputs,
    pub orders: Vec<OrderRecord>,
    pub fills: Vec<Fill>,
    pub trades: Vec<Trade>,
    pub diagnostics: BacktestDiagnostics,
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
    #[error("execution source `{alias}` uses unsupported backtest order for venue `{venue}`: role={role:?} kind={kind:?} reason={reason}")]
    UnsupportedOrderForVenue {
        alias: String,
        venue: String,
        role: SignalRole,
        kind: OrderKind,
        reason: String,
    },
}

pub fn run_backtest_with_sources(
    compiled: &CompiledProgram,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: BacktestConfig,
) -> Result<BacktestResult, BacktestError> {
    validate_config(&config)?;
    let execution = bridge::resolve_execution_source(compiled, &config.execution_source_alias)?;
    let execution_bars = execution_bars(
        &runtime,
        execution.source_id,
        &config.execution_source_alias,
    )?;
    let outputs = run_with_sources(compiled, runtime, vm_limits)?;
    let prepared = bridge::prepare_backtest(
        compiled,
        &outputs,
        &config.execution_source_alias,
        execution.template,
    )?;
    engine::simulate_backtest(outputs, execution_bars, &config, prepared)
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
