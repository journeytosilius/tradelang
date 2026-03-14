//! Typed abstract syntax tree nodes produced by the parser.
//!
//! This module defines the source-level statement and expression forms used
//! between parsing and compilation.

use crate::span::Span;
use crate::{Interval, MarketField, SourceTemplate};
use crate::{LastExitField, LastExitScope, PositionEventField, PositionField, PositionSide};
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
    pub executions: Vec<ExecutionDecl>,
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
pub struct ExecutionDecl {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputOptimization {
    pub span: Span,
    pub kind: InputOptimizationKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum InputOptimizationKind {
    IntegerRange {
        low: i64,
        high: i64,
        step: i64,
    },
    FloatRange {
        low: f64,
        high: f64,
        step: Option<f64>,
    },
    Choice {
        values: Vec<f64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalRole {
    LongEntry,
    LongEntry2,
    LongEntry3,
    LongExit,
    ShortEntry,
    ShortEntry2,
    ShortEntry3,
    ShortExit,
    ProtectLong,
    ProtectAfterTarget1Long,
    ProtectAfterTarget2Long,
    ProtectAfterTarget3Long,
    ProtectShort,
    ProtectAfterTarget1Short,
    ProtectAfterTarget2Short,
    ProtectAfterTarget3Short,
    TargetLong,
    TargetLong2,
    TargetLong3,
    TargetShort,
    TargetShort2,
    TargetShort3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderSpec {
    pub span: Span,
    pub execution: Option<BindingName>,
    pub kind: OrderSpecKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OrderSpecKind {
    TemplateRef(BindingName),
    Market,
    Limit {
        price: Expr,
        tif: Expr,
        post_only: Expr,
    },
    StopMarket {
        trigger_price: Expr,
        trigger_ref: Expr,
    },
    StopLimit {
        trigger_price: Expr,
        limit_price: Expr,
        tif: Expr,
        post_only: Expr,
        trigger_ref: Expr,
        expire_time_ms: Expr,
    },
    TakeProfitMarket {
        trigger_price: Expr,
        trigger_ref: Expr,
    },
    TakeProfitLimit {
        trigger_price: Expr,
        limit_price: Expr,
        tif: Expr,
        post_only: Expr,
        trigger_ref: Expr,
        expire_time_ms: Expr,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskControlKind {
    Cooldown,
    MaxBarsInTrade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortfolioControlKind {
    MaxPositions,
    MaxLongPositions,
    MaxShortPositions,
    MaxGrossExposurePct,
    MaxNetExposurePct,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortfolioGroupDecl {
    pub name: String,
    pub name_span: Span,
    pub aliases: Vec<BindingName>,
    pub span: Span,
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
        optimization: Option<InputOptimization>,
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
    Regime {
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
    OrderTemplate {
        name: String,
        name_span: Span,
        spec: Box<OrderSpec>,
    },
    Order {
        role: SignalRole,
        spec: Box<OrderSpec>,
    },
    OrderSize {
        role: SignalRole,
        expr: Expr,
    },
    RiskControl {
        kind: RiskControlKind,
        side: PositionSide,
        expr: Expr,
    },
    PortfolioControl {
        kind: PortfolioControlKind,
        expr: Expr,
    },
    PortfolioGroup {
        group: PortfolioGroupDecl,
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
    PositionField {
        field: PositionField,
        field_span: Span,
    },
    PositionEventField {
        field: PositionEventField,
        field_span: Span,
    },
    LastExitField {
        scope: LastExitScope,
        field: LastExitField,
        field_span: Span,
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
