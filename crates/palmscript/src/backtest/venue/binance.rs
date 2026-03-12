use crate::bytecode::OrderDecl;

use super::common::{validate_spot_like_order, validate_usdt_perps_like_order};

pub(super) fn validate_spot(order: &OrderDecl) -> Result<(), String> {
    validate_spot_like_order(order, "Binance spot")
}

pub(super) fn validate_usdm(order: &OrderDecl) -> Result<(), String> {
    validate_usdt_perps_like_order(order, "Binance USD-M")
}
