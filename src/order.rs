use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderKind {
    Market,
    Limit,
    StopMarket,
    StopLimit,
    TakeProfitMarket,
    TakeProfitLimit,
}

impl OrderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Market => "market",
            Self::Limit => "limit",
            Self::StopMarket => "stop_market",
            Self::StopLimit => "stop_limit",
            Self::TakeProfitMarket => "take_profit_market",
            Self::TakeProfitLimit => "take_profit_limit",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimeInForce {
    Gtc,
    Ioc,
    Fok,
    Gtd,
}

impl TimeInForce {
    pub const ALL: [Self; 4] = [Self::Gtc, Self::Ioc, Self::Fok, Self::Gtd];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gtc => "gtc",
            Self::Ioc => "ioc",
            Self::Fok => "fok",
            Self::Gtd => "gtd",
        }
    }

    pub fn from_variant(variant: &str) -> Option<Self> {
        match variant {
            "gtc" => Some(Self::Gtc),
            "ioc" => Some(Self::Ioc),
            "fok" => Some(Self::Fok),
            "gtd" => Some(Self::Gtd),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TriggerReference {
    Last,
    Mark,
    Index,
}

impl TriggerReference {
    pub const ALL: [Self; 3] = [Self::Last, Self::Mark, Self::Index];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Last => "last",
            Self::Mark => "mark",
            Self::Index => "index",
        }
    }

    pub fn from_variant(variant: &str) -> Option<Self> {
        match variant {
            "last" => Some(Self::Last),
            "mark" => Some(Self::Mark),
            "index" => Some(Self::Index),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SizeMode {
    CapitalFraction,
    RiskPct,
}

impl SizeMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CapitalFraction => "capital_fraction",
            Self::RiskPct => "risk_pct",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderFieldKind {
    #[default]
    Price,
    TriggerPrice,
    ExpireTime,
    SizeFraction,
    RiskStopPrice,
}

impl OrderFieldKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Price => "price",
            Self::TriggerPrice => "trigger_price",
            Self::ExpireTime => "expire_time",
            Self::SizeFraction => "size_fraction",
            Self::RiskStopPrice => "risk_stop_price",
        }
    }
}
