use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use super::futures_interval_text;
use crate::exchange::common::{
    decode_json_response, deserialize_f64_text, first_open_time_in_window, gate_get_fallback,
    gate_get_with_query_fallback, http_status_message, malformed_response, ms_to_api_seconds,
    no_data, normalize_margin_percent, now_ms, page_window_end_ms, parse_text_f64,
    push_bar_if_in_window, request_failed,
};
use crate::exchange::{ExchangeEndpoints, ExchangeFetchError, RiskTier};
use crate::interval::{DeclaredMarketSource, Interval};
use crate::runtime::Bar;

const PAGE_LIMIT: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UsdtPerpsRiskSource {
    PublicRiskLimitTiers,
    PublicContractApproximation,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UsdtPerpsRiskSnapshot {
    pub contract: String,
    pub fetched_at_ms: i64,
    pub source: UsdtPerpsRiskSource,
    pub tiers: Vec<RiskTier>,
}

#[derive(Clone, Debug, Serialize)]
struct GateFuturesCandlesticksQuery<'a> {
    contract: &'a str,
    interval: &'a str,
    from: i64,
    to: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GateFuturesCandlestick {
    #[serde(rename = "t")]
    timestamp_secs: i64,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "sum", default)]
    quote_volume: Option<String>,
    #[serde(rename = "v", default)]
    size_volume: Option<String>,
}

impl GateFuturesCandlestick {
    fn open_time_ms(&self) -> i64 {
        self.timestamp_secs.saturating_mul(1_000)
    }

    pub(crate) fn to_bar(
        &self,
        source: &DeclaredMarketSource,
        interval: Interval,
        is_mark_price: bool,
    ) -> Result<Bar, ExchangeFetchError> {
        let volume = if is_mark_price {
            0.0
        } else if let Some(size_volume) = self.size_volume.as_deref() {
            parse_text_f64(size_volume, source, interval, "volume")?
        } else if let Some(quote_volume) = self.quote_volume.as_deref() {
            parse_text_f64(quote_volume, source, interval, "volume")?
        } else {
            return Err(malformed_response(
                source,
                interval,
                "missing `volume` value".to_string(),
            ));
        };
        Ok(Bar {
            time: self.open_time_ms() as f64,
            open: parse_text_f64(&self.open, source, interval, "open")?,
            high: parse_text_f64(&self.high, source, interval, "high")?,
            low: parse_text_f64(&self.low, source, interval, "low")?,
            close: parse_text_f64(&self.close, source, interval, "close")?,
            volume,
            funding_rate: None,
            open_interest: None,
            mark_price: None,
            index_price: None,
            premium_index: None,
            basis: None,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
struct GateFuturesRiskLimitTier {
    contract: String,
    #[serde(rename = "risk_limit", deserialize_with = "deserialize_f64_text")]
    risk_limit: f64,
    #[serde(rename = "initial_rate", deserialize_with = "deserialize_f64_text")]
    initial_rate: f64,
    #[serde(rename = "maintenance_rate", deserialize_with = "deserialize_f64_text")]
    maintenance_rate: f64,
    #[serde(rename = "leverage_max", deserialize_with = "deserialize_f64_text")]
    leverage_max: f64,
    #[serde(rename = "deduction", deserialize_with = "deserialize_f64_text")]
    deduction: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct GateFuturesContract {
    name: String,
    #[serde(rename = "maintenance_rate", deserialize_with = "deserialize_f64_text")]
    maintenance_rate: f64,
    #[serde(rename = "leverage_max", deserialize_with = "deserialize_f64_text")]
    leverage_max: f64,
    #[serde(rename = "risk_limit_base", deserialize_with = "deserialize_f64_text")]
    risk_limit_base: f64,
    #[serde(rename = "risk_limit_max", deserialize_with = "deserialize_f64_text")]
    risk_limit_max: f64,
}

pub(crate) fn fetch_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    fetch_futures_bars(client, source, interval, from_ms, to_ms, base_url, false)
}

pub(crate) fn fetch_mark_price_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    fetch_futures_bars(client, source, interval, from_ms, to_ms, base_url, true)
}

pub(crate) fn fetch_risk_snapshot(
    client: &Client,
    source: &DeclaredMarketSource,
    endpoints: &ExchangeEndpoints,
) -> Result<UsdtPerpsRiskSnapshot, ExchangeFetchError> {
    let response = gate_get_with_query_fallback(
        client,
        &endpoints.gate_base_url,
        "/futures/usdt/risk_limit_tiers",
        &[("contract", source.symbol.as_str())],
    )
    .map_err(|err| ExchangeFetchError::RiskRequestFailed {
        alias: source.alias.clone(),
        template: source.template.as_str(),
        symbol: source.symbol.clone(),
        message: err.to_string(),
    })?;
    if response.status() == StatusCode::OK {
        let mut rows: Vec<GateFuturesRiskLimitTier> =
            response
                .json()
                .map_err(|err| ExchangeFetchError::MalformedRiskResponse {
                    alias: source.alias.clone(),
                    template: source.template.as_str(),
                    symbol: source.symbol.clone(),
                    message: err.to_string(),
                })?;
        rows.retain(|row| row.contract == source.symbol);
        rows.sort_by(|left, right| left.risk_limit.total_cmp(&right.risk_limit));
        if !rows.is_empty() {
            let mut tiers = Vec::with_capacity(rows.len());
            let mut lower_bound = 0.0;
            for row in rows {
                if row.risk_limit <= lower_bound {
                    return Err(ExchangeFetchError::MalformedRiskResponse {
                        alias: source.alias.clone(),
                        template: source.template.as_str(),
                        symbol: source.symbol.clone(),
                        message: "non-increasing Gate risk tiers".to_string(),
                    });
                }
                let _ = row.initial_rate;
                tiers.push(RiskTier {
                    lower_bound,
                    upper_bound: Some(row.risk_limit),
                    max_leverage: row.leverage_max,
                    maintenance_margin_rate: normalize_margin_percent(row.maintenance_rate),
                    maintenance_amount: row.deduction,
                });
                lower_bound = row.risk_limit;
            }
            return Ok(UsdtPerpsRiskSnapshot {
                contract: source.symbol.clone(),
                fetched_at_ms: now_ms(),
                source: UsdtPerpsRiskSource::PublicRiskLimitTiers,
                tiers,
            });
        }
    }

    fetch_contract_snapshot(client, source, endpoints)
}

fn fetch_futures_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
    is_mark_price: bool,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let interval_text = futures_interval_text(interval).ok_or_else(|| {
        malformed_response(
            source,
            interval,
            "unsupported Gate futures interval".to_string(),
        )
    })?;
    let Some(mut window_start_ms) = first_open_time_in_window(interval, from_ms, to_ms) else {
        return Err(no_data(source, interval, from_ms, to_ms));
    };
    let mut bars = Vec::new();
    let contract = if is_mark_price {
        format!("mark_{}", source.symbol)
    } else {
        source.symbol.clone()
    };

    while window_start_ms < to_ms {
        let window_end_ms = page_window_end_ms(interval, window_start_ms, PAGE_LIMIT, to_ms)
            .ok_or_else(|| {
                malformed_response(
                    source,
                    interval,
                    "unable to advance Gate futures pagination".to_string(),
                )
            })?;
        let response = gate_get_with_query_fallback(
            client,
            base_url,
            "/futures/usdt/candlesticks",
            &GateFuturesCandlesticksQuery {
                contract: contract.as_str(),
                interval: interval_text,
                from: ms_to_api_seconds(window_start_ms),
                to: ms_to_api_seconds(window_end_ms),
            },
        )
        .map_err(|err| request_failed(source, interval, err.to_string()))?;
        if response.status() != StatusCode::OK {
            return Err(request_failed(
                source,
                interval,
                http_status_message(response),
            ));
        }
        let mut rows: Vec<GateFuturesCandlestick> =
            decode_json_response(response, source, interval)?;
        rows.sort_by_key(GateFuturesCandlestick::open_time_ms);

        for row in &rows {
            let bar = row.to_bar(source, interval, is_mark_price)?;
            push_bar_if_in_window(&mut bars, bar, source, interval, from_ms, to_ms)?;
        }

        let Some(next_window_start) = interval.next_open_time(window_end_ms) else {
            break;
        };
        if next_window_start >= to_ms {
            break;
        }
        window_start_ms = next_window_start;
    }

    if bars.is_empty() {
        return Err(no_data(source, interval, from_ms, to_ms));
    }
    Ok(bars)
}

fn fetch_contract_snapshot(
    client: &Client,
    source: &DeclaredMarketSource,
    endpoints: &ExchangeEndpoints,
) -> Result<UsdtPerpsRiskSnapshot, ExchangeFetchError> {
    let response = gate_get_fallback(
        client,
        &endpoints.gate_base_url,
        &format!("/futures/usdt/contracts/{}", source.symbol),
    )
    .map_err(|err| ExchangeFetchError::RiskRequestFailed {
        alias: source.alias.clone(),
        template: source.template.as_str(),
        symbol: source.symbol.clone(),
        message: err.to_string(),
    })?;
    if response.status() != StatusCode::OK {
        return Err(ExchangeFetchError::RiskRequestFailed {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
            message: http_status_message(response),
        });
    }
    let contract: GateFuturesContract =
        response
            .json()
            .map_err(|err| ExchangeFetchError::MalformedRiskResponse {
                alias: source.alias.clone(),
                template: source.template.as_str(),
                symbol: source.symbol.clone(),
                message: err.to_string(),
            })?;
    if contract.name != source.symbol {
        return Err(ExchangeFetchError::MalformedRiskResponse {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
            message: "requested contract missing from contract response".to_string(),
        });
    }
    let upper_bound = if contract.risk_limit_base > 0.0 {
        Some(contract.risk_limit_base)
    } else if contract.risk_limit_max > 0.0 {
        Some(contract.risk_limit_max)
    } else {
        None
    };
    Ok(UsdtPerpsRiskSnapshot {
        contract: source.symbol.clone(),
        fetched_at_ms: now_ms(),
        source: UsdtPerpsRiskSource::PublicContractApproximation,
        tiers: vec![RiskTier {
            lower_bound: 0.0,
            upper_bound,
            max_leverage: contract.leverage_max,
            maintenance_margin_rate: normalize_margin_percent(contract.maintenance_rate),
            maintenance_amount: 0.0,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::GateFuturesCandlestick;
    use crate::interval::{DeclaredMarketSource, Interval, SourceTemplate};
    use serde_json::json;

    #[test]
    fn gate_futures_candlestick_row_maps_ohlcv_fields() {
        let source = DeclaredMarketSource {
            id: 0,
            alias: "src".to_string(),
            template: SourceTemplate::GateUsdtPerps,
            symbol: "BTC_USDT".to_string(),
        };
        let row: GateFuturesCandlestick = serde_json::from_value(json!({
            "t": 1704067200_i64,
            "o": "100.0",
            "h": "101.0",
            "l": "99.0",
            "c": "100.5",
            "v": "5.0",
            "sum": "500.0"
        }))
        .expect("row deserializes");
        let bar = row
            .to_bar(&source, Interval::Min1, false)
            .expect("row maps");
        assert_eq!(bar.close, 100.5);
        assert_eq!(bar.volume, 5.0);
    }
}
