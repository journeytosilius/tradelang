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
pub mod output;
pub mod parser;
pub mod runtime;
pub mod span;
pub mod talib;
pub mod token;
pub mod types;
pub mod vm;

pub use backtest::{
    run_backtest_with_sources, BacktestConfig, BacktestError, BacktestResult, BacktestSummary,
    EquityPoint, Fill, FillAction, PositionSide, PositionSnapshot, Trade,
};
pub use bytecode::{OutputDecl, OutputKind, SignalRole};
pub use compiler::{compile, CompiledProgram};
pub use diagnostic::{CompileError, Diagnostic, DiagnosticKind, RuntimeError};
pub use exchange::{fetch_source_runtime_config, ExchangeEndpoints, ExchangeFetchError};
pub use ide::{
    analyze_document, format_document, CompletionEntry, CompletionKind, DefinitionTarget,
    DocumentSymbolInfo, HoverInfo, SemanticDocument, Symbol, SymbolKind,
};
pub use interval::{
    DeclaredMarketSource, Interval, MarketBinding, MarketField, MarketSource, SourceIntervalRef,
    SourceTemplate, INTERVAL_SPECS,
};
pub use output::{
    Alert, OutputSample, OutputSeries, OutputValue, Outputs, PlotPoint, PlotSeries, StepOutput,
    TriggerEvent,
};
pub use runtime::{run_with_sources, Bar, Engine, SourceFeed, SourceRuntimeConfig, VmLimits};
pub use span::{Position, Span};
pub use talib::{MaType, TalibFlag, TalibFunctionMetadata, TalibGroup, TALIB_UPSTREAM_COMMIT};
pub use token::{Token, TokenKind};
pub use types::{Type, Value};
