//! Core value and type definitions shared by the compiler and runtime.
//!
//! These enums define the typed boundary for scalar values, series references,
//! and local slot kinds used throughout the VM.

use serde::{Deserialize, Serialize};

use crate::order::{TimeInForce, TriggerReference};
use crate::position::{ExitKind, PositionSide};
use crate::talib::MaType;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Type {
    F64,
    Bool,
    MaType,
    TimeInForce,
    TriggerReference,
    PositionSide,
    ExitKind,
    SeriesF64,
    SeriesBool,
    Void,
}

impl Type {
    pub const fn type_name(self) -> &'static str {
        match self {
            Self::F64 => "f64",
            Self::Bool => "bool",
            Self::MaType => "ma-type",
            Self::TimeInForce => "time-in-force",
            Self::TriggerReference => "trigger-reference",
            Self::PositionSide => "position-side",
            Self::ExitKind => "exit-kind",
            Self::SeriesF64 => "series<f64>",
            Self::SeriesBool => "series<bool>",
            Self::Void => "void",
        }
    }

    pub const fn is_series(self) -> bool {
        matches!(self, Self::SeriesF64 | Self::SeriesBool)
    }

    pub const fn scalar(self) -> Option<Self> {
        match self {
            Self::SeriesF64 => Some(Self::F64),
            Self::SeriesBool => Some(Self::Bool),
            Self::F64
            | Self::Bool
            | Self::MaType
            | Self::TimeInForce
            | Self::TriggerReference
            | Self::PositionSide
            | Self::ExitKind
            | Self::Void => Some(self),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotKind {
    Scalar,
    Series,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    F64(f64),
    Bool(bool),
    MaType(MaType),
    TimeInForce(TimeInForce),
    TriggerReference(TriggerReference),
    PositionSide(PositionSide),
    ExitKind(ExitKind),
    NA,
    Void,
    SeriesRef(usize),
    Tuple2([Box<Value>; 2]),
    Tuple3([Box<Value>; 3]),
}

impl Value {
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::F64(_) => "f64",
            Self::Bool(_) => "bool",
            Self::MaType(_) => "ma-type",
            Self::TimeInForce(_) => "time-in-force",
            Self::TriggerReference(_) => "trigger-reference",
            Self::PositionSide(_) => "position-side",
            Self::ExitKind(_) => "exit-kind",
            Self::NA => "na",
            Self::Void => "void",
            Self::SeriesRef(_) => "series-ref",
            Self::Tuple2(_) => "tuple2",
            Self::Tuple3(_) => "tuple3",
        }
    }

    pub const fn is_na(&self) -> bool {
        matches!(self, Self::NA)
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::F64(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn tuple_len(&self) -> Option<usize> {
        match self {
            Self::Tuple2(_) => Some(2),
            Self::Tuple3(_) => Some(3),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Type, Value};
    use crate::order::{TimeInForce, TriggerReference};
    use crate::position::ExitKind;
    use crate::talib::MaType;

    #[test]
    fn type_helpers_distinguish_series_and_scalar_forms() {
        assert!(Type::SeriesF64.is_series());
        assert!(Type::SeriesBool.is_series());
        assert!(!Type::F64.is_series());
        assert_eq!(Type::SeriesF64.scalar(), Some(Type::F64));
        assert_eq!(Type::SeriesBool.scalar(), Some(Type::Bool));
        assert_eq!(Type::Bool.scalar(), Some(Type::Bool));
        assert_eq!(Type::MaType.scalar(), Some(Type::MaType));
        assert_eq!(Type::TimeInForce.scalar(), Some(Type::TimeInForce));
        assert_eq!(
            Type::TriggerReference.scalar(),
            Some(Type::TriggerReference)
        );
        assert_eq!(Type::ExitKind.scalar(), Some(Type::ExitKind));
        assert_eq!(Type::Void.scalar(), Some(Type::Void));
    }

    #[test]
    fn value_accessors_and_type_names_match_variants() {
        assert_eq!(Value::F64(1.5).type_name(), "f64");
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::MaType(MaType::Ema).type_name(), "ma-type");
        assert_eq!(
            Value::TimeInForce(TimeInForce::Gtc).type_name(),
            "time-in-force"
        );
        assert_eq!(
            Value::TriggerReference(TriggerReference::Last).type_name(),
            "trigger-reference"
        );
        assert_eq!(Value::ExitKind(ExitKind::Target).type_name(), "exit-kind");
        assert_eq!(Value::NA.type_name(), "na");
        assert_eq!(Value::Void.type_name(), "void");
        assert_eq!(Value::SeriesRef(3).type_name(), "series-ref");
        assert_eq!(
            Value::Tuple2([Box::new(Value::F64(1.0)), Box::new(Value::F64(2.0))]).type_name(),
            "tuple2"
        );

        assert_eq!(Value::F64(1.5).as_f64(), Some(1.5));
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
        assert_eq!(Value::Bool(true).as_f64(), None);
        assert_eq!(Value::F64(1.5).as_bool(), None);
        assert!(Value::NA.is_na());
        assert!(!Value::Void.is_na());
        assert_eq!(
            Value::Tuple3([
                Box::new(Value::F64(1.0)),
                Box::new(Value::F64(2.0)),
                Box::new(Value::F64(3.0))
            ])
            .tuple_len(),
            Some(3)
        );
    }
}
