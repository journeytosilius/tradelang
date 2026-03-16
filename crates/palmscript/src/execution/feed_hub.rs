use std::collections::{BTreeMap, BTreeSet};

use reqwest::blocking::Client;

use crate::compiler::CompiledProgram;
use crate::exchange::{collect_required_source_fields, fetch_source_feed, ExchangeEndpoints};
use crate::interval::{DeclaredMarketSource, Interval, MarketField, SourceTemplate};
use crate::runtime::{Bar, SourceFeed, SourceRuntimeConfig};

use super::market_data::resolve_execution_sources;
use super::venue::fetch_quote_feed;
use super::{
    ExecutionError, FeedArmingState, FeedSnapshotState, PaperExecutionSource, PaperFeedSnapshot,
    PaperFeedSummary, PriceSnapshot, TopOfBookSnapshot, ValuationPriceSource,
};

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const FEED_STALE_MS: i64 = 15_000;

#[derive(Clone, Debug)]
pub(crate) struct SessionFeedPlan {
    pub base_interval: Interval,
    pub warmup_from_ms: i64,
    pub subscriptions: Vec<SessionFeedSubscription>,
}

#[derive(Clone, Debug)]
pub(crate) struct SessionFeedSubscription {
    pub source_id: u16,
    pub source_alias: String,
    pub template: SourceTemplate,
    pub symbol: String,
    pub endpoints: ExchangeEndpoints,
    pub canonical_interval: Interval,
    pub requested_intervals: BTreeSet<Interval>,
    pub required_fields: BTreeSet<MarketField>,
    pub warmup_from_ms: i64,
    pub quote_required: bool,
    pub execution_alias: Option<String>,
    pub source: DeclaredMarketSource,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FeedKey {
    template: String,
    symbol: String,
    endpoint_base: String,
    canonical_interval: Interval,
}

#[derive(Clone, Debug)]
struct FeedState {
    arming_state: FeedArmingState,
    history_ready: bool,
    live_ready: bool,
    latest_closed_bar_time_ms: Option<i64>,
    history: Vec<Bar>,
    top_of_book: Option<TopOfBookSnapshot>,
    last_price: Option<PriceSnapshot>,
    mark_price: Option<PriceSnapshot>,
    failure_message: Option<String>,
}

impl Default for FeedState {
    fn default() -> Self {
        Self {
            arming_state: FeedArmingState::BootstrappingHistory,
            history_ready: false,
            live_ready: false,
            latest_closed_bar_time_ms: None,
            history: Vec::new(),
            top_of_book: None,
            last_price: None,
            mark_price: None,
            failure_message: None,
        }
    }
}

#[derive(Clone, Debug)]
struct ManagedFeed {
    source: DeclaredMarketSource,
    endpoints: ExchangeEndpoints,
    canonical_interval: Interval,
    warmup_from_ms: i64,
    quote_required: bool,
    required_fields: BTreeSet<MarketField>,
    state: FeedState,
}

pub(crate) struct FeedHub {
    feeds: BTreeMap<FeedKey, ManagedFeed>,
}

impl FeedHub {
    pub(crate) fn new() -> Result<Self, ExecutionError> {
        Ok(Self {
            feeds: BTreeMap::new(),
        })
    }

    pub(crate) async fn sync(
        &mut self,
        plans: &[SessionFeedPlan],
        now_ms: i64,
    ) -> Result<(), ExecutionError> {
        let mut desired = BTreeMap::<FeedKey, SessionFeedSubscription>::new();
        for plan in plans {
            for subscription in &plan.subscriptions {
                let key = feed_key(subscription);
                desired
                    .entry(key)
                    .and_modify(|existing| {
                        existing
                            .requested_intervals
                            .extend(subscription.requested_intervals.iter().copied());
                        existing.quote_required |= subscription.quote_required;
                        existing
                            .required_fields
                            .extend(subscription.required_fields.iter().copied());
                        existing.warmup_from_ms =
                            existing.warmup_from_ms.min(subscription.warmup_from_ms);
                        existing.execution_alias = existing
                            .execution_alias
                            .clone()
                            .or_else(|| subscription.execution_alias.clone());
                        if subscription.source.id < existing.source.id {
                            existing.source = subscription.source.clone();
                            existing.source_alias = subscription.source_alias.clone();
                        }
                    })
                    .or_insert_with(|| subscription.clone());
            }
        }

        self.feeds.retain(|key, _| desired.contains_key(key));
        for (key, subscription) in desired {
            let managed = self.feeds.entry(key).or_insert_with(|| ManagedFeed {
                source: subscription.source.clone(),
                endpoints: subscription.endpoints.clone(),
                canonical_interval: subscription.canonical_interval,
                warmup_from_ms: i64::MAX,
                quote_required: subscription.quote_required,
                required_fields: BTreeSet::new(),
                state: FeedState::default(),
            });
            managed.warmup_from_ms = managed.warmup_from_ms.min(subscription.warmup_from_ms);
            managed.quote_required |= subscription.quote_required;
            managed.required_fields = subscription.required_fields.clone();
            managed.endpoints = subscription.endpoints.clone();
            refresh_feed(managed, now_ms).await?;
        }
        Ok(())
    }
}

async fn refresh_feed(managed: &mut ManagedFeed, now_ms: i64) -> Result<(), ExecutionError> {
    let interval = managed.canonical_interval;
    let next_open_time = managed
        .state
        .latest_closed_bar_time_ms
        .and_then(|open_time| interval.fixed_duration_ms().map(|step| open_time + step));
    let to_ms = interval
        .bucket_open_time(now_ms)
        .and_then(|open_time| interval.fixed_duration_ms().map(|step| open_time + step))
        .unwrap_or(now_ms);

    if !managed.state.history_ready {
        managed.state.arming_state = FeedArmingState::BootstrappingHistory;
        let source = managed.source.clone();
        let endpoints = managed.endpoints.clone();
        let from_ms = managed.warmup_from_ms;
        let required_fields = managed.required_fields.clone();
        let field_list = format_required_fields(&required_fields);
        let bars = tokio::task::spawn_blocking(move || {
            let client = blocking_client()?;
            fetch_source_feed(
                &client,
                &source,
                interval,
                from_ms,
                to_ms,
                &endpoints,
                &required_fields,
            )
            .map_err(|err| {
                ExecutionError::Fetch(format!(
                    "feed bootstrap failed for source `{}` ({}) `{}` {} window=[{}, {}) required_fields=[{}]: {err}",
                    source.alias,
                    source.template.as_str(),
                    source.symbol,
                    interval.as_str(),
                    from_ms,
                    to_ms,
                    field_list,
                ))
            })
        })
        .await
        .map_err(|err| {
            ExecutionError::Runtime(format!(
                "feed bootstrap task failed for source `{}` ({}) `{}` {}: {err}",
                managed.source.alias,
                managed.source.template.as_str(),
                managed.source.symbol,
                interval.as_str(),
            ))
        })??;
        managed.state.history = bars;
        managed.state.history_ready = true;
        managed.state.latest_closed_bar_time_ms =
            managed.state.history.last().map(|bar| bar.time as i64);
        managed.state.arming_state = FeedArmingState::ConnectingLive;
    } else if let Some(from_ms) = next_open_time.filter(|from_ms| *from_ms < to_ms) {
        let source = managed.source.clone();
        let endpoints = managed.endpoints.clone();
        let required_fields = managed.required_fields.clone();
        let field_list = format_required_fields(&required_fields);
        let bars = tokio::task::spawn_blocking(move || {
            let client = blocking_client()?;
            fetch_source_feed(
                &client,
                &source,
                interval,
                from_ms,
                to_ms,
                &endpoints,
                &required_fields,
            )
            .map_err(|err| {
                ExecutionError::Fetch(format!(
                    "feed append failed for source `{}` ({}) `{}` {} window=[{}, {}) required_fields=[{}]: {err}",
                    source.alias,
                    source.template.as_str(),
                    source.symbol,
                    interval.as_str(),
                    from_ms,
                    to_ms,
                    field_list,
                ))
            })
        })
        .await
        .map_err(|err| {
            ExecutionError::Runtime(format!(
                "feed append task failed for source `{}` ({}) `{}` {}: {err}",
                managed.source.alias,
                managed.source.template.as_str(),
                managed.source.symbol,
                interval.as_str(),
            ))
        })??;
        append_unique_bars(&mut managed.state.history, bars);
        managed.state.latest_closed_bar_time_ms =
            managed.state.history.last().map(|bar| bar.time as i64);
    }

    let endpoints = managed.endpoints.clone();
    let source = PaperExecutionSource {
        alias: managed.source.alias.clone(),
        template: managed.source.template,
        symbol: managed.source.symbol.clone(),
    };
    let quote = tokio::task::spawn_blocking(move || {
        let client = blocking_client()?;
        fetch_quote_feed(&client, &endpoints, &source, now_ms)
    })
    .await
    .map_err(|err| {
        ExecutionError::Runtime(format!(
            "feed quote task failed for source `{}` ({}) `{}`: {err}",
            managed.source.alias,
            managed.source.template.as_str(),
            managed.source.symbol,
        ))
    })?;
    match quote {
        Ok(quote) => {
            managed.state.top_of_book = quote.top_of_book;
            managed.state.last_price = quote.last_price;
            managed.state.mark_price = quote.mark_price;
            managed.state.failure_message = None;
        }
        Err(err) => {
            managed.state.failure_message = Some(format!(
                "quote refresh failed for source `{}` ({}) `{}` at {}: {err}",
                managed.source.alias,
                managed.source.template.as_str(),
                managed.source.symbol,
                now_ms,
            ));
        }
    }

    let quote_ready = !managed.quote_required || managed.state.top_of_book.is_some();
    managed.state.live_ready = managed.state.history_ready && quote_ready;
    managed.state.arming_state = if managed.state.failure_message.is_some() {
        if managed.state.history_ready {
            FeedArmingState::Degraded
        } else {
            FeedArmingState::Failed
        }
    } else if managed.state.live_ready {
        FeedArmingState::Live
    } else if managed.state.history_ready {
        FeedArmingState::ConnectingLive
    } else {
        FeedArmingState::BootstrappingHistory
    };
    Ok(())
}

fn blocking_client() -> Result<Client, ExecutionError> {
    Client::builder()
        .user_agent("palmscript-execution/0.2")
        .build()
        .map_err(|err| ExecutionError::Fetch(err.to_string()))
}

fn format_required_fields(fields: &BTreeSet<MarketField>) -> String {
    fields
        .iter()
        .map(|field| field.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

impl FeedHub {
    pub(crate) fn build_runtime(
        &self,
        compiled: &CompiledProgram,
        plan: &SessionFeedPlan,
        now_ms: i64,
    ) -> Result<Option<(SourceRuntimeConfig, i64)>, ExecutionError> {
        let mut feeds = Vec::new();
        let mut latest_base_open = None::<i64>;
        for subscription in &plan.subscriptions {
            let key = feed_key(subscription);
            let Some(managed) = self.feeds.get(&key) else {
                return Ok(None);
            };
            if !managed.state.history_ready {
                return Ok(None);
            }
            for interval in &subscription.requested_intervals {
                let bars = if *interval == subscription.canonical_interval {
                    slice_bars(&managed.state.history, plan.warmup_from_ms, now_ms)
                } else {
                    let canonical = slice_bars(&managed.state.history, plan.warmup_from_ms, now_ms);
                    aggregate_bars(*interval, &canonical)
                };
                if *interval == plan.base_interval {
                    latest_base_open = latest_base_open.max(bars.last().map(|bar| bar.time as i64));
                }
                feeds.push(SourceFeed {
                    source_id: subscription.source_id,
                    interval: *interval,
                    bars,
                });
            }
        }
        let Some(latest_base_open) = latest_base_open else {
            return Ok(None);
        };
        let runtime_to_ms = plan
            .base_interval
            .fixed_duration_ms()
            .map(|step| latest_base_open + step)
            .unwrap_or(latest_base_open + DAY_MS);
        let _ = compiled;
        Ok(Some((
            SourceRuntimeConfig {
                base_interval: plan.base_interval,
                feeds,
            },
            runtime_to_ms,
        )))
    }

    pub(crate) fn feed_snapshots_for_plan(
        &self,
        plan: &SessionFeedPlan,
        now_ms: i64,
    ) -> Vec<PaperFeedSnapshot> {
        plan.subscriptions
            .iter()
            .filter(|subscription| subscription.execution_alias.is_some())
            .map(|subscription| {
                let key = feed_key(subscription);
                let state = self.feeds.get(&key).map(|managed| &managed.state);
                let top_of_book = state
                    .and_then(|state| state.top_of_book.as_ref())
                    .map(|snapshot| snapshot_with_state(snapshot, now_ms));
                let last_price = state
                    .and_then(|state| state.last_price.as_ref())
                    .map(|snapshot| price_with_state(snapshot, now_ms));
                let mark_price = state
                    .and_then(|state| state.mark_price.as_ref())
                    .map(|snapshot| price_with_state(snapshot, now_ms));
                PaperFeedSnapshot {
                    execution_alias: subscription.source_alias.clone(),
                    template: subscription.template,
                    symbol: subscription.symbol.clone(),
                    interval: Some(subscription.canonical_interval),
                    arming_state: state.map(|state| state.arming_state),
                    history_ready: state.is_some_and(|state| state.history_ready),
                    live_ready: state.is_some_and(|state| state.live_ready),
                    latest_closed_bar_time_ms: state
                        .and_then(|state| state.latest_closed_bar_time_ms),
                    top_of_book,
                    last_price,
                    mark_price,
                    valuation_source: match subscription.template {
                        SourceTemplate::BinanceUsdm
                        | SourceTemplate::BybitUsdtPerps
                        | SourceTemplate::GateUsdtPerps => Some(ValuationPriceSource::Mark),
                        _ => Some(ValuationPriceSource::Mid),
                    },
                    failure_message: state.and_then(|state| state.failure_message.clone()),
                }
            })
            .collect()
    }

    pub(crate) fn required_feeds_for_plan(
        &self,
        plan: &SessionFeedPlan,
        now_ms: i64,
    ) -> Vec<PaperFeedSnapshot> {
        plan.subscriptions
            .iter()
            .map(|subscription| {
                let key = feed_key(subscription);
                let state = self.feeds.get(&key).map(|managed| &managed.state);
                PaperFeedSnapshot {
                    execution_alias: subscription.source_alias.clone(),
                    template: subscription.template,
                    symbol: subscription.symbol.clone(),
                    interval: Some(subscription.canonical_interval),
                    arming_state: state.map(|state| state.arming_state),
                    history_ready: state.is_some_and(|state| state.history_ready),
                    live_ready: state.is_some_and(|state| state.live_ready),
                    latest_closed_bar_time_ms: state
                        .and_then(|state| state.latest_closed_bar_time_ms),
                    top_of_book: state
                        .and_then(|state| state.top_of_book.as_ref())
                        .map(|snapshot| snapshot_with_state(snapshot, now_ms)),
                    last_price: state
                        .and_then(|state| state.last_price.as_ref())
                        .map(|snapshot| price_with_state(snapshot, now_ms)),
                    mark_price: state
                        .and_then(|state| state.mark_price.as_ref())
                        .map(|snapshot| price_with_state(snapshot, now_ms)),
                    valuation_source: match subscription.template {
                        SourceTemplate::BinanceUsdm
                        | SourceTemplate::BybitUsdtPerps
                        | SourceTemplate::GateUsdtPerps => Some(ValuationPriceSource::Mark),
                        _ => Some(ValuationPriceSource::Mid),
                    },
                    failure_message: state.and_then(|state| state.failure_message.clone()),
                }
            })
            .collect()
    }

    pub(crate) fn feed_summary_for_plan(&self, plan: &SessionFeedPlan) -> PaperFeedSummary {
        let mut summary = PaperFeedSummary {
            total_feeds: plan.subscriptions.len(),
            ..PaperFeedSummary::default()
        };
        for subscription in &plan.subscriptions {
            let key = feed_key(subscription);
            if let Some(state) = self.feeds.get(&key).map(|managed| &managed.state) {
                if state.history_ready {
                    summary.history_ready_feeds += 1;
                }
                if state.live_ready {
                    summary.live_ready_feeds += 1;
                }
                if state.failure_message.is_some() {
                    summary.failed_feeds += 1;
                }
            }
        }
        summary
    }

    pub(crate) fn fully_armed(&self, plan: &SessionFeedPlan) -> bool {
        plan.subscriptions.iter().all(|subscription| {
            let key = feed_key(subscription);
            self.feeds
                .get(&key)
                .map(|managed| managed.state.live_ready)
                .unwrap_or(false)
        })
    }

    pub(crate) fn subscription_count(&self) -> usize {
        self.feeds.len()
    }

    pub(crate) fn armed_feed_count(&self) -> usize {
        self.feeds
            .values()
            .filter(|feed| feed.state.live_ready)
            .count()
    }

    pub(crate) fn connecting_feed_count(&self) -> usize {
        self.feeds
            .values()
            .filter(|feed| {
                matches!(
                    feed.state.arming_state,
                    FeedArmingState::BootstrappingHistory | FeedArmingState::ConnectingLive
                )
            })
            .count()
    }

    pub(crate) fn degraded_feed_count(&self) -> usize {
        self.feeds
            .values()
            .filter(|feed| matches!(feed.state.arming_state, FeedArmingState::Degraded))
            .count()
    }

    pub(crate) fn failed_feed_count(&self) -> usize {
        self.feeds
            .values()
            .filter(|feed| matches!(feed.state.arming_state, FeedArmingState::Failed))
            .count()
    }
}

pub(crate) fn build_session_feed_plan(
    compiled: &CompiledProgram,
    execution_aliases: &[String],
    start_time_ms: i64,
    endpoints: &ExchangeEndpoints,
) -> Result<SessionFeedPlan, ExecutionError> {
    let base_interval = compiled
        .program
        .base_interval
        .ok_or(ExecutionError::MissingBaseInterval)?;
    let warmup_from_ms = compute_warmup_from_ms(compiled, start_time_ms);
    let execution_sources = resolve_execution_sources(compiled, execution_aliases)?;
    let execution_ids = execution_sources
        .iter()
        .map(|source| (source.id, source.alias.clone()))
        .collect::<BTreeMap<_, _>>();

    let required = collect_required_source_fields(compiled, base_interval);
    let mut source_requirements = BTreeMap::<u16, SourceRequirement>::new();
    for (requirement, fields) in required {
        let entry = source_requirements
            .entry(requirement.source_id)
            .or_default();
        entry.intervals.insert(requirement.interval);
        entry.required_fields.extend(fields);
    }

    let mut subscriptions = Vec::new();
    for (source_id, requirement) in source_requirements {
        let source = compiled
            .program
            .declared_sources
            .iter()
            .chain(compiled.program.declared_executions.iter())
            .find(|source| source.id == source_id)
            .ok_or_else(|| ExecutionError::InvalidConfig {
                message: format!("unknown source id {source_id} in paper feed plan"),
            })?;
        let canonical_interval = requirement
            .intervals
            .iter()
            .copied()
            .min_by_key(|interval| interval.ordinal())
            .unwrap_or(base_interval);
        subscriptions.push(SessionFeedSubscription {
            source_id,
            source_alias: source.alias.clone(),
            template: source.template,
            symbol: source.symbol.clone(),
            endpoints: endpoints.clone(),
            canonical_interval,
            requested_intervals: requirement.intervals,
            required_fields: requirement.required_fields,
            warmup_from_ms,
            quote_required: execution_ids.contains_key(&source_id),
            execution_alias: execution_ids.get(&source_id).cloned(),
            source: source.clone(),
        });
    }

    Ok(SessionFeedPlan {
        base_interval,
        warmup_from_ms,
        subscriptions,
    })
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

fn feed_key(subscription: &SessionFeedSubscription) -> FeedKey {
    FeedKey {
        template: subscription.template.as_str().to_string(),
        symbol: subscription.symbol.clone(),
        endpoint_base: endpoint_base_for_template(subscription.template, &subscription.endpoints),
        canonical_interval: subscription.canonical_interval,
    }
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

fn append_unique_bars(target: &mut Vec<Bar>, bars: Vec<Bar>) {
    let mut by_time = target
        .iter()
        .copied()
        .map(|bar| (bar.time as i64, bar))
        .collect::<BTreeMap<_, _>>();
    for bar in bars {
        by_time.insert(bar.time as i64, bar);
    }
    *target = by_time.into_values().collect();
}

fn slice_bars(bars: &[Bar], from_ms: i64, to_ms: i64) -> Vec<Bar> {
    bars.iter()
        .copied()
        .filter(|bar| {
            let time = bar.time as i64;
            time >= from_ms && time < to_ms
        })
        .collect()
}

fn aggregate_bars(interval: Interval, bars: &[Bar]) -> Vec<Bar> {
    let mut out = Vec::new();
    let mut current_bucket = None::<i64>;
    let mut aggregate = None::<Bar>;

    for bar in bars {
        let open_time = interval
            .bucket_open_time(bar.time as i64)
            .unwrap_or(bar.time as i64);
        match current_bucket {
            Some(bucket) if bucket == open_time => {
                if let Some(aggregate) = aggregate.as_mut() {
                    aggregate.high = aggregate.high.max(bar.high);
                    aggregate.low = aggregate.low.min(bar.low);
                    aggregate.close = bar.close;
                    aggregate.volume += bar.volume;
                    merge_scalar_fields(aggregate, *bar);
                }
            }
            Some(_) => {
                if let Some(aggregate) = aggregate.take() {
                    out.push(aggregate);
                }
                current_bucket = Some(open_time);
                aggregate = Some(bar_for_bucket(*bar, open_time));
            }
            None => {
                current_bucket = Some(open_time);
                aggregate = Some(bar_for_bucket(*bar, open_time));
            }
        }
    }

    if let Some(aggregate) = aggregate {
        out.push(aggregate);
    }
    out
}

fn bar_for_bucket(bar: Bar, open_time: i64) -> Bar {
    Bar {
        time: open_time as f64,
        ..bar
    }
}

fn merge_scalar_fields(target: &mut Bar, overlay: Bar) {
    if overlay.funding_rate.is_some() {
        target.funding_rate = overlay.funding_rate;
    }
    if overlay.open_interest.is_some() {
        target.open_interest = overlay.open_interest;
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

#[derive(Default)]
struct SourceRequirement {
    intervals: BTreeSet<Interval>,
    required_fields: BTreeSet<MarketField>,
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

#[cfg(test)]
mod tests {
    use super::{aggregate_bars, build_session_feed_plan, FeedHub};
    use crate::compile;
    use crate::exchange::ExchangeEndpoints;
    use crate::interval::{Interval, MarketField};
    use crate::runtime::Bar;
    use mockito::{Matcher, Server};
    use serde_json::json;

    fn bar(time: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Bar {
        Bar {
            open,
            high,
            low,
            close,
            volume,
            time: time as f64,
            funding_rate: None,
            open_interest: None,
            mark_price: None,
            index_price: None,
            premium_index: None,
            basis: None,
        }
    }

    #[test]
    fn aggregate_bars_closes_expected_windows() {
        let bars = vec![
            bar(0, 1.0, 2.0, 0.5, 1.5, 1.0),
            bar(60_000, 1.5, 3.0, 1.0, 2.0, 2.0),
            bar(120_000, 2.0, 4.0, 1.5, 3.5, 3.0),
        ];
        let aggregated = aggregate_bars(Interval::Min3, &bars);
        assert_eq!(aggregated.len(), 1);
        assert_eq!(aggregated[0].time as i64, 0);
        assert_eq!(aggregated[0].open, 1.0);
        assert_eq!(aggregated[0].high, 4.0);
        assert_eq!(aggregated[0].low, 0.5);
        assert_eq!(aggregated[0].close, 3.5);
        assert_eq!(aggregated[0].volume, 6.0);
    }

    #[test]
    fn aggregate_bars_preserves_latest_scalar_source_fields() {
        let mut first = bar(0, 100.0, 101.0, 99.0, 100.5, 10.0);
        first.mark_price = Some(100.25);
        first.premium_index = Some(0.001);
        let mut second = bar(3_600_000, 100.5, 102.0, 100.0, 101.5, 12.0);
        second.funding_rate = Some(0.0008);
        second.mark_price = Some(101.25);
        second.index_price = Some(101.0);
        second.premium_index = Some(0.0015);
        second.basis = Some(0.25);

        let aggregated = aggregate_bars(Interval::Hour2, &[first, second]);
        assert_eq!(aggregated.len(), 1);
        assert_eq!(aggregated[0].time as i64, 0);
        assert_eq!(aggregated[0].open, 100.0);
        assert_eq!(aggregated[0].high, 102.0);
        assert_eq!(aggregated[0].low, 99.0);
        assert_eq!(aggregated[0].close, 101.5);
        assert_eq!(aggregated[0].volume, 22.0);
        assert_eq!(aggregated[0].funding_rate, Some(0.0008));
        assert_eq!(aggregated[0].mark_price, Some(101.25));
        assert_eq!(aggregated[0].index_price, Some(101.0));
        assert_eq!(aggregated[0].premium_index, Some(0.0015));
        assert_eq!(aggregated[0].basis, Some(0.25));
    }

    #[test]
    fn build_session_feed_plan_uses_shared_canonical_feed() {
        let compiled = compile(
            "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution exec = binance.spot(\"BTCUSDT\")
use spot 5m
plot(spot.close)
plot(spot.5m.close)",
        )
        .expect("compile");
        let plan = build_session_feed_plan(
            &compiled,
            &["exec".to_string()],
            1_704_067_200_000,
            &ExchangeEndpoints::default(),
        )
        .expect("plan");
        let spot = plan
            .subscriptions
            .iter()
            .find(|subscription| subscription.source_alias == "spot")
            .expect("spot feed");
        assert_eq!(spot.canonical_interval, Interval::Min1);
        assert!(spot.requested_intervals.contains(&Interval::Min1));
        assert!(spot.requested_intervals.contains(&Interval::Min5));
    }

    #[test]
    fn build_session_feed_plan_tracks_auxiliary_field_requirements() {
        let compiled = compile(
            "interval 1h
source perp = binance.usdm(\"BTCUSDT\")
execution exec = binance.usdm(\"BTCUSDT\")
use perp 4h
plot(perp.funding_rate)
plot(perp.4h.premium_index)",
        )
        .expect("compile");
        let plan = build_session_feed_plan(
            &compiled,
            &["exec".to_string()],
            1_704_067_200_000,
            &ExchangeEndpoints::default(),
        )
        .expect("plan");
        let perp = plan
            .subscriptions
            .iter()
            .find(|subscription| subscription.source_alias == "perp")
            .expect("perp feed");
        assert!(perp.required_fields.contains(&MarketField::FundingRate));
        assert!(perp.required_fields.contains(&MarketField::PremiumIndex));
        assert!(perp.requested_intervals.contains(&Interval::Hour4));
    }

    #[tokio::test]
    async fn feed_hub_bootstraps_auxiliary_fields_for_paper_runtime() {
        let mut server = Server::new_async().await;
        server
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
                    ],
                    [
                        1704074400000_i64,
                        "102.0",
                        "103.0",
                        "101.0",
                        "102.5",
                        "12.0"
                    ],
                    [
                        1704078000000_i64,
                        "103.0",
                        "104.0",
                        "102.0",
                        "103.5",
                        "13.0"
                    ]
                ])
                .to_string(),
            )
            .create();
        server
            .mock("GET", "/fapi/v1/premiumIndexKlines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1h".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [1704067200000_i64, "0.0", "0.0", "0.0", "0.0010", "0"],
                    [1704070800000_i64, "0.0", "0.0", "0.0", "0.0015", "0"],
                    [1704074400000_i64, "0.0", "0.0", "0.0", "0.0020", "0"],
                    [1704078000000_i64, "0.0", "0.0", "0.0", "0.0025", "0"]
                ])
                .to_string(),
            )
            .create();
        server
            .mock("GET", "/fapi/v1/fundingRate")
            .match_query(Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()))
            .with_status(200)
            .with_body(
                json!([
                    { "fundingRate": "0.0008", "fundingTime": 1704067200000_i64 },
                    { "fundingRate": "0.0009", "fundingTime": 1704078000000_i64 }
                ])
                .to_string(),
            )
            .create();

        let compiled = compile(
            "interval 1h
source perp = binance.usdm(\"BTCUSDT\")
execution exec = binance.usdm(\"BTCUSDT\")
use perp 4h
plot(perp.funding_rate)
plot(perp.4h.premium_index)",
        )
        .expect("compile");
        let plan = build_session_feed_plan(
            &compiled,
            &["exec".to_string()],
            1_704_067_200_000,
            &ExchangeEndpoints {
                binance_usdm_base_url: server.url(),
                ..ExchangeEndpoints::default()
            },
        )
        .expect("plan");
        let mut hub = FeedHub::new().expect("hub");
        hub.sync(std::slice::from_ref(&plan), 1_704_082_400_000)
            .await
            .expect("sync");
        let (runtime, _) = hub
            .build_runtime(&compiled, &plan, 1_704_082_400_000)
            .expect("runtime")
            .expect("armed runtime");
        let base = runtime
            .feeds
            .iter()
            .find(|feed| feed.source_id == 0 && feed.interval == Interval::Hour1)
            .expect("base feed");
        assert!(base
            .bars
            .iter()
            .any(|bar| matches!(bar.funding_rate, Some(0.0008 | 0.0009))));
        let higher = runtime
            .feeds
            .iter()
            .find(|feed| feed.source_id == 0 && feed.interval == Interval::Hour4)
            .expect("higher feed");
        assert!(higher
            .bars
            .iter()
            .any(|bar| bar.premium_index == Some(0.0025)));
        assert!(higher
            .bars
            .iter()
            .any(|bar| bar.funding_rate == Some(0.0009)));
    }

    #[tokio::test]
    async fn feed_summary_defaults_to_zero_for_empty_hub() {
        let hub = FeedHub::new().expect("hub");
        let compiled = compile(
            "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution exec = binance.spot(\"BTCUSDT\")
plot(spot.close)",
        )
        .expect("compile");
        let plan = build_session_feed_plan(
            &compiled,
            &["exec".to_string()],
            1_704_067_200_000,
            &ExchangeEndpoints::default(),
        )
        .expect("plan");
        let summary = hub.feed_summary_for_plan(&plan);
        assert_eq!(summary.total_feeds, plan.subscriptions.len());
        assert_eq!(summary.history_ready_feeds, 0);
    }
}
