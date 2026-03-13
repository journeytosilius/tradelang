//! VM-driven execution support for PalmScript.
//!
//! The execution layer reuses the existing compiler, runtime, and backtest
//! engine to drive paper sessions over live exchange-backed data. v1 is a
//! polling closed-bar paper daemon built on the same deterministic VM and
//! order simulation path used by market/backtest mode.

mod daemon;
mod engine;
mod market_data;
mod paper;
mod state;
pub mod venue;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::backtest::{
    BacktestDiagnosticSummary, BacktestResult, BacktestSummary, DiagnosticsDetailMode,
    PerpMarginMode, PositionSnapshot,
};
use crate::exchange::ExchangeEndpoints;
use crate::runtime::VmLimits;

pub use daemon::{
    execution_daemon_status, request_execution_daemon_stop, serve_execution_daemon,
    ExecutionDaemonConfig, ExecutionDaemonStatus,
};
pub use state::{
    default_execution_state_root, list_paper_sessions, load_paper_session_export,
    load_paper_session_logs, load_paper_session_manifest, load_paper_session_snapshot,
    stop_paper_session,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Paper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionSessionStatus {
    Queued,
    Starting,
    WarmingUp,
    Live,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionSessionHealth {
    Starting,
    WarmingUp,
    Live,
    Degraded,
    Reconnecting,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaperSessionConfig {
    pub execution_source_aliases: Vec<String>,
    pub initial_capital: f64,
    pub fee_bps: f64,
    pub slippage_bps: f64,
    pub diagnostics_detail: DiagnosticsDetailMode,
    pub leverage: Option<f64>,
    pub margin_mode: Option<PerpMarginMode>,
    pub vm_limits: VmLimits,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaperSessionManifest {
    pub session_id: String,
    pub mode: ExecutionMode,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub start_time_ms: i64,
    pub status: ExecutionSessionStatus,
    pub health: ExecutionSessionHealth,
    pub stop_requested: bool,
    pub failure_message: Option<String>,
    pub script_path: Option<String>,
    pub script_sha256: String,
    pub base_interval: crate::Interval,
    pub history_capacity: usize,
    pub endpoints: ExchangeEndpoints,
    pub config: PaperSessionConfig,
    pub warmup_from_ms: Option<i64>,
    pub latest_runtime_to_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaperSessionSnapshot {
    pub session_id: String,
    pub status: ExecutionSessionStatus,
    pub health: ExecutionSessionHealth,
    pub updated_at_ms: i64,
    pub start_time_ms: i64,
    pub warmup_from_ms: Option<i64>,
    pub latest_runtime_to_ms: Option<i64>,
    pub latest_closed_bar_time_ms: Option<i64>,
    pub summary: Option<BacktestSummary>,
    pub diagnostics_summary: Option<BacktestDiagnosticSummary>,
    pub open_positions: Vec<PositionSnapshot>,
    pub open_order_count: usize,
    pub filled_order_count: usize,
    pub cancelled_order_count: usize,
    pub rejected_order_count: usize,
    pub expired_order_count: usize,
    pub fill_count: usize,
    pub trade_count: usize,
    pub failure_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaperSessionExport {
    pub manifest: PaperSessionManifest,
    pub snapshot: Option<PaperSessionSnapshot>,
    pub latest_result: Option<BacktestResult>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaperSessionLogEvent {
    pub time_ms: i64,
    pub status: ExecutionSessionStatus,
    pub health: ExecutionSessionHealth,
    pub message: String,
    pub latest_runtime_to_ms: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct SubmitPaperSession {
    pub source: String,
    pub script_path: Option<PathBuf>,
    pub config: PaperSessionConfig,
    pub start_time_ms: i64,
    pub endpoints: ExchangeEndpoints,
}

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("execution state root error: {0}")]
    StateRoot(String),
    #[error("execution state IO error at `{path}`: {message}")]
    Io { path: String, message: String },
    #[error("execution state JSON error at `{path}`: {message}")]
    Json { path: String, message: String },
    #[error("paper sessions require at least one `source` declaration")]
    MissingSources,
    #[error("paper sessions require a base interval declaration")]
    MissingBaseInterval,
    #[error("paper session `{session_id}` does not exist")]
    UnknownSession { session_id: String },
    #[error("paper session `{session_id}` has no snapshot yet")]
    MissingSnapshot { session_id: String },
    #[error("paper session `{session_id}` has no backtest result yet")]
    MissingResult { session_id: String },
    #[error("paper session `{session_id}` is already stopped")]
    AlreadyStopped { session_id: String },
    #[error("invalid paper session config: {message}")]
    InvalidConfig { message: String },
    #[error("paper session compile error: {0}")]
    Compile(String),
    #[error("paper session runtime error: {0}")]
    Runtime(String),
    #[error("paper session fetch error: {0}")]
    Fetch(String),
}

pub fn submit_paper_session(
    request: SubmitPaperSession,
) -> Result<PaperSessionManifest, ExecutionError> {
    state::submit_paper_session(request)
}

pub fn load_paper_session_script(session_id: &str) -> Result<String, ExecutionError> {
    state::load_paper_session_script(session_id)
}

pub(crate) fn append_log_event(
    session_id: &str,
    event: &PaperSessionLogEvent,
) -> Result<(), ExecutionError> {
    state::append_log_event(session_id, event)
}

pub(crate) fn persist_session_manifest(
    manifest: &PaperSessionManifest,
) -> Result<(), ExecutionError> {
    state::persist_session_manifest(manifest)
}

pub(crate) fn persist_session_snapshot(
    session_id: &str,
    snapshot: &PaperSessionSnapshot,
) -> Result<(), ExecutionError> {
    state::persist_session_snapshot(session_id, snapshot)
}

pub(crate) fn persist_session_result(
    session_id: &str,
    result: &BacktestResult,
) -> Result<(), ExecutionError> {
    state::persist_session_result(session_id, result)
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub(crate) fn render_snapshot_from_result(
    manifest: &PaperSessionManifest,
    result: &BacktestResult,
    runtime_to_ms: i64,
    updated_at_ms: i64,
) -> PaperSessionSnapshot {
    paper::snapshot_from_result(manifest, result, runtime_to_ms, updated_at_ms)
}
