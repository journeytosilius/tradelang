use std::fmt::Write;
use std::path::Path;

use palmscript::bytecode::{Constant, LocalInfo, Program};
use palmscript::exchange::binance::UsdmRiskSource as BinanceUsdmRiskSource;
use palmscript::{
    BacktestResult, CompiledProgram, ExecutionDaemonStatus, ExportDiagnosticSummary, Fill,
    OptimizeCandidateSummary, OptimizeEvaluationSummary, OptimizeResult, OrderRecord, OrderStatus,
    OutputKind, OutputValue, Outputs, PaperSessionExport, PaperSessionLogEvent,
    PaperSessionManifest, PaperSessionSnapshot, PositionSide, PositionSnapshot, SignalRole, Value,
    VenueRiskSnapshot, WalkForwardResult, WalkForwardSweepResult,
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

    let _ = writeln!(out, "Inputs");
    for input in &program.inputs {
        let _ = writeln!(
            out,
            "  name={} ty={:?} default={} optimization={}",
            input.name,
            input.ty,
            fmt_value(&input.default_value),
            fmt_input_optimization(input.optimization.as_ref())
        );
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
    let _ = writeln!(out, "max_net_exposure={:.2}", summary.max_net_exposure);
    let _ = writeln!(
        out,
        "peak_open_position_count={}",
        summary.peak_open_position_count
    );

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
        "protect_exit_count={}",
        result.diagnostics.summary.protect_exit_count
    );
    let _ = writeln!(
        out,
        "target_exit_count={}",
        result.diagnostics.summary.target_exit_count
    );
    let _ = writeln!(
        out,
        "reversal_exit_count={}",
        result.diagnostics.summary.reversal_exit_count
    );
    let _ = writeln!(
        out,
        "liquidation_exit_count={}",
        result.diagnostics.summary.liquidation_exit_count
    );
    let _ = writeln!(
        out,
        "execution_asset_return_pct={:.2}",
        result.diagnostics.capture_summary.execution_asset_return * 100.0
    );
    let _ = writeln!(
        out,
        "flat_bar_pct={:.2}",
        result.diagnostics.capture_summary.flat_bar_pct * 100.0
    );
    let _ = writeln!(
        out,
        "long_bar_pct={:.2}",
        result.diagnostics.capture_summary.long_bar_pct * 100.0
    );
    let _ = writeln!(
        out,
        "short_bar_pct={:.2}",
        result.diagnostics.capture_summary.short_bar_pct * 100.0
    );
    let _ = writeln!(
        out,
        "opportunity_cost_return_pct={:.2}",
        result.diagnostics.capture_summary.opportunity_cost_return * 100.0
    );
    let _ = writeln!(
        out,
        "longest_drawdown_bars={}",
        result.diagnostics.drawdown.longest_drawdown_bars
    );
    let _ = writeln!(
        out,
        "current_drawdown_bars={}",
        result.diagnostics.drawdown.current_drawdown_bars
    );
    let _ = writeln!(
        out,
        "longest_stagnation_bars={}",
        result.diagnostics.drawdown.longest_stagnation_bars
    );
    let _ = writeln!(
        out,
        "average_recovery_bars={:.2}",
        result.diagnostics.drawdown.average_recovery_bars
    );
    let _ = writeln!(out, "portfolio_mode={}", result.diagnostics.portfolio_mode);
    render_overfitting_risk_text(
        &mut out,
        "Overfitting Risk",
        &result.diagnostics.overfitting_risk,
    );

    if !result.diagnostics.blocked_portfolio_entries.is_empty() {
        out.push_str("Portfolio Blocks\n");
        for blocked in &result.diagnostics.blocked_portfolio_entries {
            let _ = writeln!(
                out,
                "kind={:?} alias={} group={} count={}",
                blocked.kind,
                blocked.alias,
                blocked.group.as_deref().unwrap_or("none"),
                blocked.count
            );
        }
    }
    let _ = writeln!(
        out,
        "degraded_bar_count={}",
        result.diagnostics.source_alignment.degraded_bar_count
    );
    let _ = writeln!(
        out,
        "per_bar_trace_count={}",
        result.diagnostics.per_bar_trace.len()
    );

    if !result.diagnostics.export_summaries.is_empty() {
        out.push_str("Top Export States\n");
        for summary in result.diagnostics.export_summaries.iter().take(3) {
            match summary {
                ExportDiagnosticSummary::Bool(summary) => {
                    let _ = writeln!(
                        out,
                        "name={} kind=bool rising_edge_count={} true_count={} true_while_flat_count={} trade_count={} win_rate_pct={:.2}",
                        summary.name,
                        summary.rising_edge_count,
                        summary.true_count,
                        summary.true_while_flat_count,
                        summary.trade_count,
                        summary.win_rate * 100.0
                    );
                }
                ExportDiagnosticSummary::Numeric(summary) => {
                    let _ = writeln!(
                        out,
                        "name={} kind=numeric mean={} entry_mean={} exit_mean={}",
                        summary.name,
                        fmt_opt_f64(summary.mean),
                        fmt_opt_f64(summary.entry_mean),
                        fmt_opt_f64(summary.exit_mean)
                    );
                }
            }
        }
    }

    if !result.diagnostics.opportunity_events.is_empty() {
        out.push_str("Recent Opportunity Events\n");
        for event in result
            .diagnostics
            .opportunity_events
            .iter()
            .rev()
            .take(5)
            .rev()
        {
            let _ = writeln!(
                out,
                "kind={:?} name={} role={} bar={} time={} forward_1bar_pct={}",
                event.kind,
                event.name,
                event.role.map(fmt_signal_role).unwrap_or("na"),
                event.bar_index,
                event.time,
                event
                    .forward_returns
                    .iter()
                    .find(|metric| metric.horizon_bars == 1)
                    .map(|metric| format!("{:.2}", metric.return_pct * 100.0))
                    .unwrap_or_else(|| "na".to_string())
            );
        }
    }

    if !result.diagnostics.cohorts.by_holding_time.is_empty() {
        out.push_str("Cohort Highlights\n");
        for summary in result.diagnostics.cohorts.by_holding_time.iter().take(3) {
            let _ = writeln!(
                out,
                "holding_bucket={:?} trade_count={} win_rate_pct={:.2} average_realized_pnl={:.2}",
                summary.bucket,
                summary.trade_count,
                summary.win_rate * 100.0,
                summary.average_realized_pnl
            );
        }
    }

    if !result.diagnostics.hints.is_empty() {
        out.push_str("Hints\n");
        for hint in &result.diagnostics.hints {
            let _ = writeln!(
                out,
                "kind={:?} metric={} value={}",
                hint.kind,
                hint.metric.as_deref().unwrap_or("na"),
                fmt_opt_f64(hint.value)
            );
        }
    }

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
        if let Some(free_collateral) = position.free_collateral {
            let _ = writeln!(out, "free_collateral={:.2}", free_collateral);
        }
        if let Some(isolated_margin) = position.isolated_margin {
            let _ = writeln!(out, "isolated_margin={:.2}", isolated_margin);
        }
        if let Some(maintenance_margin) = position.maintenance_margin {
            let _ = writeln!(out, "maintenance_margin={:.2}", maintenance_margin);
        }
        if let Some(liquidation_price) = position.liquidation_price {
            let _ = writeln!(out, "liquidation_price={:.2}", liquidation_price);
        }
    } else {
        out.push_str("flat\n");
    }

    if let Some(perp) = &result.perp {
        out.push_str("Perp Context\n");
        let _ = writeln!(out, "leverage={:.2}", perp.leverage);
        let _ = writeln!(out, "margin_mode={:?}", perp.margin_mode);
        let _ = writeln!(out, "mark_price_basis={:?}", perp.mark_price_basis);
        if let VenueRiskSnapshot::BinanceUsdm(snapshot) = &perp.risk_snapshot {
            let source = match snapshot.source {
                BinanceUsdmRiskSource::SignedLeverageBrackets => "signed_leverage_brackets",
                BinanceUsdmRiskSource::PublicExchangeInfoApproximation => {
                    "public_exchange_info_approximation"
                }
            };
            let _ = writeln!(out, "risk_snapshot_source={source}");
        }
    }

    out
}

pub fn render_paper_manifest_text(manifest: &PaperSessionManifest) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "session_id={}", manifest.session_id);
    let _ = writeln!(out, "status={:?}", manifest.status);
    let _ = writeln!(out, "health={:?}", manifest.health);
    let _ = writeln!(out, "start_time_ms={}", manifest.start_time_ms);
    let _ = writeln!(out, "created_at_ms={}", manifest.created_at_ms);
    let _ = writeln!(out, "updated_at_ms={}", manifest.updated_at_ms);
    let _ = writeln!(out, "base_interval={}", manifest.base_interval.as_str());
    let _ = writeln!(out, "history_capacity={}", manifest.history_capacity);
    let _ = writeln!(
        out,
        "execution_sources={}",
        manifest.config.execution_source_aliases.join(",")
    );
    let _ = writeln!(
        out,
        "initial_capital={:.2}",
        manifest.config.initial_capital
    );
    let _ = writeln!(out, "maker_fee_bps={:.2}", manifest.config.maker_fee_bps);
    let _ = writeln!(out, "taker_fee_bps={:.2}", manifest.config.taker_fee_bps);
    for (alias, schedule) in &manifest.config.execution_fee_schedules {
        let _ = writeln!(
            out,
            "fee_schedule.{alias}={:.2}/{:.2}",
            schedule.maker_bps, schedule.taker_bps
        );
    }
    let _ = writeln!(out, "slippage_bps={:.2}", manifest.config.slippage_bps);
    let _ = writeln!(out, "stop_requested={}", manifest.stop_requested);
    if let Some(warmup_from_ms) = manifest.warmup_from_ms {
        let _ = writeln!(out, "warmup_from_ms={warmup_from_ms}");
    }
    if let Some(latest_runtime_to_ms) = manifest.latest_runtime_to_ms {
        let _ = writeln!(out, "latest_runtime_to_ms={latest_runtime_to_ms}");
    }
    if let Some(message) = &manifest.failure_message {
        let _ = writeln!(out, "failure_message={message}");
    }
    out
}

pub fn render_paper_snapshot_text(snapshot: &PaperSessionSnapshot) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "session_id={}", snapshot.session_id);
    let _ = writeln!(out, "status={:?}", snapshot.status);
    let _ = writeln!(out, "health={:?}", snapshot.health);
    let _ = writeln!(out, "updated_at_ms={}", snapshot.updated_at_ms);
    if let Some(latest_runtime_to_ms) = snapshot.latest_runtime_to_ms {
        let _ = writeln!(out, "latest_runtime_to_ms={latest_runtime_to_ms}");
    }
    if let Some(latest_closed_bar_time_ms) = snapshot.latest_closed_bar_time_ms {
        let _ = writeln!(out, "latest_closed_bar_time_ms={latest_closed_bar_time_ms}");
    }
    if let Some(summary) = &snapshot.summary {
        let _ = writeln!(out, "ending_equity={:.2}", summary.ending_equity);
        let _ = writeln!(out, "total_return_pct={:.2}", summary.total_return * 100.0);
        let _ = writeln!(out, "trade_count={}", summary.trade_count);
        let _ = writeln!(out, "win_rate_pct={:.2}", summary.win_rate * 100.0);
        let _ = writeln!(out, "max_drawdown={:.2}", summary.max_drawdown);
    }
    let _ = writeln!(out, "open_positions={}", snapshot.open_positions.len());
    let _ = writeln!(out, "open_order_count={}", snapshot.open_order_count);
    let _ = writeln!(out, "fill_count={}", snapshot.fill_count);
    let _ = writeln!(out, "trade_count={}", snapshot.trade_count);
    if !snapshot.feed_snapshots.is_empty() {
        out.push_str("feeds\n");
        for feed in &snapshot.feed_snapshots {
            let top = feed
                .top_of_book
                .as_ref()
                .map(|snapshot| {
                    format!(
                        "bid={:.8} ask={:.8} mid={:.8} state={:?}",
                        snapshot.best_bid, snapshot.best_ask, snapshot.mid_price, snapshot.state
                    )
                })
                .unwrap_or_else(|| "missing".to_string());
            let last = feed
                .last_price
                .as_ref()
                .map(|snapshot| format!("{:.8}/{:?}", snapshot.price, snapshot.state))
                .unwrap_or_else(|| "none".to_string());
            let mark = feed
                .mark_price
                .as_ref()
                .map(|snapshot| format!("{:.8}/{:?}", snapshot.price, snapshot.state))
                .unwrap_or_else(|| "none".to_string());
            let _ = writeln!(
                out,
                "alias={} template={} symbol={} top_of_book={} last={} mark={} valuation_source={}",
                feed.execution_alias,
                feed.template.as_str(),
                feed.symbol,
                top,
                last,
                mark,
                feed.valuation_source
                    .map(|source| format!("{source:?}"))
                    .unwrap_or_else(|| "none".to_string())
            );
        }
    }
    if let Some(message) = &snapshot.failure_message {
        let _ = writeln!(out, "failure_message={message}");
    }
    out
}

pub fn render_paper_positions_text(session_id: &str, positions: &[PositionSnapshot]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "session_id={session_id}");
    for position in positions {
        let _ = writeln!(
            out,
            "alias={} side={} quantity={:.8} entry_price={:.8} market_price={:.8} unrealized_pnl={:.8}",
            position.execution_alias,
            fmt_position_side(position.side),
            position.quantity,
            position.entry_price,
            position.market_price,
            position.unrealized_pnl,
        );
    }
    out
}

pub fn render_paper_export_text(
    session_id: &str,
    orders: &[OrderRecord],
    fills: &[Fill],
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "session_id={session_id}");
    if !orders.is_empty() {
        out.push_str("Orders\n");
        for order in orders {
            let _ = writeln!(
                out,
                "id={} alias={} role={} kind={:?} status={:?}",
                order.id,
                order.execution_alias,
                fmt_signal_role(order.role),
                order.kind,
                order.status
            );
        }
    }
    if !fills.is_empty() {
        out.push_str("Fills\n");
        for fill in fills {
            let _ = writeln!(
                out,
                "alias={} bar={} time={} action={:?} quantity={:.8} price={:.8}",
                fill.execution_alias,
                fill.bar_index,
                fmt_opt_f64(Some(fill.time)),
                fill.action,
                fill.quantity,
                fill.price
            );
        }
    }
    out
}

pub fn render_paper_export_text_full(export: &PaperSessionExport) -> String {
    let mut out = render_paper_manifest_text(&export.manifest);
    if let Some(snapshot) = &export.snapshot {
        out.push('\n');
        out.push_str(&render_paper_snapshot_text(snapshot));
    }
    if let Some(result) = &export.latest_result {
        out.push('\n');
        out.push_str(&render_paper_export_text(
            &export.manifest.session_id,
            &result.orders,
            &result.fills,
        ));
    }
    out
}

pub fn render_paper_logs_text(session_id: &str, logs: &[PaperSessionLogEvent]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "session_id={session_id}");
    for event in logs {
        let _ = writeln!(
            out,
            "time_ms={} status={:?} health={:?} latest_runtime_to_ms={} message={}",
            event.time_ms,
            event.status,
            event.health,
            event
                .latest_runtime_to_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            event.message
        );
    }
    out
}

pub fn render_execution_daemon_status_text(status: &ExecutionDaemonStatus) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "pid={}", status.pid);
    let _ = writeln!(out, "started_at_ms={}", status.started_at_ms);
    let _ = writeln!(out, "updated_at_ms={}", status.updated_at_ms);
    let _ = writeln!(out, "poll_interval_ms={}", status.poll_interval_ms);
    let _ = writeln!(out, "once={}", status.once);
    let _ = writeln!(out, "running={}", status.running);
    let _ = writeln!(out, "stop_requested={}", status.stop_requested);
    let _ = writeln!(out, "subscription_count={}", status.subscription_count);
    let _ = writeln!(out, "state_root={}", status.state_root);
    let _ = writeln!(out, "active_sessions={}", status.active_sessions.join(","));
    out
}

pub fn render_walk_forward_text(result: &WalkForwardResult) -> String {
    let mut out = String::new();
    let summary = &result.stitched_summary;
    out.push_str("Walk-Forward Summary\n");
    let _ = writeln!(out, "segment_count={}", summary.segment_count);
    let _ = writeln!(out, "starting_equity={:.2}", summary.starting_equity);
    let _ = writeln!(out, "ending_equity={:.2}", summary.ending_equity);
    let _ = writeln!(out, "total_return_pct={:.2}", summary.total_return * 100.0);
    let _ = writeln!(out, "max_drawdown={:.2}", summary.max_drawdown);
    let _ = writeln!(out, "trade_count={}", summary.trade_count);
    let _ = writeln!(out, "win_rate_pct={:.2}", summary.win_rate * 100.0);
    let _ = writeln!(
        out,
        "positive_segment_count={}",
        summary.positive_segment_count
    );
    let _ = writeln!(
        out,
        "negative_segment_count={}",
        summary.negative_segment_count
    );
    let _ = writeln!(
        out,
        "average_segment_return_pct={:.2}",
        summary.average_segment_return * 100.0
    );

    out.push_str("Walk-Forward Config\n");
    let _ = writeln!(out, "train_bars={}", result.config.train_bars);
    let _ = writeln!(out, "test_bars={}", result.config.test_bars);
    let _ = writeln!(out, "step_bars={}", result.config.step_bars);
    let _ = writeln!(
        out,
        "execution_source={}",
        result.config.backtest.execution_source_alias
    );
    render_overfitting_risk_text(&mut out, "Overfitting Risk", &result.overfitting_risk);

    if !result.segments.is_empty() {
        out.push_str("Recent Segments\n");
        let start = result.segments.len().saturating_sub(5);
        for segment in &result.segments[start..] {
            let _ = writeln!(
                out,
                "index={} train_from={} train_to={} test_from={} test_to={} train_return_pct={:.2} test_return_pct={:.2} trade_count={} win_rate_pct={:.2} max_drawdown={:.2} protect_exit_count={} target_exit_count={} liquidation_exit_count={} flat_bar_pct={:.2}",
                segment.segment_index,
                segment.train_from,
                segment.train_to,
                segment.test_from,
                segment.test_to,
                segment.in_sample.total_return * 100.0,
                segment.out_of_sample.total_return * 100.0,
                segment.out_of_sample.trade_count,
                segment.out_of_sample.win_rate * 100.0,
                segment.out_of_sample.max_drawdown,
                segment.out_of_sample_diagnostics.summary.protect_exit_count,
                segment.out_of_sample_diagnostics.summary.target_exit_count,
                segment.out_of_sample_diagnostics.summary.liquidation_exit_count,
                segment.out_of_sample_diagnostics.capture_summary.flat_bar_pct * 100.0,
            );
            if !segment.out_of_sample_diagnostics.drift_flags.is_empty() {
                let _ = writeln!(
                    out,
                    "drift_flags={}",
                    segment
                        .out_of_sample_diagnostics
                        .drift_flags
                        .iter()
                        .map(|flag| format!("{flag:?}"))
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
        }
    }

    if !result.segments.is_empty() {
        let mut weakest = result.segments.iter().collect::<Vec<_>>();
        weakest.sort_by(|left, right| {
            left.out_of_sample
                .total_return
                .partial_cmp(&right.out_of_sample.total_return)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.push_str("Worst Segments\n");
        for segment in weakest.into_iter().take(3) {
            let top_export = segment
                .out_of_sample_diagnostics
                .export_summaries
                .iter()
                .find_map(|summary| match summary {
                    ExportDiagnosticSummary::Bool(summary) => {
                        Some(format!("{}:{}", summary.name, summary.true_count))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| "none".to_string());
            let _ = writeln!(
                out,
                "index={} test_from={} test_to={} test_return_pct={:.2} protect_exit_count={} target_exit_count={} liquidation_exit_count={} opportunity_event_count={} top_bool_export={}",
                segment.segment_index,
                segment.test_from,
                segment.test_to,
                segment.out_of_sample.total_return * 100.0,
                segment.out_of_sample_diagnostics.summary.protect_exit_count,
                segment.out_of_sample_diagnostics.summary.target_exit_count,
                segment.out_of_sample_diagnostics.summary.liquidation_exit_count,
                segment.out_of_sample_diagnostics.opportunity_event_count,
                top_export,
            );
        }
    }

    out
}

pub fn render_walk_forward_sweep_text(result: &WalkForwardSweepResult) -> String {
    let mut out = String::new();
    out.push_str("Walk-Forward Sweep Summary\n");
    let _ = writeln!(out, "candidate_count={}", result.candidate_count);
    let _ = writeln!(out, "objective={:?}", result.config.objective);
    let _ = writeln!(out, "top_n={}", result.config.top_n);
    let _ = writeln!(out, "train_bars={}", result.config.walk_forward.train_bars);
    let _ = writeln!(out, "test_bars={}", result.config.walk_forward.test_bars);
    let _ = writeln!(out, "step_bars={}", result.config.walk_forward.step_bars);

    out.push_str("Best Candidate\n");
    let _ = writeln!(
        out,
        "objective_score={:.6}",
        result.best_candidate.objective_score
    );
    let _ = writeln!(
        out,
        "ending_equity={:.2}",
        result.best_candidate.stitched_summary.ending_equity
    );
    let _ = writeln!(
        out,
        "total_return_pct={:.2}",
        result.best_candidate.stitched_summary.total_return * 100.0
    );
    let _ = writeln!(
        out,
        "max_drawdown={:.2}",
        result.best_candidate.stitched_summary.max_drawdown
    );
    let _ = writeln!(
        out,
        "overrides={}",
        fmt_input_overrides(&result.best_candidate.input_overrides)
    );

    out.push_str("Top Candidates\n");
    for (index, candidate) in result.top_candidates.iter().enumerate() {
        let _ = writeln!(
            out,
            "rank={} objective_score={:.6} ending_equity={:.2} total_return_pct={:.2} max_drawdown={:.2} overrides={}",
            index + 1,
            candidate.objective_score,
            candidate.stitched_summary.ending_equity,
            candidate.stitched_summary.total_return * 100.0,
            candidate.stitched_summary.max_drawdown,
            fmt_input_overrides(&candidate.input_overrides),
        );
    }

    out
}

pub fn render_optimize_text(result: &OptimizeResult, preset_out: Option<&Path>) -> String {
    let mut out = String::new();
    out.push_str("Optimization Summary\n");
    let _ = writeln!(out, "runner={:?}", result.config.runner);
    let _ = writeln!(out, "objective={:?}", result.config.objective);
    let _ = writeln!(out, "candidate_count={}", result.candidate_count);
    let _ = writeln!(out, "completed_trials={}", result.completed_trials);
    let _ = writeln!(out, "trials={}", result.config.trials);
    let _ = writeln!(out, "startup_trials={}", result.config.startup_trials);
    let _ = writeln!(out, "workers={}", result.config.workers);
    if let Some(path) = preset_out {
        let _ = writeln!(out, "preset_out={}", path.display());
    }

    out.push_str("Best Candidate\n");
    let _ = writeln!(out, "trial_id={}", result.best_candidate.trial_id);
    let _ = writeln!(
        out,
        "objective_score={:.6}",
        result.best_candidate.objective_score
    );
    render_optimize_candidate_summary(&mut out, &result.best_candidate);

    out.push_str("Top Candidates\n");
    for (index, candidate) in result.top_candidates.iter().enumerate() {
        let _ = writeln!(
            out,
            "rank={} trial_id={} objective_score={:.6} overrides={}",
            index + 1,
            candidate.trial_id,
            candidate.objective_score,
            fmt_input_overrides(&candidate.input_overrides)
        );
        match &candidate.summary {
            OptimizeEvaluationSummary::WalkForward {
                stitched_summary, ..
            } => {
                let _ = writeln!(
                    out,
                    "ending_equity={:.2} total_return_pct={:.2} max_drawdown={:.2}",
                    stitched_summary.ending_equity,
                    stitched_summary.total_return * 100.0,
                    stitched_summary.max_drawdown
                );
            }
            OptimizeEvaluationSummary::Backtest { summary, .. } => {
                let _ = writeln!(
                    out,
                    "ending_equity={:.2} total_return_pct={:.2} max_drawdown={:.2}",
                    summary.ending_equity,
                    summary.total_return * 100.0,
                    summary.max_drawdown
                );
            }
        }
    }

    if let Some(holdout) = &result.holdout {
        out.push_str("Holdout\n");
        let _ = writeln!(out, "bars={}", holdout.bars);
        let _ = writeln!(out, "from={}", holdout.from);
        let _ = writeln!(out, "to={}", holdout.to);
        let _ = writeln!(out, "trade_count={}", holdout.summary.trade_count);
        let _ = writeln!(
            out,
            "winning_trade_count={}",
            holdout.summary.winning_trade_count
        );
        let _ = writeln!(
            out,
            "losing_trade_count={}",
            holdout.summary.losing_trade_count
        );
        let _ = writeln!(out, "win_rate_pct={:.2}", holdout.summary.win_rate * 100.0);
        let _ = writeln!(out, "ending_equity={:.2}", holdout.summary.ending_equity);
        let _ = writeln!(
            out,
            "total_return_pct={:.2}",
            holdout.summary.total_return * 100.0
        );
        let _ = writeln!(out, "max_drawdown={:.2}", holdout.summary.max_drawdown);
        let _ = writeln!(
            out,
            "execution_asset_return_pct={:.2}",
            holdout.summary.execution_asset_return * 100.0
        );
        let _ = writeln!(
            out,
            "opportunity_cost_return_pct={:.2}",
            holdout.diagnostics.capture_summary.opportunity_cost_return * 100.0
        );
        out.push_str("Holdout Drift\n");
        let _ = writeln!(
            out,
            "total_return_delta_pct={:.2}",
            holdout.drift.total_return_delta * 100.0
        );
        let _ = writeln!(out, "trade_count_delta={}", holdout.drift.trade_count_delta);
        let _ = writeln!(
            out,
            "win_rate_delta_pct={:.2}",
            holdout.drift.win_rate_delta * 100.0
        );
        let _ = writeln!(
            out,
            "max_drawdown_delta={:.2}",
            holdout.drift.max_drawdown_delta
        );
    }

    if result.robustness.holdout_evaluated_count > 0 {
        out.push_str("Robustness\n");
        let _ = writeln!(
            out,
            "holdout_evaluated_count={}",
            result.robustness.holdout_evaluated_count
        );
        let _ = writeln!(
            out,
            "holdout_pass_count={}",
            result.robustness.holdout_pass_count
        );
        let _ = writeln!(
            out,
            "holdout_fail_count={}",
            result.robustness.holdout_fail_count
        );
        let _ = writeln!(
            out,
            "best_candidate_holdout_rank={}",
            result
                .robustness
                .best_candidate_holdout_rank
                .map(|value| value.to_string())
                .unwrap_or_else(|| "na".to_string())
        );
    }

    render_overfitting_risk_text(&mut out, "Overfitting Risk", &result.overfitting_risk);

    if !result.hints.is_empty() {
        out.push_str("Hints\n");
        for hint in &result.hints {
            let _ = writeln!(
                out,
                "kind={:?} metric={} value={}",
                hint.kind,
                hint.metric.as_deref().unwrap_or("na"),
                fmt_opt_f64(hint.value)
            );
        }
    }

    out
}

fn render_optimize_candidate_summary(out: &mut String, candidate: &OptimizeCandidateSummary) {
    let _ = writeln!(
        out,
        "overrides={}",
        fmt_input_overrides(&candidate.input_overrides)
    );
    match &candidate.summary {
        OptimizeEvaluationSummary::WalkForward {
            stitched_summary,
            zero_trade_segment_count,
            trade_count,
            win_rate,
            ..
        } => {
            let _ = writeln!(out, "runner_summary=walk_forward");
            let _ = writeln!(out, "ending_equity={:.2}", stitched_summary.ending_equity);
            let _ = writeln!(
                out,
                "total_return_pct={:.2}",
                stitched_summary.total_return * 100.0
            );
            let _ = writeln!(out, "max_drawdown={:.2}", stitched_summary.max_drawdown);
            let _ = writeln!(out, "segment_count={}", stitched_summary.segment_count);
            let _ = writeln!(out, "trade_count={}", trade_count);
            let _ = writeln!(out, "win_rate_pct={:.2}", win_rate * 100.0);
            let _ = writeln!(
                out,
                "negative_segment_count={}",
                stitched_summary.negative_segment_count
            );
            let _ = writeln!(out, "zero_trade_segment_count={zero_trade_segment_count}");
        }
        OptimizeEvaluationSummary::Backtest {
            summary,
            capture_summary,
        } => {
            let _ = writeln!(out, "runner_summary=backtest");
            let _ = writeln!(out, "ending_equity={:.2}", summary.ending_equity);
            let _ = writeln!(out, "total_return_pct={:.2}", summary.total_return * 100.0);
            let _ = writeln!(out, "max_drawdown={:.2}", summary.max_drawdown);
            let _ = writeln!(
                out,
                "execution_asset_return_pct={:.2}",
                capture_summary.execution_asset_return * 100.0
            );
        }
    }
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

fn fmt_input_optimization(
    optimization: Option<&palmscript::bytecode::InputOptimizationDecl>,
) -> String {
    match optimization {
        Some(optimization) => match &optimization.kind {
            palmscript::bytecode::InputOptimizationDeclKind::IntegerRange { low, high, step } => {
                format!("optimize(int, {low}, {high}, {step})")
            }
            palmscript::bytecode::InputOptimizationDeclKind::FloatRange { low, high, step } => {
                match step {
                    Some(step) => format!("optimize(float, {low}, {high}, {step})"),
                    None => format!("optimize(float, {low}, {high})"),
                }
            }
            palmscript::bytecode::InputOptimizationDeclKind::Choice { values } => format!(
                "optimize(choice, {})",
                values
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
        None => "none".to_string(),
    }
}

fn fmt_input_overrides(overrides: &std::collections::BTreeMap<String, f64>) -> String {
    overrides
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn fmt_value(value: &Value) -> String {
    match value {
        Value::F64(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::MaType(value) => format!("ma_type.{}", value.as_str()),
        Value::TimeInForce(value) => format!("tif.{}", value.as_str()),
        Value::TriggerReference(value) => format!("trigger_ref.{}", value.as_str()),
        Value::PositionSide(value) => format!("position_side.{}", value.as_str()),
        Value::ExitKind(value) => format!("exit_kind.{}", value.as_str()),
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

fn render_overfitting_risk_text(
    out: &mut String,
    title: &str,
    risk: &palmscript::OverfittingRiskSummary,
) {
    out.push_str(title);
    out.push('\n');
    let _ = writeln!(out, "level={:?}", risk.level);
    let _ = writeln!(out, "score={:.2}", risk.score);
    for reason in &risk.reasons {
        let _ = writeln!(
            out,
            "reason={:?} metric={} value={}",
            reason.kind,
            reason.metric.as_deref().unwrap_or("na"),
            fmt_opt_f64(reason.value)
        );
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
        SignalRole::LongEntry2 => "long_entry2",
        SignalRole::LongEntry3 => "long_entry3",
        SignalRole::LongExit => "long_exit",
        SignalRole::ShortEntry => "short_entry",
        SignalRole::ShortEntry2 => "short_entry2",
        SignalRole::ShortEntry3 => "short_entry3",
        SignalRole::ShortExit => "short_exit",
        SignalRole::ProtectLong => "protect_long",
        SignalRole::ProtectAfterTarget1Long => "protect_after_target1_long",
        SignalRole::ProtectAfterTarget2Long => "protect_after_target2_long",
        SignalRole::ProtectAfterTarget3Long => "protect_after_target3_long",
        SignalRole::ProtectShort => "protect_short",
        SignalRole::ProtectAfterTarget1Short => "protect_after_target1_short",
        SignalRole::ProtectAfterTarget2Short => "protect_after_target2_short",
        SignalRole::ProtectAfterTarget3Short => "protect_after_target3_short",
        SignalRole::TargetLong => "target_long",
        SignalRole::TargetLong2 => "target_long2",
        SignalRole::TargetLong3 => "target_long3",
        SignalRole::TargetShort => "target_short",
        SignalRole::TargetShort2 => "target_short2",
        SignalRole::TargetShort3 => "target_short3",
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
        BacktestResult, BacktestSummary, CompiledProgram, EquityPoint, ExportDiagnosticSummary,
        Fill, FillAction, OrderKind, OrderRecord, OrderStatus, OutputSample, OutputSeries,
        OutputValue, Outputs, PlotPoint, PlotSeries, PositionSide, SignalRole, Trade,
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
            inputs: vec![palmscript::bytecode::InputDecl {
                name: "fast_len".to_string(),
                ty: Type::F64,
                default_value: palmscript::Value::F64(21.0),
                optimization: Some(palmscript::bytecode::InputOptimizationDecl {
                    kind: palmscript::bytecode::InputOptimizationDeclKind::IntegerRange {
                        low: 8,
                        high: 34,
                        step: 1,
                    },
                }),
            }],
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
        assert!(rendered.contains("Inputs"));
        assert!(rendered.contains("optimization=optimize(int, 8, 34, 1)"));
        assert!(rendered.contains("Outputs"));
        assert!(rendered.contains("Instructions"));
    }

    #[test]
    fn render_backtest_text_includes_summary_and_recent_trades() {
        let result = BacktestResult {
            outputs: Outputs::default(),
            orders: vec![OrderRecord {
                id: 0,
                execution_alias: "spot".to_string(),
                role: SignalRole::LongEntry,
                kind: OrderKind::Market,
                action: FillAction::Buy,
                size_mode: None,
                size_fraction: None,
                requested_risk_pct: None,
                requested_stop_price: None,
                effective_risk_per_unit: None,
                capital_limited: false,
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
                execution_alias: "spot".to_string(),
                side: PositionSide::Long,
                quantity: 1.25,
                entry: Fill {
                    execution_alias: "spot".to_string(),
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
                    execution_alias: "spot".to_string(),
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
                net_exposure: 0.0,
                open_position_count: 0,
                long_position_count: 0,
                short_position_count: 0,
                free_collateral: None,
                isolated_margin: None,
                maintenance_margin: None,
                liquidation_price: None,
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
                max_net_exposure: 125.0,
                peak_open_position_count: 1,
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
                    protect_exit_count: 0,
                    target_exit_count: 0,
                    reversal_exit_count: 0,
                    liquidation_exit_count: 0,
                    by_order_kind: vec![],
                    by_side: vec![],
                },
                capture_summary: palmscript::BacktestCaptureSummary {
                    execution_asset_return: 0.10,
                    strategy_total_return: 0.012,
                    flat_bar_count: 1,
                    long_bar_count: 0,
                    short_bar_count: 0,
                    in_market_bar_count: 0,
                    flat_bar_pct: 1.0,
                    long_bar_pct: 0.0,
                    short_bar_pct: 0.0,
                    in_market_bar_pct: 0.0,
                    execution_return_while_flat: 0.10,
                    execution_return_while_long: 0.0,
                    execution_return_while_short: 0.0,
                    opportunity_cost_return: 0.10,
                },
                export_summaries: vec![ExportDiagnosticSummary::Bool(
                    palmscript::BoolExportDiagnosticSummary {
                        name: "trend_state".to_string(),
                        sample_count: 3,
                        na_count: 0,
                        true_count: 2,
                        false_count: 1,
                        rising_edge_count: 1,
                        falling_edge_count: 1,
                        true_while_flat_count: 1,
                        true_while_in_market_count: 1,
                        true_while_long_count: 1,
                        true_while_short_count: 0,
                        execution_return_while_true: 0.05,
                        execution_return_while_true_and_flat: 0.03,
                        trade_count: 1,
                        win_rate: 1.0,
                        average_realized_pnl: 12.0,
                        average_mae_pct: -0.02,
                        average_mfe_pct: 0.05,
                    },
                )],
                opportunity_events: vec![palmscript::OpportunityEvent {
                    execution_alias: "spot".to_string(),
                    kind: palmscript::OpportunityEventKind::ExportActivated,
                    name: "trend_state".to_string(),
                    role: None,
                    bar_index: 1,
                    time: 10.0,
                    position_snapshot: None,
                    feature_snapshot: None,
                    forward_returns: vec![palmscript::ForwardReturnMetric {
                        horizon_bars: 1,
                        return_pct: 0.1,
                        complete_window: true,
                    }],
                    forward_max_favorable_pct: Some(0.12),
                    forward_max_adverse_pct: Some(-0.02),
                }],
                per_bar_trace: vec![],
                cohorts: palmscript::CohortDiagnostics::default(),
                drawdown: palmscript::DrawdownDiagnostics::default(),
                source_alignment: palmscript::runtime::SourceAlignmentDiagnostics::default(),
                hints: vec![],
                overfitting_risk: palmscript::OverfittingRiskSummary {
                    level: palmscript::OverfittingRiskLevel::Unknown,
                    score: 0.0,
                    reasons: vec![palmscript::OverfittingRiskReason {
                        kind: palmscript::OverfittingRiskReasonKind::NoOutOfSampleValidation,
                        metric: None,
                        value: None,
                    }],
                },
                portfolio_mode: false,
                blocked_portfolio_entries: vec![],
            },
            open_position: None,
            open_positions: vec![],
            perp: None,
        };

        let rendered = render_backtest_text(&result);
        assert!(rendered.contains("Backtest Summary"));
        assert!(rendered.contains("starting_equity=1000.00"));
        assert!(rendered.contains("Order Summary"));
        assert!(rendered.contains("placed_count=1"));
        assert!(rendered.contains("Diagnostics Summary"));
        assert!(rendered.contains("order_fill_rate_pct=100.00"));
        assert!(rendered.contains("Overfitting Risk"));
        assert!(rendered.contains("level=Unknown"));
        assert!(rendered.contains("execution_asset_return_pct=10.00"));
        assert!(rendered.contains("Top Export States"));
        assert!(rendered.contains("Recent Opportunity Events"));
        assert!(rendered.contains("Recent Orders"));
        assert!(rendered.contains("role=long_entry"));
        assert!(rendered.contains("Recent Trades"));
        assert!(rendered.contains("side=long"));
        assert!(rendered.contains("Open Position"));
        assert!(rendered.contains("flat"));
    }
}
