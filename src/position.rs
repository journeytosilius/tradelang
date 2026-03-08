//! Position- and exit-scoped types shared by the compiler, runtime, and
//! backtester.
//!
//! These types model position-aware fields exposed to attached exits, recent
//! closed-trade fields exposed to strategy logic, and the shared enum types
//! reused across the public API and backtest internals.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PositionSide {
    Long,
    Short,
}

impl PositionSide {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }

    pub fn from_variant(variant: &str) -> Option<Self> {
        match variant {
            "long" => Some(Self::Long),
            "short" => Some(Self::Short),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExitKind {
    Protect,
    Target,
    Signal,
    Reversal,
    Liquidation,
}

impl ExitKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Protect => "protect",
            Self::Target => "target",
            Self::Signal => "signal",
            Self::Reversal => "reversal",
            Self::Liquidation => "liquidation",
        }
    }

    pub fn from_variant(variant: &str) -> Option<Self> {
        match variant {
            "protect" => Some(Self::Protect),
            "target" => Some(Self::Target),
            "signal" => Some(Self::Signal),
            "reversal" => Some(Self::Reversal),
            "liquidation" => Some(Self::Liquidation),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PositionField {
    EntryPrice,
    EntryTime,
    EntryBarIndex,
    BarsHeld,
    IsLong,
    IsShort,
    Side,
    MarketPrice,
    UnrealizedPnl,
    UnrealizedReturn,
    Mae,
    Mfe,
}

impl PositionField {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EntryPrice => "entry_price",
            Self::EntryTime => "entry_time",
            Self::EntryBarIndex => "entry_bar_index",
            Self::BarsHeld => "bars_held",
            Self::IsLong => "is_long",
            Self::IsShort => "is_short",
            Self::Side => "side",
            Self::MarketPrice => "market_price",
            Self::UnrealizedPnl => "unrealized_pnl",
            Self::UnrealizedReturn => "unrealized_return",
            Self::Mae => "mae",
            Self::Mfe => "mfe",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "entry_price" => Some(Self::EntryPrice),
            "entry_time" => Some(Self::EntryTime),
            "entry_bar_index" => Some(Self::EntryBarIndex),
            "bars_held" => Some(Self::BarsHeld),
            "is_long" => Some(Self::IsLong),
            "is_short" => Some(Self::IsShort),
            "side" => Some(Self::Side),
            "market_price" => Some(Self::MarketPrice),
            "unrealized_pnl" => Some(Self::UnrealizedPnl),
            "unrealized_return" => Some(Self::UnrealizedReturn),
            "mae" => Some(Self::Mae),
            "mfe" => Some(Self::Mfe),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PositionEventField {
    LongEntryFill,
    LongEntry1Fill,
    LongEntry2Fill,
    LongEntry3Fill,
    ShortEntryFill,
    ShortEntry1Fill,
    ShortEntry2Fill,
    ShortEntry3Fill,
    LongExitFill,
    ShortExitFill,
    LongProtectFill,
    ShortProtectFill,
    LongTargetFill,
    LongTarget1Fill,
    LongTarget2Fill,
    LongTarget3Fill,
    ShortTargetFill,
    ShortTarget1Fill,
    ShortTarget2Fill,
    ShortTarget3Fill,
    LongSignalExitFill,
    ShortSignalExitFill,
    LongReversalExitFill,
    ShortReversalExitFill,
    LongLiquidationFill,
    ShortLiquidationFill,
}

impl PositionEventField {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LongEntryFill => "long_entry_fill",
            Self::LongEntry1Fill => "long_entry1_fill",
            Self::LongEntry2Fill => "long_entry2_fill",
            Self::LongEntry3Fill => "long_entry3_fill",
            Self::ShortEntryFill => "short_entry_fill",
            Self::ShortEntry1Fill => "short_entry1_fill",
            Self::ShortEntry2Fill => "short_entry2_fill",
            Self::ShortEntry3Fill => "short_entry3_fill",
            Self::LongExitFill => "long_exit_fill",
            Self::ShortExitFill => "short_exit_fill",
            Self::LongProtectFill => "long_protect_fill",
            Self::ShortProtectFill => "short_protect_fill",
            Self::LongTargetFill => "long_target_fill",
            Self::LongTarget1Fill => "long_target1_fill",
            Self::LongTarget2Fill => "long_target2_fill",
            Self::LongTarget3Fill => "long_target3_fill",
            Self::ShortTargetFill => "short_target_fill",
            Self::ShortTarget1Fill => "short_target1_fill",
            Self::ShortTarget2Fill => "short_target2_fill",
            Self::ShortTarget3Fill => "short_target3_fill",
            Self::LongSignalExitFill => "long_signal_exit_fill",
            Self::ShortSignalExitFill => "short_signal_exit_fill",
            Self::LongReversalExitFill => "long_reversal_exit_fill",
            Self::ShortReversalExitFill => "short_reversal_exit_fill",
            Self::LongLiquidationFill => "long_liquidation_fill",
            Self::ShortLiquidationFill => "short_liquidation_fill",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "long_entry_fill" => Some(Self::LongEntryFill),
            "long_entry1_fill" => Some(Self::LongEntry1Fill),
            "long_entry2_fill" => Some(Self::LongEntry2Fill),
            "long_entry3_fill" => Some(Self::LongEntry3Fill),
            "short_entry_fill" => Some(Self::ShortEntryFill),
            "short_entry1_fill" => Some(Self::ShortEntry1Fill),
            "short_entry2_fill" => Some(Self::ShortEntry2Fill),
            "short_entry3_fill" => Some(Self::ShortEntry3Fill),
            "long_exit_fill" => Some(Self::LongExitFill),
            "short_exit_fill" => Some(Self::ShortExitFill),
            "long_protect_fill" => Some(Self::LongProtectFill),
            "short_protect_fill" => Some(Self::ShortProtectFill),
            "long_target_fill" => Some(Self::LongTargetFill),
            "long_target1_fill" => Some(Self::LongTarget1Fill),
            "long_target2_fill" => Some(Self::LongTarget2Fill),
            "long_target3_fill" => Some(Self::LongTarget3Fill),
            "short_target_fill" => Some(Self::ShortTargetFill),
            "short_target1_fill" => Some(Self::ShortTarget1Fill),
            "short_target2_fill" => Some(Self::ShortTarget2Fill),
            "short_target3_fill" => Some(Self::ShortTarget3Fill),
            "long_signal_exit_fill" => Some(Self::LongSignalExitFill),
            "short_signal_exit_fill" => Some(Self::ShortSignalExitFill),
            "long_reversal_exit_fill" => Some(Self::LongReversalExitFill),
            "short_reversal_exit_fill" => Some(Self::ShortReversalExitFill),
            "long_liquidation_fill" => Some(Self::LongLiquidationFill),
            "short_liquidation_fill" => Some(Self::ShortLiquidationFill),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LastExitField {
    Kind,
    Stage,
    Side,
    Price,
    Time,
    BarIndex,
    RealizedPnl,
    RealizedReturn,
    BarsHeld,
}

impl LastExitField {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kind => "kind",
            Self::Stage => "stage",
            Self::Side => "side",
            Self::Price => "price",
            Self::Time => "time",
            Self::BarIndex => "bar_index",
            Self::RealizedPnl => "realized_pnl",
            Self::RealizedReturn => "realized_return",
            Self::BarsHeld => "bars_held",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "kind" => Some(Self::Kind),
            "stage" => Some(Self::Stage),
            "side" => Some(Self::Side),
            "price" => Some(Self::Price),
            "time" => Some(Self::Time),
            "bar_index" => Some(Self::BarIndex),
            "realized_pnl" => Some(Self::RealizedPnl),
            "realized_return" => Some(Self::RealizedReturn),
            "bars_held" => Some(Self::BarsHeld),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LastExitScope {
    Global,
    Long,
    Short,
}

impl LastExitScope {
    pub const fn namespace(self) -> &'static str {
        match self {
            Self::Global => "last_exit",
            Self::Long => "last_long_exit",
            Self::Short => "last_short_exit",
        }
    }

    pub fn from_namespace(namespace: &str) -> Option<Self> {
        match namespace {
            "last_exit" => Some(Self::Global),
            "last_long_exit" => Some(Self::Long),
            "last_short_exit" => Some(Self::Short),
            _ => None,
        }
    }
}
