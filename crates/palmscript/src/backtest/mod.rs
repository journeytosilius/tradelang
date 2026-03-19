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
mod overfitting;
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
use crate::interval::SourceTemplate;
use crate::order::{OrderKind, SizeMode, TimeInForce, TriggerReference};
use crate::output::{OutputValue, Outputs};
use crate::position::PositionSide;
use crate::runtime::{slice_runtime_window, Bar, RuntimeStepper, SourceRuntimeConfig, VmLimits};

const BPS_SCALE: f64 = 10_000.0;
const EPSILON: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BacktestConfig {
    pub execution_source_alias: String,
    #[serde(default)]
    pub portfolio_execution_aliases: Vec<String>,
    #[serde(default)]
    pub spot_virtual_rebalance: bool,
    #[serde(default)]
    pub activation_time_ms: Option<i64>,
    pub initial_capital: f64,
    pub maker_fee_bps: f64,
    pub taker_fee_bps: f64,
    #[serde(default)]
    pub execution_fee_schedules: BTreeMap<String, FeeSchedule>,
    pub slippage_bps: f64,
    #[serde(default)]
    pub max_volume_fill_pct: Option<f64>,
    #[serde(default)]
    pub diagnostics_detail: DiagnosticsDetailMode,
    pub perp: Option<PerpBacktestConfig>,
    pub perp_context: Option<PerpBacktestContext>,
    #[serde(default)]
    pub portfolio_perp_contexts: BTreeMap<String, PerpBacktestContext>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FeeSchedule {
    pub maker_bps: f64,
    pub taker_bps: f64,
}

impl BacktestConfig {
    pub fn fee_schedule_for_alias(&self, alias: &str) -> FeeSchedule {
        self.execution_fee_schedules
            .get(alias)
            .copied()
            .unwrap_or(FeeSchedule {
                maker_bps: self.maker_fee_bps,
                taker_bps: self.taker_fee_bps,
            })
    }
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
    #[serde(default)]
    pub entry_module: Option<String>,
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
    pub sharpe_ratio: Option<f64>,
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
    VolumeParticipationExceeded,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpotQuoteTransfer {
    pub from_alias: String,
    pub to_alias: String,
    pub bar_index: usize,
    pub time: f64,
    pub amount: f64,
    #[serde(default)]
    pub fee: f64,
    #[serde(default)]
    pub delay_bars: usize,
    #[serde(default)]
    pub completed_bar_index: Option<usize>,
    #[serde(default)]
    pub completed_time: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArbitrageBasketRecord {
    pub buy_alias: String,
    pub sell_alias: String,
    pub entry_bar_index: usize,
    pub entry_time: f64,
    pub quantity: f64,
    pub buy_entry_price: f64,
    pub sell_entry_price: f64,
    pub entry_spread_bps: f64,
    #[serde(default)]
    pub exit_bar_index: Option<usize>,
    #[serde(default)]
    pub exit_time: Option<f64>,
    #[serde(default)]
    pub buy_exit_price: Option<f64>,
    #[serde(default)]
    pub sell_exit_price: Option<f64>,
    #[serde(default)]
    pub exit_spread_bps: Option<f64>,
    #[serde(default)]
    pub realized_pnl: Option<f64>,
    #[serde(default)]
    pub holding_bars: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ArbitragePairDiagnosticSummary {
    pub buy_alias: String,
    pub sell_alias: String,
    pub basket_count: usize,
    pub completed_basket_count: usize,
    pub total_realized_pnl: f64,
    pub average_entry_spread_bps: f64,
    pub average_exit_spread_bps: f64,
    pub average_holding_bars: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ArbitrageDiagnosticsSummary {
    pub basket_count: usize,
    pub completed_basket_count: usize,
    pub open_basket_count: usize,
    pub total_realized_pnl: f64,
    pub average_entry_spread_bps: f64,
    pub average_exit_spread_bps: f64,
    pub average_holding_bars: f64,
    #[serde(default)]
    pub by_pair: Vec<ArbitragePairDiagnosticSummary>,
    #[serde(default)]
    pub baskets: Vec<ArbitrageBasketRecord>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TransferRouteDiagnosticSummary {
    pub from_alias: String,
    pub to_alias: String,
    pub transfer_count: usize,
    pub completed_transfer_count: usize,
    pub total_amount: f64,
    pub total_fee: f64,
    pub average_delay_bars: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TransferDiagnosticsSummary {
    pub quote_transfer_count: usize,
    pub completed_quote_transfer_count: usize,
    pub pending_quote_transfer_count: usize,
    pub total_quote_amount: f64,
    pub total_quote_fee: f64,
    pub average_delay_bars: f64,
    #[serde(default)]
    pub by_route: Vec<TransferRouteDiagnosticSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssetLedgerBalance {
    pub asset: String,
    pub amount: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExchangeLedgerSnapshot {
    pub execution_alias: String,
    pub template: SourceTemplate,
    pub symbol: String,
    pub balances: Vec<AssetLedgerBalance>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerEventKind {
    InitialDeposit,
    Transfer,
    Withdrawal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LedgerEvent {
    pub kind: LedgerEventKind,
    pub execution_alias: String,
    pub counterparty_alias: Option<String>,
    pub asset: String,
    pub amount: f64,
    pub bar_index: Option<usize>,
    pub time: Option<f64>,
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
    #[serde(default)]
    pub is_regime: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BaselineComparisonSummary {
    pub strategy_total_return: f64,
    pub flat_cash_return: f64,
    pub execution_asset_return: f64,
    pub opportunity_cost_return: f64,
    pub excess_return_vs_flat_cash: f64,
    pub excess_return_vs_execution_asset: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatePerturbationKind {
    LateStart,
    EarlyEnd,
    TrimmedBoth,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatePerturbationScenarioSummary {
    pub kind: DatePerturbationKind,
    pub from: i64,
    pub to: i64,
    pub total_return: f64,
    pub execution_asset_return: f64,
    pub excess_return_vs_execution_asset: f64,
    pub trade_count: usize,
    pub max_drawdown: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DatePerturbationDiagnostics {
    pub offset_bars: usize,
    #[serde(default)]
    pub scenarios: Vec<DatePerturbationScenarioSummary>,
    pub return_min: Option<f64>,
    pub return_max: Option<f64>,
    pub return_mean: Option<f64>,
    pub excess_return_vs_execution_asset_min: Option<f64>,
    pub excess_return_vs_execution_asset_max: Option<f64>,
    pub excess_return_vs_execution_asset_mean: Option<f64>,
    pub positive_scenario_count: usize,
    pub outperformed_execution_asset_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ValidationConstraintConfig {
    pub min_trade_count: Option<usize>,
    pub min_sharpe_ratio: Option<f64>,
    pub min_holdout_trade_count: Option<usize>,
    #[serde(default)]
    pub require_positive_holdout: bool,
    pub max_zero_trade_segments: Option<usize>,
    pub min_holdout_pass_rate: Option<f64>,
    pub min_date_perturbation_positive_ratio: Option<f64>,
    pub min_date_perturbation_outperform_ratio: Option<f64>,
    pub max_overfitting_risk: Option<OverfittingRiskLevel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationConstraintKind {
    MinTradeCount,
    MinSharpeRatio,
    MinHoldoutTradeCount,
    RequirePositiveHoldout,
    MaxZeroTradeSegments,
    MinHoldoutPassRate,
    MinDatePerturbationPositiveRatio,
    MinDatePerturbationOutperformRatio,
    MaxOverfittingRisk,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValidationConstraintViolation {
    pub kind: ValidationConstraintKind,
    pub actual: Option<f64>,
    pub required: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValidationConstraintSummary {
    pub passed: bool,
    #[serde(default)]
    pub violations: Vec<ValidationConstraintViolation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConstraintFailureBreakdown {
    pub kind: ValidationConstraintKind,
    pub count: usize,
}

impl Default for ValidationConstraintSummary {
    fn default() -> Self {
        Self {
            passed: true,
            violations: Vec::new(),
        }
    }
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
    #[serde(default)]
    pub entry_module: Option<String>,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimeBucketUtcDiagnosticSummary {
    pub start_hour_utc: u8,
    pub end_hour_utc: u8,
    pub trade_count: usize,
    pub winning_trade_count: usize,
    pub win_rate: f64,
    pub total_realized_pnl: f64,
    pub average_realized_pnl: f64,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntryModuleDiagnosticSummary {
    pub name: String,
    pub trade_count: usize,
    pub long_trade_count: usize,
    pub short_trade_count: usize,
    pub win_rate: f64,
    pub total_realized_pnl: f64,
    pub average_realized_pnl: f64,
    pub average_bars_held: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CohortDiagnostics {
    pub by_side: Vec<SideDiagnosticSummary>,
    pub by_exit_classification: Vec<ExitClassificationDiagnosticSummary>,
    pub by_weekday_utc: Vec<WeekdayDiagnosticSummary>,
    pub by_hour_utc: Vec<HourDiagnosticSummary>,
    #[serde(default)]
    pub by_time_bucket_utc: Vec<TimeBucketUtcDiagnosticSummary>,
    pub by_holding_time: Vec<HoldingTimeBucketSummary>,
    #[serde(default)]
    pub by_active_regime: Vec<BoolExportActiveTradeSummary>,
    #[serde(default)]
    pub by_active_export: Vec<BoolExportActiveTradeSummary>,
    #[serde(default)]
    pub by_entry_module: Vec<EntryModuleDiagnosticSummary>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverfittingRiskLevel {
    #[default]
    Unknown,
    Low,
    Moderate,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverfittingRiskReasonKind {
    NoOutOfSampleValidation,
    TooFewTrades,
    ZeroTradeSegments,
    NegativeOutOfSampleSegments,
    SegmentReturnInstability,
    HoldoutReturnCollapse,
    LargeHoldoutReturnDrop,
    WeakHoldoutPassRate,
    BestCandidateNotRobust,
    NarrowParameterStability,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OverfittingRiskReason {
    pub kind: OverfittingRiskReasonKind,
    pub metric: Option<String>,
    pub value: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OverfittingRiskSummary {
    pub level: OverfittingRiskLevel,
    pub score: f64,
    #[serde(default)]
    pub reasons: Vec<OverfittingRiskReason>,
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
    #[serde(default)]
    pub baseline_comparison: BaselineComparisonSummary,
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
    pub overfitting_risk: OverfittingRiskSummary,
    #[serde(default)]
    pub portfolio_mode: bool,
    #[serde(default)]
    pub spot_virtual_portfolio: bool,
    #[serde(default)]
    pub blocked_portfolio_entries: Vec<PortfolioControlBlockSummary>,
    #[serde(default)]
    pub spot_quote_transfers: Vec<SpotQuoteTransfer>,
    #[serde(default)]
    pub transfer_summary: TransferDiagnosticsSummary,
    #[serde(default)]
    pub arbitrage: ArbitrageDiagnosticsSummary,
    #[serde(default)]
    pub starting_ledgers: Vec<ExchangeLedgerSnapshot>,
    #[serde(default)]
    pub ending_ledgers: Vec<ExchangeLedgerSnapshot>,
    #[serde(default)]
    pub ledger_events: Vec<LedgerEvent>,
    #[serde(default)]
    pub date_perturbation: DatePerturbationDiagnostics,
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
    run_optimize_with_source, run_optimize_with_source_resume, DirectValidationDriftSummary,
    HoldoutCandidateEvaluation, HoldoutDriftSummary, OptimizationRobustnessSummary,
    OptimizeCandidateSummary, OptimizeConfig, OptimizeDirectValidationResult, OptimizeError,
    OptimizeEvaluationSummary, OptimizeHoldoutConfig, OptimizeHoldoutResult, OptimizeObjective,
    OptimizeParamSpace, OptimizePreset, OptimizeProgressEvent, OptimizeProgressListener,
    OptimizeProgressState, OptimizeResult, OptimizeResumeState, OptimizeRunner,
    OptimizeScheduledBatch, OptimizeScheduledTrial, ParameterRobustnessSummary,
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
    #[error("backtest maker_fee_bps must be finite and >= 0, found {value}")]
    InvalidMakerFeeBps { value: f64 },
    #[error("backtest taker_fee_bps must be finite and >= 0, found {value}")]
    InvalidTakerFeeBps { value: f64 },
    #[error(
        "backtest fee schedule for execution `{alias}` must have finite maker/taker rates >= 0, found maker={maker_bps}, taker={taker_bps}"
    )]
    InvalidExecutionFeeSchedule {
        alias: String,
        maker_bps: f64,
        taker_bps: f64,
    },
    #[error("backtest slippage_bps must be finite and >= 0, found {value}")]
    InvalidSlippageBps { value: f64 },
    #[error("backtest max_volume_fill_pct must be finite and > 0 and <= 1, found {value}")]
    InvalidMaxVolumeFillPct { value: f64 },
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
    #[error("spot virtual rebalance requires at least two selected portfolio execution sources")]
    SpotVirtualRebalanceRequiresPortfolioMode,
    #[error("spot virtual rebalance only supports spot execution aliases, but `{alias}` is `{template:?}`")]
    SpotVirtualRebalanceRequiresSpotAliases {
        alias: String,
        template: crate::interval::SourceTemplate,
    },
    #[error(
        "spot virtual rebalance does not support short spot roles; alias=`{alias}` role={role:?}"
    )]
    SpotVirtualRebalanceShortRoleUnsupported { alias: String, role: SignalRole },
    #[error(
        "arbitrage declarations require portfolio mode with at least two selected execution aliases"
    )]
    ArbitrageRequiresPortfolioMode,
    #[error(
        "arbitrage scripts may not mix `arb_*` declarations with standard `entry`/`exit` orders in the same strategy"
    )]
    ArbitrageStandardSurfaceMixUnsupported,
    #[error(
        "arbitrage scripts require `arb_entry`, `arb_exit`, `arb_order entry`, and `arb_order exit`"
    )]
    IncompleteArbitrageSurface,
    #[error("arbitrage basket execution currently supports spot execution aliases only, but `{alias}` is `{template:?}`")]
    ArbitrageRequiresSpotAliases {
        alias: String,
        template: SourceTemplate,
    },
    #[error("arbitrage basket execution currently supports `market_pair(...)` only")]
    UnsupportedArbitragePairConstructor,
    #[error("arbitrage basket size must be finite and > 0, found {value}")]
    InvalidArbitrageSize { value: f64 },
    #[error("arbitrage basket buy and sell venues must differ")]
    ArbitrageSameVenue,
    #[error("arbitrage basket exit venues must close the active basket on the opposite legs")]
    ArbitrageExitVenueMismatch,
    #[error("arbitrage basket exit size must match the active basket size; expected {expected}, found {actual}")]
    ArbitrageExitSizeMismatch { expected: f64, actual: f64 },
    #[error("arbitrage basket references undeclared execution source `{alias}`")]
    ArbitrageUnknownExecutionSource { alias: String },
    #[error("spot virtual rebalance cannot be combined with `arb_*` basket execution")]
    ArbitrageSpotVirtualRebalanceUnsupported,
    #[error(
        "transfer declarations require portfolio mode with at least two selected execution aliases"
    )]
    TransferRequiresPortfolioMode,
    #[error("transfer runtime currently supports quote transfers only")]
    UnsupportedTransferAsset,
    #[error("quote transfer runtime currently supports spot execution aliases only, but `{alias}` is `{template:?}`")]
    TransferRequiresSpotAliases {
        alias: String,
        template: SourceTemplate,
    },
    #[error("transfer amount must be finite and >= 0, found {value}")]
    InvalidTransferAmount { value: f64 },
    #[error("transfer fee must be finite and >= 0, found {value}")]
    InvalidTransferFee { value: f64 },
    #[error("transfer delay_bars must be finite and >= 0, found {value}")]
    InvalidTransferDelayBars { value: f64 },
    #[error("walk-forward train_bars must be > 0, found {value}")]
    InvalidWalkForwardTrainBars { value: usize },
    #[error("walk-forward test_bars must be > 0, found {value}")]
    InvalidWalkForwardTestBars { value: usize },
    #[error("walk-forward step_bars must be > 0, found {value}")]
    InvalidWalkForwardStepBars { value: usize },
    #[error("walk-forward min_trade_count must be > 0 when set, found {value}")]
    InvalidWalkForwardMinTradeCount { value: usize },
    #[error("walk-forward requires at least {required} execution bars, but only {available} were available")]
    InsufficientWalkForwardBars { available: usize, required: usize },
}

pub fn run_backtest_with_sources(
    compiled: &CompiledProgram,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: BacktestConfig,
) -> Result<BacktestResult, BacktestError> {
    run_backtest_with_sources_internal(compiled, runtime, vm_limits, config, true)
}

pub(crate) fn run_backtest_with_sources_internal(
    compiled: &CompiledProgram,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    mut config: BacktestConfig,
    include_date_perturbation: bool,
) -> Result<BacktestResult, BacktestError> {
    validate_config(&config)?;
    if !config.portfolio_execution_aliases.is_empty() {
        return run_portfolio_backtest_with_sources(compiled, runtime, vm_limits, config);
    }
    let perturbation_runtime = include_date_perturbation.then(|| runtime.clone());
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
    let mut result = engine::simulate_backtest(
        stepper,
        execution,
        execution_bars.clone(),
        &config,
        prepared,
    )?;
    if let Some(runtime) = perturbation_runtime {
        result.diagnostics.date_perturbation = build_date_perturbation_diagnostics(
            compiled,
            &runtime,
            vm_limits,
            &config,
            &execution_bars,
        )?;
    }
    Ok(result)
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
            execution_bars(&runtime, execution.source_id, alias).map(|bars| {
                (
                    alias.clone(),
                    execution.execution_id,
                    execution.source_id,
                    execution.template,
                    execution.symbol.clone(),
                    bars,
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    engine::simulate_portfolio_backtest(runtime_steppers, execution_bars, &config, prepared)
}

fn build_date_perturbation_diagnostics(
    compiled: &CompiledProgram,
    runtime: &SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: &BacktestConfig,
    execution_bars: &[Bar],
) -> Result<DatePerturbationDiagnostics, BacktestError> {
    if execution_bars.len() < 10 {
        return Ok(DatePerturbationDiagnostics::default());
    }
    let offset_bars = (execution_bars.len() / 10).max(1);
    if execution_bars.len() <= offset_bars * 2 {
        return Ok(DatePerturbationDiagnostics::default());
    }

    let scenarios = [
        (
            DatePerturbationKind::LateStart,
            offset_bars,
            execution_bars.len(),
        ),
        (
            DatePerturbationKind::EarlyEnd,
            0usize,
            execution_bars.len() - offset_bars,
        ),
        (
            DatePerturbationKind::TrimmedBoth,
            offset_bars,
            execution_bars.len() - offset_bars,
        ),
    ];

    let mut summaries = Vec::new();
    for (kind, start_index, end_index) in scenarios {
        if end_index <= start_index + 1 {
            continue;
        }
        let from = execution_bars[start_index].time as i64;
        let to = perturbation_end_time(execution_bars, end_index);
        let sliced_runtime = slice_runtime_window(runtime, from, to);
        let result = run_backtest_with_sources_internal(
            compiled,
            sliced_runtime,
            vm_limits,
            config.clone(),
            false,
        )?;
        let excess_return_vs_execution_asset =
            result.summary.total_return - result.diagnostics.capture_summary.execution_asset_return;
        summaries.push(DatePerturbationScenarioSummary {
            kind,
            from,
            to,
            total_return: result.summary.total_return,
            execution_asset_return: result.diagnostics.capture_summary.execution_asset_return,
            excess_return_vs_execution_asset,
            trade_count: result.summary.trade_count,
            max_drawdown: result.summary.max_drawdown,
        });
    }

    if summaries.is_empty() {
        return Ok(DatePerturbationDiagnostics::default());
    }

    Ok(DatePerturbationDiagnostics {
        offset_bars,
        return_min: summaries
            .iter()
            .map(|summary| summary.total_return)
            .reduce(f64::min),
        return_max: summaries
            .iter()
            .map(|summary| summary.total_return)
            .reduce(f64::max),
        return_mean: Some(average(
            summaries.iter().map(|summary| summary.total_return),
        )),
        excess_return_vs_execution_asset_min: summaries
            .iter()
            .map(|summary| summary.excess_return_vs_execution_asset)
            .reduce(f64::min),
        excess_return_vs_execution_asset_max: summaries
            .iter()
            .map(|summary| summary.excess_return_vs_execution_asset)
            .reduce(f64::max),
        excess_return_vs_execution_asset_mean: Some(average(
            summaries
                .iter()
                .map(|summary| summary.excess_return_vs_execution_asset),
        )),
        positive_scenario_count: summaries
            .iter()
            .filter(|summary| summary.total_return > EPSILON)
            .count(),
        outperformed_execution_asset_count: summaries
            .iter()
            .filter(|summary| summary.excess_return_vs_execution_asset > EPSILON)
            .count(),
        scenarios: summaries,
    })
}

fn perturbation_end_time(execution_bars: &[Bar], end_index: usize) -> i64 {
    execution_bars
        .get(end_index)
        .map(|bar| bar.time as i64)
        .or_else(|| execution_bars.last().map(|bar| bar.time as i64 + 1))
        .unwrap_or(i64::MIN)
}

fn validate_config(config: &BacktestConfig) -> Result<(), BacktestError> {
    if !config.initial_capital.is_finite() || config.initial_capital <= 0.0 {
        return Err(BacktestError::InvalidInitialCapital {
            value: config.initial_capital,
        });
    }
    if !config.maker_fee_bps.is_finite() || config.maker_fee_bps < 0.0 {
        return Err(BacktestError::InvalidMakerFeeBps {
            value: config.maker_fee_bps,
        });
    }
    if !config.taker_fee_bps.is_finite() || config.taker_fee_bps < 0.0 {
        return Err(BacktestError::InvalidTakerFeeBps {
            value: config.taker_fee_bps,
        });
    }
    for (alias, schedule) in &config.execution_fee_schedules {
        if !schedule.maker_bps.is_finite()
            || schedule.maker_bps < 0.0
            || !schedule.taker_bps.is_finite()
            || schedule.taker_bps < 0.0
        {
            return Err(BacktestError::InvalidExecutionFeeSchedule {
                alias: alias.clone(),
                maker_bps: schedule.maker_bps,
                taker_bps: schedule.taker_bps,
            });
        }
    }
    if !config.slippage_bps.is_finite() || config.slippage_bps < 0.0 {
        return Err(BacktestError::InvalidSlippageBps {
            value: config.slippage_bps,
        });
    }
    if let Some(value) = config.max_volume_fill_pct {
        if !value.is_finite() || value <= 0.0 || value > 1.0 {
            return Err(BacktestError::InvalidMaxVolumeFillPct { value });
        }
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
