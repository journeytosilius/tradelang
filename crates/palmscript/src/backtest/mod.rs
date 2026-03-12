//! Library backtesting layer built on top of the source-aware runtime.
//!
//! The backtester reuses the existing VM execution model to produce trigger
//! outputs, internal order-field series, and a deterministic order simulation
//! for one configured execution source.

mod bridge;
mod diagnostics;
mod engine;
mod optimize;
mod orders;
mod venue;
mod walk_forward;
mod walk_forward_sweep;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::bytecode::SignalRole;
use crate::compiler::CompiledProgram;
use crate::diagnostic::RuntimeError;
use crate::exchange::{MarkPriceBasis, VenueRiskSnapshot};
use crate::order::{OrderKind, SizeMode, TimeInForce, TriggerReference};
use crate::output::{OutputValue, Outputs};
use crate::position::PositionSide;
use crate::runtime::{Bar, RuntimeStepper, SourceRuntimeConfig, VmLimits};

const BPS_SCALE: f64 = 10_000.0;
const EPSILON: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BacktestConfig {
    pub execution_source_alias: String,
    pub initial_capital: f64,
    pub fee_bps: f64,
    pub slippage_bps: f64,
    pub perp: Option<PerpBacktestConfig>,
    pub perp_context: Option<PerpBacktestContext>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerpMarginMode {
    Isolated,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerpBacktestConfig {
    pub leverage: f64,
    pub margin_mode: PerpMarginMode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerpBacktestContext {
    pub mark_price_basis: MarkPriceBasis,
    pub mark_bars: Vec<Bar>,
    pub risk_snapshot: VenueRiskSnapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerpBacktestMetadata {
    pub leverage: f64,
    pub margin_mode: PerpMarginMode,
    pub mark_price_basis: MarkPriceBasis,
    pub risk_snapshot: VenueRiskSnapshot,
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
    pub free_collateral: Option<f64>,
    pub isolated_margin: Option<f64>,
    pub maintenance_margin: Option<f64>,
    pub liquidation_price: Option<f64>,
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
    pub free_collateral: Option<f64>,
    pub isolated_margin: Option<f64>,
    pub maintenance_margin: Option<f64>,
    pub liquidation_price: Option<f64>,
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
    Rearmed,
    OcoCancelled,
    PositionClosed,
    RoleInvalidated,
    MissingPrice,
    MissingTriggerPrice,
    MissingExpireTime,
    MissingSizeFraction,
    InvalidSizeFraction,
    MissingRiskStopPrice,
    InvalidRiskPct,
    InvalidRiskDistance,
    InsufficientCollateral,
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
    pub size_mode: Option<SizeMode>,
    pub size_fraction: Option<f64>,
    pub requested_risk_pct: Option<f64>,
    pub requested_stop_price: Option<f64>,
    pub effective_risk_per_unit: Option<f64>,
    pub capital_limited: bool,
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
pub enum ExportValueType {
    Numeric,
    Bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NumericExportDiagnosticSummary {
    pub name: String,
    pub sample_count: usize,
    pub na_count: usize,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    pub entry_mean: Option<f64>,
    pub exit_mean: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoolExportDiagnosticSummary {
    pub name: String,
    pub sample_count: usize,
    pub na_count: usize,
    pub true_count: usize,
    pub false_count: usize,
    pub rising_edge_count: usize,
    pub falling_edge_count: usize,
    pub true_while_flat_count: usize,
    pub true_while_in_market_count: usize,
    pub true_while_long_count: usize,
    pub true_while_short_count: usize,
    pub execution_return_while_true: f64,
    pub execution_return_while_true_and_flat: f64,
    pub trade_count: usize,
    pub win_rate: f64,
    pub average_realized_pnl: f64,
    pub average_mae_pct: f64,
    pub average_mfe_pct: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "summary_kind", rename_all = "snake_case")]
pub enum ExportDiagnosticSummary {
    Numeric(NumericExportDiagnosticSummary),
    Bool(BoolExportDiagnosticSummary),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BacktestCaptureSummary {
    pub execution_asset_return: f64,
    pub strategy_total_return: f64,
    pub flat_bar_count: usize,
    pub long_bar_count: usize,
    pub short_bar_count: usize,
    pub in_market_bar_count: usize,
    pub flat_bar_pct: f64,
    pub long_bar_pct: f64,
    pub short_bar_pct: f64,
    pub in_market_bar_pct: f64,
    pub execution_return_while_flat: f64,
    pub execution_return_while_long: f64,
    pub execution_return_while_short: f64,
    pub opportunity_cost_return: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpportunityEventKind {
    ExportActivated,
    SignalQueued,
    SignalIgnoredCooldown,
    SignalIgnoredSameSide,
    SignalIgnoredNoPosition,
    SignalConflicted,
    SignalReplacedPendingOrder,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForwardReturnMetric {
    pub horizon_bars: usize,
    pub return_pct: f64,
    pub complete_window: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpportunityEvent {
    pub kind: OpportunityEventKind,
    pub name: String,
    pub role: Option<SignalRole>,
    pub bar_index: usize,
    pub time: f64,
    pub position_snapshot: Option<PositionSnapshot>,
    pub feature_snapshot: Option<FeatureSnapshot>,
    pub forward_returns: Vec<ForwardReturnMetric>,
    pub forward_max_favorable_pct: Option<f64>,
    pub forward_max_adverse_pct: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeExitClassification {
    Signal,
    Protect,
    Target,
    Reversal,
    Liquidation,
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
    pub placed_position: Option<PositionSnapshot>,
    pub fill_position: Option<PositionSnapshot>,
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
    pub protect_exit_count: usize,
    pub target_exit_count: usize,
    pub reversal_exit_count: usize,
    pub liquidation_exit_count: usize,
    pub by_order_kind: Vec<OrderKindDiagnosticSummary>,
    pub by_side: Vec<SideDiagnosticSummary>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BacktestDiagnostics {
    pub order_diagnostics: Vec<OrderDiagnostic>,
    pub trade_diagnostics: Vec<TradeDiagnostic>,
    pub summary: BacktestDiagnosticSummary,
    pub capture_summary: BacktestCaptureSummary,
    pub export_summaries: Vec<ExportDiagnosticSummary>,
    pub opportunity_events: Vec<OpportunityEvent>,
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
    pub perp: Option<PerpBacktestMetadata>,
}

pub use optimize::{
    run_optimize_with_source, run_optimize_with_source_resume, OptimizeCandidateSummary,
    OptimizeConfig, OptimizeError, OptimizeEvaluationSummary, OptimizeHoldoutConfig,
    OptimizeHoldoutResult, OptimizeObjective, OptimizeParamSpace, OptimizePreset,
    OptimizeProgressEvent, OptimizeProgressListener, OptimizeProgressState, OptimizeResult,
    OptimizeResumeState, OptimizeRunner, OptimizeScheduledBatch, OptimizeScheduledTrial,
};
pub use walk_forward::{
    run_walk_forward_with_sources, WalkForwardConfig, WalkForwardEquityPoint, WalkForwardResult,
    WalkForwardSegmentDiagnostics, WalkForwardSegmentResult, WalkForwardStitchedSummary,
    WalkForwardWindowSummary,
};
pub use walk_forward_sweep::{
    run_walk_forward_sweep_with_source, InputSweepDefinition, WalkForwardSweepCandidateSummary,
    WalkForwardSweepConfig, WalkForwardSweepError, WalkForwardSweepObjective,
    WalkForwardSweepResult,
};

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
    #[error("backtest leverage must be finite and >= 1.0, found {value}")]
    InvalidLeverage { value: f64 },
    #[error("backtest only supports isolated perp margin mode, found {mode:?}")]
    UnsupportedPerpMarginMode { mode: PerpMarginMode },
    #[error("spot execution source `{alias}` cannot use perp-only backtest settings")]
    SpotPerpConfigMismatch { alias: String },
    #[error("perp execution source `{alias}` requires a venue risk snapshot and mark-price feed")]
    MissingPerpContext { alias: String },
    #[error(
        "perp execution source `{alias}` requires mark-price bars aligned to the execution window"
    )]
    MissingPerpMarkFeed { alias: String },
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
    #[error("walk-forward train_bars must be > 0, found {value}")]
    InvalidWalkForwardTrainBars { value: usize },
    #[error("walk-forward test_bars must be > 0, found {value}")]
    InvalidWalkForwardTestBars { value: usize },
    #[error("walk-forward step_bars must be > 0, found {value}")]
    InvalidWalkForwardStepBars { value: usize },
    #[error("walk-forward requires at least {required} execution bars, but only {available} were available")]
    InsufficientWalkForwardBars { available: usize, required: usize },
}

pub fn run_backtest_with_sources(
    compiled: &CompiledProgram,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    mut config: BacktestConfig,
) -> Result<BacktestResult, BacktestError> {
    validate_config(&config)?;
    let execution = bridge::resolve_execution_source(compiled, &config.execution_source_alias)?;
    match execution.template {
        crate::interval::SourceTemplate::BinanceSpot
        | crate::interval::SourceTemplate::BybitSpot
        | crate::interval::SourceTemplate::GateSpot => {
            if config.perp.is_some() || config.perp_context.is_some() {
                return Err(BacktestError::SpotPerpConfigMismatch {
                    alias: config.execution_source_alias.clone(),
                });
            }
        }
        crate::interval::SourceTemplate::BinanceUsdm
        | crate::interval::SourceTemplate::BybitUsdtPerps
        | crate::interval::SourceTemplate::GateUsdtPerps => {
            if config.perp.is_none() {
                config.perp = Some(PerpBacktestConfig {
                    leverage: 1.0,
                    margin_mode: PerpMarginMode::Isolated,
                });
            }
            if config.perp_context.is_none()
                && config
                    .perp
                    .as_ref()
                    .is_some_and(|perp| perp.leverage > 1.0 + EPSILON)
            {
                return Err(BacktestError::MissingPerpContext {
                    alias: config.execution_source_alias.clone(),
                });
            }
        }
    }
    let execution_bars = execution_bars(
        &runtime,
        execution.source_id,
        &config.execution_source_alias,
    )?;
    let prepared =
        bridge::prepare_backtest(compiled, &config.execution_source_alias, execution.template)?;
    let stepper = RuntimeStepper::try_new(compiled, runtime, vm_limits)?;
    engine::simulate_backtest(stepper, execution_bars, &config, prepared)
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
    if let Some(perp) = &config.perp {
        if !perp.leverage.is_finite() || perp.leverage < 1.0 {
            return Err(BacktestError::InvalidLeverage {
                value: perp.leverage,
            });
        }
        if !matches!(perp.margin_mode, PerpMarginMode::Isolated) {
            return Err(BacktestError::UnsupportedPerpMarginMode {
                mode: perp.margin_mode,
            });
        }
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

pub(crate) fn average(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut count = 0usize;
    let mut sum = 0.0;
    for value in values {
        sum += value;
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
}

pub(crate) fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}
