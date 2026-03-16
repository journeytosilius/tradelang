use std::collections::BTreeMap;

use crate::backtest::{
    average, ratio, ArbitrageBasketRecord, ArbitrageDiagnosticsSummary,
    ArbitragePairDiagnosticSummary, BacktestCaptureSummary, BacktestDiagnosticSummary,
    BacktestSummary, BaselineComparisonSummary, BoolExportActiveTradeSummary, CohortDiagnostics,
    DrawdownDiagnostics, EntryModuleDiagnosticSummary, EquityPoint,
    ExitClassificationDiagnosticSummary, ExportDiagnosticSummary, HoldingTimeBucket,
    HoldingTimeBucketSummary, HourDiagnosticSummary, ImprovementHint, ImprovementHintKind,
    SideDiagnosticSummary, SpotQuoteTransfer, TimeBucketUtcDiagnosticSummary, TradeDiagnostic,
    TradeExitClassification, TransferDiagnosticsSummary, TransferRouteDiagnosticSummary,
    WeekdayDiagnosticSummary,
};
use crate::position::PositionSide;

const TIME_BUCKET_HOURS_UTC: u8 = 4;

pub(crate) fn build_cohort_diagnostics(
    trade_diagnostics: &[TradeDiagnostic],
    export_summaries: &[ExportDiagnosticSummary],
) -> CohortDiagnostics {
    CohortDiagnostics {
        by_side: build_side_summaries(trade_diagnostics),
        by_exit_classification: build_exit_classification_summaries(trade_diagnostics),
        by_weekday_utc: build_weekday_summaries(trade_diagnostics),
        by_hour_utc: build_hour_summaries(trade_diagnostics),
        by_time_bucket_utc: build_time_bucket_summaries(trade_diagnostics),
        by_holding_time: build_holding_time_summaries(trade_diagnostics),
        by_active_export: build_active_export_summaries(trade_diagnostics, export_summaries),
        by_entry_module: build_entry_module_summaries(trade_diagnostics),
    }
}

pub(crate) fn aggregate_time_bucket_summaries<'a>(
    cohorts: impl IntoIterator<Item = &'a CohortDiagnostics>,
) -> Vec<TimeBucketUtcDiagnosticSummary> {
    let mut aggregates = BTreeMap::<(u8, u8), (usize, usize, f64)>::new();
    for cohort in cohorts {
        for summary in &cohort.by_time_bucket_utc {
            let entry = aggregates
                .entry((summary.start_hour_utc, summary.end_hour_utc))
                .or_insert((0, 0, 0.0));
            entry.0 += summary.trade_count;
            entry.1 += summary.winning_trade_count;
            entry.2 += summary.total_realized_pnl;
        }
    }

    aggregates
        .into_iter()
        .map(
            |(
                (start_hour_utc, end_hour_utc),
                (trade_count, winning_trade_count, total_realized_pnl),
            )| {
                TimeBucketUtcDiagnosticSummary {
                    start_hour_utc,
                    end_hour_utc,
                    trade_count,
                    winning_trade_count,
                    win_rate: ratio(winning_trade_count, trade_count),
                    total_realized_pnl,
                    average_realized_pnl: average_realized_pnl(total_realized_pnl, trade_count),
                }
            },
        )
        .collect()
}

pub(crate) fn build_transfer_diagnostics(
    transfers: &[SpotQuoteTransfer],
) -> TransferDiagnosticsSummary {
    let mut by_route = BTreeMap::<(String, String), Vec<&SpotQuoteTransfer>>::new();
    for transfer in transfers {
        by_route
            .entry((transfer.from_alias.clone(), transfer.to_alias.clone()))
            .or_default()
            .push(transfer);
    }

    TransferDiagnosticsSummary {
        quote_transfer_count: transfers.len(),
        completed_quote_transfer_count: transfers
            .iter()
            .filter(|transfer| transfer.completed_bar_index.is_some())
            .count(),
        pending_quote_transfer_count: transfers
            .iter()
            .filter(|transfer| transfer.completed_bar_index.is_none())
            .count(),
        total_quote_amount: transfers.iter().map(|transfer| transfer.amount).sum(),
        total_quote_fee: transfers.iter().map(|transfer| transfer.fee).sum(),
        average_delay_bars: average(transfers.iter().map(|transfer| transfer.delay_bars as f64)),
        by_route: by_route
            .into_iter()
            .map(
                |((from_alias, to_alias), route_transfers)| TransferRouteDiagnosticSummary {
                    from_alias,
                    to_alias,
                    transfer_count: route_transfers.len(),
                    completed_transfer_count: route_transfers
                        .iter()
                        .filter(|transfer| transfer.completed_bar_index.is_some())
                        .count(),
                    total_amount: route_transfers.iter().map(|transfer| transfer.amount).sum(),
                    total_fee: route_transfers.iter().map(|transfer| transfer.fee).sum(),
                    average_delay_bars: average(
                        route_transfers
                            .iter()
                            .map(|transfer| transfer.delay_bars as f64),
                    ),
                },
            )
            .collect(),
    }
}

pub(crate) fn aggregate_transfer_diagnostics<'a>(
    summaries: impl IntoIterator<Item = &'a TransferDiagnosticsSummary>,
) -> TransferDiagnosticsSummary {
    let collected = summaries.into_iter().collect::<Vec<_>>();
    let mut by_route = BTreeMap::<(String, String), (usize, usize, f64, f64, Vec<f64>)>::new();
    for summary in &collected {
        for route in &summary.by_route {
            let entry = by_route
                .entry((route.from_alias.clone(), route.to_alias.clone()))
                .or_insert((0, 0, 0.0, 0.0, Vec::new()));
            entry.0 += route.transfer_count;
            entry.1 += route.completed_transfer_count;
            entry.2 += route.total_amount;
            entry.3 += route.total_fee;
            entry.4.push(route.average_delay_bars);
        }
    }

    TransferDiagnosticsSummary {
        quote_transfer_count: collected
            .iter()
            .map(|summary| summary.quote_transfer_count)
            .sum(),
        completed_quote_transfer_count: collected
            .iter()
            .map(|summary| summary.completed_quote_transfer_count)
            .sum(),
        pending_quote_transfer_count: collected
            .iter()
            .map(|summary| summary.pending_quote_transfer_count)
            .sum(),
        total_quote_amount: collected
            .iter()
            .map(|summary| summary.total_quote_amount)
            .sum(),
        total_quote_fee: collected
            .iter()
            .map(|summary| summary.total_quote_fee)
            .sum(),
        average_delay_bars: average(
            collected
                .iter()
                .map(|summary| summary.average_delay_bars)
                .filter(|value| value.is_finite()),
        ),
        by_route: by_route
            .into_iter()
            .map(
                |(
                    (from_alias, to_alias),
                    (
                        transfer_count,
                        completed_transfer_count,
                        total_amount,
                        total_fee,
                        average_delay_values,
                    ),
                )| TransferRouteDiagnosticSummary {
                    from_alias,
                    to_alias,
                    transfer_count,
                    completed_transfer_count,
                    total_amount,
                    total_fee,
                    average_delay_bars: average(average_delay_values),
                },
            )
            .collect(),
    }
}

pub(crate) fn build_arbitrage_diagnostics(
    baskets: &[ArbitrageBasketRecord],
) -> ArbitrageDiagnosticsSummary {
    let mut by_pair = BTreeMap::<(String, String), Vec<&ArbitrageBasketRecord>>::new();
    for basket in baskets {
        by_pair
            .entry((basket.buy_alias.clone(), basket.sell_alias.clone()))
            .or_default()
            .push(basket);
    }

    ArbitrageDiagnosticsSummary {
        basket_count: baskets.len(),
        completed_basket_count: baskets
            .iter()
            .filter(|basket| basket.exit_bar_index.is_some())
            .count(),
        open_basket_count: baskets
            .iter()
            .filter(|basket| basket.exit_bar_index.is_none())
            .count(),
        total_realized_pnl: baskets
            .iter()
            .filter_map(|basket| basket.realized_pnl)
            .sum(),
        average_entry_spread_bps: average(baskets.iter().map(|basket| basket.entry_spread_bps)),
        average_exit_spread_bps: average(
            baskets.iter().filter_map(|basket| basket.exit_spread_bps),
        ),
        average_holding_bars: average(
            baskets
                .iter()
                .filter_map(|basket| basket.holding_bars.map(|bars| bars as f64)),
        ),
        by_pair: by_pair
            .into_iter()
            .map(
                |((buy_alias, sell_alias), pair_baskets)| ArbitragePairDiagnosticSummary {
                    buy_alias,
                    sell_alias,
                    basket_count: pair_baskets.len(),
                    completed_basket_count: pair_baskets
                        .iter()
                        .filter(|basket| basket.exit_bar_index.is_some())
                        .count(),
                    total_realized_pnl: pair_baskets
                        .iter()
                        .filter_map(|basket| basket.realized_pnl)
                        .sum(),
                    average_entry_spread_bps: average(
                        pair_baskets.iter().map(|basket| basket.entry_spread_bps),
                    ),
                    average_exit_spread_bps: average(
                        pair_baskets
                            .iter()
                            .filter_map(|basket| basket.exit_spread_bps),
                    ),
                    average_holding_bars: average(
                        pair_baskets
                            .iter()
                            .filter_map(|basket| basket.holding_bars.map(|bars| bars as f64)),
                    ),
                },
            )
            .collect(),
        baskets: baskets.to_vec(),
    }
}

pub(crate) fn aggregate_arbitrage_diagnostics<'a>(
    summaries: impl IntoIterator<Item = &'a ArbitrageDiagnosticsSummary>,
) -> ArbitrageDiagnosticsSummary {
    let collected = summaries.into_iter().collect::<Vec<_>>();
    let mut by_pair =
        BTreeMap::<(String, String), (usize, usize, f64, Vec<f64>, Vec<f64>, Vec<f64>)>::new();
    let mut baskets = Vec::new();
    for summary in &collected {
        baskets.extend(summary.baskets.iter().cloned());
        for pair in &summary.by_pair {
            let entry = by_pair
                .entry((pair.buy_alias.clone(), pair.sell_alias.clone()))
                .or_insert((0, 0, 0.0, Vec::new(), Vec::new(), Vec::new()));
            entry.0 += pair.basket_count;
            entry.1 += pair.completed_basket_count;
            entry.2 += pair.total_realized_pnl;
            entry.3.push(pair.average_entry_spread_bps);
            entry.4.push(pair.average_exit_spread_bps);
            entry.5.push(pair.average_holding_bars);
        }
    }

    ArbitrageDiagnosticsSummary {
        basket_count: collected.iter().map(|summary| summary.basket_count).sum(),
        completed_basket_count: collected
            .iter()
            .map(|summary| summary.completed_basket_count)
            .sum(),
        open_basket_count: collected
            .iter()
            .map(|summary| summary.open_basket_count)
            .sum(),
        total_realized_pnl: collected
            .iter()
            .map(|summary| summary.total_realized_pnl)
            .sum(),
        average_entry_spread_bps: average(
            collected
                .iter()
                .map(|summary| summary.average_entry_spread_bps)
                .filter(|value| value.is_finite()),
        ),
        average_exit_spread_bps: average(
            collected
                .iter()
                .map(|summary| summary.average_exit_spread_bps)
                .filter(|value| value.is_finite()),
        ),
        average_holding_bars: average(
            collected
                .iter()
                .map(|summary| summary.average_holding_bars)
                .filter(|value| value.is_finite()),
        ),
        by_pair: by_pair
            .into_iter()
            .map(
                |(
                    (buy_alias, sell_alias),
                    (
                        basket_count,
                        completed_basket_count,
                        total_realized_pnl,
                        entry_spreads,
                        exit_spreads,
                        holding_bars,
                    ),
                )| ArbitragePairDiagnosticSummary {
                    buy_alias,
                    sell_alias,
                    basket_count,
                    completed_basket_count,
                    total_realized_pnl,
                    average_entry_spread_bps: average(entry_spreads),
                    average_exit_spread_bps: average(exit_spreads),
                    average_holding_bars: average(holding_bars),
                },
            )
            .collect(),
        baskets,
    }
}

pub(crate) fn build_drawdown_diagnostics(equity_curve: &[EquityPoint]) -> DrawdownDiagnostics {
    if equity_curve.is_empty() {
        return DrawdownDiagnostics::default();
    }

    let mut peak_equity = f64::NEG_INFINITY;
    let mut last_peak_index = 0usize;
    let mut drawdown_start = None::<usize>;
    let mut longest_drawdown_bars = 0usize;
    let mut recovery_durations = Vec::new();

    for (index, point) in equity_curve.iter().enumerate() {
        if point.equity >= peak_equity {
            peak_equity = point.equity;
            if let Some(start) = drawdown_start.take() {
                recovery_durations.push(index.saturating_sub(start));
            }
            last_peak_index = index;
            continue;
        }

        let start = *drawdown_start.get_or_insert(index);
        longest_drawdown_bars = longest_drawdown_bars.max(index.saturating_sub(start) + 1);
    }

    let current_drawdown_bars = drawdown_start
        .map(|start| equity_curve.len().saturating_sub(start))
        .unwrap_or(0);
    let longest_stagnation_bars = equity_curve
        .iter()
        .enumerate()
        .map(|(index, _)| index.saturating_sub(last_peak_index))
        .max()
        .unwrap_or(0);

    DrawdownDiagnostics {
        longest_drawdown_bars,
        current_drawdown_bars,
        longest_stagnation_bars,
        average_recovery_bars: average(recovery_durations.into_iter().map(|bars| bars as f64)),
    }
}

pub(crate) fn build_baseline_comparison(
    summary: &BacktestSummary,
    capture_summary: &BacktestCaptureSummary,
) -> BaselineComparisonSummary {
    BaselineComparisonSummary {
        strategy_total_return: summary.total_return,
        flat_cash_return: 0.0,
        execution_asset_return: capture_summary.execution_asset_return,
        opportunity_cost_return: capture_summary.opportunity_cost_return,
        excess_return_vs_flat_cash: summary.total_return,
        excess_return_vs_execution_asset: summary.total_return
            - capture_summary.execution_asset_return,
    }
}

pub(crate) fn build_backtest_hints(
    summary: &BacktestSummary,
    diagnostics_summary: &BacktestDiagnosticSummary,
    cohorts: &CohortDiagnostics,
    drawdown: &DrawdownDiagnostics,
) -> Vec<ImprovementHint> {
    let mut hints = Vec::new();
    if summary.trade_count < 5 {
        hints.push(ImprovementHint {
            kind: ImprovementHintKind::TooFewTrades,
            metric: Some("trade_count".to_string()),
            value: Some(summary.trade_count as f64),
        });
    }
    if drawdown.longest_drawdown_bars > 64 {
        hints.push(ImprovementHint {
            kind: ImprovementHintKind::HighDrawdownDuration,
            metric: Some("longest_drawdown_bars".to_string()),
            value: Some(drawdown.longest_drawdown_bars as f64),
        });
    }
    let short_side = cohorts
        .by_side
        .iter()
        .find(|summary| summary.side == PositionSide::Short);
    let long_side = cohorts
        .by_side
        .iter()
        .find(|summary| summary.side == PositionSide::Long);
    if let (Some(short_side), Some(long_side)) = (short_side, long_side) {
        if short_side.trade_count > 0
            && long_side.trade_count > 0
            && short_side.average_realized_pnl < long_side.average_realized_pnl
            && short_side.win_rate + 0.10 < long_side.win_rate
        {
            hints.push(ImprovementHint {
                kind: ImprovementHintKind::ShortSideUnderperforms,
                metric: Some("average_realized_pnl_delta".to_string()),
                value: Some(short_side.average_realized_pnl - long_side.average_realized_pnl),
            });
        }
    }
    if diagnostics_summary.average_mfe_pct < diagnostics_summary.average_mae_pct.abs() {
        hints.push(ImprovementHint {
            kind: ImprovementHintKind::SignalQualityWeak,
            metric: Some("mfe_minus_mae_pct".to_string()),
            value: Some(diagnostics_summary.average_mfe_pct - diagnostics_summary.average_mae_pct),
        });
    }
    hints
}

fn build_side_summaries(trade_diagnostics: &[TradeDiagnostic]) -> Vec<SideDiagnosticSummary> {
    let mut summaries = Vec::new();
    for side in [PositionSide::Long, PositionSide::Short] {
        let matching = trade_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.side == side)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        summaries.push(SideDiagnosticSummary {
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
    summaries
}

fn build_exit_classification_summaries(
    trade_diagnostics: &[TradeDiagnostic],
) -> Vec<ExitClassificationDiagnosticSummary> {
    let mut summaries = Vec::new();
    for classification in [
        TradeExitClassification::Signal,
        TradeExitClassification::Protect,
        TradeExitClassification::Target,
        TradeExitClassification::Reversal,
        TradeExitClassification::Liquidation,
    ] {
        let matching = trade_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.exit_classification == classification)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        summaries.push(ExitClassificationDiagnosticSummary {
            classification,
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
        });
    }
    summaries
}

fn build_weekday_summaries(trade_diagnostics: &[TradeDiagnostic]) -> Vec<WeekdayDiagnosticSummary> {
    let mut summaries = Vec::new();
    for weekday in 0..7u8 {
        let matching = trade_diagnostics
            .iter()
            .filter(|diagnostic| weekday_utc(exit_time_ms(diagnostic)) == weekday)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        summaries.push(WeekdayDiagnosticSummary {
            weekday_utc: weekday,
            trade_count: matching.len(),
            win_rate: ratio(
                matching
                    .iter()
                    .filter(|diagnostic| diagnostic.realized_pnl > 0.0)
                    .count(),
                matching.len(),
            ),
            total_realized_pnl: matching
                .iter()
                .map(|diagnostic| diagnostic.realized_pnl)
                .sum(),
        });
    }
    summaries
}

fn build_hour_summaries(trade_diagnostics: &[TradeDiagnostic]) -> Vec<HourDiagnosticSummary> {
    let mut summaries = Vec::new();
    for hour in 0..24u8 {
        let matching = trade_diagnostics
            .iter()
            .filter(|diagnostic| hour_utc(exit_time_ms(diagnostic)) == hour)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        summaries.push(HourDiagnosticSummary {
            hour_utc: hour,
            trade_count: matching.len(),
            win_rate: ratio(
                matching
                    .iter()
                    .filter(|diagnostic| diagnostic.realized_pnl > 0.0)
                    .count(),
                matching.len(),
            ),
            total_realized_pnl: matching
                .iter()
                .map(|diagnostic| diagnostic.realized_pnl)
                .sum(),
        });
    }
    summaries
}

fn build_time_bucket_summaries(
    trade_diagnostics: &[TradeDiagnostic],
) -> Vec<TimeBucketUtcDiagnosticSummary> {
    let mut summaries = Vec::new();
    let bucket_width = TIME_BUCKET_HOURS_UTC as usize;
    for start_hour in (0..24usize).step_by(bucket_width) {
        let end_hour = (start_hour + bucket_width).min(24);
        let matching = trade_diagnostics
            .iter()
            .filter(|diagnostic| {
                let hour = hour_utc(exit_time_ms(diagnostic));
                hour >= start_hour as u8 && hour < end_hour as u8
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        let winning_trade_count = matching
            .iter()
            .filter(|diagnostic| diagnostic.realized_pnl > 0.0)
            .count();
        let total_realized_pnl = matching
            .iter()
            .map(|diagnostic| diagnostic.realized_pnl)
            .sum::<f64>();
        summaries.push(TimeBucketUtcDiagnosticSummary {
            start_hour_utc: start_hour as u8,
            end_hour_utc: end_hour as u8,
            trade_count: matching.len(),
            winning_trade_count,
            win_rate: ratio(winning_trade_count, matching.len()),
            total_realized_pnl,
            average_realized_pnl: average_realized_pnl(total_realized_pnl, matching.len()),
        });
    }
    summaries
}

fn build_holding_time_summaries(
    trade_diagnostics: &[TradeDiagnostic],
) -> Vec<HoldingTimeBucketSummary> {
    let buckets = [
        HoldingTimeBucket::Bars0To1,
        HoldingTimeBucket::Bars2To5,
        HoldingTimeBucket::Bars6To15,
        HoldingTimeBucket::Bars16To31,
        HoldingTimeBucket::Bars32Plus,
    ];
    let mut summaries = Vec::new();
    for bucket in buckets {
        let matching = trade_diagnostics
            .iter()
            .filter(|diagnostic| holding_time_bucket(diagnostic.bars_held) == bucket)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        summaries.push(HoldingTimeBucketSummary {
            bucket,
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
        });
    }
    summaries
}

fn build_active_export_summaries(
    trade_diagnostics: &[TradeDiagnostic],
    export_summaries: &[ExportDiagnosticSummary],
) -> Vec<BoolExportActiveTradeSummary> {
    let mut summaries = Vec::new();
    for summary in export_summaries {
        let ExportDiagnosticSummary::Bool(bool_summary) = summary else {
            continue;
        };
        let mut active_trade_count = 0usize;
        let mut inactive_trade_count = 0usize;
        let mut active_wins = 0usize;
        let mut inactive_wins = 0usize;
        for trade in trade_diagnostics {
            let active = trade
                .entry_snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot
                        .values
                        .iter()
                        .find(|value| value.name == bool_summary.name)
                })
                .and_then(|value| match value.value {
                    crate::output::OutputValue::Bool(flag) => Some(flag),
                    _ => None,
                })
                .unwrap_or(false);
            if active {
                active_trade_count += 1;
                if trade.realized_pnl > 0.0 {
                    active_wins += 1;
                }
            } else {
                inactive_trade_count += 1;
                if trade.realized_pnl > 0.0 {
                    inactive_wins += 1;
                }
            }
        }
        summaries.push(BoolExportActiveTradeSummary {
            name: bool_summary.name.clone(),
            active_trade_count,
            inactive_trade_count,
            active_win_rate: ratio(active_wins, active_trade_count),
            inactive_win_rate: ratio(inactive_wins, inactive_trade_count),
        });
    }
    summaries
}

fn build_entry_module_summaries(
    trade_diagnostics: &[TradeDiagnostic],
) -> Vec<EntryModuleDiagnosticSummary> {
    let mut buckets = BTreeMap::<String, Vec<&TradeDiagnostic>>::new();
    for diagnostic in trade_diagnostics {
        let Some(module) = diagnostic.entry_module.as_ref() else {
            continue;
        };
        buckets.entry(module.clone()).or_default().push(diagnostic);
    }

    buckets
        .into_iter()
        .map(|(name, matching)| EntryModuleDiagnosticSummary {
            name,
            trade_count: matching.len(),
            long_trade_count: matching
                .iter()
                .filter(|diagnostic| diagnostic.side == PositionSide::Long)
                .count(),
            short_trade_count: matching
                .iter()
                .filter(|diagnostic| diagnostic.side == PositionSide::Short)
                .count(),
            win_rate: ratio(
                matching
                    .iter()
                    .filter(|diagnostic| diagnostic.realized_pnl > 0.0)
                    .count(),
                matching.len(),
            ),
            total_realized_pnl: matching
                .iter()
                .map(|diagnostic| diagnostic.realized_pnl)
                .sum(),
            average_realized_pnl: average(
                matching.iter().map(|diagnostic| diagnostic.realized_pnl),
            ),
            average_bars_held: average(
                matching
                    .iter()
                    .map(|diagnostic| diagnostic.bars_held as f64),
            ),
        })
        .collect()
}

fn average_realized_pnl(total_realized_pnl: f64, trade_count: usize) -> f64 {
    if trade_count == 0 {
        0.0
    } else {
        total_realized_pnl / trade_count as f64
    }
}

fn exit_time_ms(diagnostic: &TradeDiagnostic) -> i64 {
    diagnostic
        .exit_snapshot
        .as_ref()
        .map(|snapshot| snapshot.time as i64)
        .or_else(|| {
            diagnostic
                .entry_snapshot
                .as_ref()
                .map(|snapshot| (snapshot.time + diagnostic.duration_ms) as i64)
        })
        .unwrap_or_default()
}

fn holding_time_bucket(bars_held: usize) -> HoldingTimeBucket {
    match bars_held {
        0..=1 => HoldingTimeBucket::Bars0To1,
        2..=5 => HoldingTimeBucket::Bars2To5,
        6..=15 => HoldingTimeBucket::Bars6To15,
        16..=31 => HoldingTimeBucket::Bars16To31,
        _ => HoldingTimeBucket::Bars32Plus,
    }
}

fn weekday_utc(time_ms: i64) -> u8 {
    let days = time_ms.div_euclid(86_400_000);
    ((days + 3).rem_euclid(7)) as u8
}

fn hour_utc(time_ms: i64) -> u8 {
    time_ms.rem_euclid(86_400_000).div_euclid(3_600_000) as u8
}
