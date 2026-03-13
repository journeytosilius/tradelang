use std::collections::{BTreeMap, BTreeSet};

use reqwest::blocking::Client;

use crate::backtest::{PerpBacktestConfig, PerpBacktestContext, PerpMarginMode};
use crate::compiler::CompiledProgram;
use crate::exchange::{
    fetch_perp_backtest_context, fetch_source_runtime_config, ExchangeEndpoints,
};
use crate::interval::{DeclaredMarketSource, Interval, SourceTemplate};
use crate::runtime::SourceRuntimeConfig;

use super::venue::fetch_quote_feed;
use super::{
    ExecutionError, FeedSnapshotState, PaperExecutionSource, PaperFeedSnapshot,
    PaperSessionManifest, PriceSnapshot, TopOfBookSnapshot, ValuationPriceSource,
};

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const FEED_STALE_MS: i64 = 15_000;

pub(crate) struct MarketDataBootstrap {
    pub runtime: SourceRuntimeConfig,
    pub warmup_from_ms: i64,
    pub runtime_to_ms: i64,
    pub perp: Option<PerpBacktestConfig>,
    pub perp_context: Option<PerpBacktestContext>,
    pub portfolio_perp_contexts: BTreeMap<String, PerpBacktestContext>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FeedSubscriptionKey {
    template: String,
    symbol: String,
    endpoint_base: String,
}

#[derive(Clone, Debug, PartialEq)]
struct CachedFeedSnapshot {
    source: PaperExecutionSource,
    top_of_book: Option<TopOfBookSnapshot>,
    last_price: Option<PriceSnapshot>,
    mark_price: Option<PriceSnapshot>,
}

pub(crate) struct SharedMarketDataBus {
    client: Client,
    feeds: BTreeMap<FeedSubscriptionKey, CachedFeedSnapshot>,
}

impl SharedMarketDataBus {
    pub(crate) fn new() -> Result<Self, ExecutionError> {
        let client = Client::builder()
            .user_agent("palmscript-execution/0.1")
            .build()
            .map_err(|err| ExecutionError::Fetch(err.to_string()))?;
        Ok(Self {
            client,
            feeds: BTreeMap::new(),
        })
    }

    pub(crate) fn sync(&mut self, manifests: &[PaperSessionManifest], now_ms: i64) {
        let desired = manifests
            .iter()
            .filter(|manifest| {
                matches!(
                    manifest.status,
                    super::ExecutionSessionStatus::Queued
                        | super::ExecutionSessionStatus::Starting
                        | super::ExecutionSessionStatus::WarmingUp
                        | super::ExecutionSessionStatus::Live
                )
            })
            .flat_map(|manifest| {
                manifest
                    .config
                    .execution_source_aliases
                    .iter()
                    .filter_map(move |alias| {
                        manifest
                            .execution_sources
                            .iter()
                            .find(|source| source.alias == *alias)
                            .cloned()
                            .map(|source| {
                                (
                                    subscription_key(
                                        source.template,
                                        &source.symbol,
                                        &manifest.endpoints,
                                    ),
                                    (source, manifest.endpoints.clone()),
                                )
                            })
                    })
            })
            .collect::<BTreeMap<_, _>>();

        let desired_keys = desired.keys().cloned().collect::<BTreeSet<_>>();
        self.feeds.retain(|key, _| desired_keys.contains(key));

        for (key, (source, endpoints)) in desired {
            if let Ok(feed) = fetch_quote_feed(&self.client, &endpoints, &source, now_ms) {
                self.feeds.insert(
                    key,
                    CachedFeedSnapshot {
                        source,
                        top_of_book: feed.top_of_book,
                        last_price: feed.last_price,
                        mark_price: feed.mark_price,
                    },
                );
            }
        }
    }

    pub(crate) fn snapshots_for_manifest(
        &self,
        manifest: &PaperSessionManifest,
        now_ms: i64,
    ) -> Vec<PaperFeedSnapshot> {
        manifest
            .config
            .execution_source_aliases
            .iter()
            .filter_map(|alias| {
                let source = manifest
                    .execution_sources
                    .iter()
                    .find(|source| source.alias == *alias)?;
                let key = subscription_key(source.template, &source.symbol, &manifest.endpoints);
                let cached = self.feeds.get(&key);
                Some(PaperFeedSnapshot {
                    execution_alias: alias.clone(),
                    template: source.template,
                    symbol: source.symbol.clone(),
                    top_of_book: cached.and_then(|cached| {
                        cached
                            .top_of_book
                            .as_ref()
                            .map(|snapshot| snapshot_with_state(snapshot, now_ms))
                    }),
                    last_price: cached.and_then(|cached| {
                        cached
                            .last_price
                            .as_ref()
                            .map(|snapshot| price_with_state(snapshot, now_ms))
                    }),
                    mark_price: cached.and_then(|cached| {
                        cached
                            .mark_price
                            .as_ref()
                            .map(|snapshot| price_with_state(snapshot, now_ms))
                    }),
                    valuation_source: match source.template {
                        SourceTemplate::BinanceUsdm
                        | SourceTemplate::BybitUsdtPerps
                        | SourceTemplate::GateUsdtPerps => Some(ValuationPriceSource::Mark),
                        _ => Some(ValuationPriceSource::Mid),
                    },
                })
            })
            .collect()
    }

    pub(crate) fn subscription_count(&self) -> usize {
        self.feeds.len()
    }
}

type ResolvedPerpContexts = (
    Option<PerpBacktestConfig>,
    Option<PerpBacktestContext>,
    BTreeMap<String, PerpBacktestContext>,
);

pub(crate) fn bootstrap_runtime(
    compiled: &CompiledProgram,
    execution_aliases: &[String],
    leverage: Option<f64>,
    margin_mode: Option<PerpMarginMode>,
    start_time_ms: i64,
    now_ms: i64,
    endpoints: &ExchangeEndpoints,
) -> Result<MarketDataBootstrap, ExecutionError> {
    let base_interval = compiled
        .program
        .base_interval
        .ok_or(ExecutionError::MissingBaseInterval)?;
    let runtime_to_ms = base_interval.bucket_open_time(now_ms).unwrap_or(now_ms);
    let warmup_from_ms = compute_warmup_from_ms(compiled, start_time_ms);
    let runtime = fetch_source_runtime_config(compiled, warmup_from_ms, runtime_to_ms, endpoints)
        .map_err(|err| ExecutionError::Fetch(err.to_string()))?;
    let (perp, perp_context, portfolio_perp_contexts) = resolve_perp_contexts(
        compiled,
        execution_aliases,
        PerpBootstrapOptions {
            leverage,
            margin_mode: margin_mode.unwrap_or(PerpMarginMode::Isolated),
            base_interval,
            from_ms: warmup_from_ms,
            to_ms: runtime_to_ms,
        },
        endpoints,
    )?;
    Ok(MarketDataBootstrap {
        runtime,
        warmup_from_ms,
        runtime_to_ms,
        perp,
        perp_context,
        portfolio_perp_contexts,
    })
}

struct PerpBootstrapOptions {
    leverage: Option<f64>,
    margin_mode: PerpMarginMode,
    base_interval: Interval,
    from_ms: i64,
    to_ms: i64,
}

fn resolve_perp_contexts(
    compiled: &CompiledProgram,
    execution_aliases: &[String],
    options: PerpBootstrapOptions,
    endpoints: &ExchangeEndpoints,
) -> Result<ResolvedPerpContexts, ExecutionError> {
    let mut shared_perp = None;
    let mut single_context = None;
    let mut portfolio_contexts = BTreeMap::new();
    for alias in execution_aliases {
        let source = compiled
            .program
            .declared_sources
            .iter()
            .find(|source| source.alias == *alias)
            .ok_or_else(|| ExecutionError::InvalidConfig {
                message: format!("unknown execution source `{alias}`"),
            })?;
        match source.template {
            SourceTemplate::BinanceSpot | SourceTemplate::BybitSpot | SourceTemplate::GateSpot => {
                if options.leverage.is_some() {
                    return Err(ExecutionError::InvalidConfig {
                        message: format!(
                            "spot paper session source `{}` does not accept leverage",
                            source.alias
                        ),
                    });
                }
            }
            SourceTemplate::BinanceUsdm
            | SourceTemplate::BybitUsdtPerps
            | SourceTemplate::GateUsdtPerps => {
                let perp = PerpBacktestConfig {
                    leverage: options.leverage.unwrap_or(1.0),
                    margin_mode: options.margin_mode,
                };
                let context = fetch_perp_backtest_context(
                    source,
                    options.base_interval,
                    options.from_ms,
                    options.to_ms,
                    endpoints,
                )
                .map_err(|err| ExecutionError::Fetch(err.to_string()))?
                .ok_or_else(|| {
                    ExecutionError::Fetch(format!(
                        "missing perp backtest context for execution alias `{}`",
                        source.alias
                    ))
                })?;
                if shared_perp.is_none() {
                    shared_perp = Some(perp.clone());
                }
                if execution_aliases.len() == 1 {
                    single_context = Some(context.clone());
                }
                portfolio_contexts.insert(alias.clone(), context);
            }
        }
    }
    Ok((shared_perp, single_context, portfolio_contexts))
}

fn compute_warmup_from_ms(compiled: &CompiledProgram, start_time_ms: i64) -> i64 {
    let max_interval_duration_ms = compiled
        .program
        .source_intervals
        .iter()
        .map(|requirement| requirement.interval)
        .chain(compiled.program.base_interval)
        .map(interval_duration_hint_ms)
        .max()
        .unwrap_or(DAY_MS);
    let warmup_bars = compiled.program.history_capacity.max(2) as i64 + 2;
    start_time_ms.saturating_sub(max_interval_duration_ms.saturating_mul(warmup_bars))
}

fn interval_duration_hint_ms(interval: Interval) -> i64 {
    interval.fixed_duration_ms().unwrap_or(31 * DAY_MS)
}

fn subscription_key(
    template: SourceTemplate,
    symbol: &str,
    endpoints: &ExchangeEndpoints,
) -> FeedSubscriptionKey {
    let endpoint_base = match template {
        SourceTemplate::BinanceSpot => endpoints.binance_spot_base_url.clone(),
        SourceTemplate::BinanceUsdm => endpoints.binance_usdm_base_url.clone(),
        SourceTemplate::BybitSpot | SourceTemplate::BybitUsdtPerps => {
            endpoints.bybit_base_url.clone()
        }
        SourceTemplate::GateSpot | SourceTemplate::GateUsdtPerps => endpoints.gate_base_url.clone(),
    };
    FeedSubscriptionKey {
        template: template.as_str().to_string(),
        symbol: symbol.to_string(),
        endpoint_base,
    }
}

fn snapshot_with_state(snapshot: &TopOfBookSnapshot, now_ms: i64) -> TopOfBookSnapshot {
    let mut snapshot = snapshot.clone();
    if now_ms.saturating_sub(snapshot.time_ms) > FEED_STALE_MS {
        snapshot.state = FeedSnapshotState::Stale;
    }
    snapshot
}

fn price_with_state(snapshot: &PriceSnapshot, now_ms: i64) -> PriceSnapshot {
    let mut snapshot = snapshot.clone();
    if now_ms.saturating_sub(snapshot.time_ms) > FEED_STALE_MS {
        snapshot.state = FeedSnapshotState::Stale;
    }
    snapshot
}

pub(crate) fn resolve_execution_sources<'a>(
    compiled: &'a CompiledProgram,
    aliases: &[String],
) -> Result<Vec<&'a DeclaredMarketSource>, ExecutionError> {
    aliases
        .iter()
        .map(|alias| {
            compiled
                .program
                .execution_targets()
                .iter()
                .find(|source| source.alias == *alias)
                .ok_or_else(|| ExecutionError::InvalidConfig {
                    message: format!("unknown execution source `{alias}`"),
                })
        })
        .collect()
}
