pub mod spot;
pub mod usdt_perps;

pub use usdt_perps::{UsdtPerpsRiskSnapshot, UsdtPerpsRiskSource};

use crate::interval::Interval;

pub(crate) fn interval_text(interval: Interval) -> Option<&'static str> {
    match interval {
        Interval::Min1 => Some("1"),
        Interval::Min3 => Some("3"),
        Interval::Min5 => Some("5"),
        Interval::Min15 => Some("15"),
        Interval::Min30 => Some("30"),
        Interval::Hour1 => Some("60"),
        Interval::Hour2 => Some("120"),
        Interval::Hour4 => Some("240"),
        Interval::Hour6 => Some("360"),
        Interval::Hour12 => Some("720"),
        Interval::Day1 => Some("D"),
        Interval::Week1 => Some("W"),
        Interval::Month1 => Some("M"),
        _ => None,
    }
}
