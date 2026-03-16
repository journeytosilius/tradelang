mod accumulator;
mod analysis;
mod events;

pub(crate) use accumulator::{DiagnosticsAccumulator, OrderDiagnosticContext};
pub(crate) use analysis::{
    aggregate_arbitrage_diagnostics, aggregate_time_bucket_summaries,
    aggregate_transfer_diagnostics, build_arbitrage_diagnostics, build_backtest_hints,
    build_baseline_comparison, build_cohort_diagnostics, build_drawdown_diagnostics,
    build_transfer_diagnostics,
};
pub(crate) use events::{build_diagnostics_summary, build_order_diagnostics, snapshot_from_step};
