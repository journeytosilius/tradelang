use crate::backtest::bridge::{capture_request, PreparedBacktest};
use crate::backtest::diagnostics::{
    build_diagnostics_summary, build_order_diagnostics, snapshot_from_step, DiagnosticsAccumulator,
    OrderDiagnosticContext,
};
use crate::backtest::orders::{
    add_to_position, adjusted_price, close_position, close_trade_slice, empty_request_slots,
    evaluate_active_order, fill_action_for_role, is_attached_exit_role, liquidation_trigger_price,
    missing_field_reason, open_position, position_side_for_entry, realize_perp_close,
    refresh_position_risk, request_applicable, resolve_entry_sizing, role_index,
    unrealized_pnl_for_position, update_open_trade_excursions, AccountingMode, ActiveOrder,
    CapturedOrderRequest, CloseExecution, EntryProgressState, EntrySizingSpec,
    FillExecutionContext, OpenTrade, PositionState, TradeEntryContext, WorkingState, ROLE_COUNT,
    ROLE_PRIORITY,
};
use crate::backtest::{
    BacktestConfig, BacktestDiagnostics, BacktestError, BacktestResult, BacktestSummary,
    EquityPoint, FeatureSnapshot, Fill, OpportunityEventKind, OrderEndReason, OrderRecord,
    OrderStatus, PerpBacktestMetadata, PositionSnapshot, Trade, TradeDiagnostic,
    TradeExitClassification,
};
use crate::bytecode::{
    LastExitFieldDecl, PositionEventFieldDecl, PositionFieldDecl, RiskControlDecl, RiskControlKind,
    SignalRole,
};
use crate::exchange::{RiskTier, VenueRiskSnapshot};
use crate::order::OrderKind;
use crate::output::StepOutput;
use crate::position::{
    ExitKind, LastExitField, LastExitScope, PositionEventField, PositionField, PositionSide,
};
use crate::runtime::{Bar, RuntimeStep, RuntimeStepper};
use crate::types::Value;

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

pub(crate) fn simulate_backtest(
    mut stepper: RuntimeStepper,
    execution_bars: Vec<Bar>,
    config: &BacktestConfig,
    prepared: PreparedBacktest,
) -> Result<BacktestResult, BacktestError> {
    let fee_rate = config.fee_bps / crate::backtest::BPS_SCALE;
    let slippage_rate = config.slippage_bps / crate::backtest::BPS_SCALE;
    let accounting = accounting_mode(config);
    let aligned_mark_bars = aligned_mark_bars(config, &execution_bars)?;
    let mut cash = config.initial_capital;
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

    while let Some(open_time) = stepper.peek_open_time() {
        let next_execution = execution_bars.get(execution_cursor).copied();
        let current_execution =
            next_execution.filter(|bar| bar.time.is_finite() && bar.time == open_time as f64);
        let current_mark =
            current_execution.and_then(|_| aligned_mark_bars.get(execution_cursor).copied());
        let mut position_events = PositionEventStep::default();
        let mut filled_record_indices = Vec::new();
        if let Some(bar) = current_execution {
            if let Some(open_trade) = open_trade.as_mut() {
                update_open_trade_excursions(open_trade, bar.high, bar.low);
            }

            if let Some(timeout_outcome) = maybe_force_time_exit(
                &prepared.risk_controls,
                execution_cursor,
                bar.time,
                bar.open,
                &accounting,
                fee_rate,
                &mut cash,
                &mut position,
                &mut open_trade,
                &mut fills,
                &mut trades,
                &mut trade_diagnostics,
                &mut total_realized_pnl,
                last_snapshot.clone(),
            ) {
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
                current_position_snapshot(position.as_ref(), bar.open, bar.time),
                execution_cursor,
                bar.time,
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

                let evaluation =
                    evaluate_active_order(&active, bar.time, bar.open, bar.high, bar.low);
                active.first_eval_done = true;

                match evaluation {
                    crate::backtest::orders::Evaluation::None => {
                        active_orders[slot] = Some(active);
                    }
                    crate::backtest::orders::Evaluation::Expire => {
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
                        orders[active.record_index].trigger_time = Some(trigger_time);
                        active.state = WorkingState::RestingLimit { active_after_time };
                        active_orders[slot] = Some(active);
                    }
                    crate::backtest::orders::Evaluation::Fill(execution) => {
                        let action = fill_action_for_role(role);
                        let execution_price = if matches!(active.request.kind, OrderKind::Market) {
                            adjusted_price(execution.raw_price, action, slippage_rate)
                        } else {
                            execution.price
                        };

                        let close_outcome = maybe_close_position_for_role(
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
                                        execution_context,
                                        position_state,
                                        open_trade_state,
                                        EntrySizingSpec {
                                            size_mode: active.request.size_mode,
                                            size_value: active.request.size_value,
                                            stop_price: active.request.size_stop_price,
                                        },
                                        &accounting,
                                        fee_rate,
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
                                        execution_context,
                                        next_side,
                                        TradeEntryContext {
                                            order_id: active.record_index,
                                            role,
                                            kind: active.request.kind,
                                            snapshot: last_snapshot.clone(),
                                        },
                                        EntrySizingSpec {
                                            size_mode: active.request.size_mode,
                                            size_value: active.request.size_value,
                                            stop_price: active.request.size_stop_price,
                                        },
                                        &accounting,
                                        fee_rate,
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
                            position_state.side,
                            execution_cursor,
                            bar.time,
                            liquidation_price,
                            fee_rate,
                            &mut cash,
                            &mut position,
                            &mut open_trade,
                            &mut fills,
                            &mut trades,
                            &mut trade_diagnostics,
                            &mut total_realized_pnl,
                        );
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

        let overrides = build_runtime_overrides(
            &prepared.position_fields,
            &prepared.position_event_fields,
            &prepared.last_exit_fields,
            position.as_ref(),
            open_trade.as_ref(),
            last_exit.as_ref(),
            last_long_exit.as_ref(),
            last_short_exit.as_ref(),
            current_execution.map(|bar| bar.close).or(last_mark_price),
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
        let decision_position_snapshot = current_execution
            .and_then(|bar| current_position_snapshot(position.as_ref(), bar.close, bar.time));

        if let Some(bar) = current_execution {
            if position_events.long_entry_fill || position_events.short_entry_fill {
                if let Some(open_trade) = open_trade.as_mut() {
                    open_trade.entry_snapshot = snapshot.clone();
                }
            }
            let fill_position = current_position_snapshot(position.as_ref(), bar.close, bar.time);
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
            max_gross_exposure = max_gross_exposure.max(gross_exposure);
            let equity = match &accounting {
                AccountingMode::Spot => cash + quantity * mark_price,
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
                free_collateral: accounting.is_perp().then_some(cash),
                isolated_margin: position.as_ref().map(|state| state.isolated_margin),
                maintenance_margin: position.as_ref().map(|state| state.maintenance_margin),
                liquidation_price: position.as_ref().and_then(|state| state.liquidation_price),
            });
            last_mark_price = Some(mark_price);
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
        );
        last_snapshot = snapshot;
    }

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
            export_summaries,
            opportunity_events,
        },
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
fn build_runtime_overrides(
    position_fields: &[PositionFieldDecl],
    position_event_fields: &[PositionEventFieldDecl],
    last_exit_fields: &[LastExitFieldDecl],
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
) {
    for event in &output.trigger_events {
        let Some(role) = prepared.signal_roles.get(&event.output_id).copied() else {
            continue;
        };
        let Some(template) = prepared.order_templates.get(&role).copied() else {
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
            event_kind,
            &event.name,
            role,
            bar_index,
            signal_time,
            position_snapshot,
            snapshot,
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
) {
    let (Some(before), Some(after)) = (position_before_step, position_after_step) else {
        return;
    };
    if before.side != after.side {
        return;
    }

    let mut roles = [None, None];
    roles[0] = resolve_active_protect_role(before.side, target_consumption, prepared);
    roles[1] = resolve_active_target_role(before.side, target_consumption, prepared);
    for role in roles.into_iter().flatten() {
        let template = prepared.order_templates[&role];
        let slot = role_index(role);
        diagnostics.record_signal_event(
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
) {
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
                OpportunityEventKind::SignalIgnoredCooldown,
                signal_name.as_deref().unwrap_or(role.canonical_name()),
                role,
                bar_index,
                time,
                placed_position.as_ref(),
                placed_snapshot.as_ref().or(signal_snapshot.as_ref()),
            );
            continue;
        }
        if !request_applicable(request, position, entry_progress) {
            diagnostics.record_signal_event(
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
            continue;
        }

        if let Some(existing) = active_orders[slot].take() {
            diagnostics.record_signal_event(
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

        let mut record =
            crate::backtest::orders::order_record(request, bar_index, time, orders.len());
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
    prepared.order_templates.contains_key(&role).then_some(role)
}

fn resolve_active_protect_role(
    side: PositionSide,
    state: TargetConsumptionState,
    prepared: &PreparedBacktest,
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
        .find(|role| prepared.order_templates.contains_key(role))
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
        let trade = close_trade_slice(open_trade, exit_fill.clone(), close_quantity);
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
        trade_id: trades.len(),
        side,
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
    orders: &mut [OrderRecord],
) {
    let Some(position) = position else {
        return;
    };
    let active_protect = resolve_active_protect_role(position.side, target_consumption, prepared);
    let active_target = resolve_active_target_role(position.side, target_consumption, prepared);

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
    mark_price: f64,
    market_time: f64,
) -> Option<PositionSnapshot> {
    position.map(|state| PositionSnapshot {
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
) -> Option<CloseOutcome> {
    let position_side = position.as_ref()?.side;
    let max_bars = risk_control_bars(controls, position_side, RiskControlKind::MaxBarsInTrade)?;
    let bars_held = bar_index.saturating_sub(position.as_ref()?.entry_bar_index);
    if bars_held < max_bars {
        return None;
    }

    Some(maybe_close_position_for_role(
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
        let trade = close_trade_slice(open_trade, exit_fill.clone(), quantity);
        (
            trade,
            open_trade.entry_order_id,
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
        trade_id: trades.len(),
        side,
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
