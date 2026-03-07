//! Bytecode program representation for compiled PalmScript scripts.
//!
//! The compiler emits a [`Program`] made of typed locals, constants, and
//! fixed-layout instructions. The VM executes this representation directly.

use crate::span::Span;
use crate::types::{SlotKind, Type, Value};
use crate::{DeclaredMarketSource, MarketBinding, SourceIntervalRef};
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
    UnpackTuple,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputKind {
    #[default]
    ExportSeries,
    Trigger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalRole {
    LongEntry,
    LongExit,
    ShortEntry,
    ShortExit,
}

impl SignalRole {
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::LongEntry => "long_entry",
            Self::LongExit => "long_exit",
            Self::ShortEntry => "short_entry",
            Self::ShortExit => "short_exit",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputDecl {
    pub name: String,
    pub kind: OutputKind,
    pub signal_role: Option<SignalRole>,
    pub ty: Type,
    pub slot: u16,
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
    pub outputs: Vec<OutputDecl>,
    pub base_interval: Option<crate::Interval>,
    pub declared_sources: Vec<DeclaredMarketSource>,
    pub source_intervals: Vec<SourceIntervalRef>,
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
}

#[cfg(test)]
mod tests {
    use super::{Instruction, LocalInfo, OpCode, Program};
    use crate::span::{Position, Span};
    use crate::types::{SlotKind, Type};
    use crate::{
        DeclaredMarketSource, MarketBinding, MarketField, MarketSource, SourceIntervalRef,
    };

    #[test]
    fn instruction_builders_assign_operands_and_span() {
        let span = Span::new(Position::new(1, 1, 2), Position::new(3, 1, 4));
        let instruction = Instruction::new(OpCode::LoadConst)
            .with_a(1)
            .with_b(2)
            .with_c(3)
            .with_span(span);
        assert_eq!(instruction.opcode, OpCode::LoadConst);
        assert_eq!(instruction.a, 1);
        assert_eq!(instruction.b, 2);
        assert_eq!(instruction.c, 3);
        assert_eq!(instruction.span, Some(span));
    }

    #[test]
    fn local_info_helpers_set_expected_defaults() {
        let scalar = LocalInfo::scalar(Some("x".to_string()), Type::F64, false);
        assert_eq!(scalar.kind, SlotKind::Scalar);
        assert_eq!(scalar.history_capacity, 1);
        assert_eq!(scalar.update_mask, 0);
        assert_eq!(scalar.market_binding, None);

        let binding = MarketBinding {
            source: MarketSource::Named {
                source_id: 0,
                interval: None,
            },
            field: MarketField::Close,
        };
        let series = LocalInfo::series(
            Some("close".to_string()),
            Type::SeriesF64,
            true,
            7,
            Some(binding),
        );
        assert_eq!(series.kind, SlotKind::Series);
        assert_eq!(series.history_capacity, 2);
        assert_eq!(series.update_mask, 7);
        assert_eq!(series.market_binding, Some(binding));
    }

    #[test]
    fn program_default_has_no_sources() {
        let program = Program::default();
        assert!(program.declared_sources.is_empty());
        assert!(program.source_intervals.is_empty());
        let _ = (
            DeclaredMarketSource {
                id: 0,
                alias: "x".to_string(),
                template: crate::interval::SourceTemplate::BinanceSpot,
                symbol: "BTCUSDT".to_string(),
            },
            SourceIntervalRef {
                source_id: 0,
                interval: crate::Interval::Min1,
            },
        );
    }
}
