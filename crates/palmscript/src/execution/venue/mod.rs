pub mod binance;
pub mod bybit;
pub mod gate;

use crate::interval::{DeclaredMarketSource, SourceTemplate};

use super::ExecutionError;

pub(crate) fn validate_paper_source(source: &DeclaredMarketSource) -> Result<(), ExecutionError> {
    match source.template {
        SourceTemplate::BinanceSpot | SourceTemplate::BinanceUsdm => binance::validate(source),
        SourceTemplate::BybitSpot | SourceTemplate::BybitUsdtPerps => bybit::validate(source),
        SourceTemplate::GateSpot | SourceTemplate::GateUsdtPerps => gate::validate(source),
    }
}
