use crate::backtest::{
    FeatureSnapshot, Fill, FillAction, OrderEndReason, OrderRecord, OrderStatus, PositionSide,
    EPSILON,
};
use crate::bytecode::SignalRole;
use crate::order::{OrderKind, TimeInForce, TriggerReference};

pub(crate) const ROLE_PRIORITY: [SignalRole; 4] = [
    SignalRole::LongExit,
    SignalRole::ShortExit,
    SignalRole::LongEntry,
    SignalRole::ShortEntry,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CapturedOrderRequest {
    pub role: SignalRole,
    pub kind: OrderKind,
    pub tif: Option<TimeInForce>,
    pub post_only: bool,
    pub trigger_ref: Option<TriggerReference>,
    pub price: Option<f64>,
    pub trigger_price: Option<f64>,
    pub expire_time: Option<f64>,
    pub signal_time: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct SignalBatch {
    pub time: f64,
    pub requests: [Option<CapturedOrderRequest>; 4],
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
}

#[derive(Clone, Debug)]
pub(crate) struct OpenTrade {
    pub side: PositionSide,
    pub quantity: f64,
    pub entry: Fill,
    pub entry_order_id: usize,
    pub entry_role: SignalRole,
    pub entry_kind: OrderKind,
    pub entry_snapshot: Option<FeatureSnapshot>,
    pub mae_price_delta: f64,
    pub mfe_price_delta: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct TradeEntryContext {
    pub order_id: usize,
    pub role: SignalRole,
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

pub(crate) fn empty_request_slots() -> [Option<CapturedOrderRequest>; 4] {
    [None, None, None, None]
}

pub(crate) fn role_index(role: SignalRole) -> usize {
    match role {
        SignalRole::LongEntry => 0,
        SignalRole::LongExit => 1,
        SignalRole::ShortEntry => 2,
        SignalRole::ShortExit => 3,
    }
}

pub(crate) fn fill_action_for_role(role: SignalRole) -> FillAction {
    match role {
        SignalRole::LongEntry | SignalRole::ShortExit => FillAction::Buy,
        SignalRole::ShortEntry | SignalRole::LongExit => FillAction::Sell,
    }
}

pub(crate) fn position_side_for_entry(role: SignalRole) -> Option<PositionSide> {
    match role {
        SignalRole::LongEntry => Some(PositionSide::Long),
        SignalRole::ShortEntry => Some(PositionSide::Short),
        SignalRole::LongExit | SignalRole::ShortExit => None,
    }
}

pub(crate) fn role_applicable(role: SignalRole, position: Option<&PositionState>) -> bool {
    match position.map(|state| state.side) {
        None => matches!(role, SignalRole::LongEntry | SignalRole::ShortEntry),
        Some(PositionSide::Long) => matches!(role, SignalRole::LongExit | SignalRole::ShortEntry),
        Some(PositionSide::Short) => {
            matches!(role, SignalRole::ShortExit | SignalRole::LongEntry)
        }
    }
}

pub(crate) fn order_record(
    request: CapturedOrderRequest,
    bar_index: usize,
    time: f64,
    id: usize,
) -> OrderRecord {
    OrderRecord {
        id,
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

fn would_cross_on_open(action: FillAction, open: f64, limit_price: f64) -> bool {
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

pub(crate) fn open_position(
    execution: FillExecutionContext,
    side: PositionSide,
    entry_context: TradeEntryContext,
    fee_rate: f64,
    cash: &mut f64,
) -> (PositionState, OpenTrade, Fill) {
    let action = match side {
        PositionSide::Long => FillAction::Buy,
        PositionSide::Short => FillAction::Sell,
    };
    let quantity = *cash / (execution.execution_price * (1.0 + fee_rate));
    let notional = quantity * execution.execution_price;
    let fee = notional * fee_rate;
    match side {
        PositionSide::Long => *cash -= notional + fee,
        PositionSide::Short => *cash += notional - fee,
    }
    zero_small_cash(cash);
    let signed_quantity = match side {
        PositionSide::Long => quantity,
        PositionSide::Short => -quantity,
    };
    let fill = Fill {
        bar_index: execution.bar_index,
        time: execution.time,
        action,
        quantity,
        raw_price: execution.raw_price,
        price: execution.execution_price,
        notional,
        fee,
    };
    let position = PositionState {
        side,
        quantity: signed_quantity,
        entry_bar_index: execution.bar_index,
        entry_time: execution.time,
        entry_price: execution.execution_price,
    };
    let trade = OpenTrade {
        side,
        quantity,
        entry: fill.clone(),
        entry_order_id: entry_context.order_id,
        entry_role: entry_context.role,
        entry_kind: entry_context.kind,
        entry_snapshot: entry_context.snapshot,
        mae_price_delta: 0.0,
        mfe_price_delta: 0.0,
    };
    (position, trade, fill)
}

pub(crate) fn close_position(
    bar_index: usize,
    time: f64,
    raw_price: f64,
    execution_price: f64,
    fee_rate: f64,
    cash: &mut f64,
    position: &PositionState,
) -> Fill {
    let action = match position.side {
        PositionSide::Long => FillAction::Sell,
        PositionSide::Short => FillAction::Buy,
    };
    let quantity = position.quantity.abs();
    let notional = quantity * execution_price;
    let fee = notional * fee_rate;
    match position.side {
        PositionSide::Long => *cash += notional - fee,
        PositionSide::Short => *cash -= notional + fee,
    }
    zero_small_cash(cash);
    Fill {
        bar_index,
        time,
        action,
        quantity,
        raw_price,
        price: execution_price,
        notional,
        fee,
    }
}

pub(crate) fn close_trade(open_trade: OpenTrade, exit: Fill) -> crate::backtest::Trade {
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
    crate::backtest::Trade {
        side: open_trade.side,
        quantity: open_trade.quantity,
        entry: open_trade.entry,
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

pub(crate) fn zero_small_cash(cash: &mut f64) {
    if cash.abs() < EPSILON {
        *cash = 0.0;
    }
}
