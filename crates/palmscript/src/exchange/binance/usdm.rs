use std::env;

use hmac::{Hmac, Mac};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::spot::{fetch_binance_bars, BinanceKlineEndpoint};
use crate::exchange::common::{
    decode_json_response, deserialize_f64_text, deserialize_i64_text, deserialize_option_f64_text,
    http_status_message, malformed_response, no_data, normalize_margin_percent, now_ms,
    request_failed,
};
use crate::exchange::{ExchangeEndpoints, ExchangeFetchError, RiskTier};
use crate::interval::{DeclaredMarketSource, Interval, MarketField, SourceTemplate};
use crate::runtime::Bar;

const PAGE_LIMIT: usize = 1500;
const FUNDING_RATE_LIMIT: usize = 1000;
const BASIS_LIMIT: usize = 500;

struct ScalarKlineFieldEndpoint {
    path: &'static str,
    field: MarketField,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UsdmRiskSource {
    SignedLeverageBrackets,
    PublicExchangeInfoApproximation,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UsdmRiskSnapshot {
    pub symbol: String,
    pub fetched_at_ms: i64,
    pub source: UsdmRiskSource,
    pub brackets: Vec<RiskTier>,
}

#[derive(Clone, Debug, Deserialize)]
struct BinanceServerTimeResponse {
    #[serde(rename = "serverTime")]
    server_time: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct BinanceLeverageBracketResponse {
    symbol: String,
    brackets: Vec<BinanceLeverageBracketTier>,
}

#[derive(Clone, Debug, Deserialize)]
struct BinanceLeverageBracketTier {
    #[serde(rename = "initialLeverage")]
    initial_leverage: f64,
    #[serde(rename = "notionalFloor", deserialize_with = "deserialize_f64_text")]
    notional_floor: f64,
    #[serde(rename = "notionalCap", deserialize_with = "deserialize_f64_text")]
    notional_cap: f64,
    #[serde(rename = "maintMarginRatio", deserialize_with = "deserialize_f64_text")]
    maint_margin_ratio: f64,
    #[serde(rename = "cum", deserialize_with = "deserialize_f64_text")]
    cumulative_maint_amount: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct BinanceExchangeInfoResponse {
    symbols: Vec<BinanceExchangeInfoSymbol>,
}

#[derive(Clone, Debug, Deserialize)]
struct BinanceExchangeInfoSymbol {
    symbol: String,
    #[serde(
        rename = "maintMarginPercent",
        default,
        deserialize_with = "deserialize_option_f64_text"
    )]
    maint_margin_percent: Option<f64>,
    #[serde(
        rename = "requiredMarginPercent",
        default,
        deserialize_with = "deserialize_option_f64_text"
    )]
    required_margin_percent: Option<f64>,
}

pub(crate) fn fetch_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    fetch_binance_bars(
        client,
        source,
        interval,
        from_ms,
        to_ms,
        base_url,
        BinanceKlineEndpoint {
            path: "/fapi/v1/klines",
            page_limit: PAGE_LIMIT,
        },
    )
}

pub(crate) fn fetch_mark_price_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    fetch_binance_bars(
        client,
        source,
        interval,
        from_ms,
        to_ms,
        base_url,
        BinanceKlineEndpoint {
            path: "/fapi/v1/markPriceKlines",
            page_limit: PAGE_LIMIT,
        },
    )
}

pub(crate) fn fetch_index_price_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    fetch_scalar_kline_bars(
        client,
        source,
        interval,
        from_ms,
        to_ms,
        base_url,
        ScalarKlineFieldEndpoint {
            path: "/fapi/v1/indexPriceKlines",
            field: MarketField::IndexPrice,
        },
    )
}

pub(crate) fn fetch_premium_index_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    fetch_scalar_kline_bars(
        client,
        source,
        interval,
        from_ms,
        to_ms,
        base_url,
        ScalarKlineFieldEndpoint {
            path: "/fapi/v1/premiumIndexKlines",
            field: MarketField::PremiumIndex,
        },
    )
}

pub(crate) fn fetch_funding_rate_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let samples = fetch_funding_rate_samples(client, source, interval, from_ms, to_ms, base_url)?;
    Ok(densify_scalar_samples(
        interval,
        from_ms,
        to_ms,
        &samples,
        MarketField::FundingRate,
    ))
}

pub(crate) fn fetch_basis_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let samples = fetch_basis_samples(client, source, interval, from_ms, to_ms, base_url)?;
    Ok(densify_scalar_samples(
        interval,
        from_ms,
        to_ms,
        &samples,
        MarketField::Basis,
    ))
}

pub(crate) fn fetch_risk_snapshot(
    client: &Client,
    source: &DeclaredMarketSource,
    endpoints: &ExchangeEndpoints,
) -> Result<UsdmRiskSnapshot, ExchangeFetchError> {
    let api_key = env::var("PALMSCRIPT_BINANCE_USDM_API_KEY");
    let api_secret = env::var("PALMSCRIPT_BINANCE_USDM_API_SECRET");
    let (Ok(api_key), Ok(api_secret)) = (api_key, api_secret) else {
        return fetch_public_risk_snapshot(client, source, endpoints);
    };
    let server_time = fetch_server_time(client, endpoints)?;
    let query = format!("symbol={}&timestamp={server_time}", source.symbol);
    let signature =
        sign_query(&api_secret, &query).map_err(|err| ExchangeFetchError::RiskRequestFailed {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
            message: err,
        })?;
    let response = client
        .get(format!(
            "{}/fapi/v1/leverageBracket?{}&signature={}",
            endpoints.binance_usdm_base_url.trim_end_matches('/'),
            query,
            signature
        ))
        .header("X-MBX-APIKEY", api_key)
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
    let payload: Vec<BinanceLeverageBracketResponse> =
        response
            .json()
            .map_err(|err| ExchangeFetchError::MalformedRiskResponse {
                alias: source.alias.clone(),
                template: source.template.as_str(),
                symbol: source.symbol.clone(),
                message: err.to_string(),
            })?;
    let Some(symbol_entry) = payload
        .into_iter()
        .find(|entry| entry.symbol == source.symbol)
    else {
        return Err(ExchangeFetchError::MalformedRiskResponse {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
            message: "requested symbol missing from leverage bracket response".to_string(),
        });
    };
    let brackets = symbol_entry
        .brackets
        .into_iter()
        .map(|tier| RiskTier {
            lower_bound: tier.notional_floor,
            upper_bound: Some(tier.notional_cap),
            max_leverage: tier.initial_leverage,
            maintenance_margin_rate: tier.maint_margin_ratio,
            maintenance_amount: tier.cumulative_maint_amount,
        })
        .collect::<Vec<_>>();
    if brackets.is_empty() {
        return Err(ExchangeFetchError::MissingRiskTiers {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
        });
    }
    Ok(UsdmRiskSnapshot {
        symbol: source.symbol.clone(),
        fetched_at_ms: server_time,
        source: UsdmRiskSource::SignedLeverageBrackets,
        brackets,
    })
}

fn fetch_public_risk_snapshot(
    client: &Client,
    source: &DeclaredMarketSource,
    endpoints: &ExchangeEndpoints,
) -> Result<UsdmRiskSnapshot, ExchangeFetchError> {
    let response = client
        .get(format!(
            "{}/fapi/v1/exchangeInfo",
            endpoints.binance_usdm_base_url.trim_end_matches('/')
        ))
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
    let payload: BinanceExchangeInfoResponse =
        response
            .json()
            .map_err(|err| ExchangeFetchError::MalformedRiskResponse {
                alias: source.alias.clone(),
                template: source.template.as_str(),
                symbol: source.symbol.clone(),
                message: err.to_string(),
            })?;
    let Some(symbol_entry) = payload
        .symbols
        .into_iter()
        .find(|entry| entry.symbol == source.symbol)
    else {
        return Err(ExchangeFetchError::MalformedRiskResponse {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
            message: "requested symbol missing from exchangeInfo response".to_string(),
        });
    };
    let Some(required_margin_percent) = symbol_entry.required_margin_percent else {
        return Err(ExchangeFetchError::MalformedRiskResponse {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
            message: "exchangeInfo did not include requiredMarginPercent".to_string(),
        });
    };
    let Some(maint_margin_percent) = symbol_entry.maint_margin_percent else {
        return Err(ExchangeFetchError::MalformedRiskResponse {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
            message: "exchangeInfo did not include maintMarginPercent".to_string(),
        });
    };
    let initial_margin_rate = normalize_margin_percent(required_margin_percent);
    let maintenance_margin_rate = normalize_margin_percent(maint_margin_percent);
    if !initial_margin_rate.is_finite() || initial_margin_rate <= 0.0 {
        return Err(ExchangeFetchError::MalformedRiskResponse {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
            message: "exchangeInfo requiredMarginPercent must be greater than zero".to_string(),
        });
    }
    if !maintenance_margin_rate.is_finite() || maintenance_margin_rate < 0.0 {
        return Err(ExchangeFetchError::MalformedRiskResponse {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
            message: "exchangeInfo maintMarginPercent must be zero or greater".to_string(),
        });
    }
    Ok(UsdmRiskSnapshot {
        symbol: source.symbol.clone(),
        fetched_at_ms: now_ms(),
        source: UsdmRiskSource::PublicExchangeInfoApproximation,
        brackets: vec![RiskTier {
            lower_bound: 0.0,
            upper_bound: None,
            max_leverage: 1.0 / initial_margin_rate,
            maintenance_margin_rate,
            maintenance_amount: 0.0,
        }],
    })
}

#[derive(Clone, Debug, Serialize)]
struct FundingRateQuery<'a> {
    symbol: &'a str,
    #[serde(rename = "startTime")]
    start_time: i64,
    #[serde(rename = "endTime")]
    end_time: i64,
    limit: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct FundingRateRow {
    #[serde(rename = "fundingRate", deserialize_with = "deserialize_f64_text")]
    funding_rate: f64,
    #[serde(rename = "fundingTime", deserialize_with = "deserialize_i64_text")]
    funding_time: i64,
}

#[derive(Clone, Debug, Serialize)]
struct BasisQuery<'a> {
    pair: &'a str,
    #[serde(rename = "contractType")]
    contract_type: &'a str,
    period: &'a str,
    #[serde(rename = "startTime")]
    start_time: i64,
    #[serde(rename = "endTime")]
    end_time: i64,
    limit: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct BasisRow {
    #[serde(deserialize_with = "deserialize_f64_text")]
    basis: f64,
    #[serde(rename = "timestamp", deserialize_with = "deserialize_i64_text")]
    timestamp: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScalarSample {
    time_ms: i64,
    value: f64,
}

fn fetch_scalar_kline_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
    endpoint: ScalarKlineFieldEndpoint,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let bars = fetch_binance_bars(
        client,
        source,
        interval,
        from_ms,
        to_ms,
        base_url,
        BinanceKlineEndpoint {
            path: endpoint.path,
            page_limit: PAGE_LIMIT,
        },
    )?;
    Ok(bars
        .into_iter()
        .map(|bar| auxiliary_bar(bar.time as i64, endpoint.field, bar.close))
        .collect())
}

fn fetch_funding_rate_samples(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<ScalarSample>, ExchangeFetchError> {
    let mut start_time = from_ms;
    let mut samples = Vec::new();
    loop {
        let response = client
            .get(format!(
                "{}/fapi/v1/fundingRate",
                base_url.trim_end_matches('/')
            ))
            .query(&FundingRateQuery {
                symbol: source.symbol.as_str(),
                start_time,
                end_time: to_ms.saturating_sub(1),
                limit: FUNDING_RATE_LIMIT,
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
        let rows: Vec<FundingRateRow> = decode_json_response(response, source, interval)?;
        if rows.is_empty() {
            break;
        }
        let mut last_time = None;
        let row_count = rows.len();
        for row in rows {
            if row.funding_time < from_ms || row.funding_time >= to_ms {
                continue;
            }
            if samples
                .last()
                .is_some_and(|previous: &ScalarSample| previous.time_ms >= row.funding_time)
            {
                return Err(malformed_response(
                    source,
                    interval,
                    "non-increasing funding-rate response".to_string(),
                ));
            }
            last_time = Some(row.funding_time);
            samples.push(ScalarSample {
                time_ms: row.funding_time,
                value: row.funding_rate,
            });
        }
        if row_count < FUNDING_RATE_LIMIT {
            break;
        }
        let Some(last_time) = last_time else {
            break;
        };
        let Some(next_start) = last_time.checked_add(1) else {
            break;
        };
        if next_start >= to_ms {
            break;
        }
        start_time = next_start;
    }
    if samples.is_empty() {
        return Err(no_data(source, interval, from_ms, to_ms));
    }
    Ok(samples)
}

fn fetch_basis_samples(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<ScalarSample>, ExchangeFetchError> {
    let period = basis_period(interval);
    let mut start_time = from_ms;
    let mut samples = Vec::new();
    loop {
        let response = client
            .get(format!(
                "{}/futures/data/basis",
                base_url.trim_end_matches('/')
            ))
            .query(&BasisQuery {
                pair: source.symbol.as_str(),
                contract_type: "PERPETUAL",
                period,
                start_time,
                end_time: to_ms.saturating_sub(1),
                limit: BASIS_LIMIT,
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
        let rows: Vec<BasisRow> = decode_json_response(response, source, interval)?;
        if rows.is_empty() {
            break;
        }
        let mut last_time = None;
        let row_count = rows.len();
        for row in rows {
            if row.timestamp < from_ms || row.timestamp >= to_ms {
                continue;
            }
            if samples
                .last()
                .is_some_and(|previous: &ScalarSample| previous.time_ms >= row.timestamp)
            {
                return Err(malformed_response(
                    source,
                    interval,
                    "non-increasing basis response".to_string(),
                ));
            }
            last_time = Some(row.timestamp);
            samples.push(ScalarSample {
                time_ms: row.timestamp,
                value: row.basis,
            });
        }
        if row_count < BASIS_LIMIT {
            break;
        }
        let Some(last_time) = last_time else {
            break;
        };
        let Some(next_start) = last_time.checked_add(1) else {
            break;
        };
        if next_start >= to_ms {
            break;
        }
        start_time = next_start;
    }
    if samples.is_empty() {
        return Err(no_data(source, interval, from_ms, to_ms));
    }
    Ok(samples)
}

fn densify_scalar_samples(
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    samples: &[ScalarSample],
    field: MarketField,
) -> Vec<Bar> {
    let mut bars = Vec::new();
    let mut sample_index = 0usize;
    let mut current_value = None;
    let Some(mut open_time) = interval.bucket_open_time(from_ms) else {
        return bars;
    };
    if open_time < from_ms {
        let Some(next_open) = interval.next_open_time(open_time) else {
            return bars;
        };
        open_time = next_open;
    }
    while open_time < to_ms {
        let Some(close_time) = interval.next_open_time(open_time) else {
            break;
        };
        while let Some(sample) = samples.get(sample_index) {
            if sample.time_ms >= close_time {
                break;
            }
            current_value = Some(sample.value);
            sample_index += 1;
        }
        let mut bar = empty_bar(open_time);
        set_auxiliary_value(&mut bar, field, current_value);
        bars.push(bar);
        open_time = close_time;
    }
    bars
}

fn auxiliary_bar(open_time_ms: i64, field: MarketField, value: f64) -> Bar {
    let mut bar = empty_bar(open_time_ms);
    set_auxiliary_value(&mut bar, field, Some(value));
    bar
}

fn empty_bar(open_time_ms: i64) -> Bar {
    Bar {
        open: f64::NAN,
        high: f64::NAN,
        low: f64::NAN,
        close: f64::NAN,
        volume: f64::NAN,
        time: open_time_ms as f64,
        funding_rate: None,
        open_interest: None,
        mark_price: None,
        index_price: None,
        premium_index: None,
        basis: None,
    }
}

fn set_auxiliary_value(bar: &mut Bar, field: MarketField, value: Option<f64>) {
    match field {
        MarketField::FundingRate => bar.funding_rate = value,
        MarketField::MarkPrice => bar.mark_price = value,
        MarketField::IndexPrice => bar.index_price = value,
        MarketField::PremiumIndex => bar.premium_index = value,
        MarketField::Basis => bar.basis = value,
        MarketField::Open
        | MarketField::High
        | MarketField::Low
        | MarketField::Close
        | MarketField::Volume
        | MarketField::Time => {}
    }
}

fn basis_period(interval: Interval) -> &'static str {
    match interval {
        Interval::Sec1 | Interval::Min1 | Interval::Min3 | Interval::Min5 => "5m",
        Interval::Min15 => "15m",
        Interval::Min30 => "30m",
        Interval::Hour1 => "1h",
        Interval::Hour2 => "2h",
        Interval::Hour4 => "4h",
        Interval::Hour6 | Interval::Hour8 => "6h",
        Interval::Hour12 => "12h",
        Interval::Day1 | Interval::Day3 | Interval::Week1 | Interval::Month1 => "1d",
    }
}

fn fetch_server_time(
    client: &Client,
    endpoints: &ExchangeEndpoints,
) -> Result<i64, ExchangeFetchError> {
    let response = client
        .get(format!(
            "{}/fapi/v1/time",
            endpoints.binance_usdm_base_url.trim_end_matches('/')
        ))
        .send()
        .map_err(|err| ExchangeFetchError::RiskRequestFailed {
            alias: "binance".to_string(),
            template: SourceTemplate::BinanceUsdm.as_str(),
            symbol: String::new(),
            message: err.to_string(),
        })?;
    if response.status() != StatusCode::OK {
        return Err(ExchangeFetchError::RiskRequestFailed {
            alias: "binance".to_string(),
            template: SourceTemplate::BinanceUsdm.as_str(),
            symbol: String::new(),
            message: http_status_message(response),
        });
    }
    let body: BinanceServerTimeResponse =
        response
            .json()
            .map_err(|err| ExchangeFetchError::MalformedRiskResponse {
                alias: "binance".to_string(),
                template: SourceTemplate::BinanceUsdm.as_str(),
                symbol: String::new(),
                message: err.to_string(),
            })?;
    Ok(body.server_time)
}

fn sign_query(secret: &str, query: &str) -> Result<String, String> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|err| err.to_string())?;
    mac.update(query.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}
