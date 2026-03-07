use std::fmt::Write;

use palmscript::bytecode::{Constant, LocalInfo, Program};
use palmscript::{
    BacktestResult, CompiledProgram, OutputKind, OutputValue, Outputs, PositionSide, Value,
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

#[allow(dead_code)]
fn _output_kind(_kind: OutputKind) {}

#[cfg(test)]
mod tests {
    use super::{render_backtest_text, render_bytecode_text, render_outputs_text};
    use palmscript::bytecode::{Constant, LocalInfo, OutputDecl, OutputKind, Program};
    use palmscript::span::{Position, Span};
    use palmscript::types::Type;
    use palmscript::{
        BacktestResult, BacktestSummary, CompiledProgram, EquityPoint, Fill, FillAction,
        OutputSample, OutputSeries, OutputValue, Outputs, PlotPoint, PlotSeries, PositionSide,
        Trade,
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
            open_position: None,
        };

        let rendered = render_backtest_text(&result);
        assert!(rendered.contains("Backtest Summary"));
        assert!(rendered.contains("starting_equity=1000.00"));
        assert!(rendered.contains("Recent Trades"));
        assert!(rendered.contains("side=long"));
        assert!(rendered.contains("Open Position"));
        assert!(rendered.contains("flat"));
    }
}
