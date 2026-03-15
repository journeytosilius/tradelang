use crate::backtest::{
    FeatureSnapshot, Fill, FillAction, OrderEndReason, OrderRecord, OrderStatus, PositionSide,
    EPSILON,
};
use crate::bytecode::SignalRole;
use crate::exchange::RiskTier;
use crate::order::{OrderKind, SizeMode, TimeInForce, TriggerReference};

pub(crate) const ROLE_COUNT: usize = 22;
pub(crate) const ROLE_PRIORITY: [SignalRole; ROLE_COUNT] = [
    SignalRole::ProtectLong,
    SignalRole::ProtectAfterTarget1Long,
    SignalRole::ProtectAfterTarget2Long,
    SignalRole::ProtectAfterTarget3Long,
    SignalRole::ProtectShort,
    SignalRole::ProtectAfterTarget1Short,
    SignalRole::ProtectAfterTarget2Short,
    SignalRole::ProtectAfterTarget3Short,
    SignalRole::TargetLong,
    SignalRole::TargetLong2,
    SignalRole::TargetLong3,
    SignalRole::TargetShort,
    SignalRole::TargetShort2,
    SignalRole::TargetShort3,
    SignalRole::LongExit,
    SignalRole::ShortExit,
    SignalRole::LongEntry,
    SignalRole::LongEntry2,
    SignalRole::LongEntry3,
    SignalRole::ShortEntry,
    SignalRole::ShortEntry2,
    SignalRole::ShortEntry3,
];

#[derive(Clone, Debug)]
pub(crate) enum AccountingMode {
    Spot,
    PerpIsolated {
        leverage: f64,
        risk_tiers: Vec<RiskTier>,
    },
}

impl AccountingMode {
    pub(crate) const fn is_perp(&self) -> bool {
        matches!(self, Self::PerpIsolated { .. })
    }

    fn risk_tiers(&self) -> &[RiskTier] {
        match self {
            Self::Spot => &[],
            Self::PerpIsolated { risk_tiers, .. } => risk_tiers,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CapturedOrderRequest {
    pub role: SignalRole,
    pub kind: OrderKind,
    pub tif: Option<TimeInForce>,
    pub post_only: bool,
    pub trigger_ref: Option<TriggerReference>,
    pub size_mode: Option<SizeMode>,
    pub price: Option<f64>,
    pub trigger_price: Option<f64>,
    pub expire_time: Option<f64>,
    pub has_size_field: bool,
    pub size_value: Option<f64>,
    pub size_stop_price: Option<f64>,
    pub signal_time: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum WorkingState {
    Ready,
    RestingLimit { active_after_time: f64 },
}

#[derive(Clone, Debug)]
pub(crate) struct ActiveOrder {
    pub request: CapturedOrderRequest,
    pub record_index: usize,
    pub first_eval_done: bool,
    pub state: WorkingState,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FillExecution {
    pub raw_price: f64,
    pub price: f64,
    pub trigger_time: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Evaluation {
    None,
    Fill(FillExecution),
    Cancel(OrderEndReason),
    Expire,
    MoveToRestingLimit {
        active_after_time: f64,
        trigger_time: f64,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct PositionState {
    pub side: PositionSide,
    pub quantity: f64,
    pub entry_bar_index: usize,
    pub entry_time: f64,
    pub entry_price: f64,
    pub isolated_margin: f64,
    pub maintenance_margin: f64,
    pub liquidation_price: Option<f64>,
}

#[derive(Clone, Debug)]
pub(crate) struct OpenTrade {
    pub side: PositionSide,
    pub quantity: f64,
    pub entry: Fill,
    pub entry_order_id: usize,
    pub entry_role: SignalRole,
    pub entry_module: Option<String>,
    pub entry_kind: OrderKind,
    pub entry_snapshot: Option<FeatureSnapshot>,
    pub mae_price_delta: f64,
    pub mfe_price_delta: f64,
    pub remaining_entry_fee: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EntryProgressState {
    pub long_stage: u8,
    pub short_stage: u8,
}

#[derive(Clone, Debug)]
pub(crate) struct TradeEntryContext {
    pub order_id: usize,
    pub role: SignalRole,
    pub module: Option<String>,
    pub kind: OrderKind,
    pub snapshot: Option<FeatureSnapshot>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FillExecutionContext {
    pub bar_index: usize,
    pub time: f64,
    pub raw_price: f64,
    pub execution_price: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SizingResolution {
    pub quantity: f64,
    pub capital_limited: bool,
    pub effective_risk_per_unit: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EntrySizingSpec {
    pub size_mode: Option<SizeMode>,
    pub size_value: Option<f64>,
    pub stop_price: Option<f64>,
}

pub(crate) struct PerpCloseRealization {
    pub action: FillAction,
    pub released_margin: f64,
    pub payout: f64,
}

pub(crate) struct CloseExecution<'a> {
    pub execution: FillExecutionContext,
    pub cash: &'a mut f64,
    pub position: &'a PositionState,
    pub quantity: Option<f64>,
}

pub(crate) fn empty_request_slots() -> [Option<CapturedOrderRequest>; ROLE_COUNT] {
    std::array::from_fn(|_| None)
}

pub(crate) fn role_index(role: SignalRole) -> usize {
    match role {
        SignalRole::LongEntry => 0,
        SignalRole::LongEntry2 => 1,
        SignalRole::LongEntry3 => 2,
        SignalRole::LongExit => 3,
        SignalRole::ShortEntry => 4,
        SignalRole::ShortEntry2 => 5,
        SignalRole::ShortEntry3 => 6,
        SignalRole::ShortExit => 7,
        SignalRole::ProtectLong => 8,
        SignalRole::ProtectAfterTarget1Long => 9,
        SignalRole::ProtectAfterTarget2Long => 10,
        SignalRole::ProtectAfterTarget3Long => 11,
        SignalRole::ProtectShort => 12,
        SignalRole::ProtectAfterTarget1Short => 13,
        SignalRole::ProtectAfterTarget2Short => 14,
        SignalRole::ProtectAfterTarget3Short => 15,
        SignalRole::TargetLong => 16,
        SignalRole::TargetLong2 => 17,
        SignalRole::TargetLong3 => 18,
        SignalRole::TargetShort => 19,
        SignalRole::TargetShort2 => 20,
        SignalRole::TargetShort3 => 21,
    }
}

pub(crate) fn fill_action_for_role(role: SignalRole) -> FillAction {
    match role {
        SignalRole::LongEntry | SignalRole::LongEntry2 | SignalRole::LongEntry3 => FillAction::Buy,
        SignalRole::ShortEntry | SignalRole::ShortEntry2 | SignalRole::ShortEntry3 => {
            FillAction::Sell
        }
        SignalRole::ProtectLong
        | SignalRole::ProtectAfterTarget1Long
        | SignalRole::ProtectAfterTarget2Long
        | SignalRole::ProtectAfterTarget3Long
        | SignalRole::TargetLong
        | SignalRole::TargetLong2
        | SignalRole::TargetLong3
        | SignalRole::LongExit => FillAction::Sell,
        SignalRole::ProtectShort
        | SignalRole::ProtectAfterTarget1Short
        | SignalRole::ProtectAfterTarget2Short
        | SignalRole::ProtectAfterTarget3Short
        | SignalRole::TargetShort
        | SignalRole::TargetShort2
        | SignalRole::TargetShort3
        | SignalRole::ShortExit => FillAction::Buy,
    }
}

pub(crate) fn position_side_for_entry(role: SignalRole) -> Option<PositionSide> {
    match role {
        SignalRole::LongEntry | SignalRole::LongEntry2 | SignalRole::LongEntry3 => {
            Some(PositionSide::Long)
        }
        SignalRole::ShortEntry | SignalRole::ShortEntry2 | SignalRole::ShortEntry3 => {
            Some(PositionSide::Short)
        }
        _ => None,
    }
}

pub(crate) fn request_applicable(
    request: CapturedOrderRequest,
    position: Option<&PositionState>,
    entry_progress: EntryProgressState,
) -> bool {
    match position.map(|state| state.side) {
        None => request.role.is_entry() && request.role.entry_stage() == Some(1),
        Some(PositionSide::Long) => {
            matches!(request.role, SignalRole::LongExit)
                || (request.role.is_protect() && request.role.is_long())
                || (request.role.is_target() && request.role.is_long())
                || (request.role.is_entry()
                    && request.role.is_long()
                    && (request.role.entry_stage() == Some(1)
                        || request.role.entry_stage() == Some(entry_progress.long_stage + 1))
                    && request.has_size_field)
                || matches!(request.role, SignalRole::ShortEntry)
        }
        Some(PositionSide::Short) => {
            matches!(request.role, SignalRole::ShortExit)
                || (request.role.is_protect() && request.role.is_short())
                || (request.role.is_target() && request.role.is_short())
                || (request.role.is_entry()
                    && request.role.is_short()
                    && (request.role.entry_stage() == Some(1)
                        || request.role.entry_stage() == Some(entry_progress.short_stage + 1))
                    && request.has_size_field)
                || matches!(request.role, SignalRole::LongEntry)
        }
    }
}

pub(crate) const fn is_attached_exit_role(role: SignalRole) -> bool {
    role.is_attached_exit()
}

pub(crate) fn order_record(
    execution_alias: &str,
    request: CapturedOrderRequest,
    bar_index: usize,
    time: f64,
    id: usize,
) -> OrderRecord {
    OrderRecord {
        id,
        execution_alias: execution_alias.to_string(),
        role: request.role,
        kind: request.kind,
        action: fill_action_for_role(request.role),
        tif: request.tif,
        post_only: request.post_only,
        trigger_ref: request.trigger_ref,
        signal_time: request.signal_time,
        placed_bar_index: bar_index,
        placed_time: time,
        trigger_time: None,
        fill_bar_index: None,
        fill_time: None,
        raw_price: None,
        fill_price: None,
        limit_price: request.price,
        trigger_price: request.trigger_price,
        expire_time: request.expire_time,
        size_mode: request.size_mode,
        size_fraction: match request.size_mode {
            Some(SizeMode::CapitalFraction) => request.size_value,
            _ => None,
        },
        requested_risk_pct: match request.size_mode {
            Some(SizeMode::RiskPct) => request.size_value,
            _ => None,
        },
        requested_stop_price: match request.size_mode {
            Some(SizeMode::RiskPct) => request.size_stop_price,
            _ => None,
        },
        effective_risk_per_unit: None,
        capital_limited: false,
        status: OrderStatus::Open,
        end_reason: None,
    }
}

pub(crate) fn missing_field_reason(request: CapturedOrderRequest) -> Option<OrderEndReason> {
    match request.kind {
        OrderKind::Market => None,
        OrderKind::Limit if request.price.is_none() => Some(OrderEndReason::MissingPrice),
        OrderKind::StopMarket | OrderKind::TakeProfitMarket if request.trigger_price.is_none() => {
            Some(OrderEndReason::MissingTriggerPrice)
        }
        OrderKind::StopLimit | OrderKind::TakeProfitLimit if request.trigger_price.is_none() => {
            Some(OrderEndReason::MissingTriggerPrice)
        }
        OrderKind::StopLimit | OrderKind::TakeProfitLimit if request.price.is_none() => {
            Some(OrderEndReason::MissingPrice)
        }
        _ if matches!(request.tif, Some(TimeInForce::Gtd)) && request.expire_time.is_none() => {
            Some(OrderEndReason::MissingExpireTime)
        }
        _ if request.has_size_field && request.size_value.is_none() => {
            Some(match request.size_mode {
                Some(SizeMode::RiskPct) => OrderEndReason::InvalidRiskPct,
                _ => OrderEndReason::MissingSizeFraction,
            })
        }
        _ if matches!(request.size_mode, Some(SizeMode::RiskPct))
            && request.size_stop_price.is_none() =>
        {
            Some(OrderEndReason::MissingRiskStopPrice)
        }
        _ if request.size_value.is_some_and(|value| !value.is_finite()) => {
            Some(match request.size_mode {
                Some(SizeMode::RiskPct) => OrderEndReason::InvalidRiskPct,
                _ => OrderEndReason::InvalidSizeFraction,
            })
        }
        _ if matches!(request.size_mode, Some(SizeMode::RiskPct))
            && request.size_value.is_some_and(|value| value <= 0.0) =>
        {
            Some(OrderEndReason::InvalidRiskPct)
        }
        _ if !matches!(request.size_mode, Some(SizeMode::RiskPct))
            && request
                .size_value
                .is_some_and(|value| value <= 0.0 || value > 1.0) =>
        {
            Some(OrderEndReason::InvalidSizeFraction)
        }
        _ if request
            .size_stop_price
            .is_some_and(|value| !value.is_finite()) =>
        {
            Some(OrderEndReason::MissingRiskStopPrice)
        }
        _ => None,
    }
}

pub(crate) fn evaluate_active_order(
    order: &ActiveOrder,
    bar_time: f64,
    bar_open: f64,
    bar_high: f64,
    bar_low: f64,
) -> Evaluation {
    if matches!(order.request.tif, Some(TimeInForce::Gtd))
        && order
            .request
            .expire_time
            .is_some_and(|expire| bar_time >= expire)
    {
        return Evaluation::Expire;
    }

    let action = fill_action_for_role(order.request.role);
    if let WorkingState::RestingLimit { active_after_time } = order.state {
        if bar_time <= active_after_time {
            return Evaluation::None;
        }
        return evaluate_limit(order.request, action, bar_open, bar_high, bar_low);
    }

    let tif = order.request.tif;
    let first_eval = !order.first_eval_done;

    let evaluation = match order.request.kind {
        OrderKind::Market => Evaluation::Fill(FillExecution {
            raw_price: bar_open,
            price: bar_open,
            trigger_time: None,
        }),
        OrderKind::Limit => evaluate_limit(order.request, action, bar_open, bar_high, bar_low),
        OrderKind::StopMarket => evaluate_trigger_market(
            order.request,
            true,
            action,
            bar_time,
            bar_open,
            bar_high,
            bar_low,
        ),
        OrderKind::TakeProfitMarket => evaluate_trigger_market(
            order.request,
            false,
            action,
            bar_time,
            bar_open,
            bar_high,
            bar_low,
        ),
        OrderKind::StopLimit => evaluate_trigger_limit(
            order.request,
            true,
            action,
            bar_time,
            bar_open,
            bar_high,
            bar_low,
        ),
        OrderKind::TakeProfitLimit => evaluate_trigger_limit(
            order.request,
            false,
            action,
            bar_time,
            bar_open,
            bar_high,
            bar_low,
        ),
    };

    match (tif, first_eval, evaluation) {
        (Some(TimeInForce::Ioc), true, Evaluation::None)
        | (Some(TimeInForce::Ioc), true, Evaluation::MoveToRestingLimit { .. }) => {
            Evaluation::Cancel(OrderEndReason::IocUnfilled)
        }
        (Some(TimeInForce::Fok), true, Evaluation::None)
        | (Some(TimeInForce::Fok), true, Evaluation::MoveToRestingLimit { .. }) => {
            Evaluation::Cancel(OrderEndReason::FokUnfilled)
        }
        _ => evaluation,
    }
}

fn evaluate_limit(
    request: CapturedOrderRequest,
    action: FillAction,
    bar_open: f64,
    bar_high: f64,
    bar_low: f64,
) -> Evaluation {
    let Some(limit_price) = request.price else {
        return Evaluation::None;
    };
    if request.post_only && would_cross_on_open(action, bar_open, limit_price) {
        return Evaluation::Cancel(OrderEndReason::PostOnlyWouldCross);
    }
    if limit_touched(action, bar_high, bar_low, limit_price) {
        Evaluation::Fill(FillExecution {
            raw_price: better_of_open_and_limit(action, bar_open, limit_price),
            price: better_of_open_and_limit(action, bar_open, limit_price),
            trigger_time: None,
        })
    } else {
        Evaluation::None
    }
}

fn evaluate_trigger_market(
    request: CapturedOrderRequest,
    stop_style: bool,
    action: FillAction,
    bar_time: f64,
    bar_open: f64,
    bar_high: f64,
    bar_low: f64,
) -> Evaluation {
    let Some(trigger_price) = request.trigger_price else {
        return Evaluation::None;
    };
    if trigger_hit(stop_style, action, bar_high, bar_low, trigger_price) {
        let raw_price = if stop_style {
            worse_of_open_and_trigger(action, bar_open, trigger_price)
        } else {
            better_of_open_and_trigger(action, bar_open, trigger_price)
        };
        Evaluation::Fill(FillExecution {
            raw_price,
            price: raw_price,
            trigger_time: Some(bar_time),
        })
    } else {
        Evaluation::None
    }
}

fn evaluate_trigger_limit(
    request: CapturedOrderRequest,
    stop_style: bool,
    action: FillAction,
    bar_time: f64,
    bar_open: f64,
    bar_high: f64,
    bar_low: f64,
) -> Evaluation {
    let Some(trigger_price) = request.trigger_price else {
        return Evaluation::None;
    };
    let Some(limit_price) = request.price else {
        return Evaluation::None;
    };
    if !trigger_hit(stop_style, action, bar_high, bar_low, trigger_price) {
        return Evaluation::None;
    }
    if request.post_only && would_cross_on_open(action, bar_open, limit_price) {
        return Evaluation::Cancel(OrderEndReason::PostOnlyWouldCross);
    }
    if limit_touched(action, bar_high, bar_low, limit_price)
        && would_cross_on_open(action, bar_open, limit_price)
    {
        let raw_price = better_of_open_and_limit(action, bar_open, limit_price);
        return Evaluation::Fill(FillExecution {
            raw_price,
            price: raw_price,
            trigger_time: Some(bar_time),
        });
    }
    Evaluation::MoveToRestingLimit {
        active_after_time: bar_time,
        trigger_time: bar_time,
    }
}

fn limit_touched(action: FillAction, high: f64, low: f64, limit_price: f64) -> bool {
    match action {
        FillAction::Buy => low <= limit_price,
        FillAction::Sell => high >= limit_price,
    }
}

fn trigger_hit(
    stop_style: bool,
    action: FillAction,
    high: f64,
    low: f64,
    trigger_price: f64,
) -> bool {
    match (stop_style, action) {
        (true, FillAction::Buy) => high >= trigger_price,
        (true, FillAction::Sell) => low <= trigger_price,
        (false, FillAction::Buy) => low <= trigger_price,
        (false, FillAction::Sell) => high >= trigger_price,
    }
}

pub(crate) fn would_cross_on_open(action: FillAction, open: f64, limit_price: f64) -> bool {
    match action {
        FillAction::Buy => open <= limit_price,
        FillAction::Sell => open >= limit_price,
    }
}

fn better_of_open_and_limit(action: FillAction, open: f64, limit_price: f64) -> f64 {
    match action {
        FillAction::Buy => open.min(limit_price),
        FillAction::Sell => open.max(limit_price),
    }
}

fn worse_of_open_and_trigger(action: FillAction, open: f64, trigger_price: f64) -> f64 {
    match action {
        FillAction::Buy => open.max(trigger_price),
        FillAction::Sell => open.min(trigger_price),
    }
}

fn better_of_open_and_trigger(action: FillAction, open: f64, trigger_price: f64) -> f64 {
    match action {
        FillAction::Buy => open.min(trigger_price),
        FillAction::Sell => open.max(trigger_price),
    }
}

pub(crate) fn adjusted_price(raw_price: f64, action: FillAction, slippage_rate: f64) -> f64 {
    match action {
        FillAction::Buy => raw_price * (1.0 + slippage_rate),
        FillAction::Sell => raw_price * (1.0 - slippage_rate),
    }
}

pub(crate) fn resolve_entry_sizing(
    cash: f64,
    sizing: EntrySizingSpec,
    side: PositionSide,
    accounting: &AccountingMode,
    execution_price: f64,
    fee_rate: f64,
) -> Result<SizingResolution, OrderEndReason> {
    if execution_price <= EPSILON {
        return Ok(SizingResolution::default());
    }
    let max_quantity =
        capital_limited_entry_quantity(cash, accounting, execution_price, fee_rate, 1.0);
    if max_quantity <= EPSILON {
        return Ok(SizingResolution::default());
    }
    match sizing.size_mode.unwrap_or(SizeMode::CapitalFraction) {
        SizeMode::CapitalFraction => {
            let fraction = sizing.size_value.unwrap_or(1.0);
            if !fraction.is_finite() || fraction <= 0.0 || fraction > 1.0 {
                return Err(OrderEndReason::InvalidSizeFraction);
            }
            let quantity = capital_limited_entry_quantity(
                cash,
                accounting,
                execution_price,
                fee_rate,
                fraction,
            );
            Ok(SizingResolution {
                quantity,
                capital_limited: false,
                effective_risk_per_unit: None,
            })
        }
        SizeMode::RiskPct => {
            let pct = sizing.size_value.ok_or(OrderEndReason::InvalidRiskPct)?;
            let stop_price = sizing
                .stop_price
                .ok_or(OrderEndReason::MissingRiskStopPrice)?;
            if !pct.is_finite() || pct <= 0.0 {
                return Err(OrderEndReason::InvalidRiskPct);
            }
            let risk_per_unit = match side {
                PositionSide::Long => execution_price - stop_price,
                PositionSide::Short => stop_price - execution_price,
            };
            if !risk_per_unit.is_finite() || risk_per_unit <= EPSILON {
                return Err(OrderEndReason::InvalidRiskDistance);
            }
            let risk_budget = cash.max(0.0) * pct;
            if risk_budget <= EPSILON {
                return Ok(SizingResolution {
                    quantity: 0.0,
                    capital_limited: false,
                    effective_risk_per_unit: Some(risk_per_unit),
                });
            }
            let desired_quantity = risk_budget / risk_per_unit;
            let quantity = desired_quantity.min(max_quantity);
            Ok(SizingResolution {
                quantity,
                capital_limited: quantity + EPSILON < desired_quantity,
                effective_risk_per_unit: Some(risk_per_unit),
            })
        }
    }
}

fn capital_limited_entry_quantity(
    cash: f64,
    accounting: &AccountingMode,
    execution_price: f64,
    fee_rate: f64,
    capital_fraction: f64,
) -> f64 {
    let capital = cash.max(0.0) * capital_fraction;
    if capital <= EPSILON || execution_price <= EPSILON {
        return 0.0;
    }
    match accounting {
        AccountingMode::Spot => capital / (execution_price * (1.0 + fee_rate)),
        AccountingMode::PerpIsolated { leverage, .. } => {
            capital / (execution_price * ((1.0 / leverage) + fee_rate))
        }
    }
}

pub(crate) struct PositionFillContext<'a> {
    pub execution_alias: &'a str,
    pub execution: FillExecutionContext,
    pub accounting: &'a AccountingMode,
    pub fee_rate: f64,
}

pub(crate) fn open_position(
    context: PositionFillContext<'_>,
    side: PositionSide,
    entry_context: TradeEntryContext,
    sizing: EntrySizingSpec,
    cash: &mut f64,
) -> Result<(PositionState, OpenTrade, Fill, SizingResolution), OrderEndReason> {
    let action = match side {
        PositionSide::Long => FillAction::Buy,
        PositionSide::Short => FillAction::Sell,
    };
    let sizing = resolve_entry_sizing(
        *cash,
        sizing,
        side,
        context.accounting,
        context.execution.execution_price,
        context.fee_rate,
    )?;
    let quantity = sizing.quantity;
    let notional = quantity * context.execution.execution_price;
    let fee = notional * context.fee_rate;
    let isolated_margin = match context.accounting {
        AccountingMode::Spot => 0.0,
        AccountingMode::PerpIsolated { leverage, .. } => notional / leverage,
    };
    match context.accounting {
        AccountingMode::Spot => match side {
            PositionSide::Long => *cash -= notional + fee,
            PositionSide::Short => *cash += notional - fee,
        },
        AccountingMode::PerpIsolated { .. } => {
            *cash -= isolated_margin + fee;
        }
    }
    zero_small_cash(cash);
    let signed_quantity = match side {
        PositionSide::Long => quantity,
        PositionSide::Short => -quantity,
    };
    let fill = Fill {
        execution_alias: context.execution_alias.to_string(),
        bar_index: context.execution.bar_index,
        time: context.execution.time,
        action,
        quantity,
        raw_price: context.execution.raw_price,
        price: context.execution.execution_price,
        notional,
        fee,
    };
    let position = PositionState {
        side,
        quantity: signed_quantity,
        entry_bar_index: context.execution.bar_index,
        entry_time: context.execution.time,
        entry_price: context.execution.execution_price,
        isolated_margin,
        maintenance_margin: 0.0,
        liquidation_price: None,
    };
    let trade = OpenTrade {
        side,
        quantity,
        entry: fill.clone(),
        entry_order_id: entry_context.order_id,
        entry_role: entry_context.role,
        entry_module: entry_context.module,
        entry_kind: entry_context.kind,
        entry_snapshot: entry_context.snapshot,
        mae_price_delta: 0.0,
        mfe_price_delta: 0.0,
        remaining_entry_fee: fill.fee,
    };
    Ok((position, trade, fill, sizing))
}

pub(crate) fn add_to_position(
    context: PositionFillContext<'_>,
    position: &mut PositionState,
    open_trade: &mut OpenTrade,
    sizing: EntrySizingSpec,
    cash: &mut f64,
) -> Result<(Fill, SizingResolution), OrderEndReason> {
    let action = match position.side {
        PositionSide::Long => FillAction::Buy,
        PositionSide::Short => FillAction::Sell,
    };
    let sizing = resolve_entry_sizing(
        *cash,
        sizing,
        position.side,
        context.accounting,
        context.execution.execution_price,
        context.fee_rate,
    )?;
    let quantity = sizing.quantity;
    let notional = quantity * context.execution.execution_price;
    let fee = notional * context.fee_rate;
    let isolated_margin = match context.accounting {
        AccountingMode::Spot => 0.0,
        AccountingMode::PerpIsolated { leverage, .. } => notional / leverage,
    };
    match context.accounting {
        AccountingMode::Spot => match position.side {
            PositionSide::Long => *cash -= notional + fee,
            PositionSide::Short => *cash += notional - fee,
        },
        AccountingMode::PerpIsolated { .. } => {
            *cash -= isolated_margin + fee;
        }
    }
    zero_small_cash(cash);

    let fill = Fill {
        execution_alias: context.execution_alias.to_string(),
        bar_index: context.execution.bar_index,
        time: context.execution.time,
        action,
        quantity,
        raw_price: context.execution.raw_price,
        price: context.execution.execution_price,
        notional,
        fee,
    };

    let previous_quantity = position.quantity.abs();
    let next_quantity = previous_quantity + quantity;
    let weighted_entry_price = if next_quantity <= EPSILON {
        context.execution.execution_price
    } else {
        ((position.entry_price * previous_quantity)
            + (context.execution.execution_price * quantity))
            / next_quantity
    };
    position.entry_price = weighted_entry_price;
    position.quantity = match position.side {
        PositionSide::Long => next_quantity,
        PositionSide::Short => -next_quantity,
    };
    position.isolated_margin += isolated_margin;

    let weighted_raw_price = if next_quantity <= EPSILON {
        context.execution.raw_price
    } else {
        ((open_trade.entry.raw_price * previous_quantity)
            + (context.execution.raw_price * quantity))
            / next_quantity
    };
    open_trade.quantity = next_quantity;
    open_trade.entry.price = weighted_entry_price;
    open_trade.entry.raw_price = weighted_raw_price;
    open_trade.entry.quantity = next_quantity;
    open_trade.entry.notional = next_quantity * weighted_entry_price;
    open_trade.entry.fee += fee;
    open_trade.remaining_entry_fee += fee;

    Ok((fill, sizing))
}

pub(crate) fn close_position(
    execution_alias: &str,
    close: CloseExecution<'_>,
    fee_rate: f64,
) -> Fill {
    let CloseExecution {
        execution,
        cash,
        position,
        quantity,
    } = close;
    let action = match position.side {
        PositionSide::Long => FillAction::Sell,
        PositionSide::Short => FillAction::Buy,
    };
    let quantity = quantity.unwrap_or_else(|| position.quantity.abs());
    let notional = quantity * execution.execution_price;
    let fee = notional * fee_rate;
    match position.side {
        PositionSide::Long => *cash += notional - fee,
        PositionSide::Short => *cash -= notional + fee,
    }
    zero_small_cash(cash);
    Fill {
        execution_alias: execution_alias.to_string(),
        bar_index: execution.bar_index,
        time: execution.time,
        action,
        quantity,
        raw_price: execution.raw_price,
        price: execution.execution_price,
        notional,
        fee,
    }
}

pub(crate) fn close_trade_slice(
    execution_alias: &str,
    open_trade: &mut OpenTrade,
    exit: Fill,
    quantity: f64,
) -> crate::backtest::Trade {
    let remaining_before_close = open_trade.quantity.max(EPSILON);
    let entry_fee = if (quantity - remaining_before_close).abs() < EPSILON {
        open_trade.remaining_entry_fee
    } else {
        open_trade.remaining_entry_fee * (quantity / remaining_before_close)
    };
    open_trade.remaining_entry_fee -= entry_fee;
    let mut entry = open_trade.entry.clone();
    entry.quantity = quantity;
    entry.notional = quantity * entry.price;
    entry.fee = entry_fee;
    let signed_entry_price = match open_trade.side {
        PositionSide::Long => -entry.price,
        PositionSide::Short => entry.price,
    };
    let signed_exit_price = match open_trade.side {
        PositionSide::Long => exit.price,
        PositionSide::Short => -exit.price,
    };
    let realized_pnl = (signed_entry_price + signed_exit_price) * quantity - entry.fee - exit.fee;
    crate::backtest::Trade {
        execution_alias: execution_alias.to_string(),
        side: open_trade.side,
        entry_module: open_trade.entry_module.clone(),
        quantity,
        entry,
        exit,
        realized_pnl,
    }
}

pub(crate) fn update_open_trade_excursions(
    open_trade: &mut OpenTrade,
    bar_high: f64,
    bar_low: f64,
) {
    match open_trade.side {
        PositionSide::Long => {
            let favorable = bar_high - open_trade.entry.price;
            let adverse = bar_low - open_trade.entry.price;
            open_trade.mfe_price_delta = open_trade.mfe_price_delta.max(favorable);
            open_trade.mae_price_delta = open_trade.mae_price_delta.min(adverse);
        }
        PositionSide::Short => {
            let favorable = open_trade.entry.price - bar_low;
            let adverse = open_trade.entry.price - bar_high;
            open_trade.mfe_price_delta = open_trade.mfe_price_delta.max(favorable);
            open_trade.mae_price_delta = open_trade.mae_price_delta.min(adverse);
        }
    }
}

pub(crate) fn unrealized_pnl_for_position(position: &PositionState, mark_price: f64) -> f64 {
    match position.side {
        PositionSide::Long => (mark_price - position.entry_price) * position.quantity.abs(),
        PositionSide::Short => (position.entry_price - mark_price) * position.quantity.abs(),
    }
}

pub(crate) fn realize_perp_close(
    cash: &mut f64,
    position: &PositionState,
    execution_price: f64,
    quantity: f64,
    fee_rate: f64,
) -> PerpCloseRealization {
    let price_pnl = match position.side {
        PositionSide::Long => (execution_price - position.entry_price) * quantity,
        PositionSide::Short => (position.entry_price - execution_price) * quantity,
    };
    let released_margin = if position.quantity.abs() <= EPSILON {
        0.0
    } else {
        position.isolated_margin * (quantity / position.quantity.abs())
    };
    let fee = execution_price * quantity * fee_rate;
    let payout = (released_margin + price_pnl - fee).max(0.0);
    *cash += payout;
    zero_small_cash(cash);
    let action = match position.side {
        PositionSide::Long => FillAction::Sell,
        PositionSide::Short => FillAction::Buy,
    };
    PerpCloseRealization {
        action,
        released_margin,
        payout,
    }
}

pub(crate) fn refresh_position_risk(
    position: &mut PositionState,
    accounting: &AccountingMode,
    mark_price: f64,
) {
    if !accounting.is_perp() {
        position.maintenance_margin = 0.0;
        position.liquidation_price = None;
        return;
    }
    let quantity = position.quantity.abs();
    if quantity <= EPSILON {
        position.maintenance_margin = 0.0;
        position.liquidation_price = None;
        return;
    }
    let notional = quantity * mark_price;
    let tier = risk_tier_for_notional(accounting.risk_tiers(), notional);
    let maintenance_margin = maintenance_margin_for_tier(notional, tier);
    position.maintenance_margin = maintenance_margin;
    position.liquidation_price = liquidation_price_for_tier(position, tier);
}

pub(crate) fn liquidation_trigger_price(
    position: &PositionState,
    mark_open: f64,
    mark_high: f64,
    mark_low: f64,
) -> Option<f64> {
    let liquidation_price = position.liquidation_price?;
    match position.side {
        PositionSide::Long if mark_low <= liquidation_price => {
            Some(mark_open.min(liquidation_price))
        }
        PositionSide::Short if mark_high >= liquidation_price => {
            Some(mark_open.max(liquidation_price))
        }
        _ => None,
    }
}

fn risk_tier_for_notional(risk_tiers: &[RiskTier], notional: f64) -> &RiskTier {
    risk_tiers
        .iter()
        .find(|tier| {
            notional >= tier.lower_bound && tier.upper_bound.is_none_or(|upper| notional < upper)
        })
        .unwrap_or_else(|| {
            risk_tiers
                .last()
                .expect("perp accounting requires at least one risk tier")
        })
}

fn maintenance_margin_for_tier(notional: f64, tier: &RiskTier) -> f64 {
    (notional * tier.maintenance_margin_rate - tier.maintenance_amount).max(0.0)
}

fn liquidation_price_for_tier(position: &PositionState, tier: &RiskTier) -> Option<f64> {
    let quantity = position.quantity.abs();
    if quantity <= EPSILON {
        return None;
    }
    match position.side {
        PositionSide::Long => {
            let denominator = quantity * (1.0 - tier.maintenance_margin_rate);
            if denominator <= EPSILON {
                None
            } else {
                Some(
                    (quantity * position.entry_price
                        - position.isolated_margin
                        - tier.maintenance_amount)
                        / denominator,
                )
            }
        }
        PositionSide::Short => {
            let denominator = quantity * (1.0 + tier.maintenance_margin_rate);
            if denominator <= EPSILON {
                None
            } else {
                Some(
                    (position.isolated_margin
                        + quantity * position.entry_price
                        + tier.maintenance_amount)
                        / denominator,
                )
            }
        }
    }
}

pub(crate) fn zero_small_cash(cash: &mut f64) {
    if cash.abs() < EPSILON {
        *cash = 0.0;
    }
}
