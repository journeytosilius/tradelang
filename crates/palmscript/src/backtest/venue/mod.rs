mod binance;
mod bybit;
mod common;
mod gate;

use crate::backtest::BacktestError;
use crate::bytecode::OrderDecl;
use crate::interval::SourceTemplate;

#[derive(Clone, Copy, Debug)]
pub(crate) enum VenueOrderProfile {
    BinanceSpot,
    BinanceUsdm,
    BybitSpot,
    BybitUsdtPerps,
    GateSpot,
    GateUsdtPerps,
}

impl VenueOrderProfile {
    pub(crate) const fn from_template(template: SourceTemplate) -> Self {
        match template {
            SourceTemplate::BinanceSpot => Self::BinanceSpot,
            SourceTemplate::BinanceUsdm => Self::BinanceUsdm,
            SourceTemplate::BybitSpot => Self::BybitSpot,
            SourceTemplate::BybitUsdtPerps => Self::BybitUsdtPerps,
            SourceTemplate::GateSpot => Self::GateSpot,
            SourceTemplate::GateUsdtPerps => Self::GateUsdtPerps,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::BinanceSpot => "binance.spot",
            Self::BinanceUsdm => "binance.usdm",
            Self::BybitSpot => "bybit.spot",
            Self::BybitUsdtPerps => "bybit.usdt_perps",
            Self::GateSpot => "gate.spot",
            Self::GateUsdtPerps => "gate.usdt_perps",
        }
    }
}

pub(crate) fn validate_order_for_template(
    profile: VenueOrderProfile,
    alias: &str,
    order: &OrderDecl,
) -> Result<(), BacktestError> {
    let result = match profile {
        VenueOrderProfile::BinanceSpot => binance::validate_spot(order),
        VenueOrderProfile::BinanceUsdm => binance::validate_usdm(order),
        VenueOrderProfile::BybitSpot => bybit::validate_spot(order),
        VenueOrderProfile::BybitUsdtPerps => bybit::validate_usdt_perps(order),
        VenueOrderProfile::GateSpot => gate::validate_spot(order),
        VenueOrderProfile::GateUsdtPerps => gate::validate_usdt_perps(order),
    };
    result.map_err(|reason| BacktestError::UnsupportedOrderForVenue {
        alias: alias.to_string(),
        venue: profile.as_str().to_string(),
        role: order.role,
        kind: order.kind,
        reason,
    })
}

#[cfg(test)]
mod tests {
    use super::VenueOrderProfile;
    use crate::interval::SourceTemplate;

    #[test]
    fn profile_dispatch_matches_templates() {
        assert!(matches!(
            VenueOrderProfile::from_template(SourceTemplate::BinanceSpot),
            VenueOrderProfile::BinanceSpot
        ));
        assert!(matches!(
            VenueOrderProfile::from_template(SourceTemplate::BinanceUsdm),
            VenueOrderProfile::BinanceUsdm
        ));
        assert!(matches!(
            VenueOrderProfile::from_template(SourceTemplate::BybitSpot),
            VenueOrderProfile::BybitSpot
        ));
        assert!(matches!(
            VenueOrderProfile::from_template(SourceTemplate::BybitUsdtPerps),
            VenueOrderProfile::BybitUsdtPerps
        ));
        assert!(matches!(
            VenueOrderProfile::from_template(SourceTemplate::GateSpot),
            VenueOrderProfile::GateSpot
        ));
        assert!(matches!(
            VenueOrderProfile::from_template(SourceTemplate::GateUsdtPerps),
            VenueOrderProfile::GateUsdtPerps
        ));
    }
}
