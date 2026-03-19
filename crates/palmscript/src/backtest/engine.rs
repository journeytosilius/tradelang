use crate::backtest::bridge::{
    capture_request, PreparedArbSurface, PreparedBacktest, PreparedTransferSurface,
};
use crate::backtest::diagnostics::{
    build_arbitrage_diagnostics, build_backtest_hints, build_baseline_comparison,
    build_cohort_diagnostics, build_diagnostics_summary, build_drawdown_diagnostics,
    build_order_diagnostics, build_transfer_diagnostics, snapshot_from_step,
    DiagnosticsAccumulator, OrderDiagnosticContext,
};
use crate::backtest::orders::{
    add_to_position, adjusted_price, close_position, close_trade_slice, empty_request_slots,
    evaluate_active_order, fill_action_for_role, is_attached_exit_role, liquidation_trigger_price,
    missing_field_reason, open_position, open_position_with_quantity, position_side_for_entry,
    realize_perp_close, refresh_position_risk, request_applicable, resolve_entry_sizing,
    role_index, unrealized_pnl_for_position, update_open_trade_excursions, AccountingMode,
    ActiveOrder, CapturedOrderRequest, CloseExecution, EntryProgressState, EntrySizingSpec,
    FillExecutionContext, OpenTrade, PositionFillContext, PositionState, TradeEntryContext,
    WorkingState, ROLE_COUNT, ROLE_PRIORITY,
};
use crate::backtest::overfitting::{annualized_sharpe_ratio, build_backtest_overfitting_risk};
use crate::backtest::{
    ArbitrageBasketRecord, AssetLedgerBalance, BacktestCaptureSummary, BacktestConfig,
    BacktestDiagnostics, BacktestError, BacktestResult, BacktestSummary, DecisionReason,
    DiagnosticsDetailMode, EquityPoint, ExchangeLedgerSnapshot, FeatureSnapshot, Fill, FillAction,
    LedgerEvent, LedgerEventKind, OpportunityEventKind, OrderDecisionTrace, OrderEndReason,
    OrderRecord, OrderStatus, PerBarDecisionTrace, PerpBacktestMetadata,
    PortfolioControlBlockSummary, PortfolioControlKind as BacktestPortfolioControlKind,
    PositionSnapshot, SignalDecisionTrace, SpotQuoteTransfer, Trade, TradeDiagnostic,
    TradeExitClassification,
};
use crate::bytecode::{
    ArbOrderDecl, ArbPairConstructor, ExecutionPriceDecl, LastExitFieldDecl, LedgerFieldDecl,
    OrderDecl, PortfolioControlDecl, PortfolioControlKind as ProgramPortfolioControlKind,
    PositionEventFieldDecl, PositionFieldDecl, RiskControlDecl, RiskControlKind, SignalRole,
    TransferAssetKind, TransferDecl,
};
use crate::exchange::{RiskTier, VenueRiskSnapshot};
use crate::order::OrderKind;
use crate::output::{Outputs, StepOutput};
use crate::position::{
    ExitKind, LastExitField, LastExitScope, PositionEventField, PositionField, PositionSide,
};
use crate::runtime::{Bar, RuntimeStep, RuntimeStepper};
use crate::types::Value;
use std::collections::BTreeMap;

pub(crate) struct OrderRecordUpdate {
    pub trigger_time: Option<f64>,
    pub fill_bar_index: Option<usize>,
    pub fill_time: Option<f64>,
    pub raw_price: Option<f64>,
    pub fill_price: Option<f64>,
    pub effective_risk_per_unit: Option<f64>,
    pub capital_limited: Option<bool>,
    pub status: OrderStatus,
    pub end_reason: Option<OrderEndReason>,
}

struct CloseOutcome {
    snapshot: Option<LastExitSnapshot>,
    fully_closed_side: Option<PositionSide>,
    consumed_target_side: Option<PositionSide>,
}

#[derive(Clone, Copy, Debug, Default)]
struct PositionEventStep {
    long_entry_fill: bool,
    long_entry1_fill: bool,
    long_entry2_fill: bool,
    long_entry3_fill: bool,
    short_entry_fill: bool,
    short_entry1_fill: bool,
    short_entry2_fill: bool,
    short_entry3_fill: bool,
    long_exit_fill: bool,
    short_exit_fill: bool,
    long_protect_fill: bool,
    short_protect_fill: bool,
    long_target_fill: bool,
    long_target1_fill: bool,
    long_target2_fill: bool,
    long_target3_fill: bool,
    short_target_fill: bool,
    short_target1_fill: bool,
    short_target2_fill: bool,
    short_target3_fill: bool,
    long_signal_exit_fill: bool,
    short_signal_exit_fill: bool,
    long_reversal_exit_fill: bool,
    short_reversal_exit_fill: bool,
    long_liquidation_fill: bool,
    short_liquidation_fill: bool,
}

#[derive(Clone, Copy, Debug)]
struct LastExitSnapshot {
    kind: ExitKind,
    stage: Option<u8>,
    side: PositionSide,
    price: f64,
    time: f64,
    bar_index: usize,
    realized_pnl: f64,
    realized_return: f64,
    bars_held: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct TargetConsumptionState {
    long_stage: u8,
    short_stage: u8,
}

#[derive(Default)]
struct StepDecisionTrace {
    signal_decisions: Vec<SignalDecisionTrace>,
    order_decisions: Vec<OrderDecisionTrace>,
}

#[derive(Clone, Copy)]
struct FeeRates {
    maker: f64,
    taker: f64,
}

#[derive(Clone, Copy)]
enum FillLiquidity {
    Maker,
    Taker,
}

struct PortfolioAliasState {
    execution_id: u16,
    alias: String,
    template: crate::interval::SourceTemplate,
    symbol: String,
    quote_asset: String,
    base_asset: Option<String>,
    stepper: RuntimeStepper,
    execution_bars: Vec<Bar>,
    aligned_mark_bars: Vec<Bar>,
    execution_cursor: usize,
    accounting: AccountingMode,
    fee_rates: FeeRates,
    position: Option<PositionState>,
    open_trade: Option<OpenTrade>,
    active_orders: [Option<ActiveOrder>; ROLE_COUNT],
    pending_requests: [Option<CapturedOrderRequest>; ROLE_COUNT],
    pending_snapshots: [Option<FeatureSnapshot>; ROLE_COUNT],
    pending_signal_names: [Option<String>; ROLE_COUNT],
    pending_conflict_time: Option<f64>,
    last_mark_price: Option<f64>,
    last_snapshot: Option<FeatureSnapshot>,
    last_exit: Option<LastExitSnapshot>,
    last_long_exit: Option<LastExitSnapshot>,
    last_short_exit: Option<LastExitSnapshot>,
    target_consumption: TargetConsumptionState,
    entry_progress: EntryProgressState,
    diagnostics: DiagnosticsAccumulator,
    cash_balance: f64,
    base_balance: f64,
}

#[derive(Clone)]
struct CapturedArbRequest {
    buy_execution_id: u16,
    sell_execution_id: u16,
    quantity: f64,
    signal_time: f64,
    snapshot: Option<FeatureSnapshot>,
}

struct ActiveArbBasket {
    buy_execution_id: u16,
    sell_execution_id: u16,
    quantity: f64,
    entry_bar_index: usize,
    entry_time: f64,
    buy_alias: String,
    sell_alias: String,
    buy_entry_price: f64,
    sell_entry_price: f64,
    entry_spread_bps: f64,
}

#[derive(Clone)]
struct CapturedQuoteTransferRequest {
    from_execution_id: u16,
    to_execution_id: u16,
    amount: f64,
    fee: f64,
    delay_bars: usize,
}

struct PendingQuoteTransfer {
    from_execution_id: u16,
    to_execution_id: u16,
    amount: f64,
    fee: f64,
    requested_bar_index: usize,
    requested_time: f64,
    complete_bar_index: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct LedgerRuntimeSnapshot {
    base_free: Option<f64>,
    quote_free: Option<f64>,
    base_total: Option<f64>,
    quote_total: Option<f64>,
    mark_value_quote: Option<f64>,
}

fn portfolio_cash_total(alias_states: &[PortfolioAliasState]) -> f64 {
    alias_states.iter().map(|state| state.cash_balance).sum()
}

fn portfolio_cash_total_window(
    before_current: &[PortfolioAliasState],
    current_state: &PortfolioAliasState,
    after_current: &[PortfolioAliasState],
) -> f64 {
    before_current
        .iter()
        .chain(std::iter::once(current_state))
        .chain(after_current.iter())
        .map(|state| state.cash_balance)
        .sum()
}

fn spread_bps(sell_price: f64, buy_price: f64) -> f64 {
    if !sell_price.is_finite()
        || !buy_price.is_finite()
        || buy_price.abs() <= crate::backtest::EPSILON
    {
        0.0
    } else {
        (sell_price - buy_price) / buy_price * crate::backtest::BPS_SCALE
    }
}

fn local_entry_cash(state: &PortfolioAliasState) -> f64 {
    state.cash_balance
}

#[allow(clippy::too_many_arguments)]
fn rebalance_spot_quote_for_entry(
    before_current: &mut [PortfolioAliasState],
    current_state: &mut PortfolioAliasState,
    after_current: &mut [PortfolioAliasState],
    required_quote: f64,
    bar_index: usize,
    time: f64,
    transfers: &mut Vec<SpotQuoteTransfer>,
    ledger_events: &mut Vec<LedgerEvent>,
) {
    if current_state.base_asset.is_none() {
        return;
    }
    let target_balance = current_state.cash_balance;
    if target_balance + crate::backtest::EPSILON >= required_quote {
        return;
    }
    let mut remaining = required_quote - target_balance;
    for donor in before_current.iter_mut().chain(after_current.iter_mut()) {
        let donor_balance = donor.cash_balance;
        if donor_balance <= crate::backtest::EPSILON || remaining <= crate::backtest::EPSILON {
            continue;
        }
        let transfer_amount = donor_balance.min(remaining);
        donor.cash_balance -= transfer_amount;
        current_state.cash_balance += transfer_amount;
        transfers.push(SpotQuoteTransfer {
            from_alias: donor.alias.clone(),
            to_alias: current_state.alias.clone(),
            bar_index,
            time,
            amount: transfer_amount,
            fee: 0.0,
            delay_bars: 0,
            completed_bar_index: Some(bar_index),
            completed_time: Some(time),
        });
        ledger_events.push(LedgerEvent {
            kind: LedgerEventKind::Transfer,
            execution_alias: donor.alias.clone(),
            counterparty_alias: Some(current_state.alias.clone()),
            asset: donor.quote_asset.clone(),
            amount: transfer_amount,
            bar_index: Some(bar_index),
            time: Some(time),
        });
        remaining -= transfer_amount;
    }
}

fn step_is_active(open_time_ms: i64, activation_time_ms: Option<i64>) -> bool {
    match activation_time_ms {
        Some(activation_time_ms) => open_time_ms >= activation_time_ms,
        None => true,
    }
}

fn ledger_assets_for_symbol(
    template: crate::interval::SourceTemplate,
    symbol: &str,
) -> (Option<String>, String) {
    let include_base = matches!(
        template,
        crate::interval::SourceTemplate::BinanceSpot
            | crate::interval::SourceTemplate::BybitSpot
            | crate::interval::SourceTemplate::GateSpot
    );
    if matches!(
        template,
        crate::interval::SourceTemplate::GateSpot | crate::interval::SourceTemplate::GateUsdtPerps
    ) {
        if let Some((base, quote)) = symbol.split_once('_') {
            return (include_base.then(|| base.to_string()), quote.to_string());
        }
    }

    const KNOWN_QUOTES: [&str; 8] = ["USDT", "USDC", "FDUSD", "BUSD", "BTC", "ETH", "EUR", "USD"];
    for quote in KNOWN_QUOTES {
        if let Some(base) = symbol.strip_suffix(quote) {
            if !base.is_empty() {
                return (include_base.then(|| base.to_string()), quote.to_string());
            }
        }
    }

    (
        include_base.then(|| symbol.to_string()),
        "QUOTE".to_string(),
    )
}

fn sync_spot_base_balance(state: &mut PortfolioAliasState) {
    if state.base_asset.is_some() {
        state.base_balance = state
            .position
            .as_ref()
            .map_or(0.0, |position| position.quantity);
    }
}

fn ledger_snapshot(state: &PortfolioAliasState) -> ExchangeLedgerSnapshot {
    let mut balances = Vec::with_capacity(2);
    balances.push(AssetLedgerBalance {
        asset: state.quote_asset.clone(),
        amount: state.cash_balance,
    });
    if let Some(base_asset) = &state.base_asset {
        balances.push(AssetLedgerBalance {
            asset: base_asset.clone(),
            amount: state.base_balance,
        });
    }
    ExchangeLedgerSnapshot {
        execution_alias: state.alias.clone(),
        template: state.template,
        symbol: state.symbol.clone(),
        balances,
    }
}

fn alias_mark_price(state: &PortfolioAliasState) -> f64 {
    state
        .last_mark_price
        .or_else(|| state.position.as_ref().map(|position| position.entry_price))
        .unwrap_or(0.0)
}

fn ledger_runtime_snapshot(state: &PortfolioAliasState) -> LedgerRuntimeSnapshot {
    let mark_price = alias_mark_price(state);
    if state.base_asset.is_some() {
        LedgerRuntimeSnapshot {
            base_free: Some(state.base_balance),
            quote_free: Some(state.cash_balance),
            base_total: Some(state.base_balance),
            quote_total: Some(state.cash_balance),
            mark_value_quote: Some(state.cash_balance + state.base_balance * mark_price),
        }
    } else {
        let isolated_margin = state
            .position
            .as_ref()
            .map_or(0.0, |position| position.isolated_margin);
        let unrealized = state.position.as_ref().map_or(0.0, |position| {
            unrealized_pnl_for_position(position, alias_mark_price(state))
        });
        let total_quote = state.cash_balance + isolated_margin + unrealized;
        LedgerRuntimeSnapshot {
            base_free: None,
            quote_free: Some(state.cash_balance),
            base_total: None,
            quote_total: Some(total_quote),
            mark_value_quote: Some(total_quote),
        }
    }
}

fn single_ledger_runtime_snapshot(
    has_base_asset: bool,
    cash_balance: f64,
    base_balance: f64,
    position: Option<&PositionState>,
    mark_price: f64,
) -> LedgerRuntimeSnapshot {
    if has_base_asset {
        LedgerRuntimeSnapshot {
            base_free: Some(base_balance),
            quote_free: Some(cash_balance),
            base_total: Some(base_balance),
            quote_total: Some(cash_balance),
            mark_value_quote: Some(cash_balance + base_balance * mark_price),
        }
    } else {
        let isolated_margin = position.map_or(0.0, |position| position.isolated_margin);
        let unrealized = position.map_or(0.0, |position| {
            unrealized_pnl_for_position(position, mark_price)
        });
        let total_quote = cash_balance + isolated_margin + unrealized;
        LedgerRuntimeSnapshot {
            base_free: None,
            quote_free: Some(cash_balance),
            base_total: None,
            quote_total: Some(total_quote),
            mark_value_quote: Some(total_quote),
        }
    }
}

fn ledger_field_value(
    snapshot: Option<&LedgerRuntimeSnapshot>,
    field: crate::LedgerField,
) -> Value {
    let Some(snapshot) = snapshot else {
        return Value::NA;
    };
    match field {
        crate::LedgerField::BaseFree => snapshot.base_free.map(Value::F64).unwrap_or(Value::NA),
        crate::LedgerField::QuoteFree => snapshot.quote_free.map(Value::F64).unwrap_or(Value::NA),
        crate::LedgerField::BaseTotal => snapshot.base_total.map(Value::F64).unwrap_or(Value::NA),
        crate::LedgerField::QuoteTotal => snapshot.quote_total.map(Value::F64).unwrap_or(Value::NA),
        crate::LedgerField::MarkValueQuote => snapshot
            .mark_value_quote
            .map(Value::F64)
            .unwrap_or(Value::NA),
    }
}

fn fee_rates_for_alias(config: &BacktestConfig, alias: &str) -> FeeRates {
    let schedule = config.fee_schedule_for_alias(alias);
    FeeRates {
        maker: schedule.maker_bps / crate::backtest::BPS_SCALE,
        taker: schedule.taker_bps / crate::backtest::BPS_SCALE,
    }
}

fn fee_rate_for_liquidity(rates: FeeRates, liquidity: FillLiquidity) -> f64 {
    match liquidity {
        FillLiquidity::Maker => rates.maker,
        FillLiquidity::Taker => rates.taker,
    }
}

fn fill_liquidity_for_order(order: &ActiveOrder, first_eval: bool, bar_open: f64) -> FillLiquidity {
    let action = fill_action_for_role(order.request.role);
    match order.request.kind {
        OrderKind::Market | OrderKind::StopMarket | OrderKind::TakeProfitMarket => {
            FillLiquidity::Taker
        }
        OrderKind::Limit | OrderKind::StopLimit | OrderKind::TakeProfitLimit => {
            if matches!(order.state, WorkingState::RestingLimit { .. }) {
                return FillLiquidity::Maker;
            }
            if first_eval
                && order.request.price.is_some_and(|limit_price| {
                    crate::backtest::orders::would_cross_on_open(action, bar_open, limit_price)
                })
            {
                FillLiquidity::Taker
            } else {
                FillLiquidity::Maker
            }
        }
    }
}

fn current_bool_local(stepper: &RuntimeStepper, slot: u16) -> Option<bool> {
    match stepper.local_value(slot) {
        Some(Value::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn current_execution_alias_local(stepper: &RuntimeStepper, slot: u16) -> Option<u16> {
    match stepper.local_value(slot) {
        Some(Value::ExecutionAlias(value)) => Some(*value),
        _ => None,
    }
}

fn current_numeric_local(stepper: &RuntimeStepper, slot: u16) -> Option<f64> {
    match stepper.local_value(slot) {
        Some(Value::F64(value)) => Some(*value),
        _ => None,
    }
}

fn current_non_negative_delay_bars_local(
    stepper: &RuntimeStepper,
    slot: Option<u16>,
) -> Result<usize, BacktestError> {
    let Some(slot) = slot else {
        return Ok(0);
    };
    let value = current_numeric_local(stepper, slot).unwrap_or(0.0);
    if !value.is_finite() || value < 0.0 {
        return Err(BacktestError::InvalidTransferDelayBars { value });
    }
    Ok(value.round() as usize)
}

fn current_non_negative_numeric_local(
    stepper: &RuntimeStepper,
    slot: Option<u16>,
) -> Result<f64, BacktestError> {
    let Some(slot) = slot else {
        return Ok(0.0);
    };
    let value = current_numeric_local(stepper, slot).unwrap_or(0.0);
    if !value.is_finite() || value < 0.0 {
        return Err(BacktestError::InvalidTransferFee { value });
    }
    Ok(value)
}

fn validate_transfer_surface(
    transfer_surface: &PreparedTransferSurface,
    alias_states: &[PortfolioAliasState],
) -> Result<(), BacktestError> {
    if let Some(transfer) = transfer_surface.quote_transfer.as_ref() {
        if transfer.asset_kind != TransferAssetKind::Quote {
            return Err(BacktestError::UnsupportedTransferAsset);
        }
        for state in alias_states {
            if state.base_asset.is_none() {
                return Err(BacktestError::TransferRequiresSpotAliases {
                    alias: state.alias.clone(),
                    template: state.template,
                });
            }
        }
    }
    Ok(())
}

fn capture_quote_transfer_request(
    controller_state: &PortfolioAliasState,
    transfer: &TransferDecl,
) -> Result<Option<CapturedQuoteTransferRequest>, BacktestError> {
    if transfer.asset_kind != TransferAssetKind::Quote {
        return Err(BacktestError::UnsupportedTransferAsset);
    }
    let Some(from_execution_id) =
        current_execution_alias_local(&controller_state.stepper, transfer.from_slot)
    else {
        return Ok(None);
    };
    let Some(to_execution_id) =
        current_execution_alias_local(&controller_state.stepper, transfer.to_slot)
    else {
        return Ok(None);
    };
    let amount =
        current_numeric_local(&controller_state.stepper, transfer.amount_slot).unwrap_or(0.0);
    if !amount.is_finite() || amount < 0.0 {
        return Err(BacktestError::InvalidTransferAmount { value: amount });
    }
    if amount <= crate::backtest::EPSILON || from_execution_id == to_execution_id {
        return Ok(None);
    }
    let fee = current_non_negative_numeric_local(&controller_state.stepper, transfer.fee_slot)?;
    let delay_bars =
        current_non_negative_delay_bars_local(&controller_state.stepper, transfer.delay_bars_slot)?;
    Ok(Some(CapturedQuoteTransferRequest {
        from_execution_id,
        to_execution_id,
        amount,
        fee,
        delay_bars,
    }))
}

fn execute_quote_transfer_request(
    alias_states: &mut [PortfolioAliasState],
    request: CapturedQuoteTransferRequest,
    bar_index: usize,
    time: f64,
    spot_quote_transfers: &mut Vec<SpotQuoteTransfer>,
    ledger_events: &mut Vec<LedgerEvent>,
    pending_quote_transfers: &mut Vec<PendingQuoteTransfer>,
) -> Result<(), BacktestError> {
    let Some(from_index) = alias_index_for_execution_id(alias_states, request.from_execution_id)
    else {
        return Ok(());
    };
    let Some(to_index) = alias_index_for_execution_id(alias_states, request.to_execution_id) else {
        return Ok(());
    };
    if from_index == to_index {
        return Ok(());
    }
    let (from_state, to_state) = split_two_states_mut(alias_states, from_index, to_index);
    if from_state.base_asset.is_none() {
        return Err(BacktestError::TransferRequiresSpotAliases {
            alias: from_state.alias.clone(),
            template: from_state.template,
        });
    }
    if to_state.base_asset.is_none() {
        return Err(BacktestError::TransferRequiresSpotAliases {
            alias: to_state.alias.clone(),
            template: to_state.template,
        });
    }
    let required_quote = request.amount + request.fee;
    if from_state.cash_balance + crate::backtest::EPSILON < required_quote {
        return Ok(());
    }

    from_state.cash_balance -= required_quote;
    ledger_events.push(LedgerEvent {
        kind: LedgerEventKind::Transfer,
        execution_alias: from_state.alias.clone(),
        counterparty_alias: Some(to_state.alias.clone()),
        asset: from_state.quote_asset.clone(),
        amount: request.amount,
        bar_index: Some(bar_index),
        time: Some(time),
    });
    if request.fee > crate::backtest::EPSILON {
        ledger_events.push(LedgerEvent {
            kind: LedgerEventKind::Withdrawal,
            execution_alias: from_state.alias.clone(),
            counterparty_alias: None,
            asset: from_state.quote_asset.clone(),
            amount: request.fee,
            bar_index: Some(bar_index),
            time: Some(time),
        });
    }

    let complete_bar_index = bar_index + request.delay_bars;
    let completed_time = if request.delay_bars == 0 {
        Some(time)
    } else {
        None
    };
    let completed_bar_index_value = if request.delay_bars == 0 {
        Some(bar_index)
    } else {
        None
    };
    spot_quote_transfers.push(SpotQuoteTransfer {
        from_alias: from_state.alias.clone(),
        to_alias: to_state.alias.clone(),
        bar_index,
        time,
        amount: request.amount,
        fee: request.fee,
        delay_bars: request.delay_bars,
        completed_bar_index: completed_bar_index_value,
        completed_time,
    });

    if request.delay_bars == 0 {
        to_state.cash_balance += request.amount;
        ledger_events.push(LedgerEvent {
            kind: LedgerEventKind::Transfer,
            execution_alias: to_state.alias.clone(),
            counterparty_alias: Some(from_state.alias.clone()),
            asset: to_state.quote_asset.clone(),
            amount: request.amount,
            bar_index: Some(bar_index),
            time: Some(time),
        });
    } else {
        pending_quote_transfers.push(PendingQuoteTransfer {
            from_execution_id: request.from_execution_id,
            to_execution_id: request.to_execution_id,
            amount: request.amount,
            fee: request.fee,
            requested_bar_index: bar_index,
            requested_time: time,
            complete_bar_index,
        });
    }
    Ok(())
}

fn settle_pending_quote_transfers(
    alias_states: &mut [PortfolioAliasState],
    current_bar_index: usize,
    time: f64,
    spot_quote_transfers: &mut [SpotQuoteTransfer],
    ledger_events: &mut Vec<LedgerEvent>,
    pending_quote_transfers: &mut Vec<PendingQuoteTransfer>,
) {
    let mut settled = 0usize;
    let mut index = 0usize;
    while index < pending_quote_transfers.len() {
        if pending_quote_transfers[index].complete_bar_index > current_bar_index {
            index += 1;
            continue;
        }
        let transfer = pending_quote_transfers.remove(index);
        let Some(to_index) = alias_index_for_execution_id(alias_states, transfer.to_execution_id)
        else {
            continue;
        };
        let Some(from_index) =
            alias_index_for_execution_id(alias_states, transfer.from_execution_id)
        else {
            continue;
        };
        let (from_state, to_state) = split_two_states_mut(alias_states, from_index, to_index);
        to_state.cash_balance += transfer.amount;
        ledger_events.push(LedgerEvent {
            kind: LedgerEventKind::Transfer,
            execution_alias: to_state.alias.clone(),
            counterparty_alias: Some(from_state.alias.clone()),
            asset: to_state.quote_asset.clone(),
            amount: transfer.amount,
            bar_index: Some(current_bar_index),
            time: Some(time),
        });
        if let Some(record) = spot_quote_transfers.iter_mut().find(|record| {
            record.from_alias == from_state.alias
                && record.to_alias == to_state.alias
                && (record.amount - transfer.amount).abs() <= crate::backtest::EPSILON
                && (record.fee - transfer.fee).abs() <= crate::backtest::EPSILON
                && record.bar_index == transfer.requested_bar_index
                && (record.time - transfer.requested_time).abs() <= crate::backtest::EPSILON
                && record.completed_bar_index.is_none()
        }) {
            record.completed_bar_index = Some(current_bar_index);
            record.completed_time = Some(time);
        }
        settled += 1;
    }
    let _ = settled;
}

fn capture_arb_request(
    controller: &PortfolioAliasState,
    order: &ArbOrderDecl,
    signal_time: f64,
    snapshot: Option<FeatureSnapshot>,
) -> Result<CapturedArbRequest, BacktestError> {
    let buy_execution_id = current_execution_alias_local(&controller.stepper, order.buy_venue_slot)
        .ok_or_else(|| BacktestError::ArbitrageUnknownExecutionSource {
            alias: format!("execution#{}", order.buy_venue_slot),
        })?;
    let sell_execution_id =
        current_execution_alias_local(&controller.stepper, order.sell_venue_slot).ok_or_else(
            || BacktestError::ArbitrageUnknownExecutionSource {
                alias: format!("execution#{}", order.sell_venue_slot),
            },
        )?;
    if buy_execution_id == sell_execution_id {
        return Err(BacktestError::ArbitrageSameVenue);
    }
    let quantity = current_numeric_local(&controller.stepper, order.size_slot)
        .ok_or(BacktestError::InvalidArbitrageSize { value: f64::NAN })?;
    if !quantity.is_finite() || quantity <= crate::backtest::EPSILON {
        return Err(BacktestError::InvalidArbitrageSize { value: quantity });
    }
    Ok(CapturedArbRequest {
        buy_execution_id,
        sell_execution_id,
        quantity,
        signal_time,
        snapshot,
    })
}

fn alias_index_for_execution_id(
    alias_states: &[PortfolioAliasState],
    execution_id: u16,
) -> Option<usize> {
    alias_states
        .iter()
        .position(|state| state.execution_id == execution_id)
}

fn split_two_states_mut(
    alias_states: &mut [PortfolioAliasState],
    first_index: usize,
    second_index: usize,
) -> (&mut PortfolioAliasState, &mut PortfolioAliasState) {
    debug_assert!(first_index != second_index);
    if first_index < second_index {
        let (left, right) = alias_states.split_at_mut(second_index);
        (&mut left[first_index], &mut right[0])
    } else {
        let (left, right) = alias_states.split_at_mut(first_index);
        (&mut right[0], &mut left[second_index])
    }
}

fn validate_arb_surface(
    arb_surface: &PreparedArbSurface,
    alias_states: &[PortfolioAliasState],
    config: &BacktestConfig,
) -> Result<(), BacktestError> {
    if alias_states.len() < 2 {
        return Err(BacktestError::ArbitrageRequiresPortfolioMode);
    }
    if config.spot_virtual_rebalance {
        return Err(BacktestError::ArbitrageSpotVirtualRebalanceUnsupported);
    }
    for state in alias_states {
        if !matches!(
            state.template,
            crate::interval::SourceTemplate::BinanceSpot
                | crate::interval::SourceTemplate::BybitSpot
                | crate::interval::SourceTemplate::GateSpot
        ) {
            return Err(BacktestError::ArbitrageRequiresSpotAliases {
                alias: state.alias.clone(),
                template: state.template,
            });
        }
    }
    if arb_surface.entry_order.constructor != ArbPairConstructor::MarketPair
        || arb_surface.exit_order.constructor != ArbPairConstructor::MarketPair
    {
        return Err(BacktestError::UnsupportedArbitragePairConstructor);
    }
    let reference_base = alias_states
        .first()
        .and_then(|state| state.base_asset.as_deref());
    let reference_quote = alias_states.first().map(|state| state.quote_asset.as_str());
    for state in alias_states.iter().skip(1) {
        if state.base_asset.as_deref() != reference_base
            || Some(state.quote_asset.as_str()) != reference_quote
        {
            return Err(BacktestError::ArbitrageRequiresSpotAliases {
                alias: state.alias.clone(),
                template: state.template,
            });
        }
    }
    Ok(())
}

pub(crate) fn simulate_backtest(
    mut stepper: RuntimeStepper,
    execution: crate::backtest::bridge::ExecutionSource,
    execution_bars: Vec<Bar>,
    config: &BacktestConfig,
    prepared: PreparedBacktest,
) -> Result<BacktestResult, BacktestError> {
    if prepared.arb_surface.is_some() {
        return Err(BacktestError::ArbitrageRequiresPortfolioMode);
    }
    if prepared.transfer_surface.is_some() {
        return Err(BacktestError::TransferRequiresPortfolioMode);
    }
    let execution_alias = config.execution_source_alias.as_str();
    let (base_asset, quote_asset) = ledger_assets_for_symbol(execution.template, &execution.symbol);
    let fee_rates = fee_rates_for_alias(config, execution_alias);
    let slippage_rate = config.slippage_bps / crate::backtest::BPS_SCALE;
    let accounting = accounting_mode(config);
    let aligned_mark_bars = aligned_mark_bars(config, &execution_bars)?;
    let mut cash = config.initial_capital;
    let mut base_balance = 0.0;
    let mut position = None::<PositionState>;
    let mut open_trade = None::<OpenTrade>;
    let mut fills = Vec::<Fill>::new();
    let mut trades = Vec::<Trade>::new();
    let mut trade_diagnostics = Vec::<TradeDiagnostic>::new();
    let mut orders = Vec::<OrderRecord>::new();
    let mut order_contexts = Vec::<OrderDiagnosticContext>::new();
    let mut equity_curve = Vec::with_capacity(execution_bars.len());
    let mut active_orders: [Option<ActiveOrder>; ROLE_COUNT] = std::array::from_fn(|_| None);
    let mut pending_requests = empty_request_slots();
    let mut pending_snapshots: [Option<FeatureSnapshot>; ROLE_COUNT] =
        std::array::from_fn(|_| None);
    let mut pending_signal_names: [Option<String>; ROLE_COUNT] = std::array::from_fn(|_| None);
    let mut pending_conflict_time = None::<f64>;
    let mut total_realized_pnl = 0.0;
    let mut max_gross_exposure = 0.0_f64;
    let mut peak_equity = config.initial_capital;
    let mut max_drawdown = 0.0_f64;
    let mut execution_cursor = 0usize;
    let mut last_mark_price = None::<f64>;
    let mut last_snapshot = None::<FeatureSnapshot>;
    let mut last_exit = None::<LastExitSnapshot>;
    let mut last_long_exit = None::<LastExitSnapshot>;
    let mut last_short_exit = None::<LastExitSnapshot>;
    let mut target_consumption = TargetConsumptionState::default();
    let mut entry_progress = EntryProgressState::default();
    let mut diagnostics = DiagnosticsAccumulator::new(&prepared.exports);
    let mut per_bar_trace = Vec::<PerBarDecisionTrace>::new();
    let starting_ledgers = vec![ExchangeLedgerSnapshot {
        execution_alias: execution_alias.to_string(),
        template: execution.template,
        symbol: execution.symbol.clone(),
        balances: vec![AssetLedgerBalance {
            asset: quote_asset.clone(),
            amount: config.initial_capital,
        }],
    }];
    let ledger_events = vec![LedgerEvent {
        kind: LedgerEventKind::InitialDeposit,
        execution_alias: execution_alias.to_string(),
        counterparty_alias: None,
        asset: quote_asset.clone(),
        amount: config.initial_capital,
        bar_index: None,
        time: None,
    }];

    while let Some(open_time) = stepper.peek_open_time() {
        let next_execution = execution_bars.get(execution_cursor).copied();
        let current_execution =
            next_execution.filter(|bar| bar.time.is_finite() && bar.time == open_time as f64);
        let session_active = step_is_active(open_time, config.activation_time_ms);
        let current_mark =
            current_execution.and_then(|_| aligned_mark_bars.get(execution_cursor).copied());
        let mut position_events = PositionEventStep::default();
        let mut filled_record_indices = Vec::new();
        let mut decision_trace =
            matches!(config.diagnostics_detail, DiagnosticsDetailMode::FullTrace)
                .then(StepDecisionTrace::default);
        if let Some(bar) = current_execution.filter(|_| session_active) {
            if let Some(open_trade) = open_trade.as_mut() {
                update_open_trade_excursions(open_trade, bar.high, bar.low);
            }

            if let Some(timeout_outcome) = maybe_force_time_exit(
                execution_alias,
                &prepared.risk_controls,
                execution_cursor,
                bar.time,
                bar.open,
                &accounting,
                fee_rates.taker,
                &mut cash,
                &mut position,
                &mut open_trade,
                &mut fills,
                &mut trades,
                &mut trade_diagnostics,
                &mut total_realized_pnl,
                last_snapshot.clone(),
                decision_trace.as_mut(),
            ) {
                if base_asset.is_some() {
                    base_balance = position.as_ref().map_or(0.0, |state| state.quantity);
                }
                if let Some(snapshot) = timeout_outcome.snapshot {
                    set_exit_events(&mut position_events, snapshot.side, snapshot.kind);
                    update_last_exit_snapshots(
                        &mut last_exit,
                        &mut last_long_exit,
                        &mut last_short_exit,
                        snapshot,
                    );
                }
                if let Some(side) = timeout_outcome.fully_closed_side {
                    reset_target_consumption(&mut target_consumption, side);
                    reset_entry_progress(&mut entry_progress, side);
                    cancel_orders_for_closed_side(
                        &mut active_orders,
                        side,
                        exit_signal_role(side),
                        &mut orders,
                    );
                }
            }

            if pending_entry_requests_conflict(&pending_requests, position.as_ref(), entry_progress)
            {
                return Err(BacktestError::ConflictingSignals {
                    time: pending_conflict_time.unwrap_or(bar.time),
                });
            }

            place_pending_requests(
                &mut pending_requests,
                &mut pending_snapshots,
                &mut pending_signal_names,
                &mut active_orders,
                &mut orders,
                &mut order_contexts,
                &mut diagnostics,
                position.as_ref(),
                entry_progress,
                &prepared.risk_controls,
                last_long_exit.as_ref(),
                last_short_exit.as_ref(),
                last_snapshot.clone(),
                current_position_snapshot(position.as_ref(), execution_alias, bar.open, bar.time),
                execution_cursor,
                bar.time,
                execution_alias,
                decision_trace.as_mut(),
            );
            pending_conflict_time = None;
            let mut filled_this_bar = false;
            for role in ROLE_PRIORITY {
                if filled_this_bar {
                    break;
                }
                let slot = role_index(role);
                let Some(mut active) = active_orders[slot].take() else {
                    continue;
                };

                let first_eval = !active.first_eval_done;
                let evaluation =
                    evaluate_active_order(&active, bar.time, bar.open, bar.high, bar.low);
                active.first_eval_done = true;

                match evaluation {
                    crate::backtest::orders::Evaluation::None => {
                        record_order_decision(
                            decision_trace.as_mut(),
                            Some(orders[active.record_index].id),
                            Some(role),
                            match active.state {
                                WorkingState::Ready => DecisionReason::AwaitingTrigger,
                                WorkingState::RestingLimit { .. } => DecisionReason::AwaitingFill,
                            },
                        );
                        active_orders[slot] = Some(active);
                    }
                    crate::backtest::orders::Evaluation::Expire => {
                        record_order_decision(
                            decision_trace.as_mut(),
                            Some(orders[active.record_index].id),
                            Some(role),
                            DecisionReason::TifExpired,
                        );
                        update_order_record(
                            &mut orders[active.record_index],
                            OrderRecordUpdate {
                                trigger_time: None,
                                fill_bar_index: None,
                                fill_time: None,
                                raw_price: None,
                                fill_price: None,
                                effective_risk_per_unit: None,
                                capital_limited: None,
                                status: OrderStatus::Expired,
                                end_reason: None,
                            },
                        );
                    }
                    crate::backtest::orders::Evaluation::Cancel(reason) => {
                        record_order_decision(
                            decision_trace.as_mut(),
                            Some(orders[active.record_index].id),
                            Some(role),
                            decision_reason_for_order_end(reason),
                        );
                        update_order_record(
                            &mut orders[active.record_index],
                            OrderRecordUpdate {
                                trigger_time: None,
                                fill_bar_index: None,
                                fill_time: None,
                                raw_price: None,
                                fill_price: None,
                                effective_risk_per_unit: None,
                                capital_limited: None,
                                status: OrderStatus::Cancelled,
                                end_reason: Some(reason),
                            },
                        );
                    }
                    crate::backtest::orders::Evaluation::MoveToRestingLimit {
                        active_after_time,
                        trigger_time,
                    } => {
                        record_order_decision(
                            decision_trace.as_mut(),
                            Some(orders[active.record_index].id),
                            Some(role),
                            DecisionReason::AwaitingFill,
                        );
                        orders[active.record_index].trigger_time = Some(trigger_time);
                        active.state = WorkingState::RestingLimit { active_after_time };
                        active_orders[slot] = Some(active);
                    }
                    crate::backtest::orders::Evaluation::Fill(execution) => {
                        let action = fill_action_for_role(role);
                        let fee_rate = fee_rate_for_liquidity(
                            fee_rates,
                            fill_liquidity_for_order(&active, first_eval, bar.open),
                        );
                        let execution_price = if matches!(active.request.kind, OrderKind::Market) {
                            adjusted_price(execution.raw_price, action, slippage_rate)
                        } else {
                            execution.price
                        };
                        if fill_exceeds_volume_cap(
                            config,
                            projected_close_quantity(
                                position.as_ref(),
                                role,
                                if matches!(
                                    active.request.size_mode,
                                    Some(crate::order::SizeMode::RiskPct)
                                ) {
                                    None
                                } else {
                                    active.request.size_value
                                },
                            ),
                            bar.volume,
                        ) {
                            update_order_record(
                                &mut orders[active.record_index],
                                OrderRecordUpdate {
                                    trigger_time: execution.trigger_time,
                                    fill_bar_index: None,
                                    fill_time: None,
                                    raw_price: None,
                                    fill_price: None,
                                    effective_risk_per_unit: None,
                                    capital_limited: None,
                                    status: OrderStatus::Cancelled,
                                    end_reason: Some(OrderEndReason::VolumeParticipationExceeded),
                                },
                            );
                            continue;
                        }

                        let close_outcome = maybe_close_position_for_role(
                            execution_alias,
                            role,
                            active.record_index,
                            active.request.kind,
                            if matches!(
                                active.request.size_mode,
                                Some(crate::order::SizeMode::RiskPct)
                            ) {
                                None
                            } else {
                                active.request.size_value
                            },
                            last_snapshot.clone(),
                            execution_cursor,
                            bar.time,
                            execution.raw_price,
                            execution_price,
                            &accounting,
                            fee_rate,
                            &mut cash,
                            &mut position,
                            &mut open_trade,
                            &mut fills,
                            &mut trades,
                            &mut trade_diagnostics,
                            &mut total_realized_pnl,
                        );

                        if let Some(snapshot) = close_outcome.snapshot {
                            set_exit_events(&mut position_events, snapshot.side, snapshot.kind);
                            update_last_exit_snapshots(
                                &mut last_exit,
                                &mut last_long_exit,
                                &mut last_short_exit,
                                snapshot,
                            );
                        }
                        if base_asset.is_some() {
                            base_balance = position.as_ref().map_or(0.0, |state| state.quantity);
                        }

                        if let Some(side) = close_outcome.consumed_target_side {
                            mark_target_consumed(&mut target_consumption, side);
                        }

                        if role.is_target() {
                            set_target_stage_event(
                                &mut position_events,
                                current_side_for_role(role),
                                role.target_stage(),
                            );
                        }

                        if let Some(side) = close_outcome.fully_closed_side {
                            reset_target_consumption(&mut target_consumption, side);
                            reset_entry_progress(&mut entry_progress, side);
                            cancel_orders_for_closed_side(
                                &mut active_orders,
                                side,
                                role,
                                &mut orders,
                            );
                        }

                        if let Some(next_side) = position_side_for_entry(role) {
                            let sizing = match resolve_entry_sizing(
                                cash,
                                EntrySizingSpec {
                                    size_mode: active.request.size_mode,
                                    size_value: active.request.size_value,
                                    stop_price: active.request.size_stop_price,
                                },
                                next_side,
                                &accounting,
                                execution_price,
                                fee_rate,
                            ) {
                                Ok(sizing) => sizing,
                                Err(reason) => {
                                    update_order_record(
                                        &mut orders[active.record_index],
                                        OrderRecordUpdate {
                                            trigger_time: execution.trigger_time,
                                            fill_bar_index: None,
                                            fill_time: None,
                                            raw_price: None,
                                            fill_price: None,
                                            effective_risk_per_unit: None,
                                            capital_limited: None,
                                            status: OrderStatus::Cancelled,
                                            end_reason: Some(reason),
                                        },
                                    );
                                    continue;
                                }
                            };
                            if fill_exceeds_volume_cap(config, Some(sizing.quantity), bar.volume) {
                                update_order_record(
                                    &mut orders[active.record_index],
                                    OrderRecordUpdate {
                                        trigger_time: execution.trigger_time,
                                        fill_bar_index: None,
                                        fill_time: None,
                                        raw_price: None,
                                        fill_price: None,
                                        effective_risk_per_unit: None,
                                        capital_limited: None,
                                        status: OrderStatus::Cancelled,
                                        end_reason: Some(
                                            OrderEndReason::VolumeParticipationExceeded,
                                        ),
                                    },
                                );
                                continue;
                            }
                            if sizing.quantity <= crate::backtest::EPSILON {
                                update_order_record(
                                    &mut orders[active.record_index],
                                    OrderRecordUpdate {
                                        trigger_time: execution.trigger_time,
                                        fill_bar_index: None,
                                        fill_time: None,
                                        raw_price: None,
                                        fill_price: None,
                                        effective_risk_per_unit: None,
                                        capital_limited: None,
                                        status: OrderStatus::Cancelled,
                                        end_reason: Some(OrderEndReason::InsufficientCollateral),
                                    },
                                );
                                continue;
                            }
                            let execution_context = FillExecutionContext {
                                bar_index: execution_cursor,
                                time: bar.time,
                                raw_price: execution.raw_price,
                                execution_price,
                            };
                            if position
                                .as_ref()
                                .is_some_and(|state| state.side == next_side)
                            {
                                if let (Some(position_state), Some(open_trade_state)) =
                                    (position.as_mut(), open_trade.as_mut())
                                {
                                    let (entry_fill, entry_sizing) = match add_to_position(
                                        PositionFillContext {
                                            execution_alias,
                                            execution: execution_context,
                                            accounting: &accounting,
                                            fee_rate,
                                        },
                                        position_state,
                                        open_trade_state,
                                        EntrySizingSpec {
                                            size_mode: active.request.size_mode,
                                            size_value: active.request.size_value,
                                            stop_price: active.request.size_stop_price,
                                        },
                                        &mut cash,
                                    ) {
                                        Ok(result) => result,
                                        Err(reason) => {
                                            update_order_record(
                                                &mut orders[active.record_index],
                                                OrderRecordUpdate {
                                                    trigger_time: execution.trigger_time,
                                                    fill_bar_index: None,
                                                    fill_time: None,
                                                    raw_price: None,
                                                    fill_price: None,
                                                    effective_risk_per_unit: None,
                                                    capital_limited: None,
                                                    status: OrderStatus::Cancelled,
                                                    end_reason: Some(reason),
                                                },
                                            );
                                            continue;
                                        }
                                    };
                                    refresh_position_risk(
                                        position_state,
                                        &accounting,
                                        current_mark.map(|mark| mark.close).unwrap_or(bar.close),
                                    );
                                    update_open_trade_excursions(
                                        open_trade_state,
                                        bar.high,
                                        bar.low,
                                    );
                                    fills.push(entry_fill);
                                    if base_asset.is_some() {
                                        base_balance = position_state.quantity;
                                    }
                                    match next_side {
                                        PositionSide::Long => {
                                            position_events.long_entry_fill = true
                                        }
                                        PositionSide::Short => {
                                            position_events.short_entry_fill = true
                                        }
                                    }
                                    mark_entry_filled(
                                        &mut entry_progress,
                                        next_side,
                                        role.entry_stage().unwrap_or(1),
                                    );
                                    set_entry_stage_event(
                                        &mut position_events,
                                        next_side,
                                        role.entry_stage(),
                                    );
                                    reset_target_consumption(&mut target_consumption, next_side);
                                    orders[active.record_index].effective_risk_per_unit =
                                        entry_sizing.effective_risk_per_unit;
                                    orders[active.record_index].capital_limited =
                                        entry_sizing.capital_limited;
                                }
                            } else {
                                let (next_position, mut next_trade, entry_fill, entry_sizing) =
                                    match open_position(
                                        PositionFillContext {
                                            execution_alias,
                                            execution: execution_context,
                                            accounting: &accounting,
                                            fee_rate,
                                        },
                                        next_side,
                                        TradeEntryContext {
                                            order_id: active.record_index,
                                            role,
                                            module: prepared.signal_modules.get(&role).cloned(),
                                            kind: active.request.kind,
                                            snapshot: last_snapshot.clone(),
                                        },
                                        EntrySizingSpec {
                                            size_mode: active.request.size_mode,
                                            size_value: active.request.size_value,
                                            stop_price: active.request.size_stop_price,
                                        },
                                        &mut cash,
                                    ) {
                                        Ok(result) => result,
                                        Err(reason) => {
                                            update_order_record(
                                                &mut orders[active.record_index],
                                                OrderRecordUpdate {
                                                    trigger_time: execution.trigger_time,
                                                    fill_bar_index: None,
                                                    fill_time: None,
                                                    raw_price: None,
                                                    fill_price: None,
                                                    effective_risk_per_unit: None,
                                                    capital_limited: None,
                                                    status: OrderStatus::Cancelled,
                                                    end_reason: Some(reason),
                                                },
                                            );
                                            continue;
                                        }
                                    };
                                let mut next_position = next_position;
                                refresh_position_risk(
                                    &mut next_position,
                                    &accounting,
                                    current_mark.map(|mark| mark.close).unwrap_or(bar.close),
                                );
                                update_open_trade_excursions(&mut next_trade, bar.high, bar.low);
                                fills.push(entry_fill);
                                if base_asset.is_some() {
                                    base_balance = next_position.quantity;
                                }
                                match next_side {
                                    PositionSide::Long => position_events.long_entry_fill = true,
                                    PositionSide::Short => position_events.short_entry_fill = true,
                                }
                                mark_entry_filled(
                                    &mut entry_progress,
                                    next_side,
                                    role.entry_stage().unwrap_or(1),
                                );
                                set_entry_stage_event(
                                    &mut position_events,
                                    next_side,
                                    role.entry_stage(),
                                );
                                reset_target_consumption(&mut target_consumption, next_side);
                                position = Some(next_position);
                                open_trade = Some(next_trade);
                                orders[active.record_index].effective_risk_per_unit =
                                    entry_sizing.effective_risk_per_unit;
                                orders[active.record_index].capital_limited =
                                    entry_sizing.capital_limited;
                            }
                        }

                        let effective_risk_per_unit =
                            orders[active.record_index].effective_risk_per_unit;
                        let capital_limited = orders[active.record_index].capital_limited;
                        update_order_record(
                            &mut orders[active.record_index],
                            OrderRecordUpdate {
                                trigger_time: execution.trigger_time,
                                fill_bar_index: Some(execution_cursor),
                                fill_time: Some(bar.time),
                                raw_price: Some(execution.raw_price),
                                fill_price: Some(execution_price),
                                effective_risk_per_unit,
                                capital_limited: Some(capital_limited),
                                status: OrderStatus::Filled,
                                end_reason: None,
                            },
                        );
                        filled_record_indices.push(active.record_index);

                        invalidate_inapplicable_orders(
                            &mut active_orders,
                            position.as_ref(),
                            entry_progress,
                            &mut orders,
                        );
                        invalidate_stale_attached_orders(
                            &mut active_orders,
                            position.as_ref(),
                            target_consumption,
                            &prepared,
                            execution_alias,
                            &mut orders,
                        );
                        filled_this_bar = true;
                    }
                }
            }

            if let (Some(mark_bar), Some(position_state)) = (current_mark, position.as_mut()) {
                refresh_position_risk(position_state, &accounting, mark_bar.close);
                if position_state.entry_bar_index < execution_cursor {
                    if let Some(liquidation_price) = liquidation_trigger_price(
                        position_state,
                        mark_bar.open,
                        mark_bar.high,
                        mark_bar.low,
                    ) {
                        let liquidation_outcome = force_liquidation(
                            execution_alias,
                            position_state.side,
                            execution_cursor,
                            bar.time,
                            liquidation_price,
                            fee_rates.taker,
                            &mut cash,
                            &mut position,
                            &mut open_trade,
                            &mut fills,
                            &mut trades,
                            &mut trade_diagnostics,
                            &mut total_realized_pnl,
                        );
                        if base_asset.is_some() {
                            base_balance = position.as_ref().map_or(0.0, |state| state.quantity);
                        }
                        if let Some(snapshot) = liquidation_outcome.snapshot {
                            set_exit_events(&mut position_events, snapshot.side, snapshot.kind);
                            update_last_exit_snapshots(
                                &mut last_exit,
                                &mut last_long_exit,
                                &mut last_short_exit,
                                snapshot,
                            );
                        }
                        if let Some(side) = liquidation_outcome.fully_closed_side {
                            reset_target_consumption(&mut target_consumption, side);
                            reset_entry_progress(&mut entry_progress, side);
                            cancel_orders_for_closed_side(
                                &mut active_orders,
                                side,
                                liquidation_signal_role(side),
                                &mut orders,
                            );
                        }
                    }
                }
            }
        }

        let mark_price_for_ledger = current_execution
            .map(|bar| bar.close)
            .or(last_mark_price)
            .unwrap_or(0.0);
        let ledger_snapshot = single_ledger_runtime_snapshot(
            base_asset.is_some(),
            cash,
            base_balance,
            position.as_ref(),
            mark_price_for_ledger,
        );
        let ledger_snapshots = [(execution.execution_id, ledger_snapshot)];
        let execution_prices = [(
            execution.execution_id,
            current_execution.map(|bar| bar.close),
        )];
        let overrides = build_runtime_overrides(
            &prepared.position_fields,
            &prepared.position_event_fields,
            &prepared.last_exit_fields,
            &prepared.ledger_fields,
            &prepared.execution_price_fields,
            &ledger_snapshots,
            &execution_prices,
            position.as_ref(),
            open_trade.as_ref(),
            last_exit.as_ref(),
            last_long_exit.as_ref(),
            last_short_exit.as_ref(),
            Some(mark_price_for_ledger),
            open_time as f64,
            current_execution.map(|_| execution_cursor),
            position_events,
        );
        let RuntimeStep { output, .. } = stepper
            .step_with_overrides(&overrides)
            .map_err(BacktestError::Runtime)?
            .expect("peeked runtime step should exist");
        let step_time = open_time as f64;
        let snapshot = snapshot_from_step(&output, step_time);
        let step_bar_index = snapshot
            .as_ref()
            .map_or(execution_cursor, |feature_snapshot| {
                feature_snapshot.bar_index
            });
        let decision_position_snapshot = current_execution.and_then(|bar| {
            current_position_snapshot(position.as_ref(), execution_alias, bar.close, bar.time)
        });

        if let Some(bar) = current_execution {
            if session_active {
                if position_events.long_entry_fill || position_events.short_entry_fill {
                    if let Some(open_trade) = open_trade.as_mut() {
                        open_trade.entry_snapshot = snapshot.clone();
                    }
                }
                let fill_position = current_position_snapshot(
                    position.as_ref(),
                    execution_alias,
                    bar.close,
                    bar.time,
                );
                for record_index in filled_record_indices {
                    order_contexts[record_index].fill_snapshot = snapshot.clone();
                    order_contexts[record_index].fill_position = fill_position.clone();
                }
                let bar_return = diagnostics
                    .observe_execution_bar(bar.close, position.as_ref().map(|state| state.side));
                diagnostics.observe_exports(
                    &output,
                    snapshot.as_ref(),
                    fill_position.as_ref(),
                    execution_cursor,
                    step_time,
                    bar_return,
                    position.as_ref().map(|state| state.side),
                );
                let quantity = position.as_ref().map_or(0.0, |state| state.quantity);
                let mark_price = current_mark.map(|mark| mark.close).unwrap_or(bar.close);
                let gross_exposure = quantity.abs() * mark_price;
                let net_exposure = quantity * mark_price;
                max_gross_exposure = max_gross_exposure.max(gross_exposure);
                let equity = match &accounting {
                    AccountingMode::Spot => cash + base_balance * mark_price,
                    AccountingMode::PerpIsolated { .. } => {
                        cash + position.as_ref().map_or(0.0, |state| state.isolated_margin)
                            + position
                                .as_ref()
                                .map_or(0.0, |state| unrealized_pnl_for_position(state, mark_price))
                    }
                };
                peak_equity = peak_equity.max(equity);
                max_drawdown = max_drawdown.max(peak_equity - equity);
                equity_curve.push(EquityPoint {
                    bar_index: execution_cursor,
                    time: bar.time,
                    cash,
                    equity,
                    position_side: position.as_ref().map(|state| state.side),
                    quantity,
                    mark_price,
                    gross_exposure,
                    net_exposure,
                    open_position_count: usize::from(position.is_some()),
                    long_position_count: usize::from(
                        position
                            .as_ref()
                            .is_some_and(|state| state.side == PositionSide::Long),
                    ),
                    short_position_count: usize::from(
                        position
                            .as_ref()
                            .is_some_and(|state| state.side == PositionSide::Short),
                    ),
                    free_collateral: accounting.is_perp().then_some(cash),
                    isolated_margin: position.as_ref().map(|state| state.isolated_margin),
                    maintenance_margin: position.as_ref().map(|state| state.maintenance_margin),
                    liquidation_price: position.as_ref().and_then(|state| state.liquidation_price),
                });
                last_mark_price = Some(mark_price);
                if let Some(trace) = decision_trace.as_mut() {
                    ensure_no_signal_traces(trace, &prepared);
                    per_bar_trace.push(PerBarDecisionTrace {
                        execution_alias: execution_alias.to_string(),
                        bar_index: execution_cursor,
                        time: bar.time,
                        position_snapshot: fill_position.clone(),
                        feature_snapshot: snapshot.clone(),
                        signal_decisions: std::mem::take(&mut trace.signal_decisions),
                        order_decisions: std::mem::take(&mut trace.order_decisions),
                    });
                }
            }
            execution_cursor += 1;
        } else {
            diagnostics.observe_exports(
                &output,
                snapshot.as_ref(),
                None,
                step_bar_index,
                step_time,
                None,
                position.as_ref().map(|state| state.side),
            );
        }

        if session_active {
            enqueue_signal_requests(
                &output,
                step_time,
                &prepared,
                &mut pending_requests,
                &mut pending_snapshots,
                &mut pending_signal_names,
                &mut pending_conflict_time,
                &mut diagnostics,
                decision_position_snapshot.as_ref(),
                snapshot.as_ref(),
                step_bar_index,
                execution_alias,
                decision_trace.as_mut(),
            );
            enqueue_attached_requests(
                step_time,
                &output,
                &prepared,
                position.as_ref(),
                position.as_ref(),
                target_consumption,
                &mut pending_requests,
                &mut pending_snapshots,
                &mut pending_signal_names,
                &mut diagnostics,
                decision_position_snapshot.as_ref(),
                snapshot.as_ref(),
                step_bar_index,
                execution_alias,
                decision_trace.as_mut(),
            );
            last_snapshot = snapshot;
        }
    }

    let source_alignment = stepper.source_alignment_diagnostics();
    let outputs = stepper.finish();
    let order_diagnostics = build_order_diagnostics(&orders, &order_contexts);
    let diagnostics_summary = build_diagnostics_summary(&order_diagnostics, &trade_diagnostics);
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
            execution_alias: execution_alias.to_string(),
            side: position.side,
            quantity: position.quantity.abs(),
            entry_bar_index: position.entry_bar_index,
            entry_time: position.entry_time,
            entry_price: position.entry_price,
            market_price: last_point.mark_price,
            market_time: last_point.time,
            unrealized_pnl: unrealized_pnl_for_position(&position, last_point.mark_price),
            free_collateral: accounting.is_perp().then_some(last_point.cash),
            isolated_margin: accounting.is_perp().then_some(position.isolated_margin),
            maintenance_margin: accounting.is_perp().then_some(position.maintenance_margin),
            liquidation_price: accounting
                .is_perp()
                .then_some(position.liquidation_price)
                .flatten(),
        }),
        _ => None,
    };
    let (capture_summary, export_summaries, opportunity_events) = diagnostics.finalize(
        &execution_bars,
        &trade_diagnostics,
        (ending_equity - config.initial_capital) / config.initial_capital,
    );
    let cohorts = build_cohort_diagnostics(&trade_diagnostics, &export_summaries);
    let drawdown = build_drawdown_diagnostics(&equity_curve);
    let hints = build_backtest_hints(
        &BacktestSummary {
            starting_equity: config.initial_capital,
            ending_equity,
            realized_pnl: total_realized_pnl,
            unrealized_pnl,
            total_return: (ending_equity - config.initial_capital) / config.initial_capital,
            sharpe_ratio: annualized_sharpe_ratio(&equity_curve),
            trade_count,
            winning_trade_count,
            losing_trade_count,
            win_rate,
            max_drawdown,
            max_gross_exposure,
            max_net_exposure: equity_curve
                .iter()
                .map(|point| point.net_exposure.abs())
                .fold(0.0, f64::max),
            peak_open_position_count: equity_curve
                .iter()
                .map(|point| point.open_position_count)
                .max()
                .unwrap_or(0),
        },
        &diagnostics_summary,
        &cohorts,
        &drawdown,
    );
    let summary = BacktestSummary {
        starting_equity: config.initial_capital,
        ending_equity,
        realized_pnl: total_realized_pnl,
        unrealized_pnl,
        total_return: (ending_equity - config.initial_capital) / config.initial_capital,
        sharpe_ratio: annualized_sharpe_ratio(&equity_curve),
        trade_count,
        winning_trade_count,
        losing_trade_count,
        win_rate,
        max_drawdown,
        max_gross_exposure,
        max_net_exposure: equity_curve
            .iter()
            .map(|point| point.net_exposure.abs())
            .fold(0.0, f64::max),
        peak_open_position_count: equity_curve
            .iter()
            .map(|point| point.open_position_count)
            .max()
            .unwrap_or(0),
    };
    let overfitting_risk = build_backtest_overfitting_risk(&summary);
    let baseline_comparison = build_baseline_comparison(&summary, &capture_summary);

    Ok(BacktestResult {
        outputs,
        orders,
        fills,
        trades,
        diagnostics: BacktestDiagnostics {
            order_diagnostics,
            trade_diagnostics,
            summary: diagnostics_summary,
            capture_summary,
            baseline_comparison,
            export_summaries,
            opportunity_events,
            per_bar_trace,
            cohorts,
            drawdown,
            source_alignment,
            hints,
            overfitting_risk,
            portfolio_mode: false,
            spot_virtual_portfolio: false,
            blocked_portfolio_entries: Vec::new(),
            spot_quote_transfers: Vec::new(),
            transfer_summary: crate::backtest::TransferDiagnosticsSummary::default(),
            arbitrage: crate::backtest::ArbitrageDiagnosticsSummary::default(),
            starting_ledgers,
            ending_ledgers: vec![ExchangeLedgerSnapshot {
                execution_alias: execution_alias.to_string(),
                template: execution.template,
                symbol: execution.symbol,
                balances: {
                    let mut balances = vec![AssetLedgerBalance {
                        asset: quote_asset,
                        amount: cash,
                    }];
                    if let Some(base_asset) = base_asset {
                        balances.push(AssetLedgerBalance {
                            asset: base_asset,
                            amount: base_balance,
                        });
                    }
                    balances
                },
            }],
            ledger_events,
            date_perturbation: crate::backtest::DatePerturbationDiagnostics::default(),
        },
        equity_curve,
        summary,
        open_positions: open_position.clone().into_iter().collect(),
        open_position,
        perp: config
            .perp
            .as_ref()
            .zip(config.perp_context.as_ref())
            .map(|(perp, context)| PerpBacktestMetadata {
                leverage: perp.leverage,
                margin_mode: perp.margin_mode,
                mark_price_basis: context.mark_price_basis,
                risk_snapshot: context.risk_snapshot.clone(),
            }),
    })
}

#[allow(clippy::too_many_arguments)]
fn execute_market_pair_entry(
    alias_states: &mut [PortfolioAliasState],
    request: &CapturedArbRequest,
    bar_index: usize,
    time: f64,
    slippage_rate: f64,
    orders: &mut Vec<OrderRecord>,
    order_contexts: &mut Vec<OrderDiagnosticContext>,
    fills: &mut Vec<Fill>,
    pre_step_position_events: &mut [PositionEventStep],
) -> Result<ActiveArbBasket, BacktestError> {
    let Some(buy_index) = alias_index_for_execution_id(alias_states, request.buy_execution_id)
    else {
        return Err(BacktestError::ArbitrageUnknownExecutionSource {
            alias: format!("execution#{}", request.buy_execution_id),
        });
    };
    let Some(sell_index) = alias_index_for_execution_id(alias_states, request.sell_execution_id)
    else {
        return Err(BacktestError::ArbitrageUnknownExecutionSource {
            alias: format!("execution#{}", request.sell_execution_id),
        });
    };
    if buy_index == sell_index {
        return Err(BacktestError::ArbitrageSameVenue);
    }
    let (buy_state, sell_state) = split_two_states_mut(alias_states, buy_index, sell_index);
    if buy_state.position.is_some() || sell_state.position.is_some() {
        return Err(BacktestError::ConflictingSignals { time });
    }

    let Some(buy_bar) = buy_state
        .execution_bars
        .get(buy_state.execution_cursor)
        .copied()
        .filter(|bar| bar.time.is_finite() && bar.time == time)
    else {
        return Err(BacktestError::MissingExecutionBaseFeed {
            alias: buy_state.alias.clone(),
        });
    };
    let Some(sell_bar) = sell_state
        .execution_bars
        .get(sell_state.execution_cursor)
        .copied()
        .filter(|bar| bar.time.is_finite() && bar.time == time)
    else {
        return Err(BacktestError::MissingExecutionBaseFeed {
            alias: sell_state.alias.clone(),
        });
    };

    let buy_execution_price = adjusted_price(buy_bar.open, FillAction::Buy, slippage_rate);
    let sell_execution_price = adjusted_price(sell_bar.open, FillAction::Sell, slippage_rate);
    let entry_spread_bps = spread_bps(sell_execution_price, buy_execution_price);

    let buy_request = CapturedOrderRequest {
        role: SignalRole::LongEntry,
        kind: OrderKind::Market,
        tif: None,
        post_only: false,
        trigger_ref: None,
        size_mode: None,
        price: None,
        trigger_price: None,
        expire_time: None,
        has_size_field: false,
        size_value: None,
        size_stop_price: None,
        signal_time: request.signal_time,
    };
    let sell_request = CapturedOrderRequest {
        role: SignalRole::ShortEntry,
        ..buy_request
    };

    let buy_record_index = orders.len();
    orders.push(crate::backtest::orders::order_record(
        &buy_state.alias,
        buy_request,
        bar_index,
        time,
        buy_record_index,
    ));
    order_contexts.push(OrderDiagnosticContext {
        signal_snapshot: request.snapshot.clone(),
        placed_snapshot: request.snapshot.clone(),
        fill_snapshot: None,
        placed_position: None,
        fill_position: None,
    });

    let sell_record_index = orders.len();
    orders.push(crate::backtest::orders::order_record(
        &sell_state.alias,
        sell_request,
        bar_index,
        time,
        sell_record_index,
    ));
    order_contexts.push(OrderDiagnosticContext {
        signal_snapshot: request.snapshot.clone(),
        placed_snapshot: request.snapshot.clone(),
        fill_snapshot: None,
        placed_position: None,
        fill_position: None,
    });

    let (buy_position, buy_trade, buy_fill) = open_position_with_quantity(
        PositionFillContext {
            execution_alias: &buy_state.alias,
            execution: FillExecutionContext {
                bar_index,
                time,
                raw_price: buy_bar.open,
                execution_price: buy_execution_price,
            },
            accounting: &buy_state.accounting,
            fee_rate: buy_state.fee_rates.taker,
        },
        PositionSide::Long,
        TradeEntryContext {
            order_id: buy_record_index,
            role: SignalRole::LongEntry,
            module: None,
            kind: OrderKind::Market,
            snapshot: request.snapshot.clone(),
        },
        request.quantity,
        &mut buy_state.cash_balance,
    )
    .map_err(|_| BacktestError::InvalidArbitrageSize {
        value: request.quantity,
    })?;
    buy_state.position = Some(buy_position);
    buy_state.open_trade = Some(buy_trade);
    sync_spot_base_balance(buy_state);
    fills.push(buy_fill.clone());
    update_order_record(
        &mut orders[buy_record_index],
        OrderRecordUpdate {
            trigger_time: Some(time),
            fill_bar_index: Some(bar_index),
            fill_time: Some(time),
            raw_price: Some(buy_bar.open),
            fill_price: Some(buy_execution_price),
            effective_risk_per_unit: None,
            capital_limited: Some(false),
            status: OrderStatus::Filled,
            end_reason: None,
        },
    );
    pre_step_position_events[buy_index].long_entry_fill = true;

    let (sell_position, sell_trade, sell_fill) = open_position_with_quantity(
        PositionFillContext {
            execution_alias: &sell_state.alias,
            execution: FillExecutionContext {
                bar_index,
                time,
                raw_price: sell_bar.open,
                execution_price: sell_execution_price,
            },
            accounting: &sell_state.accounting,
            fee_rate: sell_state.fee_rates.taker,
        },
        PositionSide::Short,
        TradeEntryContext {
            order_id: sell_record_index,
            role: SignalRole::ShortEntry,
            module: None,
            kind: OrderKind::Market,
            snapshot: request.snapshot.clone(),
        },
        request.quantity,
        &mut sell_state.cash_balance,
    )
    .map_err(|_| BacktestError::InvalidArbitrageSize {
        value: request.quantity,
    })?;
    sell_state.position = Some(sell_position);
    sell_state.open_trade = Some(sell_trade);
    sync_spot_base_balance(sell_state);
    fills.push(sell_fill.clone());
    update_order_record(
        &mut orders[sell_record_index],
        OrderRecordUpdate {
            trigger_time: Some(time),
            fill_bar_index: Some(bar_index),
            fill_time: Some(time),
            raw_price: Some(sell_bar.open),
            fill_price: Some(sell_execution_price),
            effective_risk_per_unit: None,
            capital_limited: Some(false),
            status: OrderStatus::Filled,
            end_reason: None,
        },
    );
    pre_step_position_events[sell_index].short_entry_fill = true;

    Ok(ActiveArbBasket {
        buy_execution_id: request.buy_execution_id,
        sell_execution_id: request.sell_execution_id,
        quantity: request.quantity,
        entry_bar_index: bar_index,
        entry_time: time,
        buy_alias: buy_state.alias.clone(),
        sell_alias: sell_state.alias.clone(),
        buy_entry_price: buy_execution_price,
        sell_entry_price: sell_execution_price,
        entry_spread_bps,
    })
}

#[allow(clippy::too_many_arguments)]
fn execute_market_pair_exit(
    alias_states: &mut [PortfolioAliasState],
    basket: &ActiveArbBasket,
    request: &CapturedArbRequest,
    bar_index: usize,
    time: f64,
    slippage_rate: f64,
    orders: &mut Vec<OrderRecord>,
    order_contexts: &mut Vec<OrderDiagnosticContext>,
    fills: &mut Vec<Fill>,
    trades: &mut Vec<Trade>,
    trade_diagnostics: &mut Vec<TradeDiagnostic>,
    total_realized_pnl: &mut f64,
    pre_step_position_events: &mut [PositionEventStep],
    arb_baskets: &mut Vec<ArbitrageBasketRecord>,
) -> Result<(), BacktestError> {
    if request.buy_execution_id != basket.sell_execution_id
        || request.sell_execution_id != basket.buy_execution_id
    {
        return Err(BacktestError::ArbitrageExitVenueMismatch);
    }
    if (request.quantity - basket.quantity).abs() > crate::backtest::EPSILON {
        return Err(BacktestError::ArbitrageExitSizeMismatch {
            expected: basket.quantity,
            actual: request.quantity,
        });
    }
    let Some(long_index) = alias_index_for_execution_id(alias_states, basket.buy_execution_id)
    else {
        return Err(BacktestError::ArbitrageUnknownExecutionSource {
            alias: format!("execution#{}", basket.buy_execution_id),
        });
    };
    let Some(short_index) = alias_index_for_execution_id(alias_states, basket.sell_execution_id)
    else {
        return Err(BacktestError::ArbitrageUnknownExecutionSource {
            alias: format!("execution#{}", basket.sell_execution_id),
        });
    };
    let (long_state, short_state) = split_two_states_mut(alias_states, long_index, short_index);
    let Some(long_bar) = long_state
        .execution_bars
        .get(long_state.execution_cursor)
        .copied()
        .filter(|bar| bar.time.is_finite() && bar.time == time)
    else {
        return Err(BacktestError::MissingExecutionBaseFeed {
            alias: long_state.alias.clone(),
        });
    };
    let Some(short_bar) = short_state
        .execution_bars
        .get(short_state.execution_cursor)
        .copied()
        .filter(|bar| bar.time.is_finite() && bar.time == time)
    else {
        return Err(BacktestError::MissingExecutionBaseFeed {
            alias: short_state.alias.clone(),
        });
    };

    let long_execution_price = adjusted_price(long_bar.open, FillAction::Sell, slippage_rate);
    let short_execution_price = adjusted_price(short_bar.open, FillAction::Buy, slippage_rate);
    let long_trade_count_before = trades.len();

    let long_request = CapturedOrderRequest {
        role: SignalRole::LongExit,
        kind: OrderKind::Market,
        tif: None,
        post_only: false,
        trigger_ref: None,
        size_mode: None,
        price: None,
        trigger_price: None,
        expire_time: None,
        has_size_field: false,
        size_value: None,
        size_stop_price: None,
        signal_time: request.signal_time,
    };
    let short_request = CapturedOrderRequest {
        role: SignalRole::ShortExit,
        ..long_request
    };

    let long_record_index = orders.len();
    orders.push(crate::backtest::orders::order_record(
        &long_state.alias,
        long_request,
        bar_index,
        time,
        long_record_index,
    ));
    order_contexts.push(OrderDiagnosticContext {
        signal_snapshot: request.snapshot.clone(),
        placed_snapshot: request.snapshot.clone(),
        fill_snapshot: None,
        placed_position: current_position_snapshot(
            long_state.position.as_ref(),
            &long_state.alias,
            long_bar.open,
            time,
        ),
        fill_position: None,
    });

    let short_record_index = orders.len();
    orders.push(crate::backtest::orders::order_record(
        &short_state.alias,
        short_request,
        bar_index,
        time,
        short_record_index,
    ));
    order_contexts.push(OrderDiagnosticContext {
        signal_snapshot: request.snapshot.clone(),
        placed_snapshot: request.snapshot.clone(),
        fill_snapshot: None,
        placed_position: current_position_snapshot(
            short_state.position.as_ref(),
            &short_state.alias,
            short_bar.open,
            time,
        ),
        fill_position: None,
    });

    let long_outcome = maybe_close_position_for_role(
        &long_state.alias,
        SignalRole::LongExit,
        long_record_index,
        OrderKind::Market,
        Some(1.0),
        request.snapshot.clone(),
        bar_index,
        time,
        long_bar.open,
        long_execution_price,
        &long_state.accounting,
        long_state.fee_rates.taker,
        &mut long_state.cash_balance,
        &mut long_state.position,
        &mut long_state.open_trade,
        fills,
        trades,
        trade_diagnostics,
        total_realized_pnl,
    );
    sync_spot_base_balance(long_state);
    if let Some(snapshot) = long_outcome.snapshot {
        update_last_exit_snapshots(
            &mut long_state.last_exit,
            &mut long_state.last_long_exit,
            &mut long_state.last_short_exit,
            snapshot,
        );
    }
    pre_step_position_events[long_index].long_exit_fill = true;
    update_order_record(
        &mut orders[long_record_index],
        OrderRecordUpdate {
            trigger_time: Some(time),
            fill_bar_index: Some(bar_index),
            fill_time: Some(time),
            raw_price: Some(long_bar.open),
            fill_price: Some(long_execution_price),
            effective_risk_per_unit: None,
            capital_limited: Some(false),
            status: OrderStatus::Filled,
            end_reason: None,
        },
    );

    let short_outcome = maybe_close_position_for_role(
        &short_state.alias,
        SignalRole::ShortExit,
        short_record_index,
        OrderKind::Market,
        Some(1.0),
        request.snapshot.clone(),
        bar_index,
        time,
        short_bar.open,
        short_execution_price,
        &short_state.accounting,
        short_state.fee_rates.taker,
        &mut short_state.cash_balance,
        &mut short_state.position,
        &mut short_state.open_trade,
        fills,
        trades,
        trade_diagnostics,
        total_realized_pnl,
    );
    sync_spot_base_balance(short_state);
    if let Some(snapshot) = short_outcome.snapshot {
        update_last_exit_snapshots(
            &mut short_state.last_exit,
            &mut short_state.last_long_exit,
            &mut short_state.last_short_exit,
            snapshot,
        );
    }
    pre_step_position_events[short_index].short_exit_fill = true;
    update_order_record(
        &mut orders[short_record_index],
        OrderRecordUpdate {
            trigger_time: Some(time),
            fill_bar_index: Some(bar_index),
            fill_time: Some(time),
            raw_price: Some(short_bar.open),
            fill_price: Some(short_execution_price),
            effective_risk_per_unit: None,
            capital_limited: Some(false),
            status: OrderStatus::Filled,
            end_reason: None,
        },
    );
    let realized_pnl = trades
        .iter()
        .skip(long_trade_count_before)
        .map(|trade| trade.realized_pnl)
        .sum();
    arb_baskets.push(ArbitrageBasketRecord {
        buy_alias: basket.buy_alias.clone(),
        sell_alias: basket.sell_alias.clone(),
        entry_bar_index: basket.entry_bar_index,
        entry_time: basket.entry_time,
        quantity: basket.quantity,
        buy_entry_price: basket.buy_entry_price,
        sell_entry_price: basket.sell_entry_price,
        entry_spread_bps: basket.entry_spread_bps,
        exit_bar_index: Some(bar_index),
        exit_time: Some(time),
        buy_exit_price: Some(long_execution_price),
        sell_exit_price: Some(short_execution_price),
        exit_spread_bps: Some(spread_bps(long_execution_price, short_execution_price)),
        realized_pnl: Some(realized_pnl),
        holding_bars: Some(bar_index.saturating_sub(basket.entry_bar_index)),
    });
    Ok(())
}

pub(crate) fn simulate_portfolio_backtest(
    steppers: Vec<RuntimeStepper>,
    executions: Vec<(
        String,
        u16,
        u16,
        crate::interval::SourceTemplate,
        String,
        Vec<Bar>,
    )>,
    config: &BacktestConfig,
    prepared: PreparedBacktest,
) -> Result<BacktestResult, BacktestError> {
    let slippage_rate = config.slippage_bps / crate::backtest::BPS_SCALE;
    if config.spot_virtual_rebalance && executions.len() < 2 {
        return Err(BacktestError::SpotVirtualRebalanceRequiresPortfolioMode);
    }
    let initial_alias_balance = config.initial_capital / executions.len() as f64;
    let mut alias_states = Vec::with_capacity(executions.len());
    for ((alias, execution_id, _, template, symbol, execution_bars), stepper) in
        executions.into_iter().zip(steppers)
    {
        if config.spot_virtual_rebalance
            && !matches!(
                template,
                crate::interval::SourceTemplate::BinanceSpot
                    | crate::interval::SourceTemplate::BybitSpot
                    | crate::interval::SourceTemplate::GateSpot
            )
        {
            return Err(BacktestError::SpotVirtualRebalanceRequiresSpotAliases { alias, template });
        }
        let (base_asset, quote_asset) = ledger_assets_for_symbol(template, &symbol);
        let accounting = accounting_mode_for_alias(config, &alias, template);
        let fee_rates = fee_rates_for_alias(config, &alias);
        let aligned_mark_bars =
            aligned_mark_bars_for_alias(config, &alias, template, &execution_bars)?;
        alias_states.push(PortfolioAliasState {
            execution_id,
            alias,
            template,
            symbol,
            quote_asset,
            base_asset,
            aligned_mark_bars,
            stepper,
            execution_bars,
            execution_cursor: 0,
            accounting,
            fee_rates,
            position: None,
            open_trade: None,
            active_orders: std::array::from_fn(|_| None),
            pending_requests: empty_request_slots(),
            pending_snapshots: std::array::from_fn(|_| None),
            pending_signal_names: std::array::from_fn(|_| None),
            pending_conflict_time: None,
            last_mark_price: None,
            last_snapshot: None,
            last_exit: None,
            last_long_exit: None,
            last_short_exit: None,
            target_consumption: TargetConsumptionState::default(),
            entry_progress: EntryProgressState::default(),
            diagnostics: DiagnosticsAccumulator::new(&prepared.exports),
            cash_balance: initial_alias_balance,
            base_balance: 0.0,
        });
    }
    let mut spot_quote_transfers = Vec::<SpotQuoteTransfer>::new();
    let mut arb_baskets = Vec::<ArbitrageBasketRecord>::new();
    let starting_ledgers = alias_states.iter().map(ledger_snapshot).collect::<Vec<_>>();
    let mut ledger_events = alias_states
        .iter()
        .map(|state| LedgerEvent {
            kind: LedgerEventKind::InitialDeposit,
            execution_alias: state.alias.clone(),
            counterparty_alias: None,
            asset: state.quote_asset.clone(),
            amount: state.cash_balance,
            bar_index: None,
            time: None,
        })
        .collect::<Vec<_>>();
    if config.spot_virtual_rebalance {
        for order in prepared.order_templates.values() {
            if order
                .execution_alias
                .as_ref()
                .is_some_and(|alias| alias_states.iter().any(|state| &state.alias == alias))
                && order.role.is_short()
            {
                return Err(BacktestError::SpotVirtualRebalanceShortRoleUnsupported {
                    alias: order
                        .execution_alias
                        .clone()
                        .unwrap_or_else(|| config.execution_source_alias.clone()),
                    role: order.role,
                });
            }
        }
    }
    if let Some(arb_surface) = prepared.arb_surface.as_ref() {
        validate_arb_surface(arb_surface, &alias_states, config)?;
    }
    if let Some(transfer_surface) = prepared.transfer_surface.as_ref() {
        validate_transfer_surface(transfer_surface, &alias_states)?;
    }
    let mut fills = Vec::<Fill>::new();
    let mut trades = Vec::<Trade>::new();
    let mut trade_diagnostics = Vec::<TradeDiagnostic>::new();
    let mut orders = Vec::<OrderRecord>::new();
    let mut order_contexts = Vec::<OrderDiagnosticContext>::new();
    let mut equity_curve = Vec::new();
    let mut peak_equity = config.initial_capital;
    let mut max_drawdown = 0.0_f64;
    let mut max_gross_exposure = 0.0_f64;
    let mut max_net_exposure = 0.0_f64;
    let mut peak_open_position_count = 0usize;
    let mut total_realized_pnl = 0.0_f64;
    let mut all_traces = Vec::<PerBarDecisionTrace>::new();
    let mut pre_step_position_events = vec![PositionEventStep::default(); alias_states.len()];
    let mut pending_arb_entry = None::<CapturedArbRequest>;
    let mut pending_arb_exit = None::<CapturedArbRequest>;
    let mut active_arb_basket = None::<ActiveArbBasket>;
    let mut pending_quote_transfer_request = None::<CapturedQuoteTransferRequest>;
    let mut pending_quote_transfers = Vec::<PendingQuoteTransfer>::new();
    let mut blocked_counts = BTreeMap::<
        (
            crate::backtest::PortfolioControlKind,
            String,
            Option<String>,
        ),
        usize,
    >::new();

    while let Some(open_time) = alias_states
        .first()
        .and_then(|state| state.stepper.peek_open_time())
    {
        let session_active = step_is_active(open_time, config.activation_time_ms);
        let arb_bar_index = alias_states
            .first()
            .map(|state| state.execution_cursor)
            .unwrap_or(0);
        settle_pending_quote_transfers(
            &mut alias_states,
            arb_bar_index,
            open_time as f64,
            &mut spot_quote_transfers,
            &mut ledger_events,
            &mut pending_quote_transfers,
        );
        if session_active {
            if let Some(request) = pending_quote_transfer_request.take() {
                execute_quote_transfer_request(
                    &mut alias_states,
                    request,
                    arb_bar_index,
                    open_time as f64,
                    &mut spot_quote_transfers,
                    &mut ledger_events,
                    &mut pending_quote_transfers,
                )?;
            }
        } else {
            pending_quote_transfer_request = None;
        }
        if let Some(_arb_surface) = prepared.arb_surface.as_ref() {
            if session_active {
                if let (Some(basket), Some(request)) =
                    (active_arb_basket.as_ref(), pending_arb_exit.as_ref())
                {
                    execute_market_pair_exit(
                        &mut alias_states,
                        basket,
                        request,
                        arb_bar_index,
                        open_time as f64,
                        slippage_rate,
                        &mut orders,
                        &mut order_contexts,
                        &mut fills,
                        &mut trades,
                        &mut trade_diagnostics,
                        &mut total_realized_pnl,
                        &mut pre_step_position_events,
                        &mut arb_baskets,
                    )?;
                    active_arb_basket = None;
                }
                pending_arb_exit = None;

                if active_arb_basket.is_none() {
                    if let Some(request) = pending_arb_entry.as_ref() {
                        active_arb_basket = Some(execute_market_pair_entry(
                            &mut alias_states,
                            request,
                            arb_bar_index,
                            open_time as f64,
                            slippage_rate,
                            &mut orders,
                            &mut order_contexts,
                            &mut fills,
                            &mut pre_step_position_events,
                        )?);
                    }
                }
                pending_arb_entry = None;
            } else {
                pending_arb_entry = None;
                pending_arb_exit = None;
            }
        }

        let mut state_index = 0usize;
        while state_index < alias_states.len() {
            let (before_current, rest) = alias_states.split_at_mut(state_index);
            let (state, after_current) = rest
                .split_first_mut()
                .expect("portfolio alias state should exist");
            let next_execution = state.execution_bars.get(state.execution_cursor).copied();
            let current_execution =
                next_execution.filter(|bar| bar.time.is_finite() && bar.time == open_time as f64);
            let current_mark = current_execution
                .and_then(|_| state.aligned_mark_bars.get(state.execution_cursor).copied());
            let mut position_events = std::mem::take(&mut pre_step_position_events[state_index]);
            let mut filled_record_indices = Vec::new();
            let mut decision_trace =
                matches!(config.diagnostics_detail, DiagnosticsDetailMode::FullTrace)
                    .then(StepDecisionTrace::default);

            if let Some(bar) = current_execution.filter(|_| session_active) {
                if let Some(open_trade) = state.open_trade.as_mut() {
                    update_open_trade_excursions(open_trade, bar.high, bar.low);
                }

                let timeout_outcome = maybe_force_time_exit(
                    &state.alias,
                    &prepared.risk_controls,
                    state.execution_cursor,
                    bar.time,
                    bar.open,
                    &state.accounting,
                    state.fee_rates.taker,
                    &mut state.cash_balance,
                    &mut state.position,
                    &mut state.open_trade,
                    &mut fills,
                    &mut trades,
                    &mut trade_diagnostics,
                    &mut total_realized_pnl,
                    state.last_snapshot.clone(),
                    decision_trace.as_mut(),
                );
                if let Some(timeout_outcome) = timeout_outcome {
                    sync_spot_base_balance(state);
                    if let Some(snapshot) = timeout_outcome.snapshot {
                        set_exit_events(&mut position_events, snapshot.side, snapshot.kind);
                        update_last_exit_snapshots(
                            &mut state.last_exit,
                            &mut state.last_long_exit,
                            &mut state.last_short_exit,
                            snapshot,
                        );
                    }
                    if let Some(side) = timeout_outcome.fully_closed_side {
                        reset_target_consumption(&mut state.target_consumption, side);
                        reset_entry_progress(&mut state.entry_progress, side);
                        cancel_orders_for_closed_side(
                            &mut state.active_orders,
                            side,
                            exit_signal_role(side),
                            &mut orders,
                        );
                    }
                }

                if pending_entry_requests_conflict(
                    &state.pending_requests,
                    state.position.as_ref(),
                    state.entry_progress,
                ) {
                    return Err(BacktestError::ConflictingSignals {
                        time: state.pending_conflict_time.unwrap_or(bar.time),
                    });
                }

                place_pending_requests(
                    &mut state.pending_requests,
                    &mut state.pending_snapshots,
                    &mut state.pending_signal_names,
                    &mut state.active_orders,
                    &mut orders,
                    &mut order_contexts,
                    &mut state.diagnostics,
                    state.position.as_ref(),
                    state.entry_progress,
                    &prepared.risk_controls,
                    state.last_long_exit.as_ref(),
                    state.last_short_exit.as_ref(),
                    state.last_snapshot.clone(),
                    current_position_snapshot(
                        state.position.as_ref(),
                        &state.alias,
                        bar.open,
                        bar.time,
                    ),
                    state.execution_cursor,
                    bar.time,
                    &state.alias,
                    decision_trace.as_mut(),
                );
                state.pending_conflict_time = None;

                let mut filled_this_bar = false;
                for role in ROLE_PRIORITY {
                    if filled_this_bar {
                        break;
                    }
                    let slot = role_index(role);
                    let Some(mut active) = state.active_orders[slot].take() else {
                        continue;
                    };

                    let first_eval = !active.first_eval_done;
                    let evaluation =
                        evaluate_active_order(&active, bar.time, bar.open, bar.high, bar.low);
                    active.first_eval_done = true;

                    match evaluation {
                        crate::backtest::orders::Evaluation::None => {
                            record_order_decision(
                                decision_trace.as_mut(),
                                Some(orders[active.record_index].id),
                                Some(role),
                                match active.state {
                                    WorkingState::Ready => DecisionReason::AwaitingTrigger,
                                    WorkingState::RestingLimit { .. } => {
                                        DecisionReason::AwaitingFill
                                    }
                                },
                            );
                            state.active_orders[slot] = Some(active);
                        }
                        crate::backtest::orders::Evaluation::Expire => {
                            record_order_decision(
                                decision_trace.as_mut(),
                                Some(orders[active.record_index].id),
                                Some(role),
                                DecisionReason::TifExpired,
                            );
                            update_order_record(
                                &mut orders[active.record_index],
                                OrderRecordUpdate {
                                    trigger_time: None,
                                    fill_bar_index: None,
                                    fill_time: None,
                                    raw_price: None,
                                    fill_price: None,
                                    effective_risk_per_unit: None,
                                    capital_limited: None,
                                    status: OrderStatus::Expired,
                                    end_reason: None,
                                },
                            );
                        }
                        crate::backtest::orders::Evaluation::Cancel(reason) => {
                            record_order_decision(
                                decision_trace.as_mut(),
                                Some(orders[active.record_index].id),
                                Some(role),
                                decision_reason_for_order_end(reason),
                            );
                            update_order_record(
                                &mut orders[active.record_index],
                                OrderRecordUpdate {
                                    trigger_time: None,
                                    fill_bar_index: None,
                                    fill_time: None,
                                    raw_price: None,
                                    fill_price: None,
                                    effective_risk_per_unit: None,
                                    capital_limited: None,
                                    status: OrderStatus::Cancelled,
                                    end_reason: Some(reason),
                                },
                            );
                        }
                        crate::backtest::orders::Evaluation::MoveToRestingLimit {
                            active_after_time,
                            trigger_time,
                        } => {
                            record_order_decision(
                                decision_trace.as_mut(),
                                Some(orders[active.record_index].id),
                                Some(role),
                                DecisionReason::AwaitingFill,
                            );
                            orders[active.record_index].trigger_time = Some(trigger_time);
                            active.state = WorkingState::RestingLimit { active_after_time };
                            state.active_orders[slot] = Some(active);
                        }
                        crate::backtest::orders::Evaluation::Fill(execution) => {
                            let action = fill_action_for_role(role);
                            let fee_rate = fee_rate_for_liquidity(
                                state.fee_rates,
                                fill_liquidity_for_order(&active, first_eval, bar.open),
                            );
                            let execution_price =
                                if matches!(active.request.kind, OrderKind::Market) {
                                    adjusted_price(execution.raw_price, action, slippage_rate)
                                } else {
                                    execution.price
                                };
                            if fill_exceeds_volume_cap(
                                config,
                                projected_close_quantity(
                                    state.position.as_ref(),
                                    role,
                                    if matches!(
                                        active.request.size_mode,
                                        Some(crate::order::SizeMode::RiskPct)
                                    ) {
                                        None
                                    } else {
                                        active.request.size_value
                                    },
                                ),
                                bar.volume,
                            ) {
                                update_order_record(
                                    &mut orders[active.record_index],
                                    OrderRecordUpdate {
                                        trigger_time: execution.trigger_time,
                                        fill_bar_index: None,
                                        fill_time: None,
                                        raw_price: None,
                                        fill_price: None,
                                        effective_risk_per_unit: None,
                                        capital_limited: None,
                                        status: OrderStatus::Cancelled,
                                        end_reason: Some(
                                            OrderEndReason::VolumeParticipationExceeded,
                                        ),
                                    },
                                );
                                continue;
                            }

                            let close_outcome = maybe_close_position_for_role(
                                &state.alias,
                                role,
                                active.record_index,
                                active.request.kind,
                                if matches!(
                                    active.request.size_mode,
                                    Some(crate::order::SizeMode::RiskPct)
                                ) {
                                    None
                                } else {
                                    active.request.size_value
                                },
                                state.last_snapshot.clone(),
                                state.execution_cursor,
                                bar.time,
                                execution.raw_price,
                                execution_price,
                                &state.accounting,
                                fee_rate,
                                &mut state.cash_balance,
                                &mut state.position,
                                &mut state.open_trade,
                                &mut fills,
                                &mut trades,
                                &mut trade_diagnostics,
                                &mut total_realized_pnl,
                            );
                            sync_spot_base_balance(state);

                            if let Some(snapshot) = close_outcome.snapshot {
                                set_exit_events(&mut position_events, snapshot.side, snapshot.kind);
                                update_last_exit_snapshots(
                                    &mut state.last_exit,
                                    &mut state.last_long_exit,
                                    &mut state.last_short_exit,
                                    snapshot,
                                );
                            }
                            if let Some(side) = close_outcome.consumed_target_side {
                                mark_target_consumed(&mut state.target_consumption, side);
                            }
                            if let Some(side) = close_outcome.fully_closed_side {
                                reset_target_consumption(&mut state.target_consumption, side);
                                reset_entry_progress(&mut state.entry_progress, side);
                                cancel_orders_for_closed_side(
                                    &mut state.active_orders,
                                    side,
                                    exit_signal_role(side),
                                    &mut orders,
                                );
                            }

                            if let Some(next_side) = position_side_for_entry(role) {
                                if next_side == PositionSide::Long
                                    && config.spot_virtual_rebalance
                                    && state.base_asset.is_some()
                                {
                                    rebalance_spot_quote_for_entry(
                                        before_current,
                                        state,
                                        after_current,
                                        portfolio_cash_total_window(
                                            before_current,
                                            state,
                                            after_current,
                                        ),
                                        state.execution_cursor,
                                        bar.time,
                                        &mut spot_quote_transfers,
                                        &mut ledger_events,
                                    );
                                }
                                let portfolio_cash = portfolio_cash_total_window(
                                    before_current,
                                    state,
                                    after_current,
                                );
                                let available_cash = local_entry_cash(state);
                                let block_reason = portfolio_entry_block_reason(
                                    &prepared,
                                    PortfolioStateWindow {
                                        before_current,
                                        current_state: state,
                                        after_current,
                                    },
                                    next_side,
                                    PortfolioEntrySizingContext {
                                        execution_price,
                                        available_cash,
                                        portfolio_cash,
                                        fee_rate,
                                        size_mode: active.request.size_mode,
                                        size_value: active.request.size_value,
                                        stop_price: active.request.size_stop_price,
                                    },
                                );
                                if let Some((reason, kind)) = block_reason {
                                    record_signal_decision(
                                        decision_trace.as_mut(),
                                        role.canonical_name(),
                                        Some(role),
                                        reason,
                                    );
                                    increment_portfolio_block_counts(
                                        &mut blocked_counts,
                                        &prepared,
                                        kind,
                                        &state.alias,
                                    );
                                    update_order_record(
                                        &mut orders[active.record_index],
                                        OrderRecordUpdate {
                                            trigger_time: execution.trigger_time,
                                            fill_bar_index: None,
                                            fill_time: None,
                                            raw_price: None,
                                            fill_price: None,
                                            effective_risk_per_unit: None,
                                            capital_limited: None,
                                            status: OrderStatus::Rejected,
                                            end_reason: Some(
                                                OrderEndReason::PortfolioControlRejected,
                                            ),
                                        },
                                    );
                                    continue;
                                }
                                let preview_sizing = match resolve_entry_sizing(
                                    available_cash,
                                    EntrySizingSpec {
                                        size_mode: active.request.size_mode,
                                        size_value: active.request.size_value,
                                        stop_price: active.request.size_stop_price,
                                    },
                                    next_side,
                                    &state.accounting,
                                    execution_price,
                                    fee_rate,
                                ) {
                                    Ok(result) => result,
                                    Err(reason) => {
                                        update_order_record(
                                            &mut orders[active.record_index],
                                            OrderRecordUpdate {
                                                trigger_time: execution.trigger_time,
                                                fill_bar_index: None,
                                                fill_time: None,
                                                raw_price: None,
                                                fill_price: None,
                                                effective_risk_per_unit: None,
                                                capital_limited: None,
                                                status: OrderStatus::Cancelled,
                                                end_reason: Some(reason),
                                            },
                                        );
                                        continue;
                                    }
                                };
                                if preview_sizing.quantity <= crate::backtest::EPSILON {
                                    update_order_record(
                                        &mut orders[active.record_index],
                                        OrderRecordUpdate {
                                            trigger_time: execution.trigger_time,
                                            fill_bar_index: None,
                                            fill_time: None,
                                            raw_price: None,
                                            fill_price: None,
                                            effective_risk_per_unit: None,
                                            capital_limited: None,
                                            status: OrderStatus::Cancelled,
                                            end_reason: Some(
                                                OrderEndReason::InsufficientCollateral,
                                            ),
                                        },
                                    );
                                    continue;
                                }
                                if fill_exceeds_volume_cap(
                                    config,
                                    Some(preview_sizing.quantity),
                                    bar.volume,
                                ) {
                                    update_order_record(
                                        &mut orders[active.record_index],
                                        OrderRecordUpdate {
                                            trigger_time: execution.trigger_time,
                                            fill_bar_index: None,
                                            fill_time: None,
                                            raw_price: None,
                                            fill_price: None,
                                            effective_risk_per_unit: None,
                                            capital_limited: None,
                                            status: OrderStatus::Cancelled,
                                            end_reason: Some(
                                                OrderEndReason::VolumeParticipationExceeded,
                                            ),
                                        },
                                    );
                                    continue;
                                }

                                let execution_context = FillExecutionContext {
                                    bar_index: state.execution_cursor,
                                    time: bar.time,
                                    raw_price: execution.raw_price,
                                    execution_price,
                                };
                                if state
                                    .position
                                    .as_ref()
                                    .is_some_and(|pos| pos.side == next_side)
                                {
                                    if let (Some(position_state), Some(open_trade_state)) =
                                        (state.position.as_mut(), state.open_trade.as_mut())
                                    {
                                        let entry_result = add_to_position(
                                            PositionFillContext {
                                                execution_alias: &state.alias,
                                                execution: execution_context,
                                                accounting: &state.accounting,
                                                fee_rate,
                                            },
                                            position_state,
                                            open_trade_state,
                                            EntrySizingSpec {
                                                size_mode: active.request.size_mode,
                                                size_value: active.request.size_value,
                                                stop_price: active.request.size_stop_price,
                                            },
                                            &mut state.cash_balance,
                                        );
                                        let (entry_fill, entry_sizing) = match entry_result {
                                            Ok(result) => result,
                                            Err(reason) => {
                                                update_order_record(
                                                    &mut orders[active.record_index],
                                                    OrderRecordUpdate {
                                                        trigger_time: execution.trigger_time,
                                                        fill_bar_index: None,
                                                        fill_time: None,
                                                        raw_price: None,
                                                        fill_price: None,
                                                        effective_risk_per_unit: None,
                                                        capital_limited: None,
                                                        status: OrderStatus::Cancelled,
                                                        end_reason: Some(reason),
                                                    },
                                                );
                                                continue;
                                            }
                                        };
                                        refresh_position_risk(
                                            position_state,
                                            &state.accounting,
                                            current_mark
                                                .map(|mark| mark.close)
                                                .unwrap_or(bar.close),
                                        );
                                        update_open_trade_excursions(
                                            open_trade_state,
                                            bar.high,
                                            bar.low,
                                        );
                                        fills.push(entry_fill);
                                        sync_spot_base_balance(state);
                                        match next_side {
                                            PositionSide::Long => {
                                                position_events.long_entry_fill = true
                                            }
                                            PositionSide::Short => {
                                                position_events.short_entry_fill = true
                                            }
                                        }
                                        mark_entry_filled(
                                            &mut state.entry_progress,
                                            next_side,
                                            role.entry_stage().unwrap_or(1),
                                        );
                                        set_entry_stage_event(
                                            &mut position_events,
                                            next_side,
                                            role.entry_stage(),
                                        );
                                        reset_target_consumption(
                                            &mut state.target_consumption,
                                            next_side,
                                        );
                                        orders[active.record_index].effective_risk_per_unit =
                                            entry_sizing.effective_risk_per_unit;
                                        orders[active.record_index].capital_limited =
                                            entry_sizing.capital_limited;
                                    }
                                } else {
                                    let (
                                        mut next_position,
                                        mut next_trade,
                                        entry_fill,
                                        entry_sizing,
                                    ) = match open_position(
                                        PositionFillContext {
                                            execution_alias: &state.alias,
                                            execution: execution_context,
                                            accounting: &state.accounting,
                                            fee_rate,
                                        },
                                        next_side,
                                        TradeEntryContext {
                                            order_id: active.record_index,
                                            role,
                                            module: prepared.signal_modules.get(&role).cloned(),
                                            kind: active.request.kind,
                                            snapshot: state.last_snapshot.clone(),
                                        },
                                        EntrySizingSpec {
                                            size_mode: active.request.size_mode,
                                            size_value: active.request.size_value,
                                            stop_price: active.request.size_stop_price,
                                        },
                                        &mut state.cash_balance,
                                    ) {
                                        Ok(result) => result,
                                        Err(reason) => {
                                            update_order_record(
                                                &mut orders[active.record_index],
                                                OrderRecordUpdate {
                                                    trigger_time: execution.trigger_time,
                                                    fill_bar_index: None,
                                                    fill_time: None,
                                                    raw_price: None,
                                                    fill_price: None,
                                                    effective_risk_per_unit: None,
                                                    capital_limited: None,
                                                    status: OrderStatus::Cancelled,
                                                    end_reason: Some(reason),
                                                },
                                            );
                                            continue;
                                        }
                                    };
                                    refresh_position_risk(
                                        &mut next_position,
                                        &state.accounting,
                                        current_mark.map(|mark| mark.close).unwrap_or(bar.close),
                                    );
                                    update_open_trade_excursions(
                                        &mut next_trade,
                                        bar.high,
                                        bar.low,
                                    );
                                    fills.push(entry_fill);
                                    if state.base_asset.is_some() {
                                        state.base_balance = next_position.quantity;
                                    }
                                    match next_side {
                                        PositionSide::Long => {
                                            position_events.long_entry_fill = true
                                        }
                                        PositionSide::Short => {
                                            position_events.short_entry_fill = true
                                        }
                                    }
                                    mark_entry_filled(
                                        &mut state.entry_progress,
                                        next_side,
                                        role.entry_stage().unwrap_or(1),
                                    );
                                    set_entry_stage_event(
                                        &mut position_events,
                                        next_side,
                                        role.entry_stage(),
                                    );
                                    reset_target_consumption(
                                        &mut state.target_consumption,
                                        next_side,
                                    );
                                    state.position = Some(next_position);
                                    state.open_trade = Some(next_trade);
                                    orders[active.record_index].effective_risk_per_unit =
                                        entry_sizing.effective_risk_per_unit;
                                    orders[active.record_index].capital_limited =
                                        entry_sizing.capital_limited;
                                }
                            }

                            let effective_risk_per_unit =
                                orders[active.record_index].effective_risk_per_unit;
                            let capital_limited = orders[active.record_index].capital_limited;
                            update_order_record(
                                &mut orders[active.record_index],
                                OrderRecordUpdate {
                                    trigger_time: execution.trigger_time,
                                    fill_bar_index: Some(state.execution_cursor),
                                    fill_time: Some(bar.time),
                                    raw_price: Some(execution.raw_price),
                                    fill_price: Some(execution_price),
                                    effective_risk_per_unit,
                                    capital_limited: Some(capital_limited),
                                    status: OrderStatus::Filled,
                                    end_reason: None,
                                },
                            );
                            filled_record_indices.push(active.record_index);
                            invalidate_inapplicable_orders(
                                &mut state.active_orders,
                                state.position.as_ref(),
                                state.entry_progress,
                                &mut orders,
                            );
                            invalidate_stale_attached_orders(
                                &mut state.active_orders,
                                state.position.as_ref(),
                                state.target_consumption,
                                &prepared,
                                &state.alias,
                                &mut orders,
                            );
                            filled_this_bar = true;
                        }
                    }
                }

                if let (Some(mark_bar), Some(position_state)) =
                    (current_mark, state.position.as_mut())
                {
                    refresh_position_risk(position_state, &state.accounting, mark_bar.close);
                    if position_state.entry_bar_index < state.execution_cursor {
                        if let Some(liquidation_price) = liquidation_trigger_price(
                            position_state,
                            mark_bar.open,
                            mark_bar.high,
                            mark_bar.low,
                        ) {
                            let liquidation_outcome = force_liquidation(
                                &state.alias,
                                position_state.side,
                                state.execution_cursor,
                                bar.time,
                                liquidation_price,
                                state.fee_rates.taker,
                                &mut state.cash_balance,
                                &mut state.position,
                                &mut state.open_trade,
                                &mut fills,
                                &mut trades,
                                &mut trade_diagnostics,
                                &mut total_realized_pnl,
                            );
                            sync_spot_base_balance(state);
                            if let Some(snapshot) = liquidation_outcome.snapshot {
                                set_exit_events(&mut position_events, snapshot.side, snapshot.kind);
                                update_last_exit_snapshots(
                                    &mut state.last_exit,
                                    &mut state.last_long_exit,
                                    &mut state.last_short_exit,
                                    snapshot,
                                );
                            }
                            if let Some(side) = liquidation_outcome.fully_closed_side {
                                reset_target_consumption(&mut state.target_consumption, side);
                                reset_entry_progress(&mut state.entry_progress, side);
                                cancel_orders_for_closed_side(
                                    &mut state.active_orders,
                                    side,
                                    liquidation_signal_role(side),
                                    &mut orders,
                                );
                            }
                        }
                    }
                }
            }

            let ledger_snapshots = before_current
                .iter()
                .chain(std::iter::once(&*state))
                .chain(after_current.iter())
                .map(|alias_state| {
                    (
                        alias_state.execution_id,
                        ledger_runtime_snapshot(alias_state),
                    )
                })
                .collect::<Vec<_>>();
            let execution_prices = before_current
                .iter()
                .chain(std::iter::once(&*state))
                .chain(after_current.iter())
                .map(|alias_state| {
                    let price = alias_state
                        .execution_bars
                        .get(alias_state.execution_cursor)
                        .copied()
                        .filter(|bar| bar.time.is_finite() && bar.time == open_time as f64)
                        .map(|bar| bar.close);
                    (alias_state.execution_id, price)
                })
                .collect::<Vec<_>>();
            let overrides = build_runtime_overrides(
                &prepared.position_fields,
                &prepared.position_event_fields,
                &prepared.last_exit_fields,
                &prepared.ledger_fields,
                &prepared.execution_price_fields,
                &ledger_snapshots,
                &execution_prices,
                state.position.as_ref(),
                state.open_trade.as_ref(),
                state.last_exit.as_ref(),
                state.last_long_exit.as_ref(),
                state.last_short_exit.as_ref(),
                current_execution
                    .map(|bar| bar.close)
                    .or(state.last_mark_price),
                open_time as f64,
                current_execution.map(|_| state.execution_cursor),
                position_events,
            );
            let RuntimeStep { output, .. } = state
                .stepper
                .step_with_overrides(&overrides)
                .map_err(BacktestError::Runtime)?
                .expect("peeked runtime step should exist");
            let step_time = open_time as f64;
            let snapshot = snapshot_from_step(&output, step_time);
            let step_bar_index = snapshot
                .as_ref()
                .map_or(state.execution_cursor, |feature_snapshot| {
                    feature_snapshot.bar_index
                });
            let decision_position_snapshot = current_execution.and_then(|bar| {
                current_position_snapshot(
                    state.position.as_ref(),
                    &state.alias,
                    bar.close,
                    bar.time,
                )
            });

            if let Some(bar) = current_execution {
                if session_active {
                    if position_events.long_entry_fill || position_events.short_entry_fill {
                        if let Some(open_trade) = state.open_trade.as_mut() {
                            open_trade.entry_snapshot = snapshot.clone();
                        }
                    }
                    let fill_position = current_position_snapshot(
                        state.position.as_ref(),
                        &state.alias,
                        bar.close,
                        bar.time,
                    );
                    for record_index in filled_record_indices {
                        order_contexts[record_index].fill_snapshot = snapshot.clone();
                        order_contexts[record_index].fill_position = fill_position.clone();
                    }
                    let bar_return = state
                        .diagnostics
                        .observe_execution_bar(bar.close, state.position.as_ref().map(|s| s.side));
                    state.diagnostics.observe_exports(
                        &output,
                        snapshot.as_ref(),
                        fill_position.as_ref(),
                        state.execution_cursor,
                        step_time,
                        bar_return,
                        state.position.as_ref().map(|s| s.side),
                    );
                    state.last_mark_price =
                        Some(current_mark.map(|mark| mark.close).unwrap_or(bar.close));
                    if let Some(trace) = decision_trace.as_mut() {
                        ensure_no_signal_traces(trace, &prepared);
                        all_traces.push(PerBarDecisionTrace {
                            execution_alias: state.alias.clone(),
                            bar_index: state.execution_cursor,
                            time: bar.time,
                            position_snapshot: fill_position.clone(),
                            feature_snapshot: snapshot.clone(),
                            signal_decisions: std::mem::take(&mut trace.signal_decisions),
                            order_decisions: std::mem::take(&mut trace.order_decisions),
                        });
                    }
                }
                state.execution_cursor += 1;
            }

            if session_active {
                if state_index == 0 {
                    if let Some(arb_surface) = prepared.arb_surface.as_ref() {
                        if active_arb_basket.is_some() {
                            if current_bool_local(&state.stepper, arb_surface.exit_signal.slot)
                                == Some(true)
                            {
                                pending_arb_exit = Some(capture_arb_request(
                                    state,
                                    &arb_surface.exit_order,
                                    step_time,
                                    snapshot.clone(),
                                )?);
                            }
                        } else if current_bool_local(&state.stepper, arb_surface.entry_signal.slot)
                            == Some(true)
                        {
                            pending_arb_entry = Some(capture_arb_request(
                                state,
                                &arb_surface.entry_order,
                                step_time,
                                snapshot.clone(),
                            )?);
                        }
                    }
                    if let Some(transfer_surface) = prepared.transfer_surface.as_ref() {
                        if let Some(transfer) = transfer_surface.quote_transfer.as_ref() {
                            pending_quote_transfer_request =
                                capture_quote_transfer_request(state, transfer)?;
                        }
                    }
                }
                enqueue_signal_requests(
                    &output,
                    step_time,
                    &prepared,
                    &mut state.pending_requests,
                    &mut state.pending_snapshots,
                    &mut state.pending_signal_names,
                    &mut state.pending_conflict_time,
                    &mut state.diagnostics,
                    decision_position_snapshot.as_ref(),
                    snapshot.as_ref(),
                    step_bar_index,
                    &state.alias,
                    decision_trace.as_mut(),
                );
                enqueue_attached_requests(
                    step_time,
                    &output,
                    &prepared,
                    state.position.as_ref(),
                    state.position.as_ref(),
                    state.target_consumption,
                    &mut state.pending_requests,
                    &mut state.pending_snapshots,
                    &mut state.pending_signal_names,
                    &mut state.diagnostics,
                    decision_position_snapshot.as_ref(),
                    snapshot.as_ref(),
                    step_bar_index,
                    &state.alias,
                    decision_trace.as_mut(),
                );
                state.last_snapshot = snapshot;
            }
            state_index += 1;
        }

        let mut gross_exposure = 0.0;
        let mut net_exposure = 0.0;
        let mut long_count = 0usize;
        let mut short_count = 0usize;
        for state in &alias_states {
            let Some(position) = state.position.as_ref() else {
                continue;
            };
            let mark_price = state.last_mark_price.unwrap_or(position.entry_price);
            let signed_notional = position.quantity * mark_price;
            gross_exposure += signed_notional.abs();
            net_exposure += signed_notional;
            match position.side {
                PositionSide::Long => long_count += 1,
                PositionSide::Short => short_count += 1,
            }
        }
        let open_position_count = long_count + short_count;
        let portfolio_cash = portfolio_cash_total(&alias_states);
        let equity = alias_states.iter().fold(0.0, |acc, state| {
            if state.base_asset.is_some() {
                let mark_price = state
                    .last_mark_price
                    .or_else(|| state.position.as_ref().map(|position| position.entry_price))
                    .unwrap_or(0.0);
                acc + state.cash_balance + state.base_balance * mark_price
            } else {
                acc + state.cash_balance
                    + state
                        .position
                        .as_ref()
                        .map_or(0.0, |position| position.isolated_margin)
                    + state.position.as_ref().map_or(0.0, |position| {
                        unrealized_pnl_for_position(
                            position,
                            state.last_mark_price.unwrap_or(position.entry_price),
                        )
                    })
            }
        });
        if config
            .activation_time_ms
            .is_none_or(|activation_time_ms| open_time >= activation_time_ms)
        {
            peak_equity = peak_equity.max(equity);
            max_drawdown = max_drawdown.max(peak_equity - equity);
            let gross_exposure_pct = if equity.abs() <= crate::backtest::EPSILON {
                0.0
            } else {
                gross_exposure / equity
            };
            let net_exposure_pct = if equity.abs() <= crate::backtest::EPSILON {
                0.0
            } else {
                net_exposure / equity
            };
            max_gross_exposure = max_gross_exposure.max(gross_exposure_pct);
            max_net_exposure = max_net_exposure.max(net_exposure_pct.abs());
            peak_open_position_count = peak_open_position_count.max(open_position_count);
            equity_curve.push(EquityPoint {
                bar_index: equity_curve.len(),
                time: open_time as f64,
                cash: portfolio_cash,
                equity,
                position_side: None,
                quantity: 0.0,
                mark_price: 0.0,
                gross_exposure: gross_exposure_pct,
                net_exposure: net_exposure_pct,
                open_position_count,
                long_position_count: long_count,
                short_position_count: short_count,
                free_collateral: None,
                isolated_margin: None,
                maintenance_margin: None,
                liquidation_price: None,
            });
        }
    }

    let ending_ledgers = alias_states.iter().map(ledger_snapshot).collect::<Vec<_>>();
    let mut outputs = Outputs::default();
    let mut source_alignment = crate::runtime::SourceAlignmentDiagnostics::default();
    let mut capture_summary = BacktestCaptureSummary::default();
    let mut export_summaries = Vec::new();
    let mut opportunity_events = Vec::new();
    let mut open_positions = Vec::new();
    for (index, state) in alias_states.into_iter().enumerate() {
        let alias_trade_diagnostics = trade_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.execution_alias == state.alias)
            .cloned()
            .collect::<Vec<_>>();
        let (alias_capture, alias_exports, alias_events) = state.diagnostics.finalize(
            &state.execution_bars,
            &alias_trade_diagnostics,
            if config.initial_capital.abs() <= crate::backtest::EPSILON {
                0.0
            } else {
                (equity_curve
                    .last()
                    .map(|point| point.equity)
                    .unwrap_or(config.initial_capital)
                    - config.initial_capital)
                    / config.initial_capital
            },
        );
        if index == 0 {
            capture_summary = alias_capture;
            export_summaries = alias_exports;
            source_alignment = state.stepper.source_alignment_diagnostics();
            outputs = state.stepper.finish();
        } else {
            let _ = state.stepper.finish();
        }
        opportunity_events.extend(alias_events);
        if let Some(mark_price) = state.last_mark_price {
            if let Some(position) = state.position.as_ref() {
                open_positions.push(
                    current_position_snapshot(
                        Some(position),
                        &state.alias,
                        mark_price,
                        equity_curve.last().map(|point| point.time).unwrap_or(0.0),
                    )
                    .expect("open position snapshot should exist"),
                );
            }
        }
    }

    if let Some(basket) = active_arb_basket.take() {
        arb_baskets.push(ArbitrageBasketRecord {
            buy_alias: basket.buy_alias,
            sell_alias: basket.sell_alias,
            entry_bar_index: basket.entry_bar_index,
            entry_time: basket.entry_time,
            quantity: basket.quantity,
            buy_entry_price: basket.buy_entry_price,
            sell_entry_price: basket.sell_entry_price,
            entry_spread_bps: basket.entry_spread_bps,
            exit_bar_index: None,
            exit_time: None,
            buy_exit_price: None,
            sell_exit_price: None,
            exit_spread_bps: None,
            realized_pnl: None,
            holding_bars: None,
        });
    }
    let order_diagnostics = build_order_diagnostics(&orders, &order_contexts);
    let diagnostics_summary = build_diagnostics_summary(&order_diagnostics, &trade_diagnostics);
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
    let summary = BacktestSummary {
        starting_equity: config.initial_capital,
        ending_equity,
        realized_pnl: total_realized_pnl,
        unrealized_pnl,
        total_return: (ending_equity - config.initial_capital) / config.initial_capital,
        sharpe_ratio: annualized_sharpe_ratio(&equity_curve),
        trade_count,
        winning_trade_count,
        losing_trade_count,
        win_rate,
        max_drawdown,
        max_gross_exposure,
        max_net_exposure,
        peak_open_position_count,
    };
    let cohorts = build_cohort_diagnostics(&trade_diagnostics, &export_summaries);
    let drawdown = build_drawdown_diagnostics(&equity_curve);
    let mut hints = build_backtest_hints(&summary, &diagnostics_summary, &cohorts, &drawdown);
    let overfitting_risk = build_backtest_overfitting_risk(&summary);
    let baseline_comparison = build_baseline_comparison(&summary, &capture_summary);
    let transfer_summary = build_transfer_diagnostics(&spot_quote_transfers);
    let arbitrage = build_arbitrage_diagnostics(&arb_baskets);
    let blocked_portfolio_entries = blocked_counts
        .into_iter()
        .map(
            |((kind, alias, group), count)| crate::backtest::PortfolioControlBlockSummary {
                kind,
                alias,
                group,
                count,
            },
        )
        .collect::<Vec<_>>();
    extend_portfolio_hints(&blocked_portfolio_entries, &mut hints);

    Ok(BacktestResult {
        outputs,
        orders,
        fills,
        trades,
        diagnostics: BacktestDiagnostics {
            order_diagnostics,
            trade_diagnostics,
            summary: diagnostics_summary,
            capture_summary,
            baseline_comparison,
            export_summaries,
            opportunity_events,
            per_bar_trace: all_traces,
            cohorts,
            drawdown,
            source_alignment,
            hints,
            overfitting_risk,
            portfolio_mode: true,
            spot_virtual_portfolio: config.spot_virtual_rebalance,
            blocked_portfolio_entries,
            spot_quote_transfers,
            transfer_summary,
            arbitrage,
            starting_ledgers,
            ending_ledgers,
            ledger_events,
            date_perturbation: crate::backtest::DatePerturbationDiagnostics::default(),
        },
        equity_curve,
        summary,
        open_position: open_positions.first().cloned(),
        open_positions,
        perp: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_runtime_overrides(
    position_fields: &[PositionFieldDecl],
    position_event_fields: &[PositionEventFieldDecl],
    last_exit_fields: &[LastExitFieldDecl],
    ledger_fields: &[LedgerFieldDecl],
    execution_price_fields: &[ExecutionPriceDecl],
    ledger_snapshots: &[(u16, LedgerRuntimeSnapshot)],
    execution_prices: &[(u16, Option<f64>)],
    position: Option<&PositionState>,
    open_trade: Option<&OpenTrade>,
    last_exit: Option<&LastExitSnapshot>,
    last_long_exit: Option<&LastExitSnapshot>,
    last_short_exit: Option<&LastExitSnapshot>,
    market_price: Option<f64>,
    _market_time: f64,
    current_bar_index: Option<usize>,
    position_events: PositionEventStep,
) -> Vec<(u16, Value)> {
    let mut overrides: Vec<(u16, Value)> = position_fields
        .iter()
        .map(|decl| {
            let value = match (decl.field, position, open_trade) {
                (PositionField::IsLong, Some(state), _) => {
                    Value::Bool(state.side == PositionSide::Long)
                }
                (PositionField::IsLong, None, _) => Value::Bool(false),
                (PositionField::IsShort, Some(state), _) => {
                    Value::Bool(state.side == PositionSide::Short)
                }
                (PositionField::IsShort, None, _) => Value::Bool(false),
                (PositionField::Side, Some(state), _) => Value::PositionSide(state.side),
                (PositionField::Side, None, _) => Value::NA,
                (PositionField::EntryPrice, Some(state), _) => Value::F64(state.entry_price),
                (PositionField::EntryTime, Some(state), _) => Value::F64(state.entry_time),
                (PositionField::EntryBarIndex, Some(state), _) => {
                    Value::F64(state.entry_bar_index as f64)
                }
                (PositionField::BarsHeld, Some(state), _) => Value::F64(
                    current_bar_index
                        .map(|index| index.saturating_sub(state.entry_bar_index) as f64)
                        .unwrap_or(0.0),
                ),
                (PositionField::MarketPrice, Some(_), _) => {
                    market_price.map(Value::F64).unwrap_or(Value::NA)
                }
                (PositionField::UnrealizedPnl, Some(state), _) => market_price
                    .map(|price| Value::F64(unrealized_pnl_for_position(state, price)))
                    .unwrap_or(Value::NA),
                (PositionField::UnrealizedReturn, Some(state), _) => market_price
                    .map(|price| {
                        let pnl = unrealized_pnl_for_position(state, price);
                        let notional = state.entry_price * state.quantity.abs();
                        if notional.abs() < crate::backtest::EPSILON {
                            Value::F64(0.0)
                        } else {
                            Value::F64(pnl / notional)
                        }
                    })
                    .unwrap_or(Value::NA),
                (PositionField::Mae, Some(_), Some(trade)) => Value::F64(trade.mae_price_delta),
                (PositionField::Mfe, Some(_), Some(trade)) => Value::F64(trade.mfe_price_delta),
                _ => Value::NA,
            };
            (decl.slot, value)
        })
        .collect();
    overrides.extend(position_event_fields.iter().map(|decl| {
        let value = match decl.field {
            PositionEventField::LongEntryFill => Value::Bool(position_events.long_entry_fill),
            PositionEventField::LongEntry1Fill => Value::Bool(position_events.long_entry1_fill),
            PositionEventField::LongEntry2Fill => Value::Bool(position_events.long_entry2_fill),
            PositionEventField::LongEntry3Fill => Value::Bool(position_events.long_entry3_fill),
            PositionEventField::ShortEntryFill => Value::Bool(position_events.short_entry_fill),
            PositionEventField::ShortEntry1Fill => Value::Bool(position_events.short_entry1_fill),
            PositionEventField::ShortEntry2Fill => Value::Bool(position_events.short_entry2_fill),
            PositionEventField::ShortEntry3Fill => Value::Bool(position_events.short_entry3_fill),
            PositionEventField::LongExitFill => Value::Bool(position_events.long_exit_fill),
            PositionEventField::ShortExitFill => Value::Bool(position_events.short_exit_fill),
            PositionEventField::LongProtectFill => Value::Bool(position_events.long_protect_fill),
            PositionEventField::ShortProtectFill => Value::Bool(position_events.short_protect_fill),
            PositionEventField::LongTargetFill => Value::Bool(position_events.long_target_fill),
            PositionEventField::LongTarget1Fill => Value::Bool(position_events.long_target1_fill),
            PositionEventField::LongTarget2Fill => Value::Bool(position_events.long_target2_fill),
            PositionEventField::LongTarget3Fill => Value::Bool(position_events.long_target3_fill),
            PositionEventField::ShortTargetFill => Value::Bool(position_events.short_target_fill),
            PositionEventField::ShortTarget1Fill => Value::Bool(position_events.short_target1_fill),
            PositionEventField::ShortTarget2Fill => Value::Bool(position_events.short_target2_fill),
            PositionEventField::ShortTarget3Fill => Value::Bool(position_events.short_target3_fill),
            PositionEventField::LongSignalExitFill => {
                Value::Bool(position_events.long_signal_exit_fill)
            }
            PositionEventField::ShortSignalExitFill => {
                Value::Bool(position_events.short_signal_exit_fill)
            }
            PositionEventField::LongReversalExitFill => {
                Value::Bool(position_events.long_reversal_exit_fill)
            }
            PositionEventField::ShortReversalExitFill => {
                Value::Bool(position_events.short_reversal_exit_fill)
            }
            PositionEventField::LongLiquidationFill => {
                Value::Bool(position_events.long_liquidation_fill)
            }
            PositionEventField::ShortLiquidationFill => {
                Value::Bool(position_events.short_liquidation_fill)
            }
        };
        (decl.slot, value)
    }));
    overrides.extend(last_exit_fields.iter().map(|decl| {
        let snapshot = match decl.scope {
            LastExitScope::Global => last_exit,
            LastExitScope::Long => last_long_exit,
            LastExitScope::Short => last_short_exit,
        };
        (decl.slot, last_exit_value(snapshot, decl.field))
    }));
    overrides.extend(ledger_fields.iter().map(|decl| {
        let snapshot = ledger_snapshots
            .iter()
            .find(|(execution_id, _)| *execution_id == decl.execution_id)
            .map(|(_, snapshot)| snapshot);
        (decl.slot, ledger_field_value(snapshot, decl.field))
    }));
    overrides.extend(execution_price_fields.iter().map(|decl| {
        let value = execution_prices
            .iter()
            .find(|(execution_id, _)| *execution_id == decl.execution_id)
            .and_then(|(_, price)| *price)
            .map(Value::F64)
            .unwrap_or(Value::NA);
        (decl.slot, value)
    }));
    overrides
}

fn last_exit_value(snapshot: Option<&LastExitSnapshot>, field: LastExitField) -> Value {
    let Some(snapshot) = snapshot else {
        return Value::NA;
    };
    match field {
        LastExitField::Kind => Value::ExitKind(snapshot.kind),
        LastExitField::Stage => snapshot
            .stage
            .map_or(Value::NA, |stage| Value::F64(stage as f64)),
        LastExitField::Side => Value::PositionSide(snapshot.side),
        LastExitField::Price => Value::F64(snapshot.price),
        LastExitField::Time => Value::F64(snapshot.time),
        LastExitField::BarIndex => Value::F64(snapshot.bar_index as f64),
        LastExitField::RealizedPnl => Value::F64(snapshot.realized_pnl),
        LastExitField::RealizedReturn => Value::F64(snapshot.realized_return),
        LastExitField::BarsHeld => Value::F64(snapshot.bars_held as f64),
    }
}

fn risk_control_bars(
    controls: &[RiskControlDecl],
    side: PositionSide,
    kind: RiskControlKind,
) -> Option<usize> {
    controls
        .iter()
        .find(|decl| decl.side == side && decl.kind == kind)
        .map(|decl| decl.bars)
}

fn cooldown_blocks_entry(
    controls: &[RiskControlDecl],
    side: PositionSide,
    bar_index: usize,
    last_long_exit: Option<&LastExitSnapshot>,
    last_short_exit: Option<&LastExitSnapshot>,
) -> bool {
    let Some(cooldown_bars) = risk_control_bars(controls, side, RiskControlKind::Cooldown) else {
        return false;
    };
    if cooldown_bars == 0 {
        return false;
    }
    let last_exit = match side {
        PositionSide::Long => last_long_exit,
        PositionSide::Short => last_short_exit,
    };
    let Some(last_exit) = last_exit else {
        return false;
    };
    bar_index <= last_exit.bar_index.saturating_add(cooldown_bars)
}

fn set_exit_events(position_events: &mut PositionEventStep, side: PositionSide, kind: ExitKind) {
    match side {
        PositionSide::Long => {
            position_events.long_exit_fill = true;
            match kind {
                ExitKind::Protect => position_events.long_protect_fill = true,
                ExitKind::Target => position_events.long_target_fill = true,
                ExitKind::Signal => position_events.long_signal_exit_fill = true,
                ExitKind::Reversal => position_events.long_reversal_exit_fill = true,
                ExitKind::Liquidation => position_events.long_liquidation_fill = true,
            }
        }
        PositionSide::Short => {
            position_events.short_exit_fill = true;
            match kind {
                ExitKind::Protect => position_events.short_protect_fill = true,
                ExitKind::Target => position_events.short_target_fill = true,
                ExitKind::Signal => position_events.short_signal_exit_fill = true,
                ExitKind::Reversal => position_events.short_reversal_exit_fill = true,
                ExitKind::Liquidation => position_events.short_liquidation_fill = true,
            }
        }
    }
}

fn set_target_stage_event(
    position_events: &mut PositionEventStep,
    side: PositionSide,
    stage: Option<u8>,
) {
    match (side, stage) {
        (PositionSide::Long, Some(1)) => position_events.long_target1_fill = true,
        (PositionSide::Long, Some(2)) => position_events.long_target2_fill = true,
        (PositionSide::Long, Some(3)) => position_events.long_target3_fill = true,
        (PositionSide::Short, Some(1)) => position_events.short_target1_fill = true,
        (PositionSide::Short, Some(2)) => position_events.short_target2_fill = true,
        (PositionSide::Short, Some(3)) => position_events.short_target3_fill = true,
        _ => {}
    }
}

fn set_entry_stage_event(
    position_events: &mut PositionEventStep,
    side: PositionSide,
    stage: Option<u8>,
) {
    match (side, stage) {
        (PositionSide::Long, Some(1)) => position_events.long_entry1_fill = true,
        (PositionSide::Long, Some(2)) => position_events.long_entry2_fill = true,
        (PositionSide::Long, Some(3)) => position_events.long_entry3_fill = true,
        (PositionSide::Short, Some(1)) => position_events.short_entry1_fill = true,
        (PositionSide::Short, Some(2)) => position_events.short_entry2_fill = true,
        (PositionSide::Short, Some(3)) => position_events.short_entry3_fill = true,
        _ => {}
    }
}

fn update_last_exit_snapshots(
    last_exit: &mut Option<LastExitSnapshot>,
    last_long_exit: &mut Option<LastExitSnapshot>,
    last_short_exit: &mut Option<LastExitSnapshot>,
    snapshot: LastExitSnapshot,
) {
    match snapshot.side {
        PositionSide::Long => *last_long_exit = Some(snapshot),
        PositionSide::Short => *last_short_exit = Some(snapshot),
    }
    *last_exit = Some(snapshot);
}

#[allow(clippy::too_many_arguments)]
fn enqueue_signal_requests(
    output: &StepOutput,
    signal_time: f64,
    prepared: &PreparedBacktest,
    pending_requests: &mut [Option<CapturedOrderRequest>; ROLE_COUNT],
    pending_snapshots: &mut [Option<FeatureSnapshot>; ROLE_COUNT],
    pending_signal_names: &mut [Option<String>; ROLE_COUNT],
    pending_conflict_time: &mut Option<f64>,
    diagnostics: &mut DiagnosticsAccumulator,
    position_snapshot: Option<&PositionSnapshot>,
    snapshot: Option<&FeatureSnapshot>,
    bar_index: usize,
    execution_alias: &str,
    decision_trace: Option<&mut StepDecisionTrace>,
) {
    let mut decision_trace = decision_trace;
    for event in &output.trigger_events {
        let Some(role) = prepared.signal_roles.get(&event.output_id).copied() else {
            continue;
        };
        let Some(template) = order_template_for_alias(prepared, role, execution_alias) else {
            continue;
        };
        let slot = role_index(role);
        let has_pending_opposite_entry = role.is_entry()
            && pending_requests.iter().flatten().any(|request| {
                request.role.is_entry()
                    && request.role.is_long() != role.is_long()
                    && request.signal_time == signal_time
            });
        let event_kind = if has_pending_opposite_entry {
            OpportunityEventKind::SignalConflicted
        } else if pending_requests[slot].is_some() {
            OpportunityEventKind::SignalReplacedPendingOrder
        } else {
            OpportunityEventKind::SignalQueued
        };
        diagnostics.record_signal_event(
            execution_alias,
            event_kind,
            &event.name,
            role,
            bar_index,
            signal_time,
            position_snapshot,
            snapshot,
        );
        record_signal_decision(
            decision_trace.as_deref_mut(),
            &event.name,
            Some(role),
            match event_kind {
                OpportunityEventKind::SignalConflicted => DecisionReason::ConflictingSignals,
                OpportunityEventKind::SignalReplacedPendingOrder => {
                    DecisionReason::SignalReplacedPendingOrder
                }
                _ => DecisionReason::SignalQueued,
            },
        );
        pending_requests[slot] = Some(capture_request(template, signal_time, output));
        pending_snapshots[slot] = snapshot.cloned();
        pending_signal_names[slot] = Some(event.name.clone());
        if role.is_entry() {
            *pending_conflict_time = Some(signal_time);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn enqueue_attached_requests(
    signal_time: f64,
    output: &StepOutput,
    prepared: &PreparedBacktest,
    position_before_step: Option<&PositionState>,
    position_after_step: Option<&PositionState>,
    target_consumption: TargetConsumptionState,
    pending_requests: &mut [Option<CapturedOrderRequest>; ROLE_COUNT],
    pending_snapshots: &mut [Option<FeatureSnapshot>; ROLE_COUNT],
    pending_signal_names: &mut [Option<String>; ROLE_COUNT],
    diagnostics: &mut DiagnosticsAccumulator,
    position_snapshot: Option<&PositionSnapshot>,
    snapshot: Option<&FeatureSnapshot>,
    bar_index: usize,
    execution_alias: &str,
    decision_trace: Option<&mut StepDecisionTrace>,
) {
    let (Some(before), Some(after)) = (position_before_step, position_after_step) else {
        return;
    };
    let mut decision_trace = decision_trace;
    if before.side != after.side {
        return;
    }

    let mut roles = [None, None];
    roles[0] =
        resolve_active_protect_role(before.side, target_consumption, prepared, execution_alias);
    roles[1] =
        resolve_active_target_role(before.side, target_consumption, prepared, execution_alias);
    for role in roles.into_iter().flatten() {
        let Some(template) = order_template_for_alias(prepared, role, execution_alias) else {
            continue;
        };
        let slot = role_index(role);
        diagnostics.record_signal_event(
            execution_alias,
            if pending_requests[slot].is_some() {
                OpportunityEventKind::SignalReplacedPendingOrder
            } else {
                OpportunityEventKind::SignalQueued
            },
            role.canonical_name(),
            role,
            bar_index,
            signal_time,
            position_snapshot,
            snapshot,
        );
        record_signal_decision(
            decision_trace.as_deref_mut(),
            role.canonical_name(),
            Some(role),
            if pending_requests[slot].is_some() {
                DecisionReason::SignalReplacedPendingOrder
            } else {
                DecisionReason::SignalQueued
            },
        );
        pending_requests[slot] = Some(capture_request(template, signal_time, output));
        pending_snapshots[slot] = snapshot.cloned();
        pending_signal_names[slot] = Some(role.canonical_name().to_string());
    }
}

#[allow(clippy::too_many_arguments)]
fn place_pending_requests(
    pending_requests: &mut [Option<CapturedOrderRequest>; ROLE_COUNT],
    pending_snapshots: &mut [Option<FeatureSnapshot>; ROLE_COUNT],
    pending_signal_names: &mut [Option<String>; ROLE_COUNT],
    active_orders: &mut [Option<ActiveOrder>; ROLE_COUNT],
    orders: &mut Vec<OrderRecord>,
    order_contexts: &mut Vec<OrderDiagnosticContext>,
    diagnostics: &mut DiagnosticsAccumulator,
    position: Option<&PositionState>,
    entry_progress: EntryProgressState,
    risk_controls: &[RiskControlDecl],
    last_long_exit: Option<&LastExitSnapshot>,
    last_short_exit: Option<&LastExitSnapshot>,
    placed_snapshot: Option<FeatureSnapshot>,
    placed_position: Option<PositionSnapshot>,
    bar_index: usize,
    time: f64,
    execution_alias: &str,
    decision_trace: Option<&mut StepDecisionTrace>,
) {
    let mut decision_trace = decision_trace;
    for role in ROLE_PRIORITY {
        let slot = role_index(role);
        let Some(request) = pending_requests[slot].take() else {
            continue;
        };
        let signal_snapshot = pending_snapshots[slot].take();
        let signal_name = pending_signal_names[slot].take();
        if role.is_entry()
            && cooldown_blocks_entry(
                risk_controls,
                current_side_for_role(role),
                bar_index,
                last_long_exit,
                last_short_exit,
            )
        {
            diagnostics.record_signal_event(
                execution_alias,
                OpportunityEventKind::SignalIgnoredCooldown,
                signal_name.as_deref().unwrap_or(role.canonical_name()),
                role,
                bar_index,
                time,
                placed_position.as_ref(),
                placed_snapshot.as_ref().or(signal_snapshot.as_ref()),
            );
            record_signal_decision(
                decision_trace.as_deref_mut(),
                signal_name.as_deref().unwrap_or(role.canonical_name()),
                Some(role),
                DecisionReason::CooldownActive,
            );
            continue;
        }
        if !request_applicable(request, position, entry_progress) {
            let decision_reason = if matches!(
                (
                    role.is_entry(),
                    role.is_long(),
                    position.map(|state| state.side)
                ),
                (true, true, Some(PositionSide::Long)) | (true, false, Some(PositionSide::Short))
            ) {
                DecisionReason::SameSidePosition
            } else {
                DecisionReason::NoPosition
            };
            diagnostics.record_signal_event(
                execution_alias,
                if matches!(
                    (
                        role.is_entry(),
                        role.is_long(),
                        position.map(|state| state.side)
                    ),
                    (true, true, Some(PositionSide::Long))
                        | (true, false, Some(PositionSide::Short))
                ) {
                    OpportunityEventKind::SignalIgnoredSameSide
                } else {
                    OpportunityEventKind::SignalIgnoredNoPosition
                },
                signal_name.as_deref().unwrap_or(role.canonical_name()),
                role,
                bar_index,
                time,
                placed_position.as_ref(),
                placed_snapshot.as_ref(),
            );
            record_signal_decision(
                decision_trace.as_deref_mut(),
                signal_name.as_deref().unwrap_or(role.canonical_name()),
                Some(role),
                decision_reason,
            );
            continue;
        }

        if let Some(existing) = active_orders[slot].take() {
            diagnostics.record_signal_event(
                execution_alias,
                OpportunityEventKind::SignalReplacedPendingOrder,
                signal_name.as_deref().unwrap_or(role.canonical_name()),
                role,
                bar_index,
                time,
                placed_position.as_ref(),
                placed_snapshot.as_ref(),
            );
            update_order_record(
                &mut orders[existing.record_index],
                OrderRecordUpdate {
                    trigger_time: None,
                    fill_bar_index: None,
                    fill_time: None,
                    raw_price: None,
                    fill_price: None,
                    effective_risk_per_unit: None,
                    capital_limited: None,
                    status: OrderStatus::Cancelled,
                    end_reason: Some(if is_attached_exit_role(role) {
                        OrderEndReason::Rearmed
                    } else {
                        OrderEndReason::Replaced
                    }),
                },
            );
        }

        let mut record = crate::backtest::orders::order_record(
            execution_alias,
            request,
            bar_index,
            time,
            orders.len(),
        );
        let record_index = orders.len();
        order_contexts.push(OrderDiagnosticContext {
            signal_snapshot,
            placed_snapshot: placed_snapshot.clone(),
            fill_snapshot: None,
            placed_position: placed_position.clone(),
            fill_position: None,
        });

        if let Some(reason) = missing_field_reason(request) {
            record.status = OrderStatus::Rejected;
            record.end_reason = Some(reason);
            record_order_decision(
                decision_trace.as_deref_mut(),
                Some(record.id),
                Some(role),
                DecisionReason::MissingOrderField,
            );
            orders.push(record);
            continue;
        }

        orders.push(record);
        active_orders[slot] = Some(ActiveOrder {
            request,
            record_index,
            first_eval_done: false,
            state: WorkingState::Ready,
        });
    }
}

fn pending_entry_requests_conflict(
    pending_requests: &[Option<CapturedOrderRequest>; ROLE_COUNT],
    position: Option<&PositionState>,
    entry_progress: EntryProgressState,
) -> bool {
    let has_long = pending_requests.iter().flatten().any(|request| {
        request.role.is_entry()
            && request.role.is_long()
            && request_applicable(*request, position, entry_progress)
    });
    let has_short = pending_requests.iter().flatten().any(|request| {
        request.role.is_entry()
            && request.role.is_short()
            && request_applicable(*request, position, entry_progress)
    });
    has_long && has_short
}

fn mark_target_consumed(state: &mut TargetConsumptionState, side: PositionSide) {
    match side {
        PositionSide::Long => state.long_stage = (state.long_stage + 1).min(3),
        PositionSide::Short => state.short_stage = (state.short_stage + 1).min(3),
    }
}

fn reset_target_consumption(state: &mut TargetConsumptionState, side: PositionSide) {
    match side {
        PositionSide::Long => state.long_stage = 0,
        PositionSide::Short => state.short_stage = 0,
    }
}

fn mark_entry_filled(state: &mut EntryProgressState, side: PositionSide, stage: u8) {
    match side {
        PositionSide::Long => state.long_stage = state.long_stage.max(stage),
        PositionSide::Short => state.short_stage = state.short_stage.max(stage),
    }
}

fn reset_entry_progress(state: &mut EntryProgressState, side: PositionSide) {
    match side {
        PositionSide::Long => state.long_stage = 0,
        PositionSide::Short => state.short_stage = 0,
    }
}

fn resolve_active_target_role(
    side: PositionSide,
    state: TargetConsumptionState,
    prepared: &PreparedBacktest,
    execution_alias: &str,
) -> Option<SignalRole> {
    let next_stage = match side {
        PositionSide::Long => state.long_stage + 1,
        PositionSide::Short => state.short_stage + 1,
    };
    let role = match (side, next_stage) {
        (PositionSide::Long, 1) => SignalRole::TargetLong,
        (PositionSide::Long, 2) => SignalRole::TargetLong2,
        (PositionSide::Long, 3) => SignalRole::TargetLong3,
        (PositionSide::Short, 1) => SignalRole::TargetShort,
        (PositionSide::Short, 2) => SignalRole::TargetShort2,
        (PositionSide::Short, 3) => SignalRole::TargetShort3,
        _ => return None,
    };
    order_template_for_alias(prepared, role, execution_alias).map(|_| role)
}

fn resolve_active_protect_role(
    side: PositionSide,
    state: TargetConsumptionState,
    prepared: &PreparedBacktest,
    execution_alias: &str,
) -> Option<SignalRole> {
    let stage = match side {
        PositionSide::Long => state.long_stage,
        PositionSide::Short => state.short_stage,
    };
    let roles: &[SignalRole] = match (side, stage) {
        (PositionSide::Long, 0) => &[SignalRole::ProtectLong],
        (PositionSide::Long, 1) => &[SignalRole::ProtectAfterTarget1Long, SignalRole::ProtectLong],
        (PositionSide::Long, 2) => &[
            SignalRole::ProtectAfterTarget2Long,
            SignalRole::ProtectAfterTarget1Long,
            SignalRole::ProtectLong,
        ],
        (PositionSide::Long, _) => &[
            SignalRole::ProtectAfterTarget3Long,
            SignalRole::ProtectAfterTarget2Long,
            SignalRole::ProtectAfterTarget1Long,
            SignalRole::ProtectLong,
        ],
        (PositionSide::Short, 0) => &[SignalRole::ProtectShort],
        (PositionSide::Short, 1) => &[
            SignalRole::ProtectAfterTarget1Short,
            SignalRole::ProtectShort,
        ],
        (PositionSide::Short, 2) => &[
            SignalRole::ProtectAfterTarget2Short,
            SignalRole::ProtectAfterTarget1Short,
            SignalRole::ProtectShort,
        ],
        (PositionSide::Short, _) => &[
            SignalRole::ProtectAfterTarget3Short,
            SignalRole::ProtectAfterTarget2Short,
            SignalRole::ProtectAfterTarget1Short,
            SignalRole::ProtectShort,
        ],
    };
    roles
        .iter()
        .copied()
        .find(|role| order_template_for_alias(prepared, *role, execution_alias).is_some())
}

fn order_template_for_alias(
    prepared: &PreparedBacktest,
    role: SignalRole,
    execution_alias: &str,
) -> Option<OrderDecl> {
    prepared
        .order_templates
        .get(&role)
        .cloned()
        .filter(|order| {
            order
                .execution_alias
                .as_deref()
                .map(|alias| alias == execution_alias)
                .unwrap_or(true)
        })
}

fn cancel_orders_for_closed_side(
    active_orders: &mut [Option<ActiveOrder>; ROLE_COUNT],
    side: PositionSide,
    filled_role: SignalRole,
    orders: &mut [OrderRecord],
) {
    let (signal_role, protect_roles, target_roles) = match side {
        PositionSide::Long => (
            SignalRole::LongExit,
            [
                SignalRole::ProtectLong,
                SignalRole::ProtectAfterTarget1Long,
                SignalRole::ProtectAfterTarget2Long,
                SignalRole::ProtectAfterTarget3Long,
            ],
            [
                SignalRole::TargetLong,
                SignalRole::TargetLong2,
                SignalRole::TargetLong3,
            ],
        ),
        PositionSide::Short => (
            SignalRole::ShortExit,
            [
                SignalRole::ProtectShort,
                SignalRole::ProtectAfterTarget1Short,
                SignalRole::ProtectAfterTarget2Short,
                SignalRole::ProtectAfterTarget3Short,
            ],
            [
                SignalRole::TargetShort,
                SignalRole::TargetShort2,
                SignalRole::TargetShort3,
            ],
        ),
    };

    cancel_active_role(
        active_orders,
        signal_role,
        orders,
        OrderEndReason::PositionClosed,
    );
    match filled_role {
        role if role.is_protect() => {
            for target_role in target_roles {
                cancel_active_role(
                    active_orders,
                    target_role,
                    orders,
                    OrderEndReason::OcoCancelled,
                );
            }
        }
        role if role.is_target() => {
            for protect_role in protect_roles {
                cancel_active_role(
                    active_orders,
                    protect_role,
                    orders,
                    OrderEndReason::OcoCancelled,
                );
            }
        }
        _ => {
            for protect_role in protect_roles {
                cancel_active_role(
                    active_orders,
                    protect_role,
                    orders,
                    OrderEndReason::PositionClosed,
                );
            }
            for target_role in target_roles {
                cancel_active_role(
                    active_orders,
                    target_role,
                    orders,
                    OrderEndReason::PositionClosed,
                );
            }
        }
    }
}

fn cancel_active_role(
    active_orders: &mut [Option<ActiveOrder>; ROLE_COUNT],
    role: SignalRole,
    orders: &mut [OrderRecord],
    reason: OrderEndReason,
) {
    let slot = role_index(role);
    let Some(active) = active_orders[slot].take() else {
        return;
    };
    update_order_record(
        &mut orders[active.record_index],
        OrderRecordUpdate {
            trigger_time: None,
            fill_bar_index: None,
            fill_time: None,
            raw_price: None,
            fill_price: None,
            effective_risk_per_unit: None,
            capital_limited: None,
            status: OrderStatus::Cancelled,
            end_reason: Some(reason),
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn maybe_close_position_for_role(
    execution_alias: &str,
    role: SignalRole,
    order_id: usize,
    order_kind: OrderKind,
    size_fraction: Option<f64>,
    exit_snapshot: Option<FeatureSnapshot>,
    bar_index: usize,
    time: f64,
    raw_price: f64,
    execution_price: f64,
    accounting: &AccountingMode,
    fee_rate: f64,
    cash: &mut f64,
    position: &mut Option<PositionState>,
    open_trade: &mut Option<OpenTrade>,
    fills: &mut Vec<Fill>,
    trades: &mut Vec<Trade>,
    trade_diagnostics: &mut Vec<TradeDiagnostic>,
    total_realized_pnl: &mut f64,
) -> CloseOutcome {
    let should_close = matches!(
        position.as_ref().map(|state| state.side),
        Some(PositionSide::Long)
            if matches!(role, SignalRole::LongExit)
                || (role.is_protect() && role.is_long())
                || (role.is_target() && role.is_long())
                || (role.is_entry() && role.is_short())
    ) || matches!(
        position.as_ref().map(|state| state.side),
        Some(PositionSide::Short)
            if matches!(role, SignalRole::ShortExit)
                || (role.is_protect() && role.is_short())
                || (role.is_target() && role.is_short())
                || (role.is_entry() && role.is_long())
    );
    if !should_close {
        return CloseOutcome {
            snapshot: None,
            fully_closed_side: None,
            consumed_target_side: None,
        };
    }

    let current_side = position
        .as_ref()
        .map(|state| state.side)
        .expect("open position should exist");
    let full_close =
        !role.is_target() || size_fraction.unwrap_or(1.0) >= 1.0 - crate::backtest::EPSILON;
    let close_quantity = position
        .as_ref()
        .map(|state| {
            if full_close {
                state.quantity.abs()
            } else {
                state.quantity.abs() * size_fraction.unwrap_or(1.0)
            }
        })
        .expect("open position should exist");
    let closed_position = position
        .as_ref()
        .expect("open position should exist")
        .clone();
    let exit_fill = match accounting {
        AccountingMode::Spot => close_position(
            execution_alias,
            CloseExecution {
                execution: FillExecutionContext {
                    bar_index,
                    time,
                    raw_price,
                    execution_price,
                },
                cash,
                position: &closed_position,
                quantity: Some(close_quantity),
            },
            fee_rate,
        ),
        AccountingMode::PerpIsolated { .. } => {
            let realization = realize_perp_close(
                cash,
                &closed_position,
                execution_price,
                close_quantity,
                fee_rate,
            );
            let notional = close_quantity * execution_price;
            let fee = notional * fee_rate;
            Fill {
                execution_alias: execution_alias.to_string(),
                bar_index,
                time,
                action: realization.action,
                quantity: close_quantity,
                raw_price,
                price: execution_price,
                notional,
                fee,
            }
        }
    };
    let (
        mut trade,
        side,
        entry_order_id,
        entry_module,
        entry_role,
        entry_kind,
        entry_snapshot_value,
        entry_time,
        entry_price,
        mae_price_delta,
        mfe_price_delta,
        bars_held,
    ) = {
        let open_trade = open_trade.as_mut().expect("open trade should exist");
        let trade = close_trade_slice(
            execution_alias,
            open_trade,
            exit_fill.clone(),
            close_quantity,
        );
        let bars_held = exit_fill
            .bar_index
            .saturating_sub(open_trade.entry.bar_index);
        if !full_close {
            open_trade.quantity = (open_trade.quantity - close_quantity).max(0.0);
        }
        (
            trade,
            open_trade.side,
            open_trade.entry_order_id,
            open_trade.entry_module.clone(),
            open_trade.entry_role,
            open_trade.entry_kind,
            open_trade.entry_snapshot.clone(),
            open_trade.entry.time,
            open_trade.entry.price,
            open_trade.mae_price_delta,
            open_trade.mfe_price_delta,
            bars_held,
        )
    };
    let entry_notional = trade.entry.notional.abs();
    let realized_pnl = match accounting {
        AccountingMode::Spot => trade.realized_pnl,
        AccountingMode::PerpIsolated { .. } => {
            let released_margin = if closed_position.quantity.abs() < crate::backtest::EPSILON {
                0.0
            } else {
                closed_position.isolated_margin * (close_quantity / closed_position.quantity.abs())
            };
            let exit_fee = exit_fill.notional * fee_rate;
            let payout = (released_margin
                + match closed_position.side {
                    PositionSide::Long => {
                        (execution_price - closed_position.entry_price) * close_quantity
                    }
                    PositionSide::Short => {
                        (closed_position.entry_price - execution_price) * close_quantity
                    }
                }
                - exit_fee)
                .max(0.0);
            let realized = payout - released_margin - trade.entry.fee;
            trade.realized_pnl = realized;
            realized
        }
    };
    let realized_return = if entry_notional.abs() < crate::backtest::EPSILON {
        0.0
    } else {
        realized_pnl / entry_notional
    };
    *total_realized_pnl += realized_pnl;
    fills.push(exit_fill.clone());
    trade_diagnostics.push(TradeDiagnostic {
        execution_alias: execution_alias.to_string(),
        trade_id: trades.len(),
        side,
        entry_module,
        entry_order_id,
        exit_order_id: order_id,
        entry_role,
        exit_role: role,
        entry_kind,
        exit_kind: order_kind,
        exit_classification: classify_exit(role),
        entry_snapshot: entry_snapshot_value,
        exit_snapshot,
        bars_held,
        duration_ms: exit_fill.time - entry_time,
        realized_pnl,
        mae_price_delta,
        mfe_price_delta,
        mae_pct: pct_delta(mae_price_delta, entry_price),
        mfe_pct: pct_delta(mfe_price_delta, entry_price),
    });
    trades.push(trade);
    let snapshot = LastExitSnapshot {
        kind: exit_kind_for_role(role),
        stage: role.target_stage().or(role.protect_stage()),
        side,
        price: exit_fill.price,
        time: exit_fill.time,
        bar_index: exit_fill.bar_index,
        realized_pnl,
        realized_return,
        bars_held,
    };

    if full_close {
        *position = None;
        *open_trade = None;
        return CloseOutcome {
            snapshot: Some(snapshot),
            fully_closed_side: Some(current_side),
            consumed_target_side: None,
        };
    }

    if let Some(state) = position.as_mut() {
        let remaining_quantity = (state.quantity.abs() - close_quantity).max(0.0);
        state.quantity = match state.side {
            PositionSide::Long => remaining_quantity,
            PositionSide::Short => -remaining_quantity,
        };
        if matches!(accounting, AccountingMode::PerpIsolated { .. }) {
            let release_fraction = if closed_position.quantity.abs() < crate::backtest::EPSILON {
                0.0
            } else {
                close_quantity / closed_position.quantity.abs()
            };
            state.isolated_margin =
                (closed_position.isolated_margin * (1.0 - release_fraction)).max(0.0);
            refresh_position_risk(state, accounting, execution_price);
        }
    }
    crate::backtest::orders::zero_small_cash(cash);
    CloseOutcome {
        snapshot: Some(snapshot),
        fully_closed_side: None,
        consumed_target_side: Some(current_side),
    }
}

fn invalidate_inapplicable_orders(
    active_orders: &mut [Option<ActiveOrder>; ROLE_COUNT],
    position: Option<&PositionState>,
    entry_progress: EntryProgressState,
    orders: &mut [OrderRecord],
) {
    for slot in active_orders.iter_mut() {
        let Some(active) = slot.as_ref() else {
            continue;
        };
        if request_applicable(active.request, position, entry_progress) {
            continue;
        }
        let record_index = active.record_index;
        let role = active.request.role;
        *slot = None;
        update_order_record(
            &mut orders[record_index],
            OrderRecordUpdate {
                trigger_time: None,
                fill_bar_index: None,
                fill_time: None,
                raw_price: None,
                fill_price: None,
                effective_risk_per_unit: None,
                capital_limited: None,
                status: OrderStatus::Cancelled,
                end_reason: Some(if is_attached_exit_role(role) {
                    OrderEndReason::PositionClosed
                } else {
                    OrderEndReason::RoleInvalidated
                }),
            },
        );
    }
}

fn invalidate_stale_attached_orders(
    active_orders: &mut [Option<ActiveOrder>; ROLE_COUNT],
    position: Option<&PositionState>,
    target_consumption: TargetConsumptionState,
    prepared: &PreparedBacktest,
    execution_alias: &str,
    orders: &mut [OrderRecord],
) {
    let Some(position) = position else {
        return;
    };
    let active_protect =
        resolve_active_protect_role(position.side, target_consumption, prepared, execution_alias);
    let active_target =
        resolve_active_target_role(position.side, target_consumption, prepared, execution_alias);

    for slot in active_orders.iter_mut() {
        let Some(active) = slot.as_ref() else {
            continue;
        };
        let role = active.request.role;
        let is_same_side_attached = role.is_attached_exit()
            && ((position.side == PositionSide::Long && role.is_long())
                || (position.side == PositionSide::Short && role.is_short()));
        if !is_same_side_attached {
            continue;
        }
        let should_keep = if role.is_protect() {
            Some(role) == active_protect
        } else if role.is_target() {
            Some(role) == active_target
        } else {
            true
        };
        if should_keep {
            continue;
        }

        let record_index = active.record_index;
        *slot = None;
        update_order_record(
            &mut orders[record_index],
            OrderRecordUpdate {
                trigger_time: None,
                fill_bar_index: None,
                fill_time: None,
                raw_price: None,
                fill_price: None,
                effective_risk_per_unit: None,
                capital_limited: None,
                status: OrderStatus::Cancelled,
                end_reason: Some(OrderEndReason::Rearmed),
            },
        );
    }
}

fn current_side_for_role(role: SignalRole) -> PositionSide {
    if role.is_long() {
        PositionSide::Long
    } else {
        PositionSide::Short
    }
}

fn update_order_record(record: &mut OrderRecord, update: OrderRecordUpdate) {
    if let Some(trigger_time) = update.trigger_time {
        record.trigger_time = Some(trigger_time);
    }
    if let Some(effective_risk_per_unit) = update.effective_risk_per_unit {
        record.effective_risk_per_unit = Some(effective_risk_per_unit);
    }
    if let Some(capital_limited) = update.capital_limited {
        record.capital_limited = capital_limited;
    }
    record.fill_bar_index = update.fill_bar_index;
    record.fill_time = update.fill_time;
    record.raw_price = update.raw_price;
    record.fill_price = update.fill_price;
    record.status = update.status;
    record.end_reason = update.end_reason;
}

fn record_signal_decision(
    trace: Option<&mut StepDecisionTrace>,
    name: &str,
    role: Option<SignalRole>,
    reason: DecisionReason,
) {
    let Some(trace) = trace else {
        return;
    };
    trace.signal_decisions.push(SignalDecisionTrace {
        name: name.to_string(),
        role,
        reason,
    });
}

fn record_order_decision(
    trace: Option<&mut StepDecisionTrace>,
    order_id: Option<usize>,
    role: Option<SignalRole>,
    reason: DecisionReason,
) {
    let Some(trace) = trace else {
        return;
    };
    trace.order_decisions.push(OrderDecisionTrace {
        order_id,
        role,
        reason,
    });
}

fn ensure_no_signal_traces(trace: &mut StepDecisionTrace, prepared: &PreparedBacktest) {
    for role in prepared.signal_roles.values().copied() {
        if trace
            .signal_decisions
            .iter()
            .any(|decision| decision.role == Some(role))
        {
            continue;
        }
        trace.signal_decisions.push(SignalDecisionTrace {
            name: role.canonical_name().to_string(),
            role: Some(role),
            reason: DecisionReason::NoSignal,
        });
    }
}

fn decision_reason_for_order_end(reason: OrderEndReason) -> DecisionReason {
    match reason {
        OrderEndReason::RoleInvalidated => DecisionReason::RoleInvalidated,
        OrderEndReason::MissingPrice
        | OrderEndReason::MissingTriggerPrice
        | OrderEndReason::MissingExpireTime
        | OrderEndReason::MissingSizeFraction
        | OrderEndReason::MissingRiskStopPrice
        | OrderEndReason::InvalidSizeFraction
        | OrderEndReason::InvalidRiskPct
        | OrderEndReason::InvalidRiskDistance => DecisionReason::MissingOrderField,
        OrderEndReason::InsufficientCollateral => DecisionReason::InsufficientCollateral,
        OrderEndReason::VolumeParticipationExceeded | OrderEndReason::PortfolioControlRejected => {
            DecisionReason::VenueRuleRejected
        }
        OrderEndReason::IocUnfilled | OrderEndReason::FokUnfilled => DecisionReason::TifExpired,
        OrderEndReason::PostOnlyWouldCross => DecisionReason::PostOnlyWouldCross,
        OrderEndReason::Replaced
        | OrderEndReason::Rearmed
        | OrderEndReason::OcoCancelled
        | OrderEndReason::PositionClosed => DecisionReason::VenueRuleRejected,
    }
}

fn projected_close_quantity(
    position: Option<&PositionState>,
    role: SignalRole,
    size_fraction: Option<f64>,
) -> Option<f64> {
    let state = position?;
    let should_close = matches!(
        state.side,
        PositionSide::Long
            if matches!(role, SignalRole::LongExit)
                || (role.is_protect() && role.is_long())
                || (role.is_target() && role.is_long())
                || (role.is_entry() && role.is_short())
    ) || matches!(
        state.side,
        PositionSide::Short
            if matches!(role, SignalRole::ShortExit)
                || (role.is_protect() && role.is_short())
                || (role.is_target() && role.is_short())
                || (role.is_entry() && role.is_long())
    );
    if !should_close {
        return None;
    }
    let full_close =
        !role.is_target() || size_fraction.unwrap_or(1.0) >= 1.0 - crate::backtest::EPSILON;
    Some(if full_close {
        state.quantity.abs()
    } else {
        state.quantity.abs() * size_fraction.unwrap_or(1.0)
    })
}

fn fill_exceeds_volume_cap(
    config: &BacktestConfig,
    requested_quantity: Option<f64>,
    bar_volume: f64,
) -> bool {
    let Some(max_fill_pct) = config.max_volume_fill_pct else {
        return false;
    };
    let Some(quantity) = requested_quantity else {
        return false;
    };
    if !quantity.is_finite() || quantity <= crate::backtest::EPSILON {
        return false;
    }
    let max_quantity = bar_volume.max(0.0) * max_fill_pct;
    quantity > max_quantity + crate::backtest::EPSILON
}

fn classify_exit(role: SignalRole) -> TradeExitClassification {
    match role {
        SignalRole::LongEntry
        | SignalRole::LongEntry2
        | SignalRole::LongEntry3
        | SignalRole::ShortEntry
        | SignalRole::ShortEntry2
        | SignalRole::ShortEntry3 => TradeExitClassification::Reversal,
        SignalRole::ProtectLong
        | SignalRole::ProtectAfterTarget1Long
        | SignalRole::ProtectAfterTarget2Long
        | SignalRole::ProtectAfterTarget3Long
        | SignalRole::ProtectShort
        | SignalRole::ProtectAfterTarget1Short
        | SignalRole::ProtectAfterTarget2Short
        | SignalRole::ProtectAfterTarget3Short => TradeExitClassification::Protect,
        SignalRole::TargetLong
        | SignalRole::TargetLong2
        | SignalRole::TargetLong3
        | SignalRole::TargetShort
        | SignalRole::TargetShort2
        | SignalRole::TargetShort3 => TradeExitClassification::Target,
        SignalRole::LongExit | SignalRole::ShortExit => TradeExitClassification::Signal,
    }
}

fn exit_kind_for_role(role: SignalRole) -> ExitKind {
    match classify_exit(role) {
        TradeExitClassification::Signal => ExitKind::Signal,
        TradeExitClassification::Protect => ExitKind::Protect,
        TradeExitClassification::Target => ExitKind::Target,
        TradeExitClassification::Reversal => ExitKind::Reversal,
        TradeExitClassification::Liquidation => ExitKind::Liquidation,
    }
}

fn current_position_snapshot(
    position: Option<&PositionState>,
    execution_alias: &str,
    mark_price: f64,
    market_time: f64,
) -> Option<PositionSnapshot> {
    position.map(|state| PositionSnapshot {
        execution_alias: execution_alias.to_string(),
        side: state.side,
        quantity: state.quantity.abs(),
        entry_bar_index: state.entry_bar_index,
        entry_time: state.entry_time,
        entry_price: state.entry_price,
        market_price: mark_price,
        market_time,
        unrealized_pnl: unrealized_pnl_for_position(state, mark_price),
        free_collateral: None,
        isolated_margin: None,
        maintenance_margin: None,
        liquidation_price: None,
    })
}

fn accounting_mode(config: &BacktestConfig) -> AccountingMode {
    match (config.perp.as_ref(), config.perp_context.as_ref()) {
        (Some(perp), Some(context)) => AccountingMode::PerpIsolated {
            leverage: perp.leverage,
            risk_tiers: risk_tiers(&context.risk_snapshot).to_vec(),
        },
        _ => AccountingMode::Spot,
    }
}

fn accounting_mode_for_alias(
    config: &BacktestConfig,
    alias: &str,
    template: crate::interval::SourceTemplate,
) -> AccountingMode {
    match template {
        crate::interval::SourceTemplate::BinanceSpot
        | crate::interval::SourceTemplate::BybitSpot
        | crate::interval::SourceTemplate::GateSpot => AccountingMode::Spot,
        crate::interval::SourceTemplate::BinanceUsdm
        | crate::interval::SourceTemplate::BybitUsdtPerps
        | crate::interval::SourceTemplate::GateUsdtPerps => {
            let leverage = config.perp.as_ref().map_or(1.0, |perp| perp.leverage);
            let context = config
                .portfolio_perp_contexts
                .get(alias)
                .or(config.perp_context.as_ref())
                .expect("portfolio perp execution requires context");
            AccountingMode::PerpIsolated {
                leverage,
                risk_tiers: risk_tiers(&context.risk_snapshot).to_vec(),
            }
        }
    }
}

fn risk_tiers(snapshot: &VenueRiskSnapshot) -> &[RiskTier] {
    match snapshot {
        VenueRiskSnapshot::BinanceUsdm(snapshot) => &snapshot.brackets,
        VenueRiskSnapshot::BybitUsdtPerps(snapshot) => &snapshot.tiers,
        VenueRiskSnapshot::GateUsdtPerps(snapshot) => &snapshot.tiers,
    }
}

fn aligned_mark_bars(
    config: &BacktestConfig,
    execution_bars: &[Bar],
) -> Result<Vec<Bar>, BacktestError> {
    let Some(context) = config.perp_context.as_ref() else {
        return Ok(execution_bars.to_vec());
    };
    let mut by_time = std::collections::BTreeMap::<i64, Bar>::new();
    for bar in &context.mark_bars {
        by_time.insert(bar.time as i64, *bar);
    }
    let mut aligned = Vec::with_capacity(execution_bars.len());
    for execution_bar in execution_bars {
        let Some(mark_bar) = by_time.get(&(execution_bar.time as i64)).copied() else {
            return Err(BacktestError::MissingPerpMarkFeed {
                alias: config.execution_source_alias.clone(),
            });
        };
        aligned.push(mark_bar);
    }
    Ok(aligned)
}

fn aligned_mark_bars_for_alias(
    config: &BacktestConfig,
    alias: &str,
    template: crate::interval::SourceTemplate,
    execution_bars: &[Bar],
) -> Result<Vec<Bar>, BacktestError> {
    match template {
        crate::interval::SourceTemplate::BinanceSpot
        | crate::interval::SourceTemplate::BybitSpot
        | crate::interval::SourceTemplate::GateSpot => Ok(execution_bars.to_vec()),
        crate::interval::SourceTemplate::BinanceUsdm
        | crate::interval::SourceTemplate::BybitUsdtPerps
        | crate::interval::SourceTemplate::GateUsdtPerps => {
            let context = config
                .portfolio_perp_contexts
                .get(alias)
                .or(config.perp_context.as_ref())
                .ok_or_else(|| BacktestError::MissingPerpContext {
                    alias: alias.to_string(),
                })?;
            let mut by_time = BTreeMap::<i64, Bar>::new();
            for bar in &context.mark_bars {
                by_time.insert(bar.time as i64, *bar);
            }
            let mut aligned = Vec::with_capacity(execution_bars.len());
            for execution_bar in execution_bars {
                let Some(mark_bar) = by_time.get(&(execution_bar.time as i64)).copied() else {
                    return Err(BacktestError::MissingPerpMarkFeed {
                        alias: alias.to_string(),
                    });
                };
                aligned.push(mark_bar);
            }
            Ok(aligned)
        }
    }
}

fn mapped_portfolio_control_kind(
    kind: ProgramPortfolioControlKind,
) -> BacktestPortfolioControlKind {
    match kind {
        ProgramPortfolioControlKind::MaxPositions => BacktestPortfolioControlKind::MaxPositions,
        ProgramPortfolioControlKind::MaxLongPositions => {
            BacktestPortfolioControlKind::MaxLongPositions
        }
        ProgramPortfolioControlKind::MaxShortPositions => {
            BacktestPortfolioControlKind::MaxShortPositions
        }
        ProgramPortfolioControlKind::MaxGrossExposurePct => {
            BacktestPortfolioControlKind::MaxGrossExposurePct
        }
        ProgramPortfolioControlKind::MaxNetExposurePct => {
            BacktestPortfolioControlKind::MaxNetExposurePct
        }
    }
}

fn decision_reason_for_portfolio_control(kind: ProgramPortfolioControlKind) -> DecisionReason {
    match kind {
        ProgramPortfolioControlKind::MaxPositions => DecisionReason::PortfolioMaxPositionsExceeded,
        ProgramPortfolioControlKind::MaxLongPositions => {
            DecisionReason::PortfolioMaxLongPositionsExceeded
        }
        ProgramPortfolioControlKind::MaxShortPositions => {
            DecisionReason::PortfolioMaxShortPositionsExceeded
        }
        ProgramPortfolioControlKind::MaxGrossExposurePct => {
            DecisionReason::PortfolioMaxGrossExposureExceeded
        }
        ProgramPortfolioControlKind::MaxNetExposurePct => {
            DecisionReason::PortfolioMaxNetExposureExceeded
        }
    }
}

fn portfolio_control_value(
    controls: &[PortfolioControlDecl],
    kind: ProgramPortfolioControlKind,
) -> Option<f64> {
    controls
        .iter()
        .find(|decl| decl.kind == kind)
        .map(|decl| decl.value)
}

fn open_position_metrics<'a>(
    states: impl Iterator<Item = &'a PortfolioAliasState>,
) -> (usize, usize, usize, f64, f64, f64, f64) {
    let mut open_position_count = 0usize;
    let mut long_count = 0usize;
    let mut short_count = 0usize;
    let mut gross_notional = 0.0;
    let mut net_notional = 0.0;
    let mut isolated_margin = 0.0;
    let mut unrealized_total = 0.0;
    for state in states {
        let Some(position) = state.position.as_ref() else {
            continue;
        };
        open_position_count += 1;
        match position.side {
            PositionSide::Long => long_count += 1,
            PositionSide::Short => short_count += 1,
        }
        let mark_price = state.last_mark_price.unwrap_or(position.entry_price);
        let signed_notional = position.quantity * mark_price;
        gross_notional += signed_notional.abs();
        net_notional += signed_notional;
        isolated_margin += position.isolated_margin;
        unrealized_total += unrealized_pnl_for_position(position, mark_price);
    }
    (
        open_position_count,
        long_count,
        short_count,
        gross_notional,
        net_notional,
        isolated_margin,
        unrealized_total,
    )
}

struct PortfolioStateWindow<'a> {
    before_current: &'a [PortfolioAliasState],
    current_state: &'a PortfolioAliasState,
    after_current: &'a [PortfolioAliasState],
}

struct PortfolioEntrySizingContext {
    execution_price: f64,
    available_cash: f64,
    portfolio_cash: f64,
    fee_rate: f64,
    size_mode: Option<crate::order::SizeMode>,
    size_value: Option<f64>,
    stop_price: Option<f64>,
}

fn portfolio_entry_block_reason(
    prepared: &PreparedBacktest,
    states: PortfolioStateWindow<'_>,
    next_side: PositionSide,
    sizing_context: PortfolioEntrySizingContext,
) -> Option<(DecisionReason, BacktestPortfolioControlKind)> {
    let sizing = resolve_entry_sizing(
        sizing_context.available_cash,
        EntrySizingSpec {
            size_mode: sizing_context.size_mode,
            size_value: sizing_context.size_value,
            stop_price: sizing_context.stop_price,
        },
        next_side,
        &states.current_state.accounting,
        sizing_context.execution_price,
        sizing_context.fee_rate,
    )
    .ok()?;
    if sizing.quantity <= crate::backtest::EPSILON {
        return None;
    }

    let (
        open_position_count,
        long_count,
        short_count,
        gross_notional,
        net_notional,
        isolated_margin,
        unrealized_total,
    ) = open_position_metrics(
        states
            .before_current
            .iter()
            .chain(std::iter::once(states.current_state))
            .chain(states.after_current.iter()),
    );

    let adds_to_existing_position = states
        .current_state
        .position
        .as_ref()
        .is_some_and(|position| position.side == next_side);
    let additional_notional = sizing.quantity * sizing_context.execution_price;
    let projected_position_count = if adds_to_existing_position {
        open_position_count
    } else {
        open_position_count + 1
    };
    let projected_long_count =
        long_count + usize::from(!adds_to_existing_position && next_side == PositionSide::Long);
    let projected_short_count =
        short_count + usize::from(!adds_to_existing_position && next_side == PositionSide::Short);
    let signed_additional = match next_side {
        PositionSide::Long => additional_notional,
        PositionSide::Short => -additional_notional,
    };
    let projected_gross_notional = gross_notional + additional_notional.abs();
    let projected_net_notional = net_notional + signed_additional;
    let projected_equity = sizing_context.portfolio_cash + isolated_margin + unrealized_total;
    let projected_gross_exposure = if projected_equity.abs() <= crate::backtest::EPSILON {
        f64::INFINITY
    } else {
        projected_gross_notional / projected_equity
    };
    let projected_net_exposure = if projected_equity.abs() <= crate::backtest::EPSILON {
        f64::INFINITY
    } else {
        projected_net_notional.abs() / projected_equity.abs()
    };

    for kind in [
        ProgramPortfolioControlKind::MaxPositions,
        ProgramPortfolioControlKind::MaxLongPositions,
        ProgramPortfolioControlKind::MaxShortPositions,
        ProgramPortfolioControlKind::MaxGrossExposurePct,
        ProgramPortfolioControlKind::MaxNetExposurePct,
    ] {
        let Some(limit) = portfolio_control_value(&prepared.portfolio_controls, kind) else {
            continue;
        };
        let exceeded = match kind {
            ProgramPortfolioControlKind::MaxPositions => projected_position_count as f64 > limit,
            ProgramPortfolioControlKind::MaxLongPositions => projected_long_count as f64 > limit,
            ProgramPortfolioControlKind::MaxShortPositions => projected_short_count as f64 > limit,
            ProgramPortfolioControlKind::MaxGrossExposurePct => projected_gross_exposure > limit,
            ProgramPortfolioControlKind::MaxNetExposurePct => projected_net_exposure > limit,
        };
        if exceeded {
            return Some((
                decision_reason_for_portfolio_control(kind),
                mapped_portfolio_control_kind(kind),
            ));
        }
    }
    None
}

fn increment_portfolio_block_counts(
    blocked_counts: &mut BTreeMap<(BacktestPortfolioControlKind, String, Option<String>), usize>,
    prepared: &PreparedBacktest,
    kind: BacktestPortfolioControlKind,
    alias: &str,
) {
    *blocked_counts
        .entry((kind, alias.to_string(), None))
        .or_default() += 1;
    for group in prepared
        .portfolio_groups
        .iter()
        .filter(|group| group.aliases.iter().any(|member| member == alias))
    {
        *blocked_counts
            .entry((kind, alias.to_string(), Some(group.name.clone())))
            .or_default() += 1;
    }
}

fn extend_portfolio_hints(
    blocked_portfolio_entries: &[PortfolioControlBlockSummary],
    hints: &mut Vec<crate::backtest::ImprovementHint>,
) {
    if blocked_portfolio_entries.is_empty() {
        return;
    }
    hints.push(crate::backtest::ImprovementHint {
        kind: crate::backtest::ImprovementHintKind::PortfolioCapsTooTight,
        metric: Some("blocked_portfolio_entries".to_string()),
        value: Some(
            blocked_portfolio_entries
                .iter()
                .map(|summary| summary.count)
                .sum::<usize>() as f64,
        ),
    });

    let exposure_block_count = blocked_portfolio_entries
        .iter()
        .filter(|summary| {
            matches!(
                summary.kind,
                BacktestPortfolioControlKind::MaxGrossExposurePct
                    | BacktestPortfolioControlKind::MaxNetExposurePct
            )
        })
        .map(|summary| summary.count)
        .sum::<usize>();
    if exposure_block_count > 0 {
        hints.push(crate::backtest::ImprovementHint {
            kind: crate::backtest::ImprovementHintKind::ExposureCapBlocksMajorityOfEntries,
            metric: Some("portfolio_exposure_blocks".to_string()),
            value: Some(exposure_block_count as f64),
        });
    }

    let position_block_count = blocked_portfolio_entries
        .iter()
        .filter(|summary| {
            matches!(
                summary.kind,
                BacktestPortfolioControlKind::MaxPositions
                    | BacktestPortfolioControlKind::MaxLongPositions
                    | BacktestPortfolioControlKind::MaxShortPositions
            )
        })
        .map(|summary| summary.count)
        .sum::<usize>();
    if position_block_count > 0 {
        hints.push(crate::backtest::ImprovementHint {
            kind: crate::backtest::ImprovementHintKind::PositionCountCapBlocksMajorityOfEntries,
            metric: Some("portfolio_position_blocks".to_string()),
            value: Some(position_block_count as f64),
        });
    }

    let long_side_block_count = blocked_portfolio_entries
        .iter()
        .filter(|summary| summary.kind == BacktestPortfolioControlKind::MaxLongPositions)
        .map(|summary| summary.count)
        .sum::<usize>();
    if long_side_block_count > 0 {
        hints.push(crate::backtest::ImprovementHint {
            kind: crate::backtest::ImprovementHintKind::LongSideCapacitySaturated,
            metric: Some("portfolio_long_blocks".to_string()),
            value: Some(long_side_block_count as f64),
        });
    }

    let short_side_block_count = blocked_portfolio_entries
        .iter()
        .filter(|summary| summary.kind == BacktestPortfolioControlKind::MaxShortPositions)
        .map(|summary| summary.count)
        .sum::<usize>();
    if short_side_block_count > 0 {
        hints.push(crate::backtest::ImprovementHint {
            kind: crate::backtest::ImprovementHintKind::ShortSideCapacitySaturated,
            metric: Some("portfolio_short_blocks".to_string()),
            value: Some(short_side_block_count as f64),
        });
    }
}

fn liquidation_signal_role(side: PositionSide) -> SignalRole {
    match side {
        PositionSide::Long => SignalRole::ProtectLong,
        PositionSide::Short => SignalRole::ProtectShort,
    }
}

fn exit_signal_role(side: PositionSide) -> SignalRole {
    match side {
        PositionSide::Long => SignalRole::LongExit,
        PositionSide::Short => SignalRole::ShortExit,
    }
}

#[allow(clippy::too_many_arguments)]
fn maybe_force_time_exit(
    execution_alias: &str,
    controls: &[RiskControlDecl],
    bar_index: usize,
    time: f64,
    execution_price: f64,
    accounting: &AccountingMode,
    fee_rate: f64,
    cash: &mut f64,
    position: &mut Option<PositionState>,
    open_trade: &mut Option<OpenTrade>,
    fills: &mut Vec<Fill>,
    trades: &mut Vec<Trade>,
    trade_diagnostics: &mut Vec<TradeDiagnostic>,
    total_realized_pnl: &mut f64,
    snapshot: Option<FeatureSnapshot>,
    decision_trace: Option<&mut StepDecisionTrace>,
) -> Option<CloseOutcome> {
    let position_side = position.as_ref()?.side;
    let max_bars = risk_control_bars(controls, position_side, RiskControlKind::MaxBarsInTrade)?;
    let bars_held = bar_index.saturating_sub(position.as_ref()?.entry_bar_index);
    if bars_held < max_bars {
        return None;
    }

    record_order_decision(
        decision_trace,
        None,
        Some(exit_signal_role(position_side)),
        DecisionReason::ForcedMaxBarsExit,
    );

    Some(maybe_close_position_for_role(
        execution_alias,
        exit_signal_role(position_side),
        usize::MAX,
        OrderKind::Market,
        None,
        snapshot,
        bar_index,
        time,
        execution_price,
        execution_price,
        accounting,
        fee_rate,
        cash,
        position,
        open_trade,
        fills,
        trades,
        trade_diagnostics,
        total_realized_pnl,
    ))
}

#[allow(clippy::too_many_arguments)]
fn force_liquidation(
    execution_alias: &str,
    side: PositionSide,
    bar_index: usize,
    time: f64,
    execution_price: f64,
    fee_rate: f64,
    cash: &mut f64,
    position: &mut Option<PositionState>,
    open_trade: &mut Option<OpenTrade>,
    fills: &mut Vec<Fill>,
    trades: &mut Vec<Trade>,
    trade_diagnostics: &mut Vec<TradeDiagnostic>,
    total_realized_pnl: &mut f64,
) -> CloseOutcome {
    let closed_position = position
        .as_ref()
        .expect("liquidation requires an open position")
        .clone();
    let quantity = closed_position.quantity.abs();
    let realization =
        realize_perp_close(cash, &closed_position, execution_price, quantity, fee_rate);
    let notional = quantity * execution_price;
    let fee = notional * fee_rate;
    let exit_fill = Fill {
        execution_alias: execution_alias.to_string(),
        bar_index,
        time,
        action: realization.action,
        quantity,
        raw_price: execution_price,
        price: execution_price,
        notional,
        fee,
    };
    let (
        mut trade,
        entry_order_id,
        entry_module,
        entry_role,
        entry_kind,
        entry_snapshot,
        entry_time,
        entry_price,
        mae_price_delta,
        mfe_price_delta,
        bars_held,
    ) = {
        let open_trade = open_trade
            .as_mut()
            .expect("liquidation requires an open trade");
        let trade = close_trade_slice(execution_alias, open_trade, exit_fill.clone(), quantity);
        (
            trade,
            open_trade.entry_order_id,
            open_trade.entry_module.clone(),
            open_trade.entry_role,
            open_trade.entry_kind,
            open_trade.entry_snapshot.clone(),
            open_trade.entry.time,
            open_trade.entry.price,
            open_trade.mae_price_delta,
            open_trade.mfe_price_delta,
            exit_fill
                .bar_index
                .saturating_sub(open_trade.entry.bar_index),
        )
    };
    let entry_notional = trade.entry.notional.abs();
    let realized_pnl = realization.payout - realization.released_margin - trade.entry.fee;
    trade.realized_pnl = realized_pnl;
    let realized_return = if entry_notional.abs() < crate::backtest::EPSILON {
        0.0
    } else {
        realized_pnl / entry_notional
    };
    *total_realized_pnl += realized_pnl;
    fills.push(exit_fill.clone());
    trade_diagnostics.push(TradeDiagnostic {
        execution_alias: execution_alias.to_string(),
        trade_id: trades.len(),
        side,
        entry_module,
        entry_order_id,
        exit_order_id: usize::MAX,
        entry_role,
        exit_role: liquidation_signal_role(side),
        entry_kind,
        exit_kind: OrderKind::Market,
        exit_classification: TradeExitClassification::Liquidation,
        entry_snapshot,
        exit_snapshot: None,
        bars_held,
        duration_ms: exit_fill.time - entry_time,
        realized_pnl,
        mae_price_delta,
        mfe_price_delta,
        mae_pct: pct_delta(mae_price_delta, entry_price),
        mfe_pct: pct_delta(mfe_price_delta, entry_price),
    });
    trades.push(trade);
    *position = None;
    *open_trade = None;
    CloseOutcome {
        snapshot: Some(LastExitSnapshot {
            kind: ExitKind::Liquidation,
            stage: None,
            side,
            price: exit_fill.price,
            time: exit_fill.time,
            bar_index: exit_fill.bar_index,
            realized_pnl,
            realized_return,
            bars_held,
        }),
        fully_closed_side: Some(side),
        consumed_target_side: None,
    }
}

fn pct_delta(delta: f64, price: f64) -> f64 {
    if price.abs() < crate::backtest::EPSILON {
        0.0
    } else {
        delta / price
    }
}
