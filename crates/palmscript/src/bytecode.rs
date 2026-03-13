//! Bytecode program representation for compiled PalmScript scripts.
//!
//! The compiler emits a [`Program`] made of typed locals, constants, and
//! fixed-layout instructions. The VM executes this representation directly.

use crate::order::{OrderFieldKind, OrderKind, SizeMode, TimeInForce, TriggerReference};
use crate::position::{LastExitField, LastExitScope, PositionEventField, PositionField};
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalRole {
    #[default]
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

impl SignalRole {
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::LongEntry => "long_entry",
            Self::LongEntry2 => "long_entry2",
            Self::LongEntry3 => "long_entry3",
            Self::LongExit => "long_exit",
            Self::ShortEntry => "short_entry",
            Self::ShortEntry2 => "short_entry2",
            Self::ShortEntry3 => "short_entry3",
            Self::ShortExit => "short_exit",
            Self::ProtectLong => "protect_long",
            Self::ProtectAfterTarget1Long => "protect_after_target1_long",
            Self::ProtectAfterTarget2Long => "protect_after_target2_long",
            Self::ProtectAfterTarget3Long => "protect_after_target3_long",
            Self::ProtectShort => "protect_short",
            Self::ProtectAfterTarget1Short => "protect_after_target1_short",
            Self::ProtectAfterTarget2Short => "protect_after_target2_short",
            Self::ProtectAfterTarget3Short => "protect_after_target3_short",
            Self::TargetLong => "target_long",
            Self::TargetLong2 => "target_long2",
            Self::TargetLong3 => "target_long3",
            Self::TargetShort => "target_short",
            Self::TargetShort2 => "target_short2",
            Self::TargetShort3 => "target_short3",
        }
    }

    pub const fn is_attached_exit(self) -> bool {
        matches!(
            self,
            Self::ProtectLong
                | Self::ProtectAfterTarget1Long
                | Self::ProtectAfterTarget2Long
                | Self::ProtectAfterTarget3Long
                | Self::ProtectShort
                | Self::ProtectAfterTarget1Short
                | Self::ProtectAfterTarget2Short
                | Self::ProtectAfterTarget3Short
                | Self::TargetLong
                | Self::TargetLong2
                | Self::TargetLong3
                | Self::TargetShort
                | Self::TargetShort2
                | Self::TargetShort3
        )
    }

    pub const fn is_entry(self) -> bool {
        matches!(
            self,
            Self::LongEntry
                | Self::LongEntry2
                | Self::LongEntry3
                | Self::ShortEntry
                | Self::ShortEntry2
                | Self::ShortEntry3
        )
    }

    pub const fn is_target(self) -> bool {
        matches!(
            self,
            Self::TargetLong
                | Self::TargetLong2
                | Self::TargetLong3
                | Self::TargetShort
                | Self::TargetShort2
                | Self::TargetShort3
        )
    }

    pub const fn is_protect(self) -> bool {
        matches!(
            self,
            Self::ProtectLong
                | Self::ProtectAfterTarget1Long
                | Self::ProtectAfterTarget2Long
                | Self::ProtectAfterTarget3Long
                | Self::ProtectShort
                | Self::ProtectAfterTarget1Short
                | Self::ProtectAfterTarget2Short
                | Self::ProtectAfterTarget3Short
        )
    }

    pub const fn entry_stage(self) -> Option<u8> {
        match self {
            Self::LongEntry | Self::ShortEntry => Some(1),
            Self::LongEntry2 | Self::ShortEntry2 => Some(2),
            Self::LongEntry3 | Self::ShortEntry3 => Some(3),
            _ => None,
        }
    }

    pub const fn target_stage(self) -> Option<u8> {
        match self {
            Self::TargetLong | Self::TargetShort => Some(1),
            Self::TargetLong2 | Self::TargetShort2 => Some(2),
            Self::TargetLong3 | Self::TargetShort3 => Some(3),
            _ => None,
        }
    }

    pub const fn protect_stage(self) -> Option<u8> {
        match self {
            Self::ProtectLong | Self::ProtectShort => Some(0),
            Self::ProtectAfterTarget1Long | Self::ProtectAfterTarget1Short => Some(1),
            Self::ProtectAfterTarget2Long | Self::ProtectAfterTarget2Short => Some(2),
            Self::ProtectAfterTarget3Long | Self::ProtectAfterTarget3Short => Some(3),
            _ => None,
        }
    }

    pub const fn is_long(self) -> bool {
        matches!(
            self,
            Self::LongEntry
                | Self::LongEntry2
                | Self::LongEntry3
                | Self::LongExit
                | Self::ProtectLong
                | Self::ProtectAfterTarget1Long
                | Self::ProtectAfterTarget2Long
                | Self::ProtectAfterTarget3Long
                | Self::TargetLong
                | Self::TargetLong2
                | Self::TargetLong3
        )
    }

    pub const fn is_short(self) -> bool {
        !self.is_long()
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
pub struct OrderFieldDecl {
    pub name: String,
    pub role: SignalRole,
    pub kind: OrderFieldKind,
    pub slot: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionFieldDecl {
    pub field: PositionField,
    pub slot: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionEventFieldDecl {
    pub field: PositionEventField,
    pub slot: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastExitFieldDecl {
    pub scope: LastExitScope,
    pub field: LastExitField,
    pub slot: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputDecl {
    pub name: String,
    pub ty: Type,
    pub default_value: Value,
    pub optimization: Option<InputOptimizationDecl>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputOptimizationDecl {
    pub kind: InputOptimizationDeclKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum InputOptimizationDeclKind {
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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderDecl {
    pub role: SignalRole,
    pub kind: OrderKind,
    pub tif: Option<TimeInForce>,
    pub post_only: bool,
    pub trigger_ref: Option<TriggerReference>,
    pub size_mode: Option<SizeMode>,
    pub price_field_id: Option<u16>,
    pub trigger_price_field_id: Option<u16>,
    pub expire_time_field_id: Option<u16>,
    pub size_field_id: Option<u16>,
    pub risk_stop_field_id: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskControlKind {
    Cooldown,
    MaxBarsInTrade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskControlDecl {
    pub side: crate::position::PositionSide,
    pub kind: RiskControlKind,
    pub bars: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortfolioControlKind {
    MaxPositions,
    MaxLongPositions,
    MaxShortPositions,
    MaxGrossExposurePct,
    MaxNetExposurePct,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortfolioControlDecl {
    pub kind: PortfolioControlKind,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioGroupDecl {
    pub name: String,
    pub aliases: Vec<String>,
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
    pub inputs: Vec<InputDecl>,
    pub outputs: Vec<OutputDecl>,
    pub order_fields: Vec<OrderFieldDecl>,
    pub position_fields: Vec<PositionFieldDecl>,
    pub position_event_fields: Vec<PositionEventFieldDecl>,
    pub last_exit_fields: Vec<LastExitFieldDecl>,
    pub orders: Vec<OrderDecl>,
    pub risk_controls: Vec<RiskControlDecl>,
    pub portfolio_controls: Vec<PortfolioControlDecl>,
    pub portfolio_groups: Vec<PortfolioGroupDecl>,
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
