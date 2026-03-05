//! Core value and type definitions shared by the compiler and runtime.
//!
//! These enums define the typed boundary for scalar values, series references,
//! and local slot kinds used throughout the VM.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Type {
    F64,
    Bool,
    SeriesF64,
    SeriesBool,
    Void,
}

impl Type {
    pub const fn is_series(self) -> bool {
        matches!(self, Self::SeriesF64 | Self::SeriesBool)
    }

    pub const fn scalar(self) -> Option<Self> {
        match self {
            Self::SeriesF64 => Some(Self::F64),
            Self::SeriesBool => Some(Self::Bool),
            Self::F64 | Self::Bool | Self::Void => Some(self),
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
    NA,
    Void,
    SeriesRef(usize),
}

impl Value {
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::F64(_) => "f64",
            Self::Bool(_) => "bool",
            Self::NA => "na",
            Self::Void => "void",
            Self::SeriesRef(_) => "series-ref",
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
}
