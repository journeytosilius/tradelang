mod accumulator;
mod analysis;
mod events;

pub(crate) use accumulator::{DiagnosticsAccumulator, OrderDiagnosticContext};
pub(crate) use analysis::{
    aggregate_time_bucket_summaries, build_backtest_hints, build_baseline_comparison,
    build_cohort_diagnostics, build_drawdown_diagnostics,
};
pub(crate) use events::{build_diagnostics_summary, build_order_diagnostics, snapshot_from_step};
