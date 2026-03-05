//! Typed abstract syntax tree nodes produced by the parser.
//!
//! This module defines the source-level statement and expression forms used
//! between parsing and compilation.

use crate::span::Span;
use serde::{Deserialize, Serialize};

pub type NodeId = u32;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ast {
    pub statements: Vec<Stmt>,
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
pub enum StmtKind {
    Let {
        name: String,
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
    Ident(String),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        callee: String,
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
