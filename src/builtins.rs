//! Builtin function identifiers shared by the compiler and VM.
//!
//! Builtins are lowered to stable numeric ids during compilation so the runtime
//! can dispatch deterministically without name lookups in the hot path.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum BuiltinId {
    Open = 0,
    High = 1,
    Low = 2,
    Close = 3,
    Volume = 4,
    Time = 5,
    Sma = 6,
    Ema = 7,
    Rsi = 8,
    Plot = 9,
}

impl BuiltinId {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "open" => Some(Self::Open),
            "high" => Some(Self::High),
            "low" => Some(Self::Low),
            "close" => Some(Self::Close),
            "volume" => Some(Self::Volume),
            "time" => Some(Self::Time),
            "sma" => Some(Self::Sma),
            "ema" => Some(Self::Ema),
            "rsi" => Some(Self::Rsi),
            "plot" => Some(Self::Plot),
            _ => None,
        }
    }

    pub fn from_u16(id: u16) -> Option<Self> {
        match id {
            0 => Some(Self::Open),
            1 => Some(Self::High),
            2 => Some(Self::Low),
            3 => Some(Self::Close),
            4 => Some(Self::Volume),
            5 => Some(Self::Time),
            6 => Some(Self::Sma),
            7 => Some(Self::Ema),
            8 => Some(Self::Rsi),
            9 => Some(Self::Plot),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::High => "high",
            Self::Low => "low",
            Self::Close => "close",
            Self::Volume => "volume",
            Self::Time => "time",
            Self::Sma => "sma",
            Self::Ema => "ema",
            Self::Rsi => "rsi",
            Self::Plot => "plot",
        }
    }
}
