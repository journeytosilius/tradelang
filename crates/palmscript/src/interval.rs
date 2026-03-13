//! Interval and market-series definitions shared across the language pipeline.
//!
//! This module provides the canonical interval registry, typed market-field and
//! source-template identifiers, and UTC boundary helpers used by the compiler
//! and runtime.

use serde::{Deserialize, Serialize};

const SECOND_MS: i64 = 1_000;
const MINUTE_MS: i64 = 60 * SECOND_MS;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;
const WEEK_MS: i64 = 7 * DAY_MS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Interval {
    Sec1,
    Min1,
    Min3,
    Min5,
    Min15,
    Min30,
    Hour1,
    Hour2,
    Hour4,
    Hour6,
    Hour8,
    Hour12,
    Day1,
    Day3,
    Week1,
    Month1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MarketField {
    Open,
    High,
    Low,
    Close,
    Volume,
    Time,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceTemplate {
    BinanceSpot,
    BinanceUsdm,
    BybitSpot,
    BybitUsdtPerps,
    GateSpot,
    GateUsdtPerps,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeclaredMarketSource {
    pub id: u16,
    pub alias: String,
    pub template: SourceTemplate,
    pub symbol: String,
}

pub type DeclaredExecutionTarget = DeclaredMarketSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceIntervalRef {
    pub source_id: u16,
    pub interval: Interval,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketSource {
    Named {
        source_id: u16,
        interval: Option<Interval>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketBinding {
    pub source: MarketSource,
    pub field: MarketField,
}

#[derive(Clone, Copy, Debug)]
pub struct IntervalSpec {
    pub interval: Interval,
    pub text: &'static str,
}

pub const INTERVAL_SPECS: [IntervalSpec; 16] = [
    IntervalSpec {
        interval: Interval::Sec1,
        text: "1s",
    },
    IntervalSpec {
        interval: Interval::Min1,
        text: "1m",
    },
    IntervalSpec {
        interval: Interval::Min3,
        text: "3m",
    },
    IntervalSpec {
        interval: Interval::Min5,
        text: "5m",
    },
    IntervalSpec {
        interval: Interval::Min15,
        text: "15m",
    },
    IntervalSpec {
        interval: Interval::Min30,
        text: "30m",
    },
    IntervalSpec {
        interval: Interval::Hour1,
        text: "1h",
    },
    IntervalSpec {
        interval: Interval::Hour2,
        text: "2h",
    },
    IntervalSpec {
        interval: Interval::Hour4,
        text: "4h",
    },
    IntervalSpec {
        interval: Interval::Hour6,
        text: "6h",
    },
    IntervalSpec {
        interval: Interval::Hour8,
        text: "8h",
    },
    IntervalSpec {
        interval: Interval::Hour12,
        text: "12h",
    },
    IntervalSpec {
        interval: Interval::Day1,
        text: "1d",
    },
    IntervalSpec {
        interval: Interval::Day3,
        text: "3d",
    },
    IntervalSpec {
        interval: Interval::Week1,
        text: "1w",
    },
    IntervalSpec {
        interval: Interval::Month1,
        text: "1M",
    },
];

impl Interval {
    pub fn parse(text: &str) -> Option<Self> {
        INTERVAL_SPECS
            .iter()
            .find(|spec| spec.text == text)
            .map(|spec| spec.interval)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sec1 => "1s",
            Self::Min1 => "1m",
            Self::Min3 => "3m",
            Self::Min5 => "5m",
            Self::Min15 => "15m",
            Self::Min30 => "30m",
            Self::Hour1 => "1h",
            Self::Hour2 => "2h",
            Self::Hour4 => "4h",
            Self::Hour6 => "6h",
            Self::Hour8 => "8h",
            Self::Hour12 => "12h",
            Self::Day1 => "1d",
            Self::Day3 => "3d",
            Self::Week1 => "1w",
            Self::Month1 => "1M",
        }
    }

    pub const fn ordinal(self) -> u8 {
        match self {
            Self::Sec1 => 0,
            Self::Min1 => 1,
            Self::Min3 => 2,
            Self::Min5 => 3,
            Self::Min15 => 4,
            Self::Min30 => 5,
            Self::Hour1 => 6,
            Self::Hour2 => 7,
            Self::Hour4 => 8,
            Self::Hour6 => 9,
            Self::Hour8 => 10,
            Self::Hour12 => 11,
            Self::Day1 => 12,
            Self::Day3 => 13,
            Self::Week1 => 14,
            Self::Month1 => 15,
        }
    }

    pub const fn mask(self) -> u32 {
        1u32 << (self.ordinal() as u32 + 1)
    }

    pub const fn fixed_duration_ms(self) -> Option<i64> {
        match self {
            Self::Sec1 => Some(SECOND_MS),
            Self::Min1 => Some(MINUTE_MS),
            Self::Min3 => Some(3 * MINUTE_MS),
            Self::Min5 => Some(5 * MINUTE_MS),
            Self::Min15 => Some(15 * MINUTE_MS),
            Self::Min30 => Some(30 * MINUTE_MS),
            Self::Hour1 => Some(HOUR_MS),
            Self::Hour2 => Some(2 * HOUR_MS),
            Self::Hour4 => Some(4 * HOUR_MS),
            Self::Hour6 => Some(6 * HOUR_MS),
            Self::Hour8 => Some(8 * HOUR_MS),
            Self::Hour12 => Some(12 * HOUR_MS),
            Self::Day1 => Some(DAY_MS),
            Self::Day3 => Some(3 * DAY_MS),
            Self::Week1 => Some(WEEK_MS),
            Self::Month1 => None,
        }
    }

    pub fn is_aligned(self, open_time_ms: i64) -> bool {
        match self {
            Self::Week1 => {
                let (days, remainder) = split_days(open_time_ms);
                remainder == 0 && weekday_monday_based(days) == 0
            }
            Self::Month1 => {
                let (days, remainder) = split_days(open_time_ms);
                if remainder != 0 {
                    return false;
                }
                let (year, month, day) = civil_from_days(days);
                let _ = year;
                let _ = month;
                day == 1
            }
            _ => self
                .fixed_duration_ms()
                .map(|duration| open_time_ms.rem_euclid(duration) == 0)
                .unwrap_or(false),
        }
    }

    pub fn next_open_time(self, open_time_ms: i64) -> Option<i64> {
        match self {
            Self::Month1 => {
                let (days, remainder) = split_days(open_time_ms);
                if remainder != 0 {
                    return None;
                }
                let (year, month, day) = civil_from_days(days);
                if day != 1 {
                    return None;
                }
                let (next_year, next_month) = if month == 12 {
                    (year + 1, 1)
                } else {
                    (year, month + 1)
                };
                Some(days_from_civil(next_year, next_month, 1) * DAY_MS)
            }
            _ => self
                .fixed_duration_ms()
                .and_then(|duration| open_time_ms.checked_add(duration)),
        }
    }

    pub fn bucket_open_time(self, time_ms: i64) -> Option<i64> {
        match self {
            Self::Week1 => {
                let (days, remainder) = split_days(time_ms);
                let day_start = time_ms - remainder;
                Some(day_start - weekday_monday_based(days) * DAY_MS)
            }
            Self::Month1 => {
                let (days, remainder) = split_days(time_ms);
                let _ = remainder;
                let (year, month, _day) = civil_from_days(days);
                Some(days_from_civil(year, month, 1) * DAY_MS)
            }
            _ => self
                .fixed_duration_ms()
                .map(|duration| time_ms - time_ms.rem_euclid(duration)),
        }
    }
}

impl MarketField {
    pub const ALL: [Self; 6] = [
        Self::Open,
        Self::High,
        Self::Low,
        Self::Close,
        Self::Volume,
        Self::Time,
    ];

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "open" => Some(Self::Open),
            "high" => Some(Self::High),
            "low" => Some(Self::Low),
            "close" => Some(Self::Close),
            "volume" => Some(Self::Volume),
            "time" => Some(Self::Time),
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
        }
    }

    pub const fn ordinal(self) -> u8 {
        match self {
            Self::Open => 0,
            Self::High => 1,
            Self::Low => 2,
            Self::Close => 3,
            Self::Volume => 4,
            Self::Time => 5,
        }
    }
}

impl SourceTemplate {
    pub fn parse(exchange: &str, venue: &str) -> Option<Self> {
        match (exchange, venue) {
            ("binance", "spot") => Some(Self::BinanceSpot),
            ("binance", "usdm") => Some(Self::BinanceUsdm),
            ("bybit", "spot") => Some(Self::BybitSpot),
            ("bybit", "usdt_perps") => Some(Self::BybitUsdtPerps),
            ("gate", "spot") => Some(Self::GateSpot),
            ("gate", "usdt_perps") => Some(Self::GateUsdtPerps),
            _ => None,
        }
    }

    pub const fn exchange_name(self) -> &'static str {
        match self {
            Self::BinanceSpot | Self::BinanceUsdm => "binance",
            Self::BybitSpot | Self::BybitUsdtPerps => "bybit",
            Self::GateSpot | Self::GateUsdtPerps => "gate",
        }
    }

    pub const fn venue_name(self) -> &'static str {
        match self {
            Self::BinanceSpot | Self::BybitSpot | Self::GateSpot => "spot",
            Self::BinanceUsdm => "usdm",
            Self::BybitUsdtPerps | Self::GateUsdtPerps => "usdt_perps",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::BinanceSpot => "binance.spot",
            Self::BinanceUsdm => "binance.usdm",
            Self::BybitSpot => "bybit.spot",
            Self::BybitUsdtPerps => "bybit.usdt_perps",
            Self::GateSpot => "gate.spot",
            Self::GateUsdtPerps => "gate.usdt_perps",
        }
    }

    pub const fn supports_interval(self, interval: Interval) -> bool {
        match self {
            Self::BinanceSpot | Self::BinanceUsdm => {
                let _ = interval;
                true
            }
            Self::BybitSpot | Self::BybitUsdtPerps => matches!(
                interval,
                Interval::Min1
                    | Interval::Min3
                    | Interval::Min5
                    | Interval::Min15
                    | Interval::Min30
                    | Interval::Hour1
                    | Interval::Hour2
                    | Interval::Hour4
                    | Interval::Hour6
                    | Interval::Hour12
                    | Interval::Day1
                    | Interval::Week1
                    | Interval::Month1
            ),
            Self::GateSpot => matches!(
                interval,
                Interval::Sec1
                    | Interval::Min1
                    | Interval::Min5
                    | Interval::Min15
                    | Interval::Min30
                    | Interval::Hour1
                    | Interval::Hour4
                    | Interval::Hour8
                    | Interval::Day1
                    | Interval::Month1
            ),
            Self::GateUsdtPerps => matches!(
                interval,
                Interval::Min1
                    | Interval::Min5
                    | Interval::Min15
                    | Interval::Min30
                    | Interval::Hour1
                    | Interval::Hour4
                    | Interval::Hour8
                    | Interval::Day1
            ),
        }
    }
}

fn split_days(open_time_ms: i64) -> (i64, i64) {
    let days = open_time_ms.div_euclid(DAY_MS);
    let remainder = open_time_ms.rem_euclid(DAY_MS);
    (days, remainder)
}

fn weekday_monday_based(days_since_epoch: i64) -> i64 {
    (days_since_epoch + 3).rem_euclid(7)
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u8, u8) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2).div_euclid(153);
    let d = doy - (153 * mp + 2).div_euclid(5) + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u8, d as u8)
}

fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let year = year as i64 - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 }.div_euclid(400);
    let yoe = year - era * 400;
    let month = month as i64;
    let day = day as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2).div_euclid(5) + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::{Interval, MarketField, SourceTemplate};

    #[test]
    fn parses_all_supported_intervals() {
        let values = [
            "1s", "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h", "8h", "12h", "1d", "3d",
            "1w", "1M",
        ];
        for value in values {
            assert_eq!(Interval::parse(value).unwrap().as_str(), value);
        }
    }

    #[test]
    fn rejects_invalid_interval_text() {
        for value in ["1W", "1H", "7m", "2d", "0m"] {
            assert!(Interval::parse(value).is_none(), "{value} should reject");
        }
    }

    #[test]
    fn parses_market_fields() {
        for value in ["open", "high", "low", "close", "volume", "time"] {
            assert_eq!(MarketField::parse(value).unwrap().as_str(), value);
        }
        assert!(MarketField::parse("foo").is_none());
    }

    #[test]
    fn parses_supported_source_templates() {
        assert_eq!(
            SourceTemplate::parse("binance", "spot"),
            Some(SourceTemplate::BinanceSpot)
        );
        assert_eq!(
            SourceTemplate::parse("binance", "usdm"),
            Some(SourceTemplate::BinanceUsdm)
        );
        assert_eq!(
            SourceTemplate::parse("bybit", "spot"),
            Some(SourceTemplate::BybitSpot)
        );
        assert_eq!(
            SourceTemplate::parse("bybit", "usdt_perps"),
            Some(SourceTemplate::BybitUsdtPerps)
        );
        assert_eq!(
            SourceTemplate::parse("gate", "spot"),
            Some(SourceTemplate::GateSpot)
        );
        assert_eq!(
            SourceTemplate::parse("gate", "usdt_perps"),
            Some(SourceTemplate::GateUsdtPerps)
        );
        assert_eq!(SourceTemplate::parse("binance", "perps"), None);
        assert_eq!(SourceTemplate::parse("hyperliquid", "spot"), None);
        assert_eq!(SourceTemplate::parse("hyperliquid", "perps"), None);
    }

    #[test]
    fn bybit_templates_only_accept_documented_intervals() {
        assert!(!SourceTemplate::BybitSpot.supports_interval(Interval::Sec1));
        assert!(!SourceTemplate::BybitUsdtPerps.supports_interval(Interval::Hour8));
        assert!(!SourceTemplate::BybitUsdtPerps.supports_interval(Interval::Day3));
        assert!(SourceTemplate::BybitSpot.supports_interval(Interval::Hour6));
        assert!(SourceTemplate::BybitUsdtPerps.supports_interval(Interval::Month1));
    }

    #[test]
    fn gate_templates_only_accept_supported_interval_subsets() {
        assert!(SourceTemplate::GateSpot.supports_interval(Interval::Sec1));
        assert!(SourceTemplate::GateSpot.supports_interval(Interval::Hour8));
        assert!(!SourceTemplate::GateSpot.supports_interval(Interval::Week1));
        assert!(!SourceTemplate::GateSpot.supports_interval(Interval::Hour2));
        assert!(SourceTemplate::GateUsdtPerps.supports_interval(Interval::Day1));
        assert!(!SourceTemplate::GateUsdtPerps.supports_interval(Interval::Sec1));
        assert!(!SourceTemplate::GateUsdtPerps.supports_interval(Interval::Week1));
    }

    #[test]
    fn month_alignment_uses_first_day_at_midnight_utc() {
        assert!(Interval::Month1.is_aligned(1_704_067_200_000));
        assert!(!Interval::Month1.is_aligned(1_704_153_600_000));
    }

    #[test]
    fn bucket_open_time_floors_to_interval_boundary() {
        assert_eq!(
            Interval::Min1.bucket_open_time(1_704_067_261_234),
            Some(1_704_067_260_000)
        );
        assert_eq!(
            Interval::Week1.bucket_open_time(1_704_240_000_000),
            Some(1_704_067_200_000)
        );
        assert_eq!(
            Interval::Month1.bucket_open_time(1_706_832_000_000),
            Some(1_706_745_600_000)
        );
    }
}
