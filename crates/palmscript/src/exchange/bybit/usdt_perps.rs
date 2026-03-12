use std::fmt;

use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::de::{self, Deserializer, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use super::{interval_text, spot::fetch_bybit_bars};
use crate::exchange::common::{
    decode_json_response, deserialize_f64_text, deserialize_option_f64_text, http_status_message,
    malformed_response, no_data, normalize_margin_percent, now_ms, parse_text_f64,
    push_bar_if_in_window, request_failed,
};
use crate::exchange::{ExchangeEndpoints, ExchangeFetchError, RiskTier};
use crate::interval::{DeclaredMarketSource, Interval};
use crate::runtime::Bar;

const PAGE_LIMIT: usize = 1000;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UsdtPerpsRiskSource {
    PublicRiskLimit,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UsdtPerpsRiskSnapshot {
    pub symbol: String,
    pub fetched_at_ms: i64,
    pub source: UsdtPerpsRiskSource,
    pub tiers: Vec<RiskTier>,
}

#[derive(Clone, Debug, Serialize)]
struct BybitKlineQuery<'a> {
    category: &'static str,
    symbol: &'a str,
    interval: &'a str,
    start: i64,
    end: i64,
    limit: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct BybitMarkPriceKlineRow {
    start_time_ms: i64,
    open: String,
    high: String,
    low: String,
    close: String,
}

impl BybitMarkPriceKlineRow {
    fn open_time(&self) -> i64 {
        self.start_time_ms
    }

    pub(crate) fn to_bar(
        &self,
        source: &DeclaredMarketSource,
        interval: Interval,
    ) -> Result<Bar, ExchangeFetchError> {
        Ok(Bar {
            time: self.start_time_ms as f64,
            open: parse_text_f64(&self.open, source, interval, "open")?,
            high: parse_text_f64(&self.high, source, interval, "high")?,
            low: parse_text_f64(&self.low, source, interval, "low")?,
            close: parse_text_f64(&self.close, source, interval, "close")?,
            volume: 0.0,
        })
    }
}

impl<'de> Deserialize<'de> for BybitMarkPriceKlineRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BybitMarkPriceKlineRowVisitor;

        impl<'de> Visitor<'de> for BybitMarkPriceKlineRowVisitor {
            type Value = BybitMarkPriceKlineRow;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a Bybit mark-price kline array with five OHLC fields")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let start_time_ms = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let open = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let high = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let low = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &self))?;
                let close = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(4, &self))?;

                while let Some(IgnoredAny) = seq.next_element()? {}

                Ok(BybitMarkPriceKlineRow {
                    start_time_ms,
                    open,
                    high,
                    low,
                    close,
                })
            }
        }

        deserializer.deserialize_seq(BybitMarkPriceKlineRowVisitor)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct BybitEnvelope<T> {
    #[serde(rename = "retCode")]
    ret_code: i32,
    #[serde(rename = "retMsg")]
    ret_msg: String,
    result: Option<T>,
    time: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
struct BybitKlineResult<T> {
    list: Vec<T>,
}

#[derive(Clone, Debug, Serialize)]
struct BybitRiskLimitQuery<'a> {
    category: &'static str,
    symbol: &'a str,
    limit: usize,
    #[serde(skip_serializing_if = "str::is_empty")]
    cursor: &'a str,
}

#[derive(Clone, Debug, Deserialize)]
struct BybitRiskLimitResult {
    list: Vec<BybitRiskLimitTier>,
    #[serde(rename = "nextPageCursor", default)]
    next_page_cursor: String,
}

#[derive(Clone, Debug, Deserialize)]
struct BybitRiskLimitTier {
    symbol: String,
    #[serde(rename = "riskLimitValue", deserialize_with = "deserialize_f64_text")]
    risk_limit_value: f64,
    #[serde(
        rename = "maintenanceMargin",
        deserialize_with = "deserialize_f64_text"
    )]
    maintenance_margin: f64,
    #[serde(rename = "initialMargin", deserialize_with = "deserialize_f64_text")]
    initial_margin: f64,
    #[serde(rename = "maxLeverage", deserialize_with = "deserialize_f64_text")]
    max_leverage: f64,
    #[serde(
        rename = "mmDeduction",
        default,
        deserialize_with = "deserialize_option_f64_text"
    )]
    mm_deduction: Option<f64>,
}

pub(crate) fn fetch_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    fetch_bybit_bars(client, source, interval, from_ms, to_ms, base_url, "linear")
}

pub(crate) fn fetch_mark_price_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let interval_text = interval_text(interval).ok_or_else(|| {
        malformed_response(source, interval, "unsupported Bybit interval".to_string())
    })?;
    let mut start_time = from_ms;
    let mut bars = Vec::new();

    loop {
        let response = client
            .get(format!(
                "{}/v5/market/mark-price-kline",
                base_url.trim_end_matches('/')
            ))
            .query(&BybitKlineQuery {
                category: "linear",
                symbol: source.symbol.as_str(),
                interval: interval_text,
                start: start_time,
                end: to_ms.saturating_sub(1),
                limit: PAGE_LIMIT,
            })
            .send()
            .map_err(|err| request_failed(source, interval, err.to_string()))?;
        if response.status() != StatusCode::OK {
            return Err(request_failed(
                source,
                interval,
                http_status_message(response),
            ));
        }
        let payload: BybitEnvelope<BybitKlineResult<BybitMarkPriceKlineRow>> =
            decode_json_response(response, source, interval)?;
        if payload.ret_code != 0 {
            return Err(request_failed(source, interval, payload.ret_msg));
        }
        let mut rows = payload
            .result
            .ok_or_else(|| {
                malformed_response(source, interval, "missing `result` body".to_string())
            })?
            .list;
        if rows.is_empty() {
            break;
        }
        rows.sort_by_key(BybitMarkPriceKlineRow::open_time);

        let mut last_open = None;
        for row in &rows {
            let bar = row.to_bar(source, interval)?;
            if push_bar_if_in_window(&mut bars, bar, source, interval, from_ms, to_ms)? {
                last_open = Some(row.open_time());
            }
        }

        if rows.len() < PAGE_LIMIT {
            break;
        }
        let Some(last_open) = last_open else {
            break;
        };
        let Some(next_start) = interval.next_open_time(last_open) else {
            break;
        };
        if next_start >= to_ms {
            break;
        }
        start_time = next_start;
    }

    if bars.is_empty() {
        return Err(no_data(source, interval));
    }
    Ok(bars)
}

pub(crate) fn fetch_risk_snapshot(
    client: &Client,
    source: &DeclaredMarketSource,
    endpoints: &ExchangeEndpoints,
) -> Result<UsdtPerpsRiskSnapshot, ExchangeFetchError> {
    let mut cursor = String::new();
    let mut fetched_at_ms = now_ms();
    let mut rows = Vec::new();

    loop {
        let response = client
            .get(format!(
                "{}/v5/market/risk-limit",
                endpoints.bybit_base_url.trim_end_matches('/')
            ))
            .query(&BybitRiskLimitQuery {
                category: "linear",
                symbol: source.symbol.as_str(),
                limit: PAGE_LIMIT,
                cursor: cursor.as_str(),
            })
            .send()
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
        let payload: BybitEnvelope<BybitRiskLimitResult> =
            response
                .json()
                .map_err(|err| ExchangeFetchError::MalformedRiskResponse {
                    alias: source.alias.clone(),
                    template: source.template.as_str(),
                    symbol: source.symbol.clone(),
                    message: err.to_string(),
                })?;
        if payload.ret_code != 0 {
            return Err(ExchangeFetchError::RiskRequestFailed {
                alias: source.alias.clone(),
                template: source.template.as_str(),
                symbol: source.symbol.clone(),
                message: payload.ret_msg,
            });
        }
        if let Some(time_ms) = payload.time {
            fetched_at_ms = time_ms;
        }
        let result = payload
            .result
            .ok_or_else(|| ExchangeFetchError::MalformedRiskResponse {
                alias: source.alias.clone(),
                template: source.template.as_str(),
                symbol: source.symbol.clone(),
                message: "missing `result` body".to_string(),
            })?;
        rows.extend(
            result
                .list
                .into_iter()
                .filter(|entry| entry.symbol == source.symbol),
        );
        if result.next_page_cursor.is_empty() {
            break;
        }
        cursor = result.next_page_cursor;
    }

    rows.sort_by(|left, right| left.risk_limit_value.total_cmp(&right.risk_limit_value));
    let mut tiers = Vec::with_capacity(rows.len());
    let mut lower_bound = 0.0;
    for row in rows {
        if row.risk_limit_value <= lower_bound {
            return Err(ExchangeFetchError::MalformedRiskResponse {
                alias: source.alias.clone(),
                template: source.template.as_str(),
                symbol: source.symbol.clone(),
                message: "non-increasing Bybit risk tiers".to_string(),
            });
        }
        let _ = row.initial_margin;
        tiers.push(RiskTier {
            lower_bound,
            upper_bound: Some(row.risk_limit_value),
            max_leverage: row.max_leverage,
            maintenance_margin_rate: normalize_margin_percent(row.maintenance_margin),
            maintenance_amount: row.mm_deduction.unwrap_or(0.0),
        });
        lower_bound = row.risk_limit_value;
    }
    if tiers.is_empty() {
        return Err(ExchangeFetchError::MissingRiskTiers {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
        });
    }
    Ok(UsdtPerpsRiskSnapshot {
        symbol: source.symbol.clone(),
        fetched_at_ms,
        source: UsdtPerpsRiskSource::PublicRiskLimit,
        tiers,
    })
}

#[cfg(test)]
mod tests {
    use super::BybitMarkPriceKlineRow;
    use crate::interval::{DeclaredMarketSource, Interval, SourceTemplate};
    use serde_json::json;

    #[test]
    fn bybit_mark_price_row_maps_ohl_fields() {
        let source = DeclaredMarketSource {
            id: 0,
            alias: "src".to_string(),
            template: SourceTemplate::BybitUsdtPerps,
            symbol: "BTCUSDT".to_string(),
        };
        let row: BybitMarkPriceKlineRow = serde_json::from_value(json!([
            1704067200000_i64,
            "100.0",
            "101.0",
            "99.0",
            "100.5"
        ]))
        .expect("row deserializes");
        let bar = row.to_bar(&source, Interval::Min1).expect("row maps");
        assert_eq!(bar.close, 100.5);
        assert_eq!(bar.volume, 0.0);
    }
}
