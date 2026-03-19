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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_kind_strings_match_expected_surface() {
        assert_eq!(OrderKind::Market.as_str(), "market");
        assert_eq!(OrderKind::Limit.as_str(), "limit");
        assert_eq!(OrderKind::StopMarket.as_str(), "stop_market");
        assert_eq!(OrderKind::StopLimit.as_str(), "stop_limit");
        assert_eq!(OrderKind::TakeProfitMarket.as_str(), "take_profit_market");
        assert_eq!(OrderKind::TakeProfitLimit.as_str(), "take_profit_limit");
    }

    #[test]
    fn time_in_force_variants_round_trip() {
        for variant in TimeInForce::ALL {
            assert_eq!(TimeInForce::from_variant(variant.as_str()), Some(variant));
        }
        assert_eq!(TimeInForce::from_variant("day"), None);
    }

    #[test]
    fn trigger_reference_variants_round_trip() {
        for variant in TriggerReference::ALL {
            assert_eq!(
                TriggerReference::from_variant(variant.as_str()),
                Some(variant)
            );
        }
        assert_eq!(TriggerReference::from_variant("close"), None);
    }

    #[test]
    fn size_mode_and_order_field_kind_strings_match_expected_surface() {
        assert_eq!(SizeMode::CapitalFraction.as_str(), "capital_fraction");
        assert_eq!(SizeMode::RiskPct.as_str(), "risk_pct");
        assert_eq!(OrderFieldKind::default(), OrderFieldKind::Price);
        assert_eq!(OrderFieldKind::Price.as_str(), "price");
        assert_eq!(OrderFieldKind::TriggerPrice.as_str(), "trigger_price");
        assert_eq!(OrderFieldKind::ExpireTime.as_str(), "expire_time");
        assert_eq!(OrderFieldKind::SizeFraction.as_str(), "size_fraction");
        assert_eq!(OrderFieldKind::RiskStopPrice.as_str(), "risk_stop_price");
    }
}
