//! TradeLang is a deterministic DSL and bytecode VM for financial time-series
//! programs.
//!
//! The crate exposes the end-to-end compilation pipeline, runtime entrypoints,
//! and the typed data structures shared across lexer, parser, compiler, and VM
//! layers.

pub mod ast;
pub mod builtins;
pub mod bytecode;
pub mod compiler;
pub mod diagnostic;
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
pub use diagnostic::{CompileError, Diagnostic, DiagnosticKind, RuntimeError};
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
