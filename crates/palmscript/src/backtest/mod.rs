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

use std::collections::BTreeMap;

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
    #[serde(default)]
    pub portfolio_execution_aliases: Vec<String>,
    #[serde(default)]
    pub activation_time_ms: Option<i64>,
    pub initial_capital: f64,
    pub fee_bps: f64,
    pub slippage_bps: f64,
    #[serde(default)]
    pub diagnostics_detail: DiagnosticsDetailMode,
    pub perp: Option<PerpBacktestConfig>,
    pub perp_context: Option<PerpBacktestContext>,
    #[serde(default)]
    pub portfolio_perp_contexts: BTreeMap<String, PerpBacktestContext>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticsDetailMode {
    #[default]
    SummaryOnly,
    FullTrace,
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
    pub execution_alias: String,
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
    pub execution_alias: String,
    pub side: PositionSide,
    pub quantity: f64,
    pub entry: Fill,
    pub exit: Fill,
    pub realized_pnl: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PositionSnapshot {
    pub execution_alias: String,
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
    pub net_exposure: f64,
    pub open_position_count: usize,
    pub long_position_count: usize,
    pub short_position_count: usize,
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
    pub max_net_exposure: f64,
    pub peak_open_position_count: usize,
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
    PortfolioControlRejected,
    IocUnfilled,
    FokUnfilled,
    PostOnlyWouldCross,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderRecord {
    pub id: usize,
    pub execution_alias: String,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortfolioControlKind {
    MaxPositions,
    MaxLongPositions,
    MaxShortPositions,
    MaxGrossExposurePct,
    MaxNetExposurePct,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortfolioControlBlockSummary {
    pub kind: PortfolioControlKind,
    pub alias: String,
    pub group: Option<String>,
    pub count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionReason {
    SignalQueued,
    SignalReplacedPendingOrder,
    NoSignal,
    ConflictingSignals,
    CooldownActive,
    SameSidePosition,
    NoPosition,
    RoleInvalidated,
    AwaitingTrigger,
    AwaitingFill,
    PostOnlyWouldCross,
    TifExpired,
    InsufficientCollateral,
    MissingOrderField,
    VenueRuleRejected,
    ForcedMaxBarsExit,
    PortfolioMaxPositionsExceeded,
    PortfolioMaxLongPositionsExceeded,
    PortfolioMaxShortPositionsExceeded,
    PortfolioMaxGrossExposureExceeded,
    PortfolioMaxNetExposureExceeded,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignalDecisionTrace {
    pub name: String,
    pub role: Option<SignalRole>,
    pub reason: DecisionReason,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderDecisionTrace {
    pub order_id: Option<usize>,
    pub role: Option<SignalRole>,
    pub reason: DecisionReason,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PerBarDecisionTrace {
    pub execution_alias: String,
    pub bar_index: usize,
    pub time: f64,
    pub position_snapshot: Option<PositionSnapshot>,
    pub feature_snapshot: Option<FeatureSnapshot>,
    pub signal_decisions: Vec<SignalDecisionTrace>,
    pub order_decisions: Vec<OrderDecisionTrace>,
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
    pub execution_alias: String,
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
    pub execution_alias: String,
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
    pub execution_alias: String,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExitClassificationDiagnosticSummary {
    pub classification: TradeExitClassification,
    pub trade_count: usize,
    pub win_rate: f64,
    pub average_realized_pnl: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeekdayDiagnosticSummary {
    pub weekday_utc: u8,
    pub trade_count: usize,
    pub win_rate: f64,
    pub total_realized_pnl: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HourDiagnosticSummary {
    pub hour_utc: u8,
    pub trade_count: usize,
    pub win_rate: f64,
    pub total_realized_pnl: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldingTimeBucket {
    Bars0To1,
    Bars2To5,
    Bars6To15,
    Bars16To31,
    Bars32Plus,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoldingTimeBucketSummary {
    pub bucket: HoldingTimeBucket,
    pub trade_count: usize,
    pub win_rate: f64,
    pub average_realized_pnl: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoolExportActiveTradeSummary {
    pub name: String,
    pub active_trade_count: usize,
    pub inactive_trade_count: usize,
    pub active_win_rate: f64,
    pub inactive_win_rate: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CohortDiagnostics {
    pub by_side: Vec<SideDiagnosticSummary>,
    pub by_exit_classification: Vec<ExitClassificationDiagnosticSummary>,
    pub by_weekday_utc: Vec<WeekdayDiagnosticSummary>,
    pub by_hour_utc: Vec<HourDiagnosticSummary>,
    pub by_holding_time: Vec<HoldingTimeBucketSummary>,
    pub by_active_export: Vec<BoolExportActiveTradeSummary>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DrawdownDiagnostics {
    pub longest_drawdown_bars: usize,
    pub current_drawdown_bars: usize,
    pub longest_stagnation_bars: usize,
    pub average_recovery_bars: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImprovementHintKind {
    TooFewTrades,
    HoldoutCollapse,
    EdgeConcentrated,
    ShortSideUnderperforms,
    CooldownBlocksSignals,
    HighDrawdownDuration,
    SignalQualityWeak,
    PortfolioCapsTooTight,
    ExposureCapBlocksMajorityOfEntries,
    PositionCountCapBlocksMajorityOfEntries,
    LongSideCapacitySaturated,
    ShortSideCapacitySaturated,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImprovementHint {
    pub kind: ImprovementHintKind,
    pub metric: Option<String>,
    pub value: Option<f64>,
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
    #[serde(default)]
    pub per_bar_trace: Vec<PerBarDecisionTrace>,
    #[serde(default)]
    pub cohorts: CohortDiagnostics,
    #[serde(default)]
    pub drawdown: DrawdownDiagnostics,
    #[serde(default)]
    pub source_alignment: crate::runtime::SourceAlignmentDiagnostics,
    #[serde(default)]
    pub hints: Vec<ImprovementHint>,
    #[serde(default)]
    pub portfolio_mode: bool,
    #[serde(default)]
    pub blocked_portfolio_entries: Vec<PortfolioControlBlockSummary>,
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
    #[serde(default)]
    pub open_positions: Vec<PositionSnapshot>,
    pub perp: Option<PerpBacktestMetadata>,
}

pub use optimize::{
    run_optimize_with_source, run_optimize_with_source_resume, HoldoutCandidateEvaluation,
    HoldoutDriftSummary, OptimizationRobustnessSummary, OptimizeCandidateSummary, OptimizeConfig,
    OptimizeError, OptimizeEvaluationSummary, OptimizeHoldoutConfig, OptimizeHoldoutResult,
    OptimizeObjective, OptimizeParamSpace, OptimizePreset, OptimizeProgressEvent,
    OptimizeProgressListener, OptimizeProgressState, OptimizeResult, OptimizeResumeState,
    OptimizeRunner, OptimizeScheduledBatch, OptimizeScheduledTrial, ParameterRobustnessSummary,
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
    #[error("backtest requires at least one declared `execution` target")]
    MissingExecutionDeclarations,
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
    #[error("portfolio mode requires one or more execution sources")]
    MissingPortfolioExecutionSources,
    #[error("portfolio mode execution source `{alias}` is duplicated")]
    DuplicatePortfolioExecutionSource { alias: String },
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
    if !config.portfolio_execution_aliases.is_empty() {
        return run_portfolio_backtest_with_sources(compiled, runtime, vm_limits, config);
    }
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

fn run_portfolio_backtest_with_sources(
    compiled: &CompiledProgram,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    mut config: BacktestConfig,
) -> Result<BacktestResult, BacktestError> {
    let aliases = resolved_execution_aliases(&config)?;
    let executions = bridge::resolve_execution_sources(compiled, &aliases)?;
    let mut execution_defs = Vec::with_capacity(executions.len());
    for (alias, execution) in aliases.iter().zip(executions.iter()) {
        match execution.template {
            crate::interval::SourceTemplate::BinanceSpot
            | crate::interval::SourceTemplate::BybitSpot
            | crate::interval::SourceTemplate::GateSpot => {}
            crate::interval::SourceTemplate::BinanceUsdm
            | crate::interval::SourceTemplate::BybitUsdtPerps
            | crate::interval::SourceTemplate::GateUsdtPerps => {
                if config.perp.is_none() {
                    config.perp = Some(PerpBacktestConfig {
                        leverage: 1.0,
                        margin_mode: PerpMarginMode::Isolated,
                    });
                }
            }
        }
        execution_defs.push((alias.clone(), execution.template));
    }
    let prepared = bridge::prepare_backtest_for_aliases(compiled, &execution_defs)?;
    let stepper_runtime = runtime.clone();
    let runtime_steppers = aliases
        .iter()
        .map(|_| RuntimeStepper::try_new(compiled, stepper_runtime.clone(), vm_limits))
        .collect::<Result<Vec<_>, _>>()?;
    let execution_bars = aliases
        .iter()
        .zip(executions.iter())
        .map(|(alias, execution)| {
            execution_bars(&runtime, execution.source_id, alias)
                .map(|bars| (alias.clone(), execution.source_id, execution.template, bars))
        })
        .collect::<Result<Vec<_>, _>>()?;
    engine::simulate_portfolio_backtest(runtime_steppers, execution_bars, &config, prepared)
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
    if !config.portfolio_execution_aliases.is_empty() {
        let mut seen = std::collections::BTreeSet::new();
        for alias in &config.portfolio_execution_aliases {
            if !seen.insert(alias.clone()) {
                return Err(BacktestError::DuplicatePortfolioExecutionSource {
                    alias: alias.clone(),
                });
            }
        }
    }
    Ok(())
}

fn resolved_execution_aliases(config: &BacktestConfig) -> Result<Vec<String>, BacktestError> {
    if !config.portfolio_execution_aliases.is_empty() {
        return Ok(config.portfolio_execution_aliases.clone());
    }
    if config.execution_source_alias.is_empty() {
        return Err(BacktestError::MissingPortfolioExecutionSources);
    }
    Ok(vec![config.execution_source_alias.clone()])
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
