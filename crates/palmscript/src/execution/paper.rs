use crate::backtest::{BacktestResult, OrderStatus};

use super::{PaperSessionManifest, PaperSessionSnapshot};

pub(crate) fn snapshot_from_result(
    manifest: &PaperSessionManifest,
    result: &BacktestResult,
    runtime_to_ms: i64,
    updated_at_ms: i64,
) -> PaperSessionSnapshot {
    let open_order_count = result
        .orders
        .iter()
        .filter(|order| order.status == OrderStatus::Open)
        .count();
    let filled_order_count = result
        .orders
        .iter()
        .filter(|order| order.status == OrderStatus::Filled)
        .count();
    let cancelled_order_count = result
        .orders
        .iter()
        .filter(|order| order.status == OrderStatus::Cancelled)
        .count();
    let rejected_order_count = result
        .orders
        .iter()
        .filter(|order| order.status == OrderStatus::Rejected)
        .count();
    let expired_order_count = result
        .orders
        .iter()
        .filter(|order| order.status == OrderStatus::Expired)
        .count();

    PaperSessionSnapshot {
        session_id: manifest.session_id.clone(),
        status: manifest.status,
        health: manifest.health,
        updated_at_ms,
        start_time_ms: manifest.start_time_ms,
        warmup_from_ms: manifest.warmup_from_ms,
        latest_runtime_to_ms: Some(runtime_to_ms),
        latest_closed_bar_time_ms: result.equity_curve.last().map(|point| point.time as i64),
        summary: Some(result.summary.clone()),
        diagnostics_summary: Some(result.diagnostics.summary.clone()),
        open_positions: result.open_positions.clone(),
        open_order_count,
        filled_order_count,
        cancelled_order_count,
        rejected_order_count,
        expired_order_count,
        fill_count: result.fills.len(),
        trade_count: result.trades.len(),
        failure_message: manifest.failure_message.clone(),
    }
}
