use std::fmt::Write;

use palmscript::bytecode::{Constant, LocalInfo, Program};
use palmscript::{
    BacktestResult, CompiledProgram, OrderStatus, OutputKind, OutputValue, Outputs, PositionSide,
    SignalRole, Value,
};

pub fn render_outputs_text(outputs: &Outputs) -> String {
    let mut out = String::new();

    if !outputs.plots.is_empty() {
        out.push_str("Plots\n");
        for series in &outputs.plots {
            for point in &series.points {
                let _ = writeln!(
                    out,
                    "plot#{} bar={} time={} value={}",
                    series.id,
                    point.bar_index,
                    fmt_opt_f64(point.time),
                    fmt_opt_f64(point.value)
                );
            }
        }
    }

    if !outputs.exports.is_empty() {
        out.push_str("Exports\n");
        for series in &outputs.exports {
            for point in &series.points {
                let _ = writeln!(
                    out,
                    "{} bar={} time={} value={}",
                    series.name,
                    point.bar_index,
                    fmt_opt_f64(point.time),
                    fmt_output_value(&point.value)
                );
            }
        }
    }

    if !outputs.triggers.is_empty() {
        out.push_str("Triggers\n");
        for series in &outputs.triggers {
            for point in &series.points {
                let _ = writeln!(
                    out,
                    "{} bar={} time={} value={}",
                    series.name,
                    point.bar_index,
                    fmt_opt_f64(point.time),
                    fmt_output_value(&point.value)
                );
            }
        }
    }

    if !outputs.trigger_events.is_empty() {
        out.push_str("Trigger Events\n");
        for event in &outputs.trigger_events {
            let _ = writeln!(
                out,
                "{} bar={} time={}",
                event.name,
                event.bar_index,
                fmt_opt_f64(event.time)
            );
        }
    }

    if !outputs.alerts.is_empty() {
        out.push_str("Alerts\n");
        for alert in &outputs.alerts {
            let _ = writeln!(out, "bar={} message={}", alert.bar_index, alert.message);
        }
    }

    out
}

pub fn render_bytecode_text(compiled: &CompiledProgram) -> String {
    let mut out = String::new();
    let program = &compiled.program;
    let _ = writeln!(
        out,
        "Strategy Intervals\n  base={}",
        program
            .base_interval
            .map(|interval| interval.as_str())
            .unwrap_or("none")
    );
    let _ = writeln!(out, "Constants");
    for (index, constant) in program.constants.iter().enumerate() {
        let _ = writeln!(out, "  [{}] {}", index, fmt_constant(constant));
    }

    let _ = writeln!(out, "Locals");
    for (index, local) in program.locals.iter().enumerate() {
        let _ = writeln!(out, "  [{}] {}", index, fmt_local(local));
    }

    let _ = writeln!(out, "Outputs");
    for output in &program.outputs {
        let _ = writeln!(
            out,
            "  name={} kind={:?} ty={:?} slot={}",
            output.name, output.kind, output.ty, output.slot
        );
    }

    render_instructions(&mut out, program);
    out
}

pub fn render_backtest_text(result: &BacktestResult) -> String {
    let mut out = String::new();
    let summary = &result.summary;
    let placed_count = result.orders.len();
    let open_count = result
        .orders
        .iter()
        .filter(|order| matches!(order.status, OrderStatus::Open))
        .count();
    let filled_count = result
        .orders
        .iter()
        .filter(|order| matches!(order.status, OrderStatus::Filled))
        .count();
    let cancelled_count = result
        .orders
        .iter()
        .filter(|order| matches!(order.status, OrderStatus::Cancelled))
        .count();
    let rejected_count = result
        .orders
        .iter()
        .filter(|order| matches!(order.status, OrderStatus::Rejected))
        .count();
    let expired_count = result
        .orders
        .iter()
        .filter(|order| matches!(order.status, OrderStatus::Expired))
        .count();

    out.push_str("Backtest Summary\n");
    let _ = writeln!(out, "starting_equity={:.2}", summary.starting_equity);
    let _ = writeln!(out, "ending_equity={:.2}", summary.ending_equity);
    let _ = writeln!(out, "realized_pnl={:.2}", summary.realized_pnl);
    let _ = writeln!(out, "unrealized_pnl={:.2}", summary.unrealized_pnl);
    let _ = writeln!(out, "total_return_pct={:.2}", summary.total_return * 100.0);
    let _ = writeln!(out, "trade_count={}", summary.trade_count);
    let _ = writeln!(out, "winning_trade_count={}", summary.winning_trade_count);
    let _ = writeln!(out, "losing_trade_count={}", summary.losing_trade_count);
    let _ = writeln!(out, "win_rate_pct={:.2}", summary.win_rate * 100.0);
    let _ = writeln!(out, "max_drawdown={:.2}", summary.max_drawdown);
    let _ = writeln!(out, "max_gross_exposure={:.2}", summary.max_gross_exposure);

    out.push_str("Order Summary\n");
    let _ = writeln!(out, "placed_count={placed_count}");
    let _ = writeln!(out, "open_count={open_count}");
    let _ = writeln!(out, "filled_count={filled_count}");
    let _ = writeln!(out, "cancelled_count={cancelled_count}");
    let _ = writeln!(out, "rejected_count={rejected_count}");
    let _ = writeln!(out, "expired_count={expired_count}");

    out.push_str("Diagnostics Summary\n");
    let _ = writeln!(
        out,
        "order_fill_rate_pct={:.2}",
        result.diagnostics.summary.order_fill_rate * 100.0
    );
    let _ = writeln!(
        out,
        "average_bars_to_fill={:.2}",
        result.diagnostics.summary.average_bars_to_fill
    );
    let _ = writeln!(
        out,
        "average_bars_held={:.2}",
        result.diagnostics.summary.average_bars_held
    );
    let _ = writeln!(
        out,
        "average_mae_pct={:.2}",
        result.diagnostics.summary.average_mae_pct * 100.0
    );
    let _ = writeln!(
        out,
        "average_mfe_pct={:.2}",
        result.diagnostics.summary.average_mfe_pct * 100.0
    );
    let _ = writeln!(
        out,
        "signal_exit_count={}",
        result.diagnostics.summary.signal_exit_count
    );
    let _ = writeln!(
        out,
        "stop_loss_exit_count={}",
        result.diagnostics.summary.stop_loss_exit_count
    );
    let _ = writeln!(
        out,
        "take_profit_exit_count={}",
        result.diagnostics.summary.take_profit_exit_count
    );
    let _ = writeln!(
        out,
        "reversal_exit_count={}",
        result.diagnostics.summary.reversal_exit_count
    );

    out.push_str("Recent Orders\n");
    let recent_orders = result.orders.iter().rev().take(5).collect::<Vec<_>>();
    for order in recent_orders.iter().rev() {
        let _ = writeln!(
            out,
            "id={} role={} kind={} status={} signal_time={} placed_time={} fill_time={} fill_price={} end_reason={}",
            order.id,
            fmt_signal_role(order.role),
            fmt_order_kind(order.kind),
            fmt_order_status(order.status),
            order.signal_time,
            order.placed_time,
            fmt_opt_f64(order.fill_time),
            fmt_opt_f64(order.fill_price),
            order.end_reason
                .map(|reason| format!("{reason:?}"))
                .unwrap_or_else(|| "na".to_string())
        );
    }

    out.push_str("Recent Trades\n");
    let recent_trades = result.trades.iter().rev().take(5).collect::<Vec<_>>();
    for trade in recent_trades.iter().rev() {
        let _ = writeln!(
            out,
            "side={} entry_time={} exit_time={} entry_price={:.2} exit_price={:.2} qty={:.6} pnl={:.2}",
            fmt_position_side(trade.side),
            trade.entry.time,
            trade.exit.time,
            trade.entry.price,
            trade.exit.price,
            trade.quantity,
            trade.realized_pnl
        );
    }

    out.push_str("Open Position\n");
    if let Some(position) = &result.open_position {
        let _ = writeln!(out, "side={}", fmt_position_side(position.side));
        let _ = writeln!(out, "quantity={:.6}", position.quantity);
        let _ = writeln!(out, "entry_price={:.2}", position.entry_price);
        let _ = writeln!(out, "market_price={:.2}", position.market_price);
        let _ = writeln!(out, "unrealized_pnl={:.2}", position.unrealized_pnl);
    } else {
        out.push_str("flat\n");
    }

    out
}

fn render_instructions(out: &mut String, program: &Program) {
    let _ = writeln!(out, "Instructions");
    for (index, instruction) in program.instructions.iter().enumerate() {
        let span = instruction
            .span
            .map(|span| format!(" @{}:{}", span.start.line, span.start.column))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  [{}] {:?} a={} b={} c={}{}",
            index, instruction.opcode, instruction.a, instruction.b, instruction.c, span
        );
    }
}

fn fmt_local(local: &LocalInfo) -> String {
    let binding = local
        .market_binding
        .map(|binding| format!("{binding:?}"))
        .unwrap_or_else(|| "None".to_string());
    format!(
        "name={:?} ty={:?} kind={:?} hidden={} history={} update_mask={} market_binding={}",
        local.name,
        local.ty,
        local.kind,
        local.hidden,
        local.history_capacity,
        local.update_mask,
        binding
    )
}

fn fmt_constant(constant: &Constant) -> String {
    match constant {
        Constant::Value(value) => fmt_value(value),
    }
}

fn fmt_value(value: &Value) -> String {
    match value {
        Value::F64(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::MaType(value) => format!("ma_type.{}", value.as_str()),
        Value::TimeInForce(value) => format!("tif.{}", value.as_str()),
        Value::TriggerReference(value) => format!("trigger_ref.{}", value.as_str()),
        Value::NA => "na".to_string(),
        Value::Void => "void".to_string(),
        Value::SeriesRef(slot) => format!("series-ref({slot})"),
        Value::Tuple2(values) => format!("({}, {})", fmt_value(&values[0]), fmt_value(&values[1])),
        Value::Tuple3(values) => format!(
            "({}, {}, {})",
            fmt_value(&values[0]),
            fmt_value(&values[1]),
            fmt_value(&values[2])
        ),
    }
}

fn fmt_opt_f64(value: Option<f64>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "na".to_string(),
    }
}

fn fmt_output_value(value: &OutputValue) -> String {
    match value {
        OutputValue::F64(value) => value.to_string(),
        OutputValue::Bool(value) => value.to_string(),
        OutputValue::NA => "na".to_string(),
    }
}

fn fmt_position_side(side: PositionSide) -> &'static str {
    match side {
        PositionSide::Long => "long",
        PositionSide::Short => "short",
    }
}

fn fmt_signal_role(role: SignalRole) -> &'static str {
    match role {
        SignalRole::LongEntry => "long_entry",
        SignalRole::LongExit => "long_exit",
        SignalRole::ShortEntry => "short_entry",
        SignalRole::ShortExit => "short_exit",
    }
}

fn fmt_order_kind(kind: palmscript::OrderKind) -> &'static str {
    match kind {
        palmscript::OrderKind::Market => "market",
        palmscript::OrderKind::Limit => "limit",
        palmscript::OrderKind::StopMarket => "stop_market",
        palmscript::OrderKind::StopLimit => "stop_limit",
        palmscript::OrderKind::TakeProfitMarket => "take_profit_market",
        palmscript::OrderKind::TakeProfitLimit => "take_profit_limit",
    }
}

fn fmt_order_status(status: OrderStatus) -> &'static str {
    match status {
        OrderStatus::Open => "open",
        OrderStatus::Filled => "filled",
        OrderStatus::Cancelled => "cancelled",
        OrderStatus::Rejected => "rejected",
        OrderStatus::Expired => "expired",
    }
}

#[allow(dead_code)]
fn _output_kind(_kind: OutputKind) {}

#[cfg(test)]
mod tests {
    use super::{render_backtest_text, render_bytecode_text, render_outputs_text};
    use palmscript::bytecode::{Constant, LocalInfo, OutputDecl, OutputKind, Program};
    use palmscript::span::{Position, Span};
    use palmscript::types::Type;
    use palmscript::{
        BacktestResult, BacktestSummary, CompiledProgram, EquityPoint, Fill, FillAction, OrderKind,
        OrderRecord, OrderStatus, OutputSample, OutputSeries, OutputValue, Outputs, PlotPoint,
        PlotSeries, PositionSide, SignalRole, Trade,
    };

    #[test]
    fn render_outputs_text_renders_all_sections() {
        let outputs = Outputs {
            plots: vec![PlotSeries {
                id: 1,
                name: Some("price".to_string()),
                points: vec![PlotPoint {
                    plot_id: 1,
                    bar_index: 0,
                    time: Some(10.0),
                    value: Some(11.5),
                }],
            }],
            exports: vec![OutputSeries {
                id: 0,
                name: "trend".to_string(),
                kind: OutputKind::ExportSeries,
                points: vec![OutputSample {
                    output_id: 0,
                    name: "trend".to_string(),
                    bar_index: 0,
                    time: Some(10.0),
                    value: OutputValue::Bool(true),
                }],
            }],
            triggers: vec![OutputSeries {
                id: 1,
                name: "entry".to_string(),
                kind: OutputKind::Trigger,
                points: vec![OutputSample {
                    output_id: 1,
                    name: "entry".to_string(),
                    bar_index: 0,
                    time: None,
                    value: OutputValue::NA,
                }],
            }],
            order_fields: vec![],
            trigger_events: vec![palmscript::TriggerEvent {
                output_id: 1,
                name: "entry".to_string(),
                bar_index: 0,
                time: Some(10.0),
            }],
            alerts: vec![palmscript::Alert {
                bar_index: 0,
                message: "hello".to_string(),
            }],
        };
        let rendered = render_outputs_text(&outputs);
        assert!(rendered.contains("Plots"));
        assert!(rendered.contains("Exports"));
        assert!(rendered.contains("Triggers"));
        assert!(rendered.contains("Trigger Events"));
        assert!(rendered.contains("Alerts"));
        assert!(rendered.contains("value=na"));
    }

    #[test]
    fn render_bytecode_text_includes_strategy_metadata_and_sections() {
        let program = Program {
            constants: vec![Constant::Value(palmscript::Value::F64(1.0))],
            locals: vec![LocalInfo::scalar(Some("x".to_string()), Type::F64, false)],
            outputs: vec![OutputDecl {
                name: "trend".to_string(),
                kind: OutputKind::ExportSeries,
                signal_role: None,
                ty: Type::SeriesBool,
                slot: 1,
            }],
            base_interval: Some(palmscript::Interval::Min1),
            instructions: vec![palmscript::bytecode::Instruction::new(
                palmscript::bytecode::OpCode::LoadConst,
            )
            .with_span(Span::new(Position::new(0, 1, 1), Position::new(4, 1, 5)))],
            ..Program::default()
        };
        let compiled = CompiledProgram {
            program,
            source: "interval 1m\nplot(1)".to_string(),
        };
        let rendered = render_bytecode_text(&compiled);
        assert!(rendered.contains("Strategy Intervals"));
        assert!(rendered.contains("base=1m"));
        assert!(rendered.contains("Constants"));
        assert!(rendered.contains("Locals"));
        assert!(rendered.contains("Outputs"));
        assert!(rendered.contains("Instructions"));
    }

    #[test]
    fn render_backtest_text_includes_summary_and_recent_trades() {
        let result = BacktestResult {
            outputs: Outputs::default(),
            orders: vec![OrderRecord {
                id: 0,
                role: SignalRole::LongEntry,
                kind: OrderKind::Market,
                action: FillAction::Buy,
                tif: None,
                post_only: false,
                trigger_ref: None,
                signal_time: 10.0,
                placed_bar_index: 1,
                placed_time: 20.0,
                trigger_time: None,
                fill_bar_index: Some(1),
                fill_time: Some(20.0),
                raw_price: Some(100.0),
                fill_price: Some(100.0),
                limit_price: None,
                trigger_price: None,
                expire_time: None,
                status: OrderStatus::Filled,
                end_reason: None,
            }],
            fills: vec![],
            trades: vec![Trade {
                side: PositionSide::Long,
                quantity: 1.25,
                entry: Fill {
                    bar_index: 1,
                    time: 10.0,
                    action: FillAction::Buy,
                    quantity: 1.25,
                    raw_price: 100.0,
                    price: 100.0,
                    notional: 125.0,
                    fee: 0.5,
                },
                exit: Fill {
                    bar_index: 2,
                    time: 20.0,
                    action: FillAction::Sell,
                    quantity: 1.25,
                    raw_price: 110.0,
                    price: 110.0,
                    notional: 137.5,
                    fee: 0.5,
                },
                realized_pnl: 12.0,
            }],
            equity_curve: vec![EquityPoint {
                bar_index: 0,
                time: 10.0,
                cash: 1000.0,
                equity: 1000.0,
                position_side: None,
                quantity: 0.0,
                mark_price: 100.0,
                gross_exposure: 0.0,
            }],
            summary: BacktestSummary {
                starting_equity: 1000.0,
                ending_equity: 1012.0,
                realized_pnl: 12.0,
                unrealized_pnl: 0.0,
                total_return: 0.012,
                trade_count: 1,
                winning_trade_count: 1,
                losing_trade_count: 0,
                win_rate: 1.0,
                max_drawdown: 10.0,
                max_gross_exposure: 125.0,
            },
            diagnostics: palmscript::BacktestDiagnostics {
                order_diagnostics: vec![],
                trade_diagnostics: vec![],
                summary: palmscript::BacktestDiagnosticSummary {
                    order_fill_rate: 1.0,
                    average_bars_to_fill: 0.0,
                    average_bars_held: 1.0,
                    average_mae_pct: -0.02,
                    average_mfe_pct: 0.05,
                    signal_exit_count: 1,
                    stop_loss_exit_count: 0,
                    take_profit_exit_count: 0,
                    reversal_exit_count: 0,
                    by_order_kind: vec![],
                    by_side: vec![],
                },
            },
            open_position: None,
        };

        let rendered = render_backtest_text(&result);
        assert!(rendered.contains("Backtest Summary"));
        assert!(rendered.contains("starting_equity=1000.00"));
        assert!(rendered.contains("Order Summary"));
        assert!(rendered.contains("placed_count=1"));
        assert!(rendered.contains("Diagnostics Summary"));
        assert!(rendered.contains("order_fill_rate_pct=100.00"));
        assert!(rendered.contains("Recent Orders"));
        assert!(rendered.contains("role=long_entry"));
        assert!(rendered.contains("Recent Trades"));
        assert!(rendered.contains("side=long"));
        assert!(rendered.contains("Open Position"));
        assert!(rendered.contains("flat"));
    }
}
