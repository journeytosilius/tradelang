use crate::backtest::{BacktestResult, OrderStatus};
use crate::position::PositionSide;

use super::{
    FeedSnapshotState, PaperFeedSnapshot, PaperSessionManifest, PaperSessionSnapshot,
    ValuationPriceSource,
};

pub(crate) fn snapshot_from_result(
    manifest: &PaperSessionManifest,
    result: &BacktestResult,
    runtime_to_ms: i64,
    updated_at_ms: i64,
    feed_snapshots: &[PaperFeedSnapshot],
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

    let mut open_positions = result.open_positions.clone();
    let mut summary = result.summary.clone();
    let previous_unrealized = summary.unrealized_pnl;
    summary.unrealized_pnl = 0.0;
    for position in &mut open_positions {
        if let Some(feed) = feed_snapshots
            .iter()
            .find(|feed| feed.execution_alias == position.execution_alias)
        {
            if let Some((market_price, _)) = valuation_price(feed) {
                position.market_price = market_price;
                position.unrealized_pnl = unrealized_pnl(
                    position.side,
                    position.quantity,
                    position.entry_price,
                    market_price,
                );
            }
        }
        summary.unrealized_pnl += position.unrealized_pnl;
    }
    summary.ending_equity = summary.ending_equity - previous_unrealized + summary.unrealized_pnl;
    if summary.starting_equity.abs() > f64::EPSILON {
        summary.total_return =
            (summary.ending_equity - summary.starting_equity) / summary.starting_equity;
    }

    PaperSessionSnapshot {
        session_id: manifest.session_id.clone(),
        status: manifest.status,
        health: manifest.health,
        updated_at_ms,
        start_time_ms: manifest.start_time_ms,
        warmup_from_ms: manifest.warmup_from_ms,
        latest_runtime_to_ms: Some(runtime_to_ms),
        latest_closed_bar_time_ms: result.equity_curve.last().map(|point| point.time as i64),
        summary: Some(summary),
        diagnostics_summary: Some(result.diagnostics.summary.clone()),
        open_positions,
        feed_snapshots: feed_snapshots.to_vec(),
        feed_summary: manifest.feed_summary.clone(),
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

fn valuation_price(feed: &PaperFeedSnapshot) -> Option<(f64, ValuationPriceSource)> {
    match feed.valuation_source.unwrap_or(ValuationPriceSource::Mid) {
        ValuationPriceSource::Mark => feed
            .mark_price
            .as_ref()
            .filter(|snapshot| snapshot.state == FeedSnapshotState::Live)
            .map(|snapshot| (snapshot.price, ValuationPriceSource::Mark))
            .or_else(|| {
                feed.top_of_book
                    .as_ref()
                    .filter(|snapshot| snapshot.state == FeedSnapshotState::Live)
                    .map(|snapshot| (snapshot.mid_price, ValuationPriceSource::Mid))
            }),
        ValuationPriceSource::Mid | ValuationPriceSource::Candle => feed
            .top_of_book
            .as_ref()
            .filter(|snapshot| snapshot.state == FeedSnapshotState::Live)
            .map(|snapshot| (snapshot.mid_price, ValuationPriceSource::Mid)),
    }
}

fn unrealized_pnl(side: PositionSide, quantity: f64, entry_price: f64, market_price: f64) -> f64 {
    match side {
        PositionSide::Long => (market_price - entry_price) * quantity,
        PositionSide::Short => (entry_price - market_price) * quantity,
    }
}
