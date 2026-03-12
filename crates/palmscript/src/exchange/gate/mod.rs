pub mod spot;
pub mod usdt_perps;

pub use usdt_perps::{UsdtPerpsRiskSnapshot, UsdtPerpsRiskSource};

use crate::interval::Interval;

pub(crate) fn spot_interval_text(interval: Interval) -> Option<&'static str> {
    match interval {
        Interval::Sec1 => Some("1s"),
        Interval::Min1 => Some("1m"),
        Interval::Min5 => Some("5m"),
        Interval::Min15 => Some("15m"),
        Interval::Min30 => Some("30m"),
        Interval::Hour1 => Some("1h"),
        Interval::Hour4 => Some("4h"),
        Interval::Hour8 => Some("8h"),
        Interval::Day1 => Some("1d"),
        Interval::Month1 => Some("30d"),
        _ => None,
    }
}

pub(crate) fn futures_interval_text(interval: Interval) -> Option<&'static str> {
    match interval {
        Interval::Min1 => Some("1m"),
        Interval::Min5 => Some("5m"),
        Interval::Min15 => Some("15m"),
        Interval::Min30 => Some("30m"),
        Interval::Hour1 => Some("1h"),
        Interval::Hour4 => Some("4h"),
        Interval::Hour8 => Some("8h"),
        Interval::Day1 => Some("1d"),
        _ => None,
    }
}
