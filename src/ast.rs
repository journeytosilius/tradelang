//! Typed abstract syntax tree nodes produced by the parser.
//!
//! This module defines the source-level statement and expression forms used
//! between parsing and compilation.

use crate::span::Span;
use crate::{Interval, MarketField, SourceTemplate};
use serde::{Deserialize, Serialize};

pub type NodeId = u32;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ast {
    pub strategy_intervals: StrategyIntervals,
    pub functions: Vec<FunctionDecl>,
    pub statements: Vec<Stmt>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StrategyIntervals {
    pub base: Vec<IntervalDecl>,
    pub sources: Vec<SourceDecl>,
    pub supplemental: Vec<SourceIntervalDecl>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IntervalDecl {
    pub interval: Interval,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceDecl {
    pub alias: String,
    pub alias_span: Span,
    pub template: SourceTemplate,
    pub template_span: Span,
    pub symbol: String,
    pub symbol_span: Span,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceIntervalDecl {
    pub source: String,
    pub source_span: Span,
    pub interval: Interval,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stmt {
    pub id: NodeId,
    pub span: Span,
    pub kind: StmtKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FunctionDecl {
    pub id: NodeId,
    pub name: String,
    pub name_span: Span,
    pub params: Vec<FunctionParam>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FunctionParam {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BindingName {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalRole {
    LongEntry,
    LongExit,
    ShortEntry,
    ShortExit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StmtKind {
    Let {
        name: String,
        name_span: Span,
        expr: Expr,
    },
    Const {
        name: String,
        name_span: Span,
        expr: Expr,
    },
    Input {
        name: String,
        name_span: Span,
        expr: Expr,
    },
    LetTuple {
        names: Vec<BindingName>,
        expr: Expr,
    },
    Export {
        name: String,
        name_span: Span,
        expr: Expr,
    },
    Trigger {
        name: String,
        name_span: Span,
        expr: Expr,
    },
    Signal {
        role: SignalRole,
        expr: Expr,
    },
    If {
        condition: Expr,
        then_block: Block,
        else_block: Block,
    },
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Expr {
    pub id: NodeId,
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExprKind {
    Number(f64),
    Bool(bool),
    Na,
    String(String),
    Ident(String),
    EnumVariant {
        namespace: String,
        namespace_span: Span,
        variant: String,
        variant_span: Span,
    },
    SourceSeries {
        source: String,
        source_span: Span,
        interval: Option<Interval>,
        field: MarketField,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Conditional {
        condition: Box<Expr>,
        when_true: Box<Expr>,
        when_false: Box<Expr>,
    },
    Call {
        callee: String,
        callee_span: Span,
        args: Vec<Expr>,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    And,
    Or,
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
