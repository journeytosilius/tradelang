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
pub mod ide;
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
    run_backtest_with_sources, run_walk_forward_sweep_with_source, run_walk_forward_with_sources,
    BacktestCaptureSummary, BacktestConfig, BacktestDiagnosticSummary, BacktestDiagnostics,
    BacktestError, BacktestResult, BacktestSummary, BinanceUsdmRiskSnapshot, BinanceUsdmRiskSource,
    BoolExportDiagnosticSummary, EquityPoint, ExportDiagnosticSummary, ExportValueType,
    FeatureSnapshot, FeatureValue, Fill, FillAction, ForwardReturnMetric,
    HyperliquidPerpsRiskSnapshot, InputSweepDefinition, MarkPriceBasis,
    NumericExportDiagnosticSummary, OpportunityEvent, OpportunityEventKind, OrderDiagnostic,
    OrderEndReason, OrderRecord, OrderStatus, PerpBacktestConfig, PerpBacktestContext,
    PerpBacktestMetadata, PerpMarginMode, PositionSnapshot, RiskTier, SideDiagnosticSummary, Trade,
    TradeDiagnostic, TradeExitClassification, VenueRiskSnapshot, WalkForwardConfig,
    WalkForwardEquityPoint, WalkForwardResult, WalkForwardSegmentDiagnostics,
    WalkForwardSegmentResult, WalkForwardStitchedSummary, WalkForwardSweepCandidateSummary,
    WalkForwardSweepConfig, WalkForwardSweepError, WalkForwardSweepObjective,
    WalkForwardSweepResult, WalkForwardWindowSummary,
};
pub use bytecode::{OutputDecl, OutputKind, SignalRole};
pub use compiler::{compile, compile_with_input_overrides, CompiledProgram};
pub use diagnostic::{CompileError, Diagnostic, DiagnosticKind, RuntimeError};
pub use exchange::{
    fetch_perp_backtest_context, fetch_source_runtime_config, ExchangeEndpoints, ExchangeFetchError,
};
pub use ide::{
    analyze_document, format_document, CompletionEntry, CompletionKind, DefinitionTarget,
    DocumentSymbolInfo, HoverInfo, SemanticDocument, Symbol, SymbolKind,
};
pub use interval::{
    DeclaredMarketSource, Interval, MarketBinding, MarketField, MarketSource, SourceIntervalRef,
    SourceTemplate, INTERVAL_SPECS,
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
