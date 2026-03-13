use crate::backtest::{
    average, ratio, BacktestDiagnosticSummary, FeatureSnapshot, FeatureValue, OrderDiagnostic,
    OrderKindDiagnosticSummary, OrderRecord, OrderStatus, PositionSide, SideDiagnosticSummary,
    TradeDiagnostic, TradeExitClassification,
};
use crate::output::StepOutput;

use super::accumulator::OrderDiagnosticContext;

pub(crate) fn build_order_diagnostics(
    orders: &[OrderRecord],
    contexts: &[OrderDiagnosticContext],
) -> Vec<OrderDiagnostic> {
    orders
        .iter()
        .zip(contexts)
        .map(|(order, context)| OrderDiagnostic {
            execution_alias: order.execution_alias.clone(),
            order_id: order.id,
            role: order.role,
            kind: order.kind,
            status: order.status,
            end_reason: order.end_reason,
            signal_snapshot: context.signal_snapshot.clone(),
            placed_snapshot: context.placed_snapshot.clone(),
            fill_snapshot: context.fill_snapshot.clone(),
            placed_position: context.placed_position.clone(),
            fill_position: context.fill_position.clone(),
            bars_to_fill: order
                .fill_bar_index
                .map(|fill_bar_index| fill_bar_index.saturating_sub(order.placed_bar_index)),
            time_to_fill_ms: order
                .fill_time
                .map(|fill_time| fill_time - order.placed_time),
        })
        .collect()
}

pub(crate) fn build_diagnostics_summary(
    order_diagnostics: &[OrderDiagnostic],
    trade_diagnostics: &[TradeDiagnostic],
) -> BacktestDiagnosticSummary {
    let order_fill_rate = ratio(
        order_diagnostics
            .iter()
            .filter(|diagnostic| matches!(diagnostic.status, OrderStatus::Filled))
            .count(),
        order_diagnostics.len(),
    );
    let average_bars_to_fill = average(
        order_diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.bars_to_fill.map(|bars| bars as f64)),
    );
    let average_bars_held = average(
        trade_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.bars_held as f64),
    );
    let average_mae_pct = average(
        trade_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.mae_pct),
    );
    let average_mfe_pct = average(
        trade_diagnostics
            .iter()
            .map(|diagnostic| diagnostic.mfe_pct),
    );

    let mut by_order_kind = Vec::new();
    for kind in [
        crate::order::OrderKind::Market,
        crate::order::OrderKind::Limit,
        crate::order::OrderKind::StopMarket,
        crate::order::OrderKind::StopLimit,
        crate::order::OrderKind::TakeProfitMarket,
        crate::order::OrderKind::TakeProfitLimit,
    ] {
        let matching = order_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.kind == kind)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        let placed_count = matching.len();
        let filled_count = matching
            .iter()
            .filter(|diagnostic| matches!(diagnostic.status, OrderStatus::Filled))
            .count();
        let cancelled_count = matching
            .iter()
            .filter(|diagnostic| matches!(diagnostic.status, OrderStatus::Cancelled))
            .count();
        let rejected_count = matching
            .iter()
            .filter(|diagnostic| matches!(diagnostic.status, OrderStatus::Rejected))
            .count();
        let expired_count = matching
            .iter()
            .filter(|diagnostic| matches!(diagnostic.status, OrderStatus::Expired))
            .count();
        by_order_kind.push(OrderKindDiagnosticSummary {
            kind,
            placed_count,
            filled_count,
            cancelled_count,
            rejected_count,
            expired_count,
            fill_rate: ratio(filled_count, placed_count),
            average_bars_to_fill: average(
                matching
                    .iter()
                    .filter_map(|diagnostic| diagnostic.bars_to_fill.map(|bars| bars as f64)),
            ),
        });
    }

    let mut by_side = Vec::new();
    for side in [PositionSide::Long, PositionSide::Short] {
        let matching = trade_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.side == side)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        by_side.push(SideDiagnosticSummary {
            side,
            trade_count: matching.len(),
            win_rate: ratio(
                matching
                    .iter()
                    .filter(|diagnostic| diagnostic.realized_pnl > 0.0)
                    .count(),
                matching.len(),
            ),
            average_realized_pnl: average(
                matching.iter().map(|diagnostic| diagnostic.realized_pnl),
            ),
            average_bars_held: average(
                matching
                    .iter()
                    .map(|diagnostic| diagnostic.bars_held as f64),
            ),
            average_mae_pct: average(matching.iter().map(|diagnostic| diagnostic.mae_pct)),
            average_mfe_pct: average(matching.iter().map(|diagnostic| diagnostic.mfe_pct)),
        });
    }

    BacktestDiagnosticSummary {
        order_fill_rate,
        average_bars_to_fill,
        average_bars_held,
        average_mae_pct,
        average_mfe_pct,
        signal_exit_count: trade_diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic.exit_classification,
                    TradeExitClassification::Signal
                )
            })
            .count(),
        protect_exit_count: trade_diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic.exit_classification,
                    TradeExitClassification::Protect
                )
            })
            .count(),
        target_exit_count: trade_diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic.exit_classification,
                    TradeExitClassification::Target
                )
            })
            .count(),
        reversal_exit_count: trade_diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic.exit_classification,
                    TradeExitClassification::Reversal
                )
            })
            .count(),
        liquidation_exit_count: trade_diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic.exit_classification,
                    TradeExitClassification::Liquidation
                )
            })
            .count(),
        by_order_kind,
        by_side,
    }
}

pub(crate) fn snapshot_from_step(step: &StepOutput, time: f64) -> Option<FeatureSnapshot> {
    let bar_index = step
        .exports
        .first()
        .map(|sample| sample.bar_index)
        .unwrap_or_default();
    let values = step
        .exports
        .iter()
        .map(|sample| FeatureValue {
            name: sample.name.clone(),
            value: sample.value.clone(),
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(FeatureSnapshot {
            bar_index,
            time,
            values,
        })
    }
}
