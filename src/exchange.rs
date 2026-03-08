//! Exchange-backed market data adapters for source-aware PalmScript runs.

use std::collections::BTreeSet;
use std::env;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::de::{self, Deserializer, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;

use crate::backtest::{
    BinanceUsdmRiskSnapshot, BinanceUsdmRiskSource, HyperliquidPerpsRiskSnapshot, MarkPriceBasis,
    PerpBacktestContext, RiskTier, VenueRiskSnapshot,
};
use crate::compiler::CompiledProgram;
use crate::interval::{DeclaredMarketSource, Interval, SourceIntervalRef, SourceTemplate};
use crate::runtime::{Bar, SourceFeed, SourceRuntimeConfig};

const BINANCE_SPOT_URL: &str = "https://api.binance.com";
const BINANCE_USDM_URL: &str = "https://fapi.binance.com";
const HYPERLIQUID_INFO_URL: &str = "https://api.hyperliquid.xyz/info";
const BINANCE_SPOT_PAGE_LIMIT: usize = 1000;
const BINANCE_USDM_PAGE_LIMIT: usize = 1500;
const HYPERLIQUID_PAGE_LIMIT: usize = 500;
const HYPERLIQUID_RECENT_CANDLE_LIMIT: usize = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceFetchConstraints {
    page_limit: usize,
    recent_candle_limit: Option<usize>,
}

impl SourceFetchConstraints {
    const fn for_template(template: SourceTemplate) -> Self {
        match template {
            SourceTemplate::BinanceSpot => Self {
                page_limit: BINANCE_SPOT_PAGE_LIMIT,
                recent_candle_limit: None,
            },
            SourceTemplate::BinanceUsdm => Self {
                page_limit: BINANCE_USDM_PAGE_LIMIT,
                recent_candle_limit: None,
            },
            SourceTemplate::HyperliquidSpot | SourceTemplate::HyperliquidPerps => Self {
                page_limit: HYPERLIQUID_PAGE_LIMIT,
                recent_candle_limit: Some(HYPERLIQUID_RECENT_CANDLE_LIMIT),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct BinanceKlineQuery<'a> {
    symbol: &'a str,
    interval: &'a str,
    #[serde(rename = "startTime")]
    start_time: i64,
    #[serde(rename = "endTime")]
    end_time: i64,
    limit: usize,
}

#[derive(Clone, Debug)]
struct BinanceKlineRow {
    open_time: i64,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
}

impl BinanceKlineRow {
    fn open_time(&self) -> i64 {
        self.open_time
    }

    fn to_bar(
        &self,
        source: &DeclaredMarketSource,
        interval: Interval,
    ) -> Result<Bar, ExchangeFetchError> {
        Ok(Bar {
            time: self.open_time as f64,
            open: parse_text_f64(&self.open, source, interval, "open")?,
            high: parse_text_f64(&self.high, source, interval, "high")?,
            low: parse_text_f64(&self.low, source, interval, "low")?,
            close: parse_text_f64(&self.close, source, interval, "close")?,
            volume: parse_text_f64(&self.volume, source, interval, "volume")?,
        })
    }
}

impl<'de> Deserialize<'de> for BinanceKlineRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BinanceKlineRowVisitor;

        impl<'de> Visitor<'de> for BinanceKlineRowVisitor {
            type Value = BinanceKlineRow;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a Binance kline array with at least six OHLCV fields")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let open_time = seq
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
                let volume = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(5, &self))?;

                while let Some(IgnoredAny) = seq.next_element()? {}

                Ok(BinanceKlineRow {
                    open_time,
                    open,
                    high,
                    low,
                    close,
                    volume,
                })
            }
        }

        deserializer.deserialize_seq(BinanceKlineRowVisitor)
    }
}

#[derive(Clone, Debug, Serialize)]
struct HyperliquidSpotMetaRequest {
    #[serde(rename = "type")]
    request_type: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct HyperliquidCandleSnapshotRequest<'a> {
    #[serde(rename = "type")]
    request_type: &'static str,
    req: HyperliquidCandleSnapshotParams<'a>,
}

#[derive(Clone, Debug, Serialize)]
struct HyperliquidCandleSnapshotParams<'a> {
    coin: &'a str,
    interval: &'a str,
    #[serde(rename = "startTime")]
    start_time: i64,
    #[serde(rename = "endTime")]
    end_time: i64,
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

#[derive(Clone, Debug, Deserialize)]
struct HyperliquidMetaResponse {
    universe: Vec<HyperliquidPerpsMetaAsset>,
    #[serde(rename = "marginTables")]
    margin_tables: Vec<HyperliquidMarginTableEntry>,
}

#[derive(Clone, Debug, Deserialize)]
struct HyperliquidPerpsMetaAsset {
    name: String,
    #[serde(rename = "marginTableId")]
    margin_table_id: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct HyperliquidMarginTable {
    #[serde(rename = "marginTiers")]
    margin_tiers: Vec<HyperliquidMarginTier>,
}

#[derive(Clone, Debug, Deserialize)]
struct HyperliquidMarginTier {
    #[serde(rename = "lowerBound", deserialize_with = "deserialize_f64_text")]
    lower_bound: f64,
    #[serde(rename = "maxLeverage")]
    max_leverage: f64,
}

type HyperliquidMarginTableEntry = (u32, HyperliquidMarginTable);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExchangeEndpoints {
    pub binance_spot_base_url: String,
    pub binance_usdm_base_url: String,
    pub hyperliquid_info_url: String,
}

impl Default for ExchangeEndpoints {
    fn default() -> Self {
        Self {
            binance_spot_base_url: BINANCE_SPOT_URL.to_string(),
            binance_usdm_base_url: BINANCE_USDM_URL.to_string(),
            hyperliquid_info_url: HYPERLIQUID_INFO_URL.to_string(),
        }
    }
}

impl ExchangeEndpoints {
    pub fn from_env() -> Self {
        Self {
            binance_spot_base_url: env::var("PALMSCRIPT_BINANCE_SPOT_BASE_URL")
                .unwrap_or_else(|_| BINANCE_SPOT_URL.to_string()),
            binance_usdm_base_url: env::var("PALMSCRIPT_BINANCE_USDM_BASE_URL")
                .unwrap_or_else(|_| BINANCE_USDM_URL.to_string()),
            hyperliquid_info_url: env::var("PALMSCRIPT_HYPERLIQUID_INFO_URL")
                .unwrap_or_else(|_| HYPERLIQUID_INFO_URL.to_string()),
        }
    }
}

#[derive(Debug, Error)]
pub enum ExchangeFetchError {
    #[error("exchange-backed runs require a base interval declaration")]
    MissingBaseInterval,
    #[error("exchange-backed runs require at least one `source` declaration")]
    MissingSources,
    #[error("invalid market time window: from {from_ms} must be less than to {to_ms}")]
    InvalidTimeWindow { from_ms: i64, to_ms: i64 },
    #[error("source `{alias}` with template `{template}` does not support interval `{interval}`")]
    UnsupportedInterval {
        alias: String,
        template: &'static str,
        interval: &'static str,
    },
    #[error("source `{alias}` ({template}) `{symbol}` {interval} requires {requested_candles} candle(s) for the requested window, but the venue only provides the most recent {max_candles} candle(s) over REST")]
    RecentHistoryLimitExceeded {
        alias: String,
        template: &'static str,
        symbol: String,
        interval: &'static str,
        requested_candles: usize,
        max_candles: usize,
    },
    #[error("failed to fetch `{alias}` ({template}) `{symbol}` {interval}: {message}")]
    RequestFailed {
        alias: String,
        template: &'static str,
        symbol: String,
        interval: &'static str,
        message: String,
    },
    #[error("malformed response for `{alias}` ({template}) `{symbol}` {interval}: {message}")]
    MalformedResponse {
        alias: String,
        template: &'static str,
        symbol: String,
        interval: &'static str,
        message: String,
    },
    #[error("no data returned for `{alias}` ({template}) `{symbol}` {interval}")]
    NoData {
        alias: String,
        template: &'static str,
        symbol: String,
        interval: &'static str,
    },
    #[error("unknown Hyperliquid spot symbol `{symbol}`")]
    UnknownHyperliquidSpotSymbol { symbol: String },
    #[error("perp risk fetch for `{alias}` ({template}) `{symbol}` failed: {message}")]
    RiskRequestFailed {
        alias: String,
        template: &'static str,
        symbol: String,
        message: String,
    },
    #[error("perp risk response for `{alias}` ({template}) `{symbol}` was malformed: {message}")]
    MalformedRiskResponse {
        alias: String,
        template: &'static str,
        symbol: String,
        message: String,
    },
    #[error("unknown Hyperliquid perp symbol `{symbol}`")]
    UnknownHyperliquidPerpSymbol { symbol: String },
    #[error("no risk tiers available for `{alias}` ({template}) `{symbol}`")]
    MissingRiskTiers {
        alias: String,
        template: &'static str,
        symbol: String,
    },
}

pub fn fetch_source_runtime_config(
    compiled: &CompiledProgram,
    from_ms: i64,
    to_ms: i64,
    endpoints: &ExchangeEndpoints,
) -> Result<SourceRuntimeConfig, ExchangeFetchError> {
    if from_ms >= to_ms {
        return Err(ExchangeFetchError::InvalidTimeWindow { from_ms, to_ms });
    }
    let base_interval = compiled
        .program
        .base_interval
        .ok_or(ExchangeFetchError::MissingBaseInterval)?;
    if compiled.program.declared_sources.is_empty() {
        return Err(ExchangeFetchError::MissingSources);
    }

    let client = Client::builder()
        .build()
        .map_err(|err| ExchangeFetchError::RequestFailed {
            alias: "client".to_string(),
            template: "http",
            symbol: String::new(),
            interval: "",
            message: err.to_string(),
        })?;

    let mut required = BTreeSet::<SourceIntervalRef>::new();
    for source in &compiled.program.declared_sources {
        required.insert(SourceIntervalRef {
            source_id: source.id,
            interval: base_interval,
        });
    }
    required.extend(compiled.program.source_intervals.iter().copied());

    let mut feeds = Vec::new();
    for requirement in required {
        let source = compiled
            .program
            .declared_sources
            .iter()
            .find(|source| source.id == requirement.source_id)
            .expect("compiled source interval references should resolve");
        if !source.template.supports_interval(requirement.interval) {
            return Err(ExchangeFetchError::UnsupportedInterval {
                alias: source.alias.clone(),
                template: source.template.as_str(),
                interval: requirement.interval.as_str(),
            });
        }
        validate_source_request(source, requirement.interval, from_ms, to_ms)?;
        let bars = fetch_source_bars(
            &client,
            source,
            requirement.interval,
            from_ms,
            to_ms,
            endpoints,
        )?;
        feeds.push(SourceFeed {
            source_id: source.id,
            interval: requirement.interval,
            bars,
        });
    }

    Ok(SourceRuntimeConfig {
        base_interval,
        feeds,
    })
}

pub fn fetch_perp_backtest_context(
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    endpoints: &ExchangeEndpoints,
) -> Result<Option<PerpBacktestContext>, ExchangeFetchError> {
    let client =
        Client::builder()
            .build()
            .map_err(|err| ExchangeFetchError::RiskRequestFailed {
                alias: source.alias.clone(),
                template: source.template.as_str(),
                symbol: source.symbol.clone(),
                message: err.to_string(),
            })?;
    match source.template {
        SourceTemplate::BinanceUsdm => {
            let mark_bars = fetch_binance_bars(
                &client,
                source,
                interval,
                from_ms,
                to_ms,
                &endpoints.binance_usdm_base_url,
                "/fapi/v1/premiumIndexKlines",
            )?;
            let risk_snapshot = fetch_binance_usdm_risk_snapshot(&client, source, endpoints)?;
            Ok(Some(PerpBacktestContext {
                mark_price_basis: MarkPriceBasis::BinancePremiumIndexKlines,
                mark_bars,
                risk_snapshot: VenueRiskSnapshot::BinanceUsdm(risk_snapshot),
            }))
        }
        SourceTemplate::HyperliquidPerps => {
            let mark_bars = fetch_hyperliquid_bars(
                &client,
                source,
                interval,
                from_ms,
                to_ms,
                &endpoints.hyperliquid_info_url,
                source.symbol.clone(),
            )?;
            let risk_snapshot = fetch_hyperliquid_perps_risk_snapshot(&client, source, endpoints)?;
            Ok(Some(PerpBacktestContext {
                mark_price_basis: MarkPriceBasis::HyperliquidExecutionFallback,
                mark_bars,
                risk_snapshot: VenueRiskSnapshot::HyperliquidPerps(risk_snapshot),
            }))
        }
        SourceTemplate::BinanceSpot | SourceTemplate::HyperliquidSpot => Ok(None),
    }
}

fn fetch_source_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    endpoints: &ExchangeEndpoints,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    match source.template {
        SourceTemplate::BinanceSpot => fetch_binance_bars(
            client,
            source,
            interval,
            from_ms,
            to_ms,
            &endpoints.binance_spot_base_url,
            "/api/v3/klines",
        ),
        SourceTemplate::BinanceUsdm => fetch_binance_bars(
            client,
            source,
            interval,
            from_ms,
            to_ms,
            &endpoints.binance_usdm_base_url,
            "/fapi/v1/klines",
        ),
        SourceTemplate::HyperliquidPerps => fetch_hyperliquid_bars(
            client,
            source,
            interval,
            from_ms,
            to_ms,
            &endpoints.hyperliquid_info_url,
            source.symbol.clone(),
        ),
        SourceTemplate::HyperliquidSpot => {
            let coin = resolve_hyperliquid_spot_coin(client, &source.symbol, endpoints)?;
            fetch_hyperliquid_bars(
                client,
                source,
                interval,
                from_ms,
                to_ms,
                &endpoints.hyperliquid_info_url,
                coin,
            )
        }
    }
}

fn fetch_binance_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
    path: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let page_limit = SourceFetchConstraints::for_template(source.template).page_limit;
    let mut start_time = from_ms;
    let mut bars: Vec<Bar> = Vec::new();
    loop {
        let response = client
            .get(format!("{}{}", base_url.trim_end_matches('/'), path))
            .query(&BinanceKlineQuery {
                symbol: source.symbol.as_str(),
                interval: interval.as_str(),
                start_time,
                end_time: to_ms.saturating_sub(1),
                limit: page_limit,
            })
            .send()
            .map_err(|err| request_failed(source, interval, err.to_string()))?;
        if response.status() != StatusCode::OK {
            return Err(request_failed(
                source,
                interval,
                format!("HTTP {}", response.status()),
            ));
        }
        let rows: Vec<BinanceKlineRow> = response
            .json()
            .map_err(|err| malformed_response(source, interval, err.to_string()))?;
        if rows.is_empty() {
            break;
        }

        let mut last_open = None;
        for row in &rows {
            let bar = row.to_bar(source, interval)?;
            let open_time = bar.time as i64;
            if open_time < from_ms || open_time >= to_ms {
                continue;
            }
            if let Some(previous) = bars.last() {
                let previous_open = previous.time as i64;
                if open_time <= previous_open {
                    return Err(malformed_response(
                        source,
                        interval,
                        "non-increasing kline response".to_string(),
                    ));
                }
            }
            last_open = Some(row.open_time());
            bars.push(bar);
        }

        if rows.len() < page_limit {
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

fn fetch_hyperliquid_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    info_url: &str,
    coin: String,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let page_limit = SourceFetchConstraints::for_template(source.template).page_limit;
    let mut start_time = from_ms;
    let mut bars: Vec<Bar> = Vec::new();
    loop {
        let response = client
            .post(info_url)
            .json(&HyperliquidCandleSnapshotRequest {
                request_type: "candleSnapshot",
                req: HyperliquidCandleSnapshotParams {
                    coin: &coin,
                    interval: interval.as_str(),
                    start_time,
                    end_time: to_ms,
                },
            })
            .send()
            .map_err(|err| request_failed(source, interval, err.to_string()))?;
        if response.status() != StatusCode::OK {
            return Err(request_failed(
                source,
                interval,
                format!("HTTP {}", response.status()),
            ));
        }
        let rows: Vec<HyperliquidCandle> = response
            .json()
            .map_err(|err| malformed_response(source, interval, err.to_string()))?;
        if rows.is_empty() {
            break;
        }

        let mut last_open = None;
        for row in &rows {
            let bar = row.to_bar(source, interval)?;
            let open_time = bar.time as i64;
            if open_time < from_ms || open_time >= to_ms {
                continue;
            }
            if let Some(previous) = bars.last() {
                let previous_open = previous.time as i64;
                if open_time <= previous_open {
                    return Err(malformed_response(
                        source,
                        interval,
                        "non-increasing candle response".to_string(),
                    ));
                }
            }
            last_open = Some(open_time);
            bars.push(bar);
        }

        if rows.len() < page_limit {
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

fn fetch_binance_usdm_risk_snapshot(
    client: &Client,
    source: &DeclaredMarketSource,
    endpoints: &ExchangeEndpoints,
) -> Result<BinanceUsdmRiskSnapshot, ExchangeFetchError> {
    let api_key = env::var("PALMSCRIPT_BINANCE_USDM_API_KEY");
    let api_secret = env::var("PALMSCRIPT_BINANCE_USDM_API_SECRET");
    let (Ok(api_key), Ok(api_secret)) = (api_key, api_secret) else {
        return fetch_binance_usdm_public_risk_snapshot(client, source, endpoints);
    };
    let server_time = fetch_binance_server_time(client, endpoints)?;
    let query = format!("symbol={}&timestamp={server_time}", source.symbol);
    let signature = sign_binance_query(&api_secret, &query).map_err(|err| {
        ExchangeFetchError::RiskRequestFailed {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
            message: err,
        }
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
            message: format!("HTTP {}", response.status()),
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
    Ok(BinanceUsdmRiskSnapshot {
        symbol: source.symbol.clone(),
        fetched_at_ms: server_time,
        source: BinanceUsdmRiskSource::SignedLeverageBrackets,
        brackets,
    })
}

fn fetch_binance_usdm_public_risk_snapshot(
    client: &Client,
    source: &DeclaredMarketSource,
    endpoints: &ExchangeEndpoints,
) -> Result<BinanceUsdmRiskSnapshot, ExchangeFetchError> {
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
            message: format!("HTTP {}", response.status()),
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
    let fetched_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    Ok(BinanceUsdmRiskSnapshot {
        symbol: source.symbol.clone(),
        fetched_at_ms,
        source: BinanceUsdmRiskSource::PublicExchangeInfoApproximation,
        brackets: vec![RiskTier {
            lower_bound: 0.0,
            upper_bound: None,
            max_leverage: 1.0 / initial_margin_rate,
            maintenance_margin_rate,
            maintenance_amount: 0.0,
        }],
    })
}

fn fetch_hyperliquid_perps_risk_snapshot(
    client: &Client,
    source: &DeclaredMarketSource,
    endpoints: &ExchangeEndpoints,
) -> Result<HyperliquidPerpsRiskSnapshot, ExchangeFetchError> {
    let fetched_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    let response = client
        .post(&endpoints.hyperliquid_info_url)
        .json(&HyperliquidSpotMetaRequest {
            request_type: "meta",
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
            message: format!("HTTP {}", response.status()),
        });
    }
    let meta: HyperliquidMetaResponse =
        response
            .json()
            .map_err(|err| ExchangeFetchError::MalformedRiskResponse {
                alias: source.alias.clone(),
                template: source.template.as_str(),
                symbol: source.symbol.clone(),
                message: err.to_string(),
            })?;
    let Some(asset) = meta
        .universe
        .into_iter()
        .find(|asset| asset.name == source.symbol)
    else {
        return Err(ExchangeFetchError::UnknownHyperliquidPerpSymbol {
            symbol: source.symbol.clone(),
        });
    };
    let Some((_, table)) = meta
        .margin_tables
        .into_iter()
        .find(|(table_id, _)| *table_id == asset.margin_table_id)
    else {
        return Err(ExchangeFetchError::MissingRiskTiers {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
        });
    };
    let mut tiers = Vec::with_capacity(table.margin_tiers.len());
    for (index, tier) in table.margin_tiers.iter().enumerate() {
        let upper_bound = table
            .margin_tiers
            .get(index + 1)
            .map(|next| next.lower_bound);
        // Hyperliquid documents maintenance margin as half of the initial margin
        // implied by the tier max leverage.
        let maintenance_margin_rate = 0.5 / tier.max_leverage;
        tiers.push(RiskTier {
            lower_bound: tier.lower_bound,
            upper_bound,
            max_leverage: tier.max_leverage,
            maintenance_margin_rate,
            maintenance_amount: 0.0,
        });
    }
    if tiers.is_empty() {
        return Err(ExchangeFetchError::MissingRiskTiers {
            alias: source.alias.clone(),
            template: source.template.as_str(),
            symbol: source.symbol.clone(),
        });
    }
    Ok(HyperliquidPerpsRiskSnapshot {
        coin: source.symbol.clone(),
        fetched_at_ms,
        margin_table_id: asset.margin_table_id,
        tiers,
    })
}

fn fetch_binance_server_time(
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
            message: format!("HTTP {}", response.status()),
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

fn sign_binance_query(secret: &str, query: &str) -> Result<String, String> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|err| err.to_string())?;
    mac.update(query.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn deserialize_f64_text<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    struct F64TextVisitor;

    impl<'de> Visitor<'de> for F64TextVisitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a float or float-like string")
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
            Ok(value as f64)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(value as f64)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value
                .parse::<f64>()
                .map_err(|err| E::custom(err.to_string()))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_any(F64TextVisitor)
}

fn deserialize_option_f64_text<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptionF64TextVisitor;

    impl<'de> Visitor<'de> for OptionF64TextVisitor {
        type Value = Option<f64>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an optional float or float-like string")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserialize_f64_text(deserializer).map(Some)
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
            Ok(Some(value))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
            Ok(Some(value as f64))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(Some(value as f64))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value
                .parse::<f64>()
                .map(Some)
                .map_err(|err| E::custom(err.to_string()))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_option(OptionF64TextVisitor)
}

fn normalize_margin_percent(raw: f64) -> f64 {
    if raw > 1.0 {
        raw / 100.0
    } else {
        raw
    }
}

fn resolve_hyperliquid_spot_coin(
    client: &Client,
    symbol: &str,
    endpoints: &ExchangeEndpoints,
) -> Result<String, ExchangeFetchError> {
    let response = client
        .post(&endpoints.hyperliquid_info_url)
        .json(&HyperliquidSpotMetaRequest {
            request_type: "spotMeta",
        })
        .send()
        .map_err(|err| ExchangeFetchError::RequestFailed {
            alias: "spotMeta".to_string(),
            template: SourceTemplate::HyperliquidSpot.as_str(),
            symbol: symbol.to_string(),
            interval: "",
            message: err.to_string(),
        })?;
    if response.status() != StatusCode::OK {
        return Err(ExchangeFetchError::RequestFailed {
            alias: "spotMeta".to_string(),
            template: SourceTemplate::HyperliquidSpot.as_str(),
            symbol: symbol.to_string(),
            interval: "",
            message: format!("HTTP {}", response.status()),
        });
    }
    let meta: HyperliquidSpotMeta =
        response
            .json()
            .map_err(|err| ExchangeFetchError::MalformedResponse {
                alias: "spotMeta".to_string(),
                template: SourceTemplate::HyperliquidSpot.as_str(),
                symbol: symbol.to_string(),
                interval: "",
                message: err.to_string(),
            })?;

    if meta
        .universe
        .iter()
        .any(|entry| entry.name.eq_ignore_ascii_case(symbol))
    {
        return Ok(symbol.to_string());
    }
    let canonical_pair = format!("{}/USDC", symbol.to_ascii_uppercase());
    if let Some(entry) = meta
        .universe
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(&canonical_pair))
    {
        return Ok(entry.name.clone());
    }
    if let Some(token) = meta
        .tokens
        .iter()
        .find(|token| token.name.eq_ignore_ascii_case(symbol))
    {
        if let Some(entry) = meta
            .universe
            .iter()
            .find(|entry| entry.tokens.first().copied() == Some(token.index))
        {
            return Ok(entry.name.clone());
        }
    }
    Err(ExchangeFetchError::UnknownHyperliquidSpotSymbol {
        symbol: symbol.to_string(),
    })
}

fn validate_source_request(
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
) -> Result<(), ExchangeFetchError> {
    let constraints = SourceFetchConstraints::for_template(source.template);
    if let Some(max_candles) = constraints.recent_candle_limit {
        let requested_candles = requested_candle_count(interval, from_ms, to_ms);
        if requested_candles > max_candles {
            return Err(ExchangeFetchError::RecentHistoryLimitExceeded {
                alias: source.alias.clone(),
                template: source.template.as_str(),
                symbol: source.symbol.clone(),
                interval: interval.as_str(),
                requested_candles,
                max_candles,
            });
        }
    }
    Ok(())
}

fn requested_candle_count(interval: Interval, from_ms: i64, to_ms: i64) -> usize {
    if from_ms >= to_ms {
        return 0;
    }
    let Some(mut open_time) = first_open_time_in_window(interval, from_ms, to_ms) else {
        return 0;
    };

    let mut count = 0usize;
    while open_time < to_ms {
        count = count.saturating_add(1);
        let Some(next_open) = interval.next_open_time(open_time) else {
            break;
        };
        open_time = next_open;
    }
    count
}

fn first_open_time_in_window(interval: Interval, from_ms: i64, to_ms: i64) -> Option<i64> {
    if from_ms >= to_ms {
        return None;
    }
    let bucket_open = interval.bucket_open_time(from_ms)?;
    let first_open = if bucket_open >= from_ms {
        bucket_open
    } else {
        interval.next_open_time(bucket_open)?
    };
    (first_open < to_ms).then_some(first_open)
}

fn parse_text_f64(
    value: &str,
    source: &DeclaredMarketSource,
    interval: Interval,
    field: &str,
) -> Result<f64, ExchangeFetchError> {
    value
        .parse::<f64>()
        .map_err(|_| malformed_response(source, interval, format!("invalid `{field}` value")))
}

fn request_failed(
    source: &DeclaredMarketSource,
    interval: Interval,
    message: String,
) -> ExchangeFetchError {
    ExchangeFetchError::RequestFailed {
        alias: source.alias.clone(),
        template: source.template.as_str(),
        symbol: source.symbol.clone(),
        interval: interval.as_str(),
        message,
    }
}

fn malformed_response(
    source: &DeclaredMarketSource,
    interval: Interval,
    message: String,
) -> ExchangeFetchError {
    ExchangeFetchError::MalformedResponse {
        alias: source.alias.clone(),
        template: source.template.as_str(),
        symbol: source.symbol.clone(),
        interval: interval.as_str(),
        message,
    }
}

fn no_data(source: &DeclaredMarketSource, interval: Interval) -> ExchangeFetchError {
    ExchangeFetchError::NoData {
        alias: source.alias.clone(),
        template: source.template.as_str(),
        symbol: source.symbol.clone(),
        interval: interval.as_str(),
    }
}

#[derive(Clone, Debug, Deserialize)]
struct HyperliquidSpotMeta {
    universe: Vec<HyperliquidSpotUniverseEntry>,
    tokens: Vec<HyperliquidSpotToken>,
}

#[derive(Clone, Debug, Deserialize)]
struct HyperliquidSpotUniverseEntry {
    name: String,
    #[serde(default)]
    tokens: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize)]
struct HyperliquidSpotToken {
    name: String,
    index: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct HyperliquidCandle {
    #[serde(rename = "t")]
    open_time: i64,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "v")]
    volume: String,
}

impl HyperliquidCandle {
    fn to_bar(
        &self,
        source: &DeclaredMarketSource,
        interval: Interval,
    ) -> Result<Bar, ExchangeFetchError> {
        Ok(Bar {
            time: self.open_time as f64,
            open: self.open.parse().map_err(|_| {
                malformed_response(source, interval, "invalid `open` value".to_string())
            })?,
            high: self.high.parse().map_err(|_| {
                malformed_response(source, interval, "invalid `high` value".to_string())
            })?,
            low: self.low.parse().map_err(|_| {
                malformed_response(source, interval, "invalid `low` value".to_string())
            })?,
            close: self.close.parse().map_err(|_| {
                malformed_response(source, interval, "invalid `close` value".to_string())
            })?,
            volume: self.volume.parse().map_err(|_| {
                malformed_response(source, interval, "invalid `volume` value".to_string())
            })?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        fetch_perp_backtest_context, fetch_source_runtime_config, requested_candle_count,
        resolve_hyperliquid_spot_coin, BinanceKlineRow, ExchangeEndpoints, ExchangeFetchError,
        HyperliquidCandle, SourceFetchConstraints, BINANCE_USDM_PAGE_LIMIT,
        HYPERLIQUID_RECENT_CANDLE_LIMIT,
    };
    use crate::backtest::{BinanceUsdmRiskSource, MarkPriceBasis, VenueRiskSnapshot};
    use crate::compile;
    use crate::interval::{DeclaredMarketSource, Interval, SourceTemplate};
    use crate::runtime::Bar;
    use mockito::{Matcher, Server};
    use serde_json::json;
    use std::env;
    use std::sync::{Mutex, OnceLock};

    fn sample_source(template: SourceTemplate, symbol: &str) -> DeclaredMarketSource {
        DeclaredMarketSource {
            id: 0,
            alias: "src".to_string(),
            template,
            symbol: symbol.to_string(),
        }
    }

    fn binance_usdm_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn binance_kline_row_maps_ohlcv_fields() {
        let source = sample_source(SourceTemplate::BinanceSpot, "BTCUSDT");
        let row: BinanceKlineRow = serde_json::from_value(json!([
            1704067200000_i64,
            "1.0",
            "2.0",
            "0.5",
            "1.5",
            "10.0",
            1704067259999_i64,
            "15.0",
            42_u64,
            "6.0",
            "7.0",
            "0"
        ]))
        .expect("row deserializes");
        let bar = row.to_bar(&source, Interval::Min1).expect("row maps");
        assert_eq!(
            bar,
            Bar {
                time: 1704067200000.0,
                open: 1.0,
                high: 2.0,
                low: 0.5,
                close: 1.5,
                volume: 10.0,
            }
        );
    }

    #[test]
    fn hyperliquid_candle_maps_ohlcv_fields() {
        let source = sample_source(SourceTemplate::HyperliquidPerps, "BTC");
        let candle: HyperliquidCandle = serde_json::from_value(json!({
            "t": 1704067200000_i64,
            "o": "10.0",
            "h": "12.0",
            "l": "9.0",
            "c": "11.5",
            "v": "5.0"
        }))
        .expect("candle deserializes");
        let bar = candle.to_bar(&source, Interval::Min1).expect("candle maps");
        assert_eq!(
            bar,
            Bar {
                time: 1704067200000.0,
                open: 10.0,
                high: 12.0,
                low: 9.0,
                close: 11.5,
                volume: 5.0,
            }
        );
    }

    #[test]
    fn resolves_hyperliquid_spot_symbol_from_meta() {
        let mut server = Server::new();
        let _meta = server
            .mock("POST", "/info")
            .match_body(Matcher::Json(json!({ "type": "spotMeta" })))
            .with_status(200)
            .with_body(
                json!({
                    "universe": [{"name": "@107", "tokens": [107, 0]}],
                    "tokens": [{"name": "HYPE", "index": 107}]
                })
                .to_string(),
            )
            .create();

        let endpoints = ExchangeEndpoints {
            binance_spot_base_url: String::new(),
            binance_usdm_base_url: String::new(),
            hyperliquid_info_url: format!("{}/info", server.url()),
        };
        let client = reqwest::blocking::Client::new();
        let coin = resolve_hyperliquid_spot_coin(&client, "HYPE", &endpoints).expect("coin");
        assert_eq!(coin, "@107");
    }

    #[test]
    fn fetch_source_runtime_config_builds_all_required_feeds() {
        let mut server = Server::new();
        let _binance = server
            .mock("GET", "/api/v3/klines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1m".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [1704067200000_i64, "1.0", "2.0", "0.5", "1.5", "10.0"],
                    [1704067260000_i64, "2.0", "3.0", "1.5", "2.5", "11.0"]
                ])
                .to_string(),
            )
            .create();
        let _hyper_base = server
            .mock("POST", "/info")
            .match_body(Matcher::PartialJson(json!({
                "type": "candleSnapshot",
                "req": { "coin": "BTC", "interval": "1m" }
            })))
            .with_status(200)
            .with_body(
                json!([
                    {"t": 1704067200000_i64, "o": "10.0", "h": "11.0", "l": "9.0", "c": "10.5", "v": "5.0"},
                    {"t": 1704067260000_i64, "o": "10.5", "h": "12.0", "l": "10.0", "c": "11.5", "v": "6.0"}
                ])
                .to_string(),
            )
            .create();
        let _hyper_hour = server
            .mock("POST", "/info")
            .match_body(Matcher::PartialJson(json!({
                "type": "candleSnapshot",
                "req": { "coin": "BTC", "interval": "1h" }
            })))
            .with_status(200)
            .with_body(
                json!([
                    {"t": 1704067200000_i64, "o": "10.0", "h": "12.0", "l": "9.0", "c": "11.5", "v": "11.0"}
                ])
                .to_string(),
            )
            .create();

        let compiled = compile(
            "interval 1m\nsource bn = binance.spot(\"BTCUSDT\")\nsource hl = hyperliquid.perps(\"BTC\")\nuse hl 1h\nplot(bn.close - hl.1h.close)",
        )
        .expect("compile");
        let endpoints = ExchangeEndpoints {
            binance_spot_base_url: server.url(),
            binance_usdm_base_url: server.url(),
            hyperliquid_info_url: format!("{}/info", server.url()),
        };

        let config =
            fetch_source_runtime_config(&compiled, 1704067200000, 1704067320000, &endpoints)
                .expect("config");
        assert_eq!(config.base_interval, Interval::Min1);
        assert_eq!(config.feeds.len(), 3);
    }

    #[test]
    fn fetch_perp_backtest_context_reads_hyperliquid_margin_table_and_mark_bars() {
        let mut server = Server::new();
        let _meta = server
            .mock("POST", "/info")
            .match_body(Matcher::Json(json!({ "type": "meta" })))
            .with_status(200)
            .with_body(
                json!({
                    "universe": [{"name": "BTC", "marginTableId": 56, "maxLeverage": 40}],
                    "marginTables": [
                        [56, {"description": "tiered", "marginTiers": [
                            {"lowerBound": "0.0", "maxLeverage": 40},
                            {"lowerBound": "150000000.0", "maxLeverage": 20}
                        ]}]
                    ],
                    "collateralToken": "USDC"
                })
                .to_string(),
            )
            .create();
        let _candles = server
            .mock("POST", "/info")
            .match_body(Matcher::PartialJson(json!({
                "type": "candleSnapshot",
                "req": { "coin": "BTC", "interval": "1m" }
            })))
            .with_status(200)
            .with_body(
                json!([
                    {"t": 1704067200000_i64, "o": "10.0", "h": "11.0", "l": "9.0", "c": "10.5", "v": "5.0"},
                    {"t": 1704067260000_i64, "o": "10.5", "h": "12.0", "l": "10.0", "c": "11.5", "v": "6.0"}
                ])
                .to_string(),
            )
            .create();
        let source = sample_source(SourceTemplate::HyperliquidPerps, "BTC");
        let context = fetch_perp_backtest_context(
            &source,
            Interval::Min1,
            1704067200000,
            1704067320000,
            &ExchangeEndpoints {
                binance_spot_base_url: String::new(),
                binance_usdm_base_url: String::new(),
                hyperliquid_info_url: format!("{}/info", server.url()),
            },
        )
        .expect("context")
        .expect("perp context");
        assert_eq!(
            context.mark_price_basis,
            MarkPriceBasis::HyperliquidExecutionFallback
        );
        assert_eq!(context.mark_bars.len(), 2);
        match context.risk_snapshot {
            VenueRiskSnapshot::HyperliquidPerps(snapshot) => {
                assert_eq!(snapshot.coin, "BTC");
                assert_eq!(snapshot.margin_table_id, 56);
                assert_eq!(snapshot.tiers.len(), 2);
                assert_eq!(snapshot.tiers[0].maintenance_margin_rate, 0.0125);
            }
            other => panic!("unexpected snapshot: {other:?}"),
        }
    }

    #[test]
    fn fetch_perp_backtest_context_reads_binance_signed_risk_snapshot() {
        let _env_guard = binance_usdm_env_lock().lock().expect("env lock");
        let mut server = Server::new();
        let _time = server
            .mock("GET", "/fapi/v1/time")
            .with_status(200)
            .with_body(json!({ "serverTime": 1704067200000_i64 }).to_string())
            .create();
        let _marks = server
            .mock("GET", "/fapi/v1/premiumIndexKlines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1m".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [1704067200000_i64, "100.0", "101.0", "99.0", "100.5", "0"],
                    [1704067260000_i64, "100.5", "102.0", "100.0", "101.5", "0"]
                ])
                .to_string(),
            )
            .create();
        let _brackets = server
            .mock("GET", "/fapi/v1/leverageBracket")
            .match_header("x-mbx-apikey", "test-key")
            .match_query(Matcher::Regex(
                "symbol=BTCUSDT.*timestamp=1704067200000.*signature=".into(),
            ))
            .with_status(200)
            .with_body(
                json!([
                    {
                        "symbol": "BTCUSDT",
                        "brackets": [
                            {
                                "initialLeverage": 20,
                                "notionalFloor": "0",
                                "notionalCap": "100000",
                                "maintMarginRatio": "0.01",
                                "cum": "0"
                            }
                        ]
                    }
                ])
                .to_string(),
            )
            .create();

        env::set_var("PALMSCRIPT_BINANCE_USDM_API_KEY", "test-key");
        env::set_var("PALMSCRIPT_BINANCE_USDM_API_SECRET", "test-secret");
        let source = sample_source(SourceTemplate::BinanceUsdm, "BTCUSDT");
        let context = fetch_perp_backtest_context(
            &source,
            Interval::Min1,
            1704067200000,
            1704067320000,
            &ExchangeEndpoints {
                binance_spot_base_url: String::new(),
                binance_usdm_base_url: server.url(),
                hyperliquid_info_url: String::new(),
            },
        )
        .expect("context")
        .expect("perp context");
        env::remove_var("PALMSCRIPT_BINANCE_USDM_API_KEY");
        env::remove_var("PALMSCRIPT_BINANCE_USDM_API_SECRET");

        assert_eq!(
            context.mark_price_basis,
            MarkPriceBasis::BinancePremiumIndexKlines
        );
        match context.risk_snapshot {
            VenueRiskSnapshot::BinanceUsdm(snapshot) => {
                assert_eq!(snapshot.symbol, "BTCUSDT");
                assert_eq!(
                    snapshot.source,
                    BinanceUsdmRiskSource::SignedLeverageBrackets
                );
                assert_eq!(snapshot.brackets.len(), 1);
                assert_eq!(snapshot.brackets[0].maintenance_margin_rate, 0.01);
            }
            other => panic!("unexpected snapshot: {other:?}"),
        }
    }

    #[test]
    fn fetch_perp_backtest_context_falls_back_to_public_binance_exchange_info() {
        let _env_guard = binance_usdm_env_lock().lock().expect("env lock");
        let mut server = Server::new();
        let _marks = server
            .mock("GET", "/fapi/v1/premiumIndexKlines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1m".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [1704067200000_i64, "100.0", "101.0", "99.0", "100.5", "0"],
                    [1704067260000_i64, "100.5", "102.0", "100.0", "101.5", "0"]
                ])
                .to_string(),
            )
            .create();
        let _exchange_info = server
            .mock("GET", "/fapi/v1/exchangeInfo")
            .with_status(200)
            .with_body(
                json!({
                    "symbols": [
                        {
                            "symbol": "BTCUSDT",
                            "maintMarginPercent": "2.5",
                            "requiredMarginPercent": "5.0"
                        }
                    ]
                })
                .to_string(),
            )
            .create();

        env::remove_var("PALMSCRIPT_BINANCE_USDM_API_KEY");
        env::remove_var("PALMSCRIPT_BINANCE_USDM_API_SECRET");
        let source = sample_source(SourceTemplate::BinanceUsdm, "BTCUSDT");
        let context = fetch_perp_backtest_context(
            &source,
            Interval::Min1,
            1704067200000,
            1704067320000,
            &ExchangeEndpoints {
                binance_spot_base_url: String::new(),
                binance_usdm_base_url: server.url(),
                hyperliquid_info_url: String::new(),
            },
        )
        .expect("context")
        .expect("perp context");

        assert_eq!(
            context.mark_price_basis,
            MarkPriceBasis::BinancePremiumIndexKlines
        );
        match context.risk_snapshot {
            VenueRiskSnapshot::BinanceUsdm(snapshot) => {
                assert_eq!(
                    snapshot.source,
                    BinanceUsdmRiskSource::PublicExchangeInfoApproximation
                );
                assert_eq!(snapshot.brackets.len(), 1);
                assert_eq!(snapshot.brackets[0].max_leverage, 20.0);
                assert_eq!(snapshot.brackets[0].maintenance_margin_rate, 0.025);
            }
            other => panic!("unexpected snapshot: {other:?}"),
        }
    }

    #[test]
    fn hyperliquid_recent_history_limit_is_enforced_before_fetch() {
        let compiled =
            compile("interval 1m\nsource hl = hyperliquid.perps(\"BTC\")\nplot(hl.close)")
                .expect("compile");
        let err = fetch_source_runtime_config(
            &compiled,
            1_704_067_200_000,
            1_704_067_200_000 + 5_001 * 60_000,
            &ExchangeEndpoints::default(),
        )
        .expect_err("history limit should fail");
        assert_eq!(
            err.to_string(),
            "source `hl` (hyperliquid.perps) `BTC` 1m requires 5001 candle(s) for the requested window, but the venue only provides the most recent 5000 candle(s) over REST"
        );
    }

    #[test]
    fn requested_candle_count_skips_partial_open_bucket() {
        assert_eq!(
            requested_candle_count(Interval::Min1, 1_704_067_200_001, 1_704_067_260_000),
            0
        );
        assert_eq!(
            requested_candle_count(Interval::Min1, 1_704_067_200_000, 1_704_067_320_000),
            2
        );
    }

    #[test]
    fn source_fetch_constraints_match_supported_templates() {
        assert_eq!(
            SourceFetchConstraints::for_template(SourceTemplate::BinanceUsdm).page_limit,
            BINANCE_USDM_PAGE_LIMIT
        );
        assert_eq!(
            SourceFetchConstraints::for_template(SourceTemplate::HyperliquidSpot)
                .recent_candle_limit,
            Some(HYPERLIQUID_RECENT_CANDLE_LIMIT)
        );
    }

    #[test]
    fn rejects_market_fetch_for_scripts_without_sources() {
        let mut compiled =
            compile("interval 1m\nsource a = binance.spot(\"BTCUSDT\")\nplot(a.close)")
                .expect("compile");
        compiled.program.declared_sources.clear();
        let err = fetch_source_runtime_config(
            &compiled,
            1704067200000,
            1704067260000,
            &ExchangeEndpoints::default(),
        )
        .expect_err("missing sources should fail");
        assert!(matches!(err, ExchangeFetchError::MissingSources));
    }
}
