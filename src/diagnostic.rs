//! Compile-time and runtime error types used across the crate.
//!
//! Diagnostics preserve spans for source-level failures, while runtime errors
//! report VM faults such as stack underflow, type mismatches, and invalid
//! program state.

use crate::bytecode::OpCode;
use crate::span::Span;
use crate::Interval;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    Lex,
    Parse,
    Type,
    Compile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(kind: DiagnosticKind, message: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            message: message.into(),
            span,
        }
    }
}

#[derive(Debug, Error)]
#[error("compile failed with {diagnostics_len} diagnostic(s)")]
pub struct CompileError {
    pub diagnostics: Vec<Diagnostic>,
    diagnostics_len: usize,
}

impl CompileError {
    pub fn new(diagnostics: Vec<Diagnostic>) -> Self {
        let diagnostics_len = diagnostics.len();
        Self {
            diagnostics,
            diagnostics_len,
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum RuntimeError {
    #[error("instruction budget exhausted at bar {bar_index}, pc {pc}")]
    InstructionBudgetExceeded { bar_index: usize, pc: usize },
    #[error("stack underflow at pc {pc} while executing {opcode:?}")]
    StackUnderflow { pc: usize, opcode: OpCode },
    #[error("type mismatch at pc {pc}: expected {expected}, found {found}")]
    TypeMismatch {
        pc: usize,
        expected: &'static str,
        found: &'static str,
    },
    #[error("arity mismatch for builtin {builtin}: expected {expected}, found {found}")]
    ArityMismatch {
        builtin: &'static str,
        expected: usize,
        found: usize,
    },
    #[error("unknown builtin id {builtin_id}")]
    UnknownBuiltin { builtin_id: u16 },
    #[error("invalid jump target {target} at pc {pc}")]
    InvalidJump { pc: usize, target: usize },
    #[error("invalid local slot {slot}")]
    InvalidLocalSlot { slot: usize },
    #[error("invalid series slot {slot}")]
    InvalidSeriesSlot { slot: usize },
    #[error("script requires multi-interval runtime configuration")]
    MissingIntervalConfig,
    #[error("missing interval feed for {interval:?}")]
    MissingIntervalFeed { interval: Interval },
    #[error("duplicate interval feed for {interval:?}")]
    DuplicateIntervalFeed { interval: Interval },
    #[error("unexpected interval feed for {interval:?}")]
    UnexpectedIntervalFeed { interval: Interval },
    #[error("lower interval reference {referenced:?} is not allowed with base interval {base:?}")]
    LowerIntervalReference {
        base: Interval,
        referenced: Interval,
    },
    #[error("bar open time {open_time} is not aligned to interval {interval:?}")]
    InvalidIntervalAlignment { interval: Interval, open_time: i64 },
    #[error("interval feed {interval:?} is not strictly increasing at open time {open_time}")]
    UnsortedIntervalFeed { interval: Interval, open_time: i64 },
    #[error("interval feed {interval:?} contains a duplicate bar at open time {open_time}")]
    DuplicateIntervalBar { interval: Interval, open_time: i64 },
    #[error("required history {required} for slot {slot} exceeds max_history_capacity {limit}")]
    HistoryCapacityExceeded {
        slot: usize,
        required: usize,
        limit: usize,
    },
}
