use std::env;

use hmac::{Hmac, Mac};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::Deserialize;
use sha2::Sha256;

use super::spot::{fetch_binance_bars, BinanceKlineEndpoint};
use crate::exchange::common::{
    deserialize_f64_text, deserialize_option_f64_text, http_status_message,
    normalize_margin_percent, now_ms,
};
use crate::exchange::{ExchangeEndpoints, ExchangeFetchError, RiskTier};
use crate::interval::{DeclaredMarketSource, Interval, SourceTemplate};
use crate::runtime::Bar;

const PAGE_LIMIT: usize = 1500;

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
