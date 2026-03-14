//! PalmScript is a deterministic DSL and bytecode VM for financial time-series
//! programs.
//!
//! The crate exposes the end-to-end compilation pipeline, runtime entrypoints,
//! and the typed data structures shared across lexer, parser, compiler, and VM
//! layers.

pub mod ast;
pub mod backtest;
pub mod builtins;
pub mod bytecode;
pub mod compiler;
pub mod diagnostic;
pub mod exchange;
pub mod execution;
pub mod ide;
pub mod ide_lsp;
mod indicators;
pub mod interval;
pub mod lexer;
mod order;
pub mod output;
pub mod parser;
pub mod position;
pub mod runtime;
pub mod span;
pub mod talib;
pub mod token;
pub mod types;
pub mod vm;

pub use backtest::{
    run_backtest_with_sources, run_optimize_with_source, run_optimize_with_source_resume,
    run_walk_forward_sweep_with_source, run_walk_forward_with_sources, BacktestCaptureSummary,
    BacktestConfig, BacktestDiagnosticSummary, BacktestDiagnostics, BacktestError, BacktestResult,
    BacktestSummary, BoolExportDiagnosticSummary, CohortDiagnostics, DecisionReason,
    DiagnosticsDetailMode, DrawdownDiagnostics, EquityPoint, ExportDiagnosticSummary,
    ExportValueType, FeatureSnapshot, FeatureValue, FeeSchedule, Fill, FillAction,
    ForwardReturnMetric, InputSweepDefinition, NumericExportDiagnosticSummary, OpportunityEvent,
    OpportunityEventKind, OptimizeCandidateSummary, OptimizeConfig, OptimizeError,
    OptimizeEvaluationSummary, OptimizeHoldoutConfig, OptimizeHoldoutResult, OptimizeObjective,
    OptimizeParamSpace, OptimizePreset, OptimizeProgressEvent, OptimizeProgressListener,
    OptimizeProgressState, OptimizeResult, OptimizeResumeState, OptimizeRunner,
    OptimizeScheduledBatch, OptimizeScheduledTrial, OrderDiagnostic, OrderEndReason, OrderRecord,
    OrderStatus, OverfittingRiskLevel, OverfittingRiskReason, OverfittingRiskReasonKind,
    OverfittingRiskSummary, PerpBacktestConfig, PerpBacktestContext, PerpBacktestMetadata,
    PerpMarginMode, PositionSnapshot, SideDiagnosticSummary, Trade, TradeDiagnostic,
    TradeExitClassification, WalkForwardConfig, WalkForwardEquityPoint, WalkForwardResult,
    WalkForwardSegmentDiagnostics, WalkForwardSegmentResult, WalkForwardStitchedSummary,
    WalkForwardSweepCandidateSummary, WalkForwardSweepConfig, WalkForwardSweepError,
    WalkForwardSweepObjective, WalkForwardSweepResult, WalkForwardWindowSummary,
};
pub use bytecode::{OutputDecl, OutputKind, SignalRole};
pub use compiler::{compile, compile_with_input_overrides, CompiledProgram};
pub use diagnostic::{CompileError, Diagnostic, DiagnosticKind, RuntimeError};
pub use exchange::{
    fetch_perp_backtest_context, fetch_source_runtime_config, ExchangeEndpoints,
    ExchangeFetchError, MarkPriceBasis, RiskTier, VenueRiskSnapshot,
};
pub use execution::{
    default_execution_state_root, execution_daemon_status, list_paper_sessions,
    load_paper_session_export, load_paper_session_logs, load_paper_session_manifest,
    load_paper_session_script, load_paper_session_snapshot, request_execution_daemon_stop,
    serve_execution_daemon, stop_paper_session, submit_paper_session, ExecutionDaemonConfig,
    ExecutionDaemonStatus, ExecutionError, ExecutionMode, ExecutionSessionHealth,
    ExecutionSessionStatus, PaperSessionConfig, PaperSessionExport, PaperSessionLogEvent,
    PaperSessionManifest, PaperSessionSnapshot, SubmitPaperSession,
};
pub use ide::{
    analyze_document, format_document, highlight_document, CompletionEntry, CompletionKind,
    DefinitionTarget, DocumentSymbolInfo, HighlightKind, HighlightToken, HoverInfo,
    SemanticDocument, Symbol, SymbolKind,
};
pub use ide_lsp::{server_capabilities as lsp_server_capabilities, IdeLspSession, OpenDocument};
pub use interval::{
    DeclaredExecutionTarget, DeclaredMarketSource, Interval, MarketBinding, MarketField,
    MarketSource, SourceIntervalRef, SourceTemplate, INTERVAL_SPECS,
};
pub use order::{OrderFieldKind, OrderKind, SizeMode, TimeInForce, TriggerReference};
pub use output::{
    Alert, OrderFieldSample, OrderFieldSeries, OutputSample, OutputSeries, OutputValue, Outputs,
    PlotPoint, PlotSeries, StepOutput, TriggerEvent,
};
pub use position::{
    ExitKind, LastExitField, LastExitScope, PositionEventField, PositionField, PositionSide,
};
pub use runtime::{run_with_sources, Bar, Engine, SourceFeed, SourceRuntimeConfig, VmLimits};
pub use span::{Position, Span};
pub use talib::{MaType, TalibFlag, TalibFunctionMetadata, TalibGroup, TALIB_UPSTREAM_COMMIT};
pub use token::{Token, TokenKind};
pub use types::{Type, Value};
