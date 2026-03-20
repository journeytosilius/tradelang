//! Exchange-backed market data adapters for source-aware PalmScript runs.

pub mod binance;
pub mod bybit;
mod cache;
mod common;
pub mod gate;

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::thread;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::backtest::PerpBacktestContext;
use crate::compiler::CompiledProgram;
use crate::exchange::cache::{
    HistoricalBarCacheKey, HistoricalBarFamily, HistoricalCache, HistoricalRiskAccessMode,
    HistoricalRiskCacheKey,
};
use crate::interval::{
    DeclaredMarketSource, Interval, MarketField, SourceIntervalRef, SourceTemplate,
};
use crate::runtime::{Bar, SourceFeed, SourceRuntimeConfig};

const BINANCE_SPOT_URL: &str = "https://api.binance.com";
const BINANCE_USDM_URL: &str = "https://fapi.binance.com";
const BYBIT_URL: &str = "https://api.bybit.com";
const GATE_URL: &str = "https://api.gateio.ws/api/v4";
const BINANCE_SPOT_WS_URL: &str = "wss://stream.binance.com:9443/ws";
const BINANCE_USDM_WS_URL: &str = "wss://fstream.binance.com/ws";
const BYBIT_SPOT_WS_URL: &str = "wss://stream.bybit.com/v5/public/spot";
const BYBIT_USDM_WS_URL: &str = "wss://stream.bybit.com/v5/public/linear";
const GATE_SPOT_WS_URL: &str = "wss://api.gateio.ws/ws/v4/";
const GATE_USDM_WS_URL: &str = "wss://fx-ws.gateio.ws/v4/ws/usdt";
const HISTORICAL_FETCH_WORKERS_ENV_VAR: &str = "PALMSCRIPT_HISTORICAL_FETCH_WORKERS";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkPriceBasis {
    BinanceMarkPriceKlines,
    BybitMarkPriceKlines,
    GateMarkPriceCandlesticks,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RiskTier {
    pub lower_bound: f64,
    pub upper_bound: Option<f64>,
    pub max_leverage: f64,
    pub maintenance_margin_rate: f64,
    pub maintenance_amount: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "venue_kind", rename_all = "snake_case")]
pub enum VenueRiskSnapshot {
    BinanceUsdm(binance::UsdmRiskSnapshot),
    BybitUsdtPerps(bybit::UsdtPerpsRiskSnapshot),
    GateUsdtPerps(gate::UsdtPerpsRiskSnapshot),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExchangeEndpoints {
    pub binance_spot_base_url: String,
    pub binance_usdm_base_url: String,
    pub bybit_base_url: String,
    pub gate_base_url: String,
    pub binance_spot_ws_url: String,
    pub binance_usdm_ws_url: String,
    pub bybit_spot_ws_url: String,
    pub bybit_usdm_ws_url: String,
    pub gate_spot_ws_url: String,
    pub gate_usdm_ws_url: String,
}

impl Default for ExchangeEndpoints {
    fn default() -> Self {
        Self {
            binance_spot_base_url: BINANCE_SPOT_URL.to_string(),
            binance_usdm_base_url: BINANCE_USDM_URL.to_string(),
            bybit_base_url: BYBIT_URL.to_string(),
            gate_base_url: GATE_URL.to_string(),
            binance_spot_ws_url: BINANCE_SPOT_WS_URL.to_string(),
            binance_usdm_ws_url: BINANCE_USDM_WS_URL.to_string(),
            bybit_spot_ws_url: BYBIT_SPOT_WS_URL.to_string(),
            bybit_usdm_ws_url: BYBIT_USDM_WS_URL.to_string(),
            gate_spot_ws_url: GATE_SPOT_WS_URL.to_string(),
            gate_usdm_ws_url: GATE_USDM_WS_URL.to_string(),
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
            bybit_base_url: env::var("PALMSCRIPT_BYBIT_BASE_URL")
                .unwrap_or_else(|_| BYBIT_URL.to_string()),
            gate_base_url: env::var("PALMSCRIPT_GATE_BASE_URL")
                .unwrap_or_else(|_| GATE_URL.to_string()),
            binance_spot_ws_url: env::var("PALMSCRIPT_BINANCE_SPOT_WS_URL")
                .unwrap_or_else(|_| BINANCE_SPOT_WS_URL.to_string()),
            binance_usdm_ws_url: env::var("PALMSCRIPT_BINANCE_USDM_WS_URL")
                .unwrap_or_else(|_| BINANCE_USDM_WS_URL.to_string()),
            bybit_spot_ws_url: env::var("PALMSCRIPT_BYBIT_SPOT_WS_URL")
                .unwrap_or_else(|_| BYBIT_SPOT_WS_URL.to_string()),
            bybit_usdm_ws_url: env::var("PALMSCRIPT_BYBIT_USDM_WS_URL")
                .unwrap_or_else(|_| BYBIT_USDM_WS_URL.to_string()),
            gate_spot_ws_url: env::var("PALMSCRIPT_GATE_SPOT_WS_URL")
                .unwrap_or_else(|_| GATE_SPOT_WS_URL.to_string()),
            gate_usdm_ws_url: env::var("PALMSCRIPT_GATE_USDM_WS_URL")
                .unwrap_or_else(|_| GATE_USDM_WS_URL.to_string()),
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
    #[error(
        "no data returned for `{alias}` ({template}) `{symbol}` {interval} requested_window=[{from_ms}, {to_ms})"
    )]
    NoData {
        alias: String,
        template: &'static str,
        symbol: String,
        interval: &'static str,
        from_ms: i64,
        to_ms: i64,
    },
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
    #[error("no risk tiers available for `{alias}` ({template}) `{symbol}`")]
    MissingRiskTiers {
        alias: String,
        template: &'static str,
        symbol: String,
    },
}

pub(crate) type SourceFieldRequirements = BTreeMap<SourceIntervalRef, BTreeSet<MarketField>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HistoricalRequestWindow {
    from_ms: i64,
    to_ms: i64,
}

#[derive(Clone, Debug)]
struct HistoricalFeedFetchSpec {
    index: usize,
    source: DeclaredMarketSource,
    interval: Interval,
    fields: BTreeSet<MarketField>,
}

#[derive(Debug)]
struct HistoricalFeedFetchResult {
    index: usize,
    source_id: u16,
    interval: Interval,
    bars: Vec<Bar>,
}

pub fn historical_fetch_workers(task_count: usize) -> usize {
    configured_historical_fetch_workers(
        task_count,
        thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1),
        env::var(HISTORICAL_FETCH_WORKERS_ENV_VAR)
            .ok()
            .and_then(|raw| raw.parse::<usize>().ok())
            .filter(|value| *value > 0),
    )
}

fn configured_historical_fetch_workers(
    task_count: usize,
    available_parallelism: usize,
    configured: Option<usize>,
) -> usize {
    if task_count == 0 {
        return 1;
    }
    let conservative_default = available_parallelism.clamp(1, 4);
    configured
        .unwrap_or(conservative_default)
        .max(1)
        .min(task_count)
}

pub fn fetch_source_runtime_config(
    compiled: &CompiledProgram,
    from_ms: i64,
    to_ms: i64,
    endpoints: &ExchangeEndpoints,
) -> Result<SourceRuntimeConfig, ExchangeFetchError> {
    let cache = HistoricalCache::discover();
    fetch_source_runtime_config_with_cache(compiled, from_ms, to_ms, endpoints, cache.as_ref())
}

fn fetch_source_runtime_config_with_cache(
    compiled: &CompiledProgram,
    from_ms: i64,
    to_ms: i64,
    endpoints: &ExchangeEndpoints,
    cache: Option<&HistoricalCache>,
) -> Result<SourceRuntimeConfig, ExchangeFetchError> {
    let window = HistoricalRequestWindow { from_ms, to_ms };
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

    let required = collect_required_source_fields(compiled, base_interval);
    let mut fetch_specs = Vec::with_capacity(required.len());
    for (index, (requirement, fields)) in required.into_iter().enumerate() {
        let source = compiled
            .program
            .declared_sources
            .iter()
            .chain(compiled.program.declared_executions.iter())
            .find(|source| source.id == requirement.source_id)
            .expect("compiled source interval references should resolve");
        if !source.template.supports_interval(requirement.interval) {
            return Err(ExchangeFetchError::UnsupportedInterval {
                alias: source.alias.clone(),
                template: source.template.as_str(),
                interval: requirement.interval.as_str(),
            });
        }
        fetch_specs.push(HistoricalFeedFetchSpec {
            index,
            source: source.clone(),
            interval: requirement.interval,
            fields,
        });
    }

    let worker_count = historical_fetch_workers(fetch_specs.len());
    let mut feed_results = Vec::with_capacity(fetch_specs.len());
    for chunk in fetch_specs.chunks(worker_count.max(1)) {
        let chunk_results = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for spec in chunk {
                let client = client.clone();
                let endpoints = endpoints.clone();
                let cache = cache.cloned();
                let alias = spec.source.alias.clone();
                let template = spec.source.template.as_str();
                let symbol = spec.source.symbol.clone();
                let interval = spec.interval.as_str();
                let spec = spec.clone();
                handles.push((
                    alias,
                    template,
                    symbol,
                    interval,
                    scope.spawn(move || {
                        fetch_source_feed_with_cache(
                            &client,
                            &spec.source,
                            spec.interval,
                            window,
                            &endpoints,
                            cache.as_ref(),
                            &spec.fields,
                        )
                        .map(|bars| HistoricalFeedFetchResult {
                            index: spec.index,
                            source_id: spec.source.id,
                            interval: spec.interval,
                            bars,
                        })
                    }),
                ));
            }
            let mut results = Vec::with_capacity(handles.len());
            for (alias, template, symbol, interval, handle) in handles {
                let result = handle
                    .join()
                    .map_err(|_| ExchangeFetchError::RequestFailed {
                        alias,
                        template,
                        symbol,
                        interval,
                        message: "historical feed fetch worker panicked".to_string(),
                    })??;
                results.push(result);
            }
            Ok::<_, ExchangeFetchError>(results)
        })?;
        feed_results.extend(chunk_results);
    }
    feed_results.sort_by_key(|result| result.index);

    let feeds = feed_results
        .into_iter()
        .map(|result| SourceFeed {
            source_id: result.source_id,
            interval: result.interval,
            bars: result.bars,
        })
        .collect();

    Ok(SourceRuntimeConfig {
        base_interval,
        feeds,
    })
}

pub(crate) fn collect_required_source_fields(
    compiled: &CompiledProgram,
    base_interval: Interval,
) -> SourceFieldRequirements {
    let mut required = SourceFieldRequirements::new();
    for source in &compiled.program.declared_sources {
        insert_ohlcv_requirement(&mut required, source.id, base_interval);
    }
    for execution in &compiled.program.declared_executions {
        insert_ohlcv_requirement(&mut required, execution.id, base_interval);
    }
    for local in &compiled.program.locals {
        let Some(binding) = local.market_binding else {
            continue;
        };
        let crate::interval::MarketSource::Named {
            source_id,
            interval,
        } = binding.source;
        required
            .entry(SourceIntervalRef {
                source_id,
                interval: interval.unwrap_or(base_interval),
            })
            .or_default()
            .insert(binding.field);
    }
    required
}

fn insert_ohlcv_requirement(
    required: &mut SourceFieldRequirements,
    source_id: u16,
    interval: Interval,
) {
    let fields = required
        .entry(SourceIntervalRef {
            source_id,
            interval,
        })
        .or_default();
    for field in MarketField::ALL {
        if field.is_ohlcv() {
            fields.insert(field);
        }
    }
}

pub fn fetch_perp_backtest_context(
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    endpoints: &ExchangeEndpoints,
) -> Result<Option<PerpBacktestContext>, ExchangeFetchError> {
    let cache = HistoricalCache::discover();
    fetch_perp_backtest_context_with_cache(
        source,
        interval,
        from_ms,
        to_ms,
        endpoints,
        cache.as_ref(),
    )
}

fn fetch_perp_backtest_context_with_cache(
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    endpoints: &ExchangeEndpoints,
    cache: Option<&HistoricalCache>,
) -> Result<Option<PerpBacktestContext>, ExchangeFetchError> {
    let window = HistoricalRequestWindow { from_ms, to_ms };
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
            let mark_bars = cached_perp_mark_bars(
                cache,
                source,
                interval,
                window,
                HistoricalBarFamily::PerpMarkPrice,
                &endpoints.binance_usdm_base_url,
                |gap_from_ms, gap_to_ms| {
                    binance::usdm::fetch_mark_price_bars(
                        &client,
                        source,
                        interval,
                        gap_from_ms,
                        gap_to_ms,
                        &endpoints.binance_usdm_base_url,
                    )
                },
            )?;
            let risk_snapshot = cached_risk_snapshot(
                cache,
                source,
                HistoricalRiskAccessMode::binance_usdm(),
                &endpoints.binance_usdm_base_url,
                || {
                    binance::usdm::fetch_risk_snapshot(&client, source, endpoints)
                        .map(VenueRiskSnapshot::BinanceUsdm)
                },
            )?;
            Ok(Some(PerpBacktestContext {
                mark_price_basis: MarkPriceBasis::BinanceMarkPriceKlines,
                mark_bars,
                risk_snapshot,
            }))
        }
        SourceTemplate::BybitUsdtPerps => {
            let mark_bars = cached_perp_mark_bars(
                cache,
                source,
                interval,
                window,
                HistoricalBarFamily::PerpMarkPrice,
                &endpoints.bybit_base_url,
                |gap_from_ms, gap_to_ms| {
                    bybit::usdt_perps::fetch_mark_price_bars(
                        &client,
                        source,
                        interval,
                        gap_from_ms,
                        gap_to_ms,
                        &endpoints.bybit_base_url,
                    )
                },
            )?;
            let risk_snapshot = cached_risk_snapshot(
                cache,
                source,
                HistoricalRiskAccessMode::PublicOnly,
                &endpoints.bybit_base_url,
                || {
                    bybit::usdt_perps::fetch_risk_snapshot(&client, source, endpoints)
                        .map(VenueRiskSnapshot::BybitUsdtPerps)
                },
            )?;
            Ok(Some(PerpBacktestContext {
                mark_price_basis: MarkPriceBasis::BybitMarkPriceKlines,
                mark_bars,
                risk_snapshot,
            }))
        }
        SourceTemplate::GateUsdtPerps => {
            let mark_bars = cached_perp_mark_bars(
                cache,
                source,
                interval,
                window,
                HistoricalBarFamily::PerpMarkPrice,
                &endpoints.gate_base_url,
                |gap_from_ms, gap_to_ms| {
                    gate::usdt_perps::fetch_mark_price_bars(
                        &client,
                        source,
                        interval,
                        gap_from_ms,
                        gap_to_ms,
                        &endpoints.gate_base_url,
                    )
                },
            )?;
            let risk_snapshot = cached_risk_snapshot(
                cache,
                source,
                HistoricalRiskAccessMode::PublicOnly,
                &endpoints.gate_base_url,
                || {
                    gate::usdt_perps::fetch_risk_snapshot(&client, source, endpoints)
                        .map(VenueRiskSnapshot::GateUsdtPerps)
                },
            )?;
            Ok(Some(PerpBacktestContext {
                mark_price_basis: MarkPriceBasis::GateMarkPriceCandlesticks,
                mark_bars,
                risk_snapshot,
            }))
        }
        SourceTemplate::BinanceSpot | SourceTemplate::BybitSpot | SourceTemplate::GateSpot => {
            Ok(None)
        }
    }
}

fn fetch_source_feed_with_cache(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    window: HistoricalRequestWindow,
    endpoints: &ExchangeEndpoints,
    cache: Option<&HistoricalCache>,
    fields: &BTreeSet<MarketField>,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let mut merged = BTreeMap::<i64, Bar>::new();

    if fields.iter().any(|field| field.is_ohlcv()) {
        let bars = cached_source_bars(
            cache,
            source,
            interval,
            window,
            HistoricalBarFamily::Ohlcv,
            endpoint_base_for_template(source.template, endpoints),
            |gap_from_ms, gap_to_ms| {
                fetch_source_bars(client, source, interval, gap_from_ms, gap_to_ms, endpoints)
            },
        )?;
        merge_bars(&mut merged, bars);
    }

    if matches!(source.template, SourceTemplate::BinanceUsdm) {
        for field in fields {
            let bars = match field {
                MarketField::FundingRate => optional_source_auxiliary_bars(cached_source_bars(
                    cache,
                    source,
                    interval,
                    window,
                    HistoricalBarFamily::FundingRate,
                    endpoints.binance_usdm_base_url.clone(),
                    |gap_from_ms, gap_to_ms| {
                        binance::usdm::fetch_funding_rate_bars(
                            client,
                            source,
                            interval,
                            gap_from_ms,
                            gap_to_ms,
                            &endpoints.binance_usdm_base_url,
                        )
                    },
                ))?,
                MarketField::MarkPrice => optional_source_auxiliary_bars(cached_source_bars(
                    cache,
                    source,
                    interval,
                    window,
                    HistoricalBarFamily::SourceMarkPrice,
                    endpoints.binance_usdm_base_url.clone(),
                    |gap_from_ms, gap_to_ms| {
                        binance::usdm::fetch_mark_price_bars(
                            client,
                            source,
                            interval,
                            gap_from_ms,
                            gap_to_ms,
                            &endpoints.binance_usdm_base_url,
                        )
                        .map(|bars| map_scalar_close_field(bars, MarketField::MarkPrice))
                    },
                ))?,
                MarketField::IndexPrice => optional_source_auxiliary_bars(cached_source_bars(
                    cache,
                    source,
                    interval,
                    window,
                    HistoricalBarFamily::IndexPrice,
                    endpoints.binance_usdm_base_url.clone(),
                    |gap_from_ms, gap_to_ms| {
                        binance::usdm::fetch_index_price_bars(
                            client,
                            source,
                            interval,
                            gap_from_ms,
                            gap_to_ms,
                            &endpoints.binance_usdm_base_url,
                        )
                    },
                ))?,
                MarketField::PremiumIndex => optional_source_auxiliary_bars(cached_source_bars(
                    cache,
                    source,
                    interval,
                    window,
                    HistoricalBarFamily::PremiumIndex,
                    endpoints.binance_usdm_base_url.clone(),
                    |gap_from_ms, gap_to_ms| {
                        binance::usdm::fetch_premium_index_bars(
                            client,
                            source,
                            interval,
                            gap_from_ms,
                            gap_to_ms,
                            &endpoints.binance_usdm_base_url,
                        )
                    },
                ))?,
                MarketField::Basis => optional_source_auxiliary_bars(cached_source_bars(
                    cache,
                    source,
                    interval,
                    window,
                    HistoricalBarFamily::Basis,
                    endpoints.binance_usdm_base_url.clone(),
                    |gap_from_ms, gap_to_ms| {
                        binance::usdm::fetch_basis_bars(
                            client,
                            source,
                            interval,
                            gap_from_ms,
                            gap_to_ms,
                            &endpoints.binance_usdm_base_url,
                        )
                    },
                ))?,
                MarketField::Open
                | MarketField::High
                | MarketField::Low
                | MarketField::Close
                | MarketField::Volume
                | MarketField::Time => None,
            };
            if let Some(bars) = bars {
                merge_bars(&mut merged, bars);
            }
        }
    }

    Ok(merged.into_values().collect())
}

pub(crate) fn fetch_source_feed(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    endpoints: &ExchangeEndpoints,
    fields: &BTreeSet<MarketField>,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let window = HistoricalRequestWindow { from_ms, to_ms };
    fetch_source_feed_with_cache(client, source, interval, window, endpoints, None, fields)
}

fn cached_source_bars<F>(
    cache: Option<&HistoricalCache>,
    source: &DeclaredMarketSource,
    interval: Interval,
    window: HistoricalRequestWindow,
    family: HistoricalBarFamily,
    base_url: String,
    fetch: F,
) -> Result<Vec<Bar>, ExchangeFetchError>
where
    F: FnMut(i64, i64) -> Result<Vec<Bar>, ExchangeFetchError>,
{
    let Some(cache) = cache else {
        let mut fetch = fetch;
        return fetch(window.from_ms, window.to_ms);
    };
    cache.bars(
        HistoricalBarCacheKey {
            template: source.template,
            symbol: source.symbol.clone(),
            interval,
            family,
            base_url: normalize_cache_base_url(base_url),
        },
        window.from_ms,
        window.to_ms,
        fetch,
        |window_from_ms, window_to_ms| {
            common::no_data(source, interval, window_from_ms, window_to_ms)
        },
    )
}

fn cached_perp_mark_bars<F>(
    cache: Option<&HistoricalCache>,
    source: &DeclaredMarketSource,
    interval: Interval,
    window: HistoricalRequestWindow,
    family: HistoricalBarFamily,
    base_url: &str,
    fetch: F,
) -> Result<Vec<Bar>, ExchangeFetchError>
where
    F: FnMut(i64, i64) -> Result<Vec<Bar>, ExchangeFetchError>,
{
    cached_source_bars(
        cache,
        source,
        interval,
        window,
        family,
        base_url.to_string(),
        fetch,
    )
}

fn cached_risk_snapshot<F>(
    cache: Option<&HistoricalCache>,
    source: &DeclaredMarketSource,
    access_mode: HistoricalRiskAccessMode,
    base_url: &str,
    fetch: F,
) -> Result<VenueRiskSnapshot, ExchangeFetchError>
where
    F: FnOnce() -> Result<VenueRiskSnapshot, ExchangeFetchError>,
{
    let key = HistoricalRiskCacheKey {
        template: source.template,
        symbol: source.symbol.clone(),
        access_mode,
        base_url: normalize_cache_base_url(base_url.to_string()),
    };
    if let Some(cache) = cache {
        if let Some(snapshot) = cache.load_risk_snapshot(&key) {
            return Ok(snapshot);
        }
        let snapshot = fetch()?;
        cache.store_risk_snapshot(key, &snapshot);
        return Ok(snapshot);
    }
    fetch()
}

fn endpoint_base_for_template(template: SourceTemplate, endpoints: &ExchangeEndpoints) -> String {
    match template {
        SourceTemplate::BinanceSpot => endpoints.binance_spot_base_url.clone(),
        SourceTemplate::BinanceUsdm => endpoints.binance_usdm_base_url.clone(),
        SourceTemplate::BybitSpot | SourceTemplate::BybitUsdtPerps => {
            endpoints.bybit_base_url.clone()
        }
        SourceTemplate::GateSpot | SourceTemplate::GateUsdtPerps => endpoints.gate_base_url.clone(),
    }
}

fn normalize_cache_base_url(base_url: String) -> String {
    base_url.trim_end_matches('/').to_string()
}

fn optional_source_auxiliary_bars(
    result: Result<Vec<Bar>, ExchangeFetchError>,
) -> Result<Option<Vec<Bar>>, ExchangeFetchError> {
    match result {
        Ok(bars) => Ok(Some(bars)),
        Err(ExchangeFetchError::NoData { .. }) => Ok(None),
        Err(err) => Err(err),
    }
}

pub(crate) fn fetch_source_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    endpoints: &ExchangeEndpoints,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    match source.template {
        SourceTemplate::BinanceSpot => binance::spot::fetch_bars(
            client,
            source,
            interval,
            from_ms,
            to_ms,
            &endpoints.binance_spot_base_url,
        ),
        SourceTemplate::BinanceUsdm => binance::usdm::fetch_bars(
            client,
            source,
            interval,
            from_ms,
            to_ms,
            &endpoints.binance_usdm_base_url,
        ),
        SourceTemplate::BybitSpot => bybit::spot::fetch_bars(
            client,
            source,
            interval,
            from_ms,
            to_ms,
            &endpoints.bybit_base_url,
        ),
        SourceTemplate::BybitUsdtPerps => bybit::usdt_perps::fetch_bars(
            client,
            source,
            interval,
            from_ms,
            to_ms,
            &endpoints.bybit_base_url,
        ),
        SourceTemplate::GateSpot => gate::spot::fetch_bars(
            client,
            source,
            interval,
            from_ms,
            to_ms,
            &endpoints.gate_base_url,
        ),
        SourceTemplate::GateUsdtPerps => gate::usdt_perps::fetch_bars(
            client,
            source,
            interval,
            from_ms,
            to_ms,
            &endpoints.gate_base_url,
        ),
    }
}

pub(super) fn merge_bars(merged: &mut BTreeMap<i64, Bar>, bars: Vec<Bar>) {
    for bar in bars {
        let open_time = bar.time as i64;
        let entry = merged
            .entry(open_time)
            .or_insert_with(|| empty_bar(open_time));
        merge_bar(entry, bar);
    }
}

fn map_scalar_close_field(bars: Vec<Bar>, field: MarketField) -> Vec<Bar> {
    bars.into_iter()
        .map(|bar| {
            let mut mapped = empty_bar(bar.time as i64);
            match field {
                MarketField::MarkPrice => mapped.mark_price = Some(bar.close),
                MarketField::IndexPrice => mapped.index_price = Some(bar.close),
                MarketField::PremiumIndex => mapped.premium_index = Some(bar.close),
                MarketField::Basis => mapped.basis = Some(bar.close),
                MarketField::FundingRate => mapped.funding_rate = Some(bar.close),
                MarketField::Open
                | MarketField::High
                | MarketField::Low
                | MarketField::Close
                | MarketField::Volume
                | MarketField::Time => {}
            }
            mapped
        })
        .collect()
}

fn merge_bar(target: &mut Bar, overlay: Bar) {
    if overlay.open.is_finite() {
        target.open = overlay.open;
    }
    if overlay.high.is_finite() {
        target.high = overlay.high;
    }
    if overlay.low.is_finite() {
        target.low = overlay.low;
    }
    if overlay.close.is_finite() {
        target.close = overlay.close;
    }
    if overlay.volume.is_finite() {
        target.volume = overlay.volume;
    }
    target.time = overlay.time;
    if overlay.funding_rate.is_some() {
        target.funding_rate = overlay.funding_rate;
    }
    if overlay.mark_price.is_some() {
        target.mark_price = overlay.mark_price;
    }
    if overlay.index_price.is_some() {
        target.index_price = overlay.index_price;
    }
    if overlay.premium_index.is_some() {
        target.premium_index = overlay.premium_index;
    }
    if overlay.basis.is_some() {
        target.basis = overlay.basis;
    }
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

#[cfg(test)]
mod tests {
    use super::{
        cache::HistoricalCache, fetch_perp_backtest_context,
        fetch_perp_backtest_context_with_cache, fetch_source_runtime_config,
        fetch_source_runtime_config_with_cache, ExchangeEndpoints, ExchangeFetchError,
        MarkPriceBasis, VenueRiskSnapshot,
    };
    use crate::compile;
    use crate::exchange::binance::UsdmRiskSource;
    use crate::exchange::bybit::UsdtPerpsRiskSource;
    use crate::exchange::gate::UsdtPerpsRiskSource as GateUsdtPerpsRiskSource;
    use crate::interval::{DeclaredMarketSource, Interval, SourceTemplate};
    use mockito::{Matcher, Server};
    use serde_json::json;
    use std::env;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn sample_source(template: SourceTemplate, symbol: &str) -> DeclaredMarketSource {
        DeclaredMarketSource {
            id: 0,
            alias: "src".to_string(),
            template,
            symbol: symbol.to_string(),
        }
    }

    fn bybit_envelope(rows: &[serde_json::Value]) -> String {
        json!({
            "retCode": 0,
            "retMsg": "OK",
            "result": { "list": rows },
            "time": 1704067200000_i64
        })
        .to_string()
    }

    fn binance_usdm_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn configured_historical_fetch_workers_defaults_to_bounded_parallelism() {
        assert_eq!(super::configured_historical_fetch_workers(8, 47, None), 4);
    }

    #[test]
    fn configured_historical_fetch_workers_honors_configured_override() {
        assert_eq!(
            super::configured_historical_fetch_workers(8, 47, Some(12)),
            8
        );
        assert_eq!(
            super::configured_historical_fetch_workers(8, 47, Some(3)),
            3
        );
    }

    #[test]
    fn fetch_source_runtime_config_builds_required_feeds_for_supported_venues() {
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
        let _gate = server
            .mock("GET", "/spot/candlesticks")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("currency_pair".into(), "BTC_USDT".into()),
                Matcher::UrlEncoded("interval".into(), "1m".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [
                        1704067200_i64,
                        "15.0",
                        "1.5",
                        "2.0",
                        "0.5",
                        "1.0",
                        "10.0",
                        true
                    ],
                    [
                        1704067260_i64,
                        "16.0",
                        "2.5",
                        "3.0",
                        "1.5",
                        "2.0",
                        "11.0",
                        true
                    ]
                ])
                .to_string(),
            )
            .create();
        let _gate_hour = server
            .mock("GET", "/spot/candlesticks")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("currency_pair".into(), "BTC_USDT".into()),
                Matcher::UrlEncoded("interval".into(), "1h".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([[
                    1704067200_i64,
                    "30.0",
                    "2.0",
                    "3.0",
                    "1.0",
                    "1.5",
                    "21.0",
                    true
                ]])
                .to_string(),
            )
            .create();

        let compiled = compile(
            "interval 1m\nsource bn = binance.spot(\"BTCUSDT\")\nsource gt = gate.spot(\"BTC_USDT\")\nuse gt 1h\nplot(bn.close - gt.1h.close)",
        )
        .expect("compile");
        let endpoints = ExchangeEndpoints {
            binance_spot_base_url: server.url(),
            binance_usdm_base_url: server.url(),
            bybit_base_url: server.url(),
            gate_base_url: server.url(),
            ..ExchangeEndpoints::default()
        };

        let config =
            fetch_source_runtime_config(&compiled, 1704067200000, 1704067320000, &endpoints)
                .expect("config");
        assert_eq!(config.base_interval, Interval::Min1);
        assert_eq!(config.feeds.len(), 3);
    }

    #[test]
    fn fetch_source_runtime_config_merges_binance_usdm_auxiliary_fields() {
        let mut server = Server::new();
        let _klines = server
            .mock("GET", "/fapi/v1/klines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1h".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [1704067200000_i64, "100.0", "101.0", "99.0", "100.5", "10.0"],
                    [
                        1704070800000_i64,
                        "101.0",
                        "102.0",
                        "100.0",
                        "101.5",
                        "11.0"
                    ]
                ])
                .to_string(),
            )
            .create();
        let _mark = server
            .mock("GET", "/fapi/v1/markPriceKlines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1h".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [1704067200000_i64, "100.0", "100.6", "99.9", "100.25", "0"],
                    [1704070800000_i64, "101.0", "101.6", "100.9", "101.25", "0"]
                ])
                .to_string(),
            )
            .create();
        let _index = server
            .mock("GET", "/fapi/v1/indexPriceKlines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1h".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [1704067200000_i64, "99.8", "100.3", "99.7", "100.0", "0"],
                    [1704070800000_i64, "100.8", "101.3", "100.7", "101.0", "0"]
                ])
                .to_string(),
            )
            .create();
        let _premium = server
            .mock("GET", "/fapi/v1/premiumIndexKlines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1h".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [1704067200000_i64, "0.0", "0.0", "0.0", "0.0010", "0"],
                    [1704070800000_i64, "0.0", "0.0", "0.0", "0.0015", "0"]
                ])
                .to_string(),
            )
            .create();
        let _funding = server
            .mock("GET", "/fapi/v1/fundingRate")
            .match_query(Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()))
            .with_status(200)
            .with_body(
                json!([
                    { "fundingRate": "0.0008", "fundingTime": 1704067200000_i64 },
                    { "fundingRate": "0.0009", "fundingTime": 1704070800000_i64 }
                ])
                .to_string(),
            )
            .create();
        let _basis = server
            .mock("GET", "/futures/data/basis")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("pair".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("contractType".into(), "PERPETUAL".into()),
                Matcher::UrlEncoded("period".into(), "1h".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    { "basis": "0.25", "timestamp": 1704067200000_i64 },
                    { "basis": "0.30", "timestamp": 1704070800000_i64 }
                ])
                .to_string(),
            )
            .create();

        let compiled = compile(
            "interval 1h\nsource perp = binance.usdm(\"BTCUSDT\")\nplot(perp.close + perp.mark_price + perp.index_price + perp.premium_index + perp.basis + perp.funding_rate)",
        )
        .expect("compile");
        let endpoints = ExchangeEndpoints {
            binance_spot_base_url: server.url(),
            binance_usdm_base_url: server.url(),
            bybit_base_url: server.url(),
            gate_base_url: server.url(),
            ..ExchangeEndpoints::default()
        };

        let config =
            fetch_source_runtime_config(&compiled, 1704067200000, 1704074400000, &endpoints)
                .expect("config");
        assert_eq!(config.feeds.len(), 1);
        let first = &config.feeds[0].bars[0];
        assert_eq!(first.close, 100.5);
        assert_eq!(first.mark_price, Some(100.25));
        assert_eq!(first.index_price, Some(100.0));
        assert_eq!(first.premium_index, Some(0.001));
        assert_eq!(first.basis, Some(0.25));
        assert_eq!(first.funding_rate, Some(0.0008));
    }

    #[test]
    fn fetch_source_runtime_config_tolerates_missing_binance_usdm_funding_rows() {
        let mut server = Server::new();
        let _klines = server
            .mock("GET", "/fapi/v1/klines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1h".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [1704067200000_i64, "100.0", "101.0", "99.0", "100.5", "10.0"],
                    [
                        1704070800000_i64,
                        "101.0",
                        "102.0",
                        "100.0",
                        "101.5",
                        "11.0"
                    ]
                ])
                .to_string(),
            )
            .create();
        let _funding = server
            .mock("GET", "/fapi/v1/fundingRate")
            .match_query(Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()))
            .with_status(200)
            .with_body("[]")
            .create();

        let compiled = compile(
            "interval 1h\nsource perp = binance.usdm(\"BTCUSDT\")\nplot(nz(perp.funding_rate, 0))",
        )
        .expect("compile");
        let endpoints = ExchangeEndpoints {
            binance_usdm_base_url: server.url(),
            ..ExchangeEndpoints::default()
        };

        let config =
            fetch_source_runtime_config(&compiled, 1704067200000, 1704074400000, &endpoints)
                .expect("config should keep OHLCV bars when funding rows are absent");
        assert_eq!(config.feeds.len(), 1);
        assert_eq!(config.feeds[0].bars.len(), 2);
        assert_eq!(config.feeds[0].bars[0].close, 100.5);
        assert_eq!(config.feeds[0].bars[0].funding_rate, None);
        assert_eq!(config.feeds[0].bars[1].funding_rate, None);
    }

    #[test]
    fn fetch_source_runtime_config_normalizes_reverse_sorted_bybit_rows() {
        let mut server = Server::new();
        let _bybit = server
            .mock("GET", "/v5/market/kline")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("category".into(), "spot".into()),
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(bybit_envelope(&[
                json!([1704067260000_i64, "2.0", "3.0", "1.5", "2.5", "11.0", "0"]),
                json!([1704067200000_i64, "1.0", "2.0", "0.5", "1.5", "10.0", "0"]),
            ]))
            .create();

        let compiled = compile("interval 1m\nsource bb = bybit.spot(\"BTCUSDT\")\nplot(bb.close)")
            .expect("compile");
        let config = fetch_source_runtime_config(
            &compiled,
            1704067200000,
            1704067320000,
            &ExchangeEndpoints {
                binance_spot_base_url: String::new(),
                binance_usdm_base_url: String::new(),
                bybit_base_url: server.url(),
                gate_base_url: String::new(),
                ..ExchangeEndpoints::default()
            },
        )
        .expect("config");

        assert_eq!(config.feeds[0].bars[0].time, 1704067200000.0);
        assert_eq!(config.feeds[0].bars[1].time, 1704067260000.0);
    }

    #[test]
    fn fetch_source_runtime_config_accepts_gate_host_root_base_url() {
        let mut server = Server::new();
        let _gate = server
            .mock("GET", "/api/v4/spot/candlesticks")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("currency_pair".into(), "BTC_USDT".into()),
                Matcher::UrlEncoded("interval".into(), "1m".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [
                        1704067200_i64,
                        "1000.0",
                        "100.5",
                        "101.0",
                        "99.0",
                        "100.0",
                        "10.0"
                    ],
                    [
                        1704067260_i64,
                        "1100.0",
                        "101.5",
                        "102.0",
                        "100.0",
                        "101.0",
                        "11.0"
                    ]
                ])
                .to_string(),
            )
            .create();

        let compiled = compile("interval 1m\nsource gt = gate.spot(\"BTC_USDT\")\nplot(gt.close)")
            .expect("compile");
        let config = fetch_source_runtime_config(
            &compiled,
            1704067200000,
            1704067320000,
            &ExchangeEndpoints {
                binance_spot_base_url: String::new(),
                binance_usdm_base_url: String::new(),
                bybit_base_url: String::new(),
                gate_base_url: server.url(),
                ..ExchangeEndpoints::default()
            },
        )
        .expect("config");

        assert_eq!(config.feeds.len(), 1);
        assert_eq!(config.feeds[0].bars.len(), 2);
    }

    #[test]
    fn fetch_source_runtime_config_reuses_cached_overlapping_segments() {
        let mut server = Server::new();
        let _initial = server
            .mock("GET", "/api/v3/klines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1m".into()),
                Matcher::UrlEncoded("startTime".into(), "1704067200000".into()),
                Matcher::UrlEncoded("endTime".into(), "1704067319999".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [1704067200000_i64, "1.0", "2.0", "0.5", "1.5", "10.0"],
                    [1704067260000_i64, "2.0", "3.0", "1.5", "2.5", "11.0"]
                ])
                .to_string(),
            )
            .expect(1)
            .create();
        let _gap_fill = server
            .mock("GET", "/api/v3/klines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1m".into()),
                Matcher::UrlEncoded("startTime".into(), "1704067320000".into()),
                Matcher::UrlEncoded("endTime".into(), "1704067379999".into()),
            ]))
            .with_status(200)
            .with_body(json!([[1704067320000_i64, "3.0", "4.0", "2.5", "3.5", "12.0"]]).to_string())
            .expect(1)
            .create();

        let compiled =
            compile("interval 1m\nsource bn = binance.spot(\"BTCUSDT\")\nplot(bn.close)")
                .expect("compile");
        let endpoints = ExchangeEndpoints {
            binance_spot_base_url: server.url(),
            ..ExchangeEndpoints::default()
        };
        let cache_dir = tempdir().expect("tempdir");
        let cache = HistoricalCache::new(cache_dir.path().join("historical"));

        let initial = fetch_source_runtime_config_with_cache(
            &compiled,
            1704067200000,
            1704067320000,
            &endpoints,
            Some(&cache),
        )
        .expect("initial config");
        assert_eq!(initial.feeds[0].bars.len(), 2);

        let expanded = fetch_source_runtime_config_with_cache(
            &compiled,
            1704067200000,
            1704067380000,
            &endpoints,
            Some(&cache),
        )
        .expect("expanded config");
        assert_eq!(expanded.feeds[0].bars.len(), 3);

        let cached = fetch_source_runtime_config_with_cache(
            &compiled,
            1704067200000,
            1704067380000,
            &endpoints,
            Some(&cache),
        )
        .expect("cached config");
        assert_eq!(cached.feeds[0].bars.len(), 3);
        assert_eq!(cached.feeds[0].bars[2].time, 1704067320000.0);
    }

    #[test]
    fn gate_http_errors_include_request_url_and_body() {
        let mut server = Server::new();
        let _gate = server
            .mock("GET", "/api/v4/spot/candlesticks")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("currency_pair".into(), "BTC_USDT".into()),
                Matcher::UrlEncoded("interval".into(), "4h".into()),
            ]))
            .with_status(400)
            .with_body(
                json!({
                    "label": "INVALID_PARAM_VALUE",
                    "message": "Candlestick range too broad. Maximum 1000 data points are allowed per request"
                })
                .to_string(),
            )
            .create();

        let compiled = compile("interval 4h\nsource gt = gate.spot(\"BTC_USDT\")\nplot(gt.close)")
            .expect("compile");
        let err = fetch_source_runtime_config(
            &compiled,
            1_640_995_200_000,
            1_655_395_200_000,
            &ExchangeEndpoints {
                binance_spot_base_url: String::new(),
                binance_usdm_base_url: String::new(),
                bybit_base_url: String::new(),
                gate_base_url: format!("{}/api/v4", server.url()),
                ..ExchangeEndpoints::default()
            },
        )
        .expect_err("gate 400 should surface");

        let message = err.to_string();
        assert!(message.contains("/spot/candlesticks"));
        assert!(message.contains("currency_pair=BTC_USDT"));
        assert!(message.contains("Candlestick range too broad"));
    }

    #[test]
    fn gate_malformed_json_errors_include_request_url_and_body_snippet() {
        let mut server = Server::new();
        let _gate = server
            .mock("GET", "/api/v4/spot/candlesticks")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("currency_pair".into(), "BTC_USDT".into()),
                Matcher::UrlEncoded("interval".into(), "4h".into()),
            ]))
            .with_status(200)
            .with_body("[[\"oops\"]]")
            .create();

        let compiled = compile("interval 4h\nsource gt = gate.spot(\"BTC_USDT\")\nplot(gt.close)")
            .expect("compile");
        let err = fetch_source_runtime_config(
            &compiled,
            1_640_995_200_000,
            1_655_395_200_000,
            &ExchangeEndpoints {
                binance_spot_base_url: String::new(),
                binance_usdm_base_url: String::new(),
                bybit_base_url: String::new(),
                gate_base_url: format!("{}/api/v4", server.url()),
                ..ExchangeEndpoints::default()
            },
        )
        .expect_err("gate malformed body should surface");

        let message = err.to_string();
        assert!(message.contains("error decoding response body from"));
        assert!(message.contains("/spot/candlesticks"));
        assert!(message.contains("[[\"oops\"]]"));
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
            .mock("GET", "/fapi/v1/markPriceKlines")
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
                        "brackets": [{
                            "initialLeverage": 20,
                            "notionalFloor": "0",
                            "notionalCap": "100000",
                            "maintMarginRatio": "0.01",
                            "cum": "0"
                        }]
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
                bybit_base_url: String::new(),
                gate_base_url: String::new(),
                ..ExchangeEndpoints::default()
            },
        )
        .expect("context")
        .expect("perp context");
        env::remove_var("PALMSCRIPT_BINANCE_USDM_API_KEY");
        env::remove_var("PALMSCRIPT_BINANCE_USDM_API_SECRET");

        assert_eq!(
            context.mark_price_basis,
            MarkPriceBasis::BinanceMarkPriceKlines
        );
        match context.risk_snapshot {
            VenueRiskSnapshot::BinanceUsdm(snapshot) => {
                assert_eq!(snapshot.source, UsdmRiskSource::SignedLeverageBrackets);
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
            .mock("GET", "/fapi/v1/markPriceKlines")
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
                    "symbols": [{
                        "symbol": "BTCUSDT",
                        "maintMarginPercent": "2.5",
                        "requiredMarginPercent": "5.0"
                    }]
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
                bybit_base_url: String::new(),
                gate_base_url: String::new(),
                ..ExchangeEndpoints::default()
            },
        )
        .expect("context")
        .expect("perp context");

        match context.risk_snapshot {
            VenueRiskSnapshot::BinanceUsdm(snapshot) => {
                assert_eq!(
                    snapshot.source,
                    UsdmRiskSource::PublicExchangeInfoApproximation
                );
                assert_eq!(snapshot.brackets[0].max_leverage, 20.0);
            }
            other => panic!("unexpected snapshot: {other:?}"),
        }
    }

    #[test]
    fn fetch_perp_backtest_context_reads_bybit_mark_bars_and_risk_tiers() {
        let mut server = Server::new();
        let _marks = server
            .mock("GET", "/v5/market/mark-price-kline")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("category".into(), "linear".into()),
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(bybit_envelope(&[
                json!([1704067260000_i64, "100.5", "102.0", "100.0", "101.5"]),
                json!([1704067200000_i64, "100.0", "101.0", "99.0", "100.5"]),
            ]))
            .create();
        let _risk = server
            .mock("GET", "/v5/market/risk-limit")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("category".into(), "linear".into()),
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            ]))
            .with_status(200)
            .with_body(
                json!({
                    "retCode": 0,
                    "retMsg": "OK",
                    "result": {
                        "list": [{
                            "symbol": "BTCUSDT",
                            "riskLimitValue": "100000",
                            "maintenanceMargin": "0.5",
                            "initialMargin": "1.0",
                            "maxLeverage": "100",
                            "mmDeduction": "0"
                        }],
                        "nextPageCursor": ""
                    },
                    "time": 1704067200123_i64
                })
                .to_string(),
            )
            .create();

        let source = sample_source(SourceTemplate::BybitUsdtPerps, "BTCUSDT");
        let context = fetch_perp_backtest_context(
            &source,
            Interval::Min1,
            1704067200000,
            1704067320000,
            &ExchangeEndpoints {
                binance_spot_base_url: String::new(),
                binance_usdm_base_url: String::new(),
                bybit_base_url: server.url(),
                gate_base_url: String::new(),
                ..ExchangeEndpoints::default()
            },
        )
        .expect("context")
        .expect("perp context");

        assert_eq!(
            context.mark_price_basis,
            MarkPriceBasis::BybitMarkPriceKlines
        );
        match context.risk_snapshot {
            VenueRiskSnapshot::BybitUsdtPerps(snapshot) => {
                assert_eq!(snapshot.source, UsdtPerpsRiskSource::PublicRiskLimit);
                assert_eq!(snapshot.tiers.len(), 1);
            }
            other => panic!("unexpected snapshot: {other:?}"),
        }
    }

    #[test]
    fn fetch_perp_backtest_context_reads_gate_mark_bars_and_risk_tiers() {
        let mut server = Server::new();
        let _marks = server
            .mock("GET", "/futures/usdt/candlesticks")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("contract".into(), "mark_BTC_USDT".into()),
                Matcher::UrlEncoded("interval".into(), "1m".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    {"t": 1704067200_i64, "o": "100.0", "h": "101.0", "l": "99.0", "c": "100.5"},
                    {"t": 1704067260_i64, "o": "100.5", "h": "102.0", "l": "100.0", "c": "101.5"}
                ])
                .to_string(),
            )
            .create();
        let _risk = server
            .mock("GET", "/futures/usdt/risk_limit_tiers")
            .match_query(Matcher::UrlEncoded("contract".into(), "BTC_USDT".into()))
            .with_status(200)
            .with_body(
                json!([{
                    "contract": "BTC_USDT",
                    "risk_limit": "100000",
                    "initial_rate": "0.01",
                    "maintenance_rate": "0.005",
                    "leverage_max": "100",
                    "deduction": "0"
                }])
                .to_string(),
            )
            .create();

        let source = sample_source(SourceTemplate::GateUsdtPerps, "BTC_USDT");
        let context = fetch_perp_backtest_context(
            &source,
            Interval::Min1,
            1704067200000,
            1704067320000,
            &ExchangeEndpoints {
                binance_spot_base_url: String::new(),
                binance_usdm_base_url: String::new(),
                bybit_base_url: String::new(),
                gate_base_url: server.url(),
                ..ExchangeEndpoints::default()
            },
        )
        .expect("context")
        .expect("perp context");

        assert_eq!(
            context.mark_price_basis,
            MarkPriceBasis::GateMarkPriceCandlesticks
        );
        match context.risk_snapshot {
            VenueRiskSnapshot::GateUsdtPerps(snapshot) => {
                assert_eq!(
                    snapshot.source,
                    GateUsdtPerpsRiskSource::PublicRiskLimitTiers
                );
                assert_eq!(snapshot.tiers.len(), 1);
            }
            other => panic!("unexpected snapshot: {other:?}"),
        }
    }

    #[test]
    fn fetch_perp_backtest_context_reuses_cached_mark_bars_and_risk_snapshot() {
        let mut server = Server::new();
        let _initial_marks = server
            .mock("GET", "/v5/market/mark-price-kline")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("category".into(), "linear".into()),
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1".into()),
                Matcher::UrlEncoded("start".into(), "1704067200000".into()),
                Matcher::UrlEncoded("end".into(), "1704067319999".into()),
            ]))
            .with_status(200)
            .with_body(bybit_envelope(&[
                json!([1704067260000_i64, "100.5", "102.0", "100.0", "101.5"]),
                json!([1704067200000_i64, "100.0", "101.0", "99.0", "100.5"]),
            ]))
            .expect(1)
            .create();
        let _gap_marks = server
            .mock("GET", "/v5/market/mark-price-kline")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("category".into(), "linear".into()),
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1".into()),
                Matcher::UrlEncoded("start".into(), "1704067320000".into()),
                Matcher::UrlEncoded("end".into(), "1704067379999".into()),
            ]))
            .with_status(200)
            .with_body(bybit_envelope(&[json!([
                1704067320000_i64,
                "101.5",
                "103.0",
                "101.0",
                "102.5"
            ])]))
            .expect(1)
            .create();
        let _risk = server
            .mock("GET", "/v5/market/risk-limit")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("category".into(), "linear".into()),
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            ]))
            .with_status(200)
            .with_body(
                json!({
                    "retCode": 0,
                    "retMsg": "OK",
                    "result": {
                        "list": [{
                            "symbol": "BTCUSDT",
                            "riskLimitValue": "100000",
                            "maintenanceMargin": "0.5",
                            "initialMargin": "1.0",
                            "maxLeverage": "100",
                            "mmDeduction": "0"
                        }],
                        "nextPageCursor": ""
                    },
                    "time": 1704067200123_i64
                })
                .to_string(),
            )
            .expect(1)
            .create();

        let source = sample_source(SourceTemplate::BybitUsdtPerps, "BTCUSDT");
        let endpoints = ExchangeEndpoints {
            bybit_base_url: server.url(),
            ..ExchangeEndpoints::default()
        };
        let cache_dir = tempdir().expect("tempdir");
        let cache = HistoricalCache::new(cache_dir.path().join("historical"));

        let initial = fetch_perp_backtest_context_with_cache(
            &source,
            Interval::Min1,
            1704067200000,
            1704067320000,
            &endpoints,
            Some(&cache),
        )
        .expect("initial context")
        .expect("perp context");
        assert_eq!(initial.mark_bars.len(), 2);

        let expanded = fetch_perp_backtest_context_with_cache(
            &source,
            Interval::Min1,
            1704067200000,
            1704067380000,
            &endpoints,
            Some(&cache),
        )
        .expect("expanded context")
        .expect("perp context");
        assert_eq!(expanded.mark_bars.len(), 3);
        assert!(matches!(
            expanded.risk_snapshot,
            VenueRiskSnapshot::BybitUsdtPerps(_)
        ));

        let cached = fetch_perp_backtest_context_with_cache(
            &source,
            Interval::Min1,
            1704067200000,
            1704067380000,
            &endpoints,
            Some(&cache),
        )
        .expect("cached context")
        .expect("perp context");
        assert_eq!(cached.mark_bars.len(), 3);
        assert_eq!(cached.mark_bars[2].time, 1704067320000.0);
    }

    #[test]
    fn gate_risk_snapshot_falls_back_to_contract_details() {
        let mut server = Server::new();
        let _marks = server
            .mock("GET", "/futures/usdt/candlesticks")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("contract".into(), "mark_BTC_USDT".into()),
                Matcher::UrlEncoded("interval".into(), "1m".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    {"t": 1704067200_i64, "o": "100.0", "h": "101.0", "l": "99.0", "c": "100.5"},
                    {"t": 1704067260_i64, "o": "100.5", "h": "102.0", "l": "100.0", "c": "101.5"}
                ])
                .to_string(),
            )
            .create();
        let _risk_404 = server
            .mock("GET", "/futures/usdt/risk_limit_tiers")
            .match_query(Matcher::UrlEncoded("contract".into(), "BTC_USDT".into()))
            .with_status(404)
            .create();
        let _contract = server
            .mock("GET", "/futures/usdt/contracts/BTC_USDT")
            .with_status(200)
            .with_body(
                json!({
                    "name": "BTC_USDT",
                    "maintenance_rate": "0.005",
                    "leverage_max": "100",
                    "risk_limit_base": "100000",
                    "risk_limit_max": "1000000"
                })
                .to_string(),
            )
            .create();

        let source = sample_source(SourceTemplate::GateUsdtPerps, "BTC_USDT");
        let context = fetch_perp_backtest_context(
            &source,
            Interval::Min1,
            1704067200000,
            1704067320000,
            &ExchangeEndpoints {
                binance_spot_base_url: String::new(),
                binance_usdm_base_url: String::new(),
                bybit_base_url: String::new(),
                gate_base_url: server.url(),
                ..ExchangeEndpoints::default()
            },
        )
        .expect("context")
        .expect("perp context");

        match context.risk_snapshot {
            VenueRiskSnapshot::GateUsdtPerps(snapshot) => {
                assert_eq!(
                    snapshot.source,
                    GateUsdtPerpsRiskSource::PublicContractApproximation
                );
                assert_eq!(snapshot.tiers.len(), 1);
            }
            other => panic!("unexpected snapshot: {other:?}"),
        }
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
