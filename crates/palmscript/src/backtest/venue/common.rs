use crate::bytecode::OrderDecl;
use crate::order::{OrderKind, TimeInForce, TriggerReference};

pub(super) fn validate_spot_like_order(order: &OrderDecl, venue_name: &str) -> Result<(), String> {
    if matches!(
        order.kind,
        OrderKind::StopMarket
            | OrderKind::StopLimit
            | OrderKind::TakeProfitMarket
            | OrderKind::TakeProfitLimit
    ) && order.trigger_ref != Some(TriggerReference::Last)
    {
        return Err(format!(
            "{venue_name} trigger orders only support trigger_ref.last"
        ));
    }
    if matches!(order.tif, Some(TimeInForce::Gtd)) {
        return Err(format!(
            "{venue_name} does not support tif.gtd in this backtester"
        ));
    }
    if order.post_only {
        if !matches!(order.kind, OrderKind::Limit) {
            return Err(format!(
                "{venue_name} post_only is only supported for limit orders"
            ));
        }
        if order.tif != Some(TimeInForce::Gtc) {
            return Err(format!("{venue_name} post_only requires tif.gtc"));
        }
    }
    Ok(())
}

pub(super) fn validate_usdt_perps_like_order(
    order: &OrderDecl,
    venue_name: &str,
) -> Result<(), String> {
    if matches!(
        order.kind,
        OrderKind::StopMarket
            | OrderKind::StopLimit
            | OrderKind::TakeProfitMarket
            | OrderKind::TakeProfitLimit
    ) && !matches!(
        order.trigger_ref,
        Some(TriggerReference::Last | TriggerReference::Mark)
    ) {
        return Err(format!(
            "{venue_name} trigger orders support trigger_ref.last and trigger_ref.mark"
        ));
    }
    if order.post_only {
        if !matches!(
            order.kind,
            OrderKind::Limit | OrderKind::StopLimit | OrderKind::TakeProfitLimit
        ) {
            return Err(format!(
                "{venue_name} post_only is only supported for limit-family orders"
            ));
        }
        if order.tif != Some(TimeInForce::Gtc) {
            return Err(format!("{venue_name} post_only requires tif.gtc"));
        }
    }
    Ok(())
}
