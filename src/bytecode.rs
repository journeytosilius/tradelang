//! Bytecode program representation for compiled TradeLang scripts.
//!
//! The compiler emits a [`Program`] made of typed locals, constants, and
//! fixed-layout instructions. The VM executes this representation directly.

use crate::span::Span;
use crate::types::{SlotKind, Type, Value};
use crate::{MarketBinding, MarketSource};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpCode {
    LoadConst,
    LoadLocal,
    StoreLocal,
    LoadSeries,
    SeriesGet,
    Neg,
    Not,
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
    Pop,
    Jump,
    JumpIfFalse,
    CallBuiltin,
    Return,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Instruction {
    pub opcode: OpCode,
    pub a: u16,
    pub b: u16,
    pub c: u16,
    pub span: Option<Span>,
}

impl Instruction {
    pub fn new(opcode: OpCode) -> Self {
        Self {
            opcode,
            a: 0,
            b: 0,
            c: 0,
            span: None,
        }
    }

    pub fn with_a(mut self, a: u16) -> Self {
        self.a = a;
        self
    }

    pub fn with_b(mut self, b: u16) -> Self {
        self.b = b;
        self
    }

    pub fn with_c(mut self, c: u16) -> Self {
        self.c = c;
        self
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Constant {
    Value(Value),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LocalInfo {
    pub name: Option<String>,
    pub ty: Type,
    pub kind: SlotKind,
    pub hidden: bool,
    pub history_capacity: usize,
    pub update_mask: u32,
    pub market_binding: Option<MarketBinding>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub instructions: Vec<Instruction>,
    pub constants: Vec<Constant>,
    pub locals: Vec<LocalInfo>,
    pub history_capacity: usize,
    pub plot_count: usize,
}

impl LocalInfo {
    pub fn scalar(name: Option<String>, ty: Type, hidden: bool) -> Self {
        Self {
            name,
            ty,
            kind: SlotKind::Scalar,
            hidden,
            history_capacity: 1,
            update_mask: 0,
            market_binding: None,
        }
    }

    pub fn series(
        name: Option<String>,
        ty: Type,
        hidden: bool,
        update_mask: u32,
        market_binding: Option<MarketBinding>,
    ) -> Self {
        Self {
            name,
            ty,
            kind: SlotKind::Series,
            hidden,
            history_capacity: 2,
            update_mask,
            market_binding,
        }
    }

    pub const fn is_base_market(&self) -> bool {
        matches!(
            self.market_binding,
            Some(MarketBinding {
                source: MarketSource::Base,
                ..
            })
        )
    }
}
