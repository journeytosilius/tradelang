//! PalmScript is a deterministic DSL and bytecode VM for financial time-series
//! programs.
//!
//! The crate exposes the end-to-end compilation pipeline, runtime entrypoints,
//! and the typed data structures shared across lexer, parser, compiler, and VM
//! layers.

pub mod ast;
pub mod builtins;
pub mod bytecode;
pub mod compiler;
pub mod data_prep;
pub mod diagnostic;
pub mod ide;
mod indicators;
pub mod interval;
pub mod lexer;
pub mod output;
pub mod parser;
pub mod pipeline;
pub mod runtime;
pub mod span;
pub mod token;
pub mod types;
pub mod vm;

pub use bytecode::{ExternalInputKind, OutputDecl, OutputKind};
pub use compiler::{
    compile, compile_with_env, CompileEnvironment, CompiledProgram, ExternalInputDecl,
};
pub use data_prep::{infer_input_interval, prepare_csv_inputs_for_program, PreparedInputs};
pub use diagnostic::{CompileError, DataPrepError, Diagnostic, DiagnosticKind, RuntimeError};
pub use ide::{
    analyze_document, format_document, load_project_config, CompletionEntry, CompletionKind,
    ConfigError, DefinitionTarget, DocumentConfig, DocumentSymbolInfo, HoverInfo, ProjectConfig,
    SemanticDocument, Symbol, SymbolKind,
};
pub use interval::{Interval, MarketBinding, MarketField, MarketSource, INTERVAL_SPECS};
pub use output::{
    Alert, OutputSample, OutputSeries, OutputValue, Outputs, PlotPoint, PlotSeries, StepOutput,
    TriggerEvent,
};
pub use pipeline::{
    PipelineEdge, PipelineEngine, PipelineNodeOutput, PipelineNodeSpec, PipelineOutputs,
    PipelineSpec,
};
pub use runtime::{
    run, run_multi_interval, Bar, Engine, ExternalInputFrame, IntervalFeed, MultiIntervalConfig,
    VmLimits,
};
pub use span::{Position, Span};
pub use token::{Token, TokenKind};
pub use types::{Type, Value};
