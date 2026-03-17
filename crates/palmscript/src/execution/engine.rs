use crate::backtest::{
    BacktestConfig, BacktestError, PerpBacktestConfig, PerpBacktestContext, PerpMarginMode,
};
use crate::compiler::compile;
use crate::run_backtest_with_sources;
use crate::runtime::VmLimits;

use std::collections::BTreeMap;

use palmscript_logger::{debug_fields, error_fields, info_fields, warn_fields, LogField};

use super::feed_hub::{build_session_feed_plan, FeedHub, SessionFeedPlan};
use super::market_data::{
    compute_warmup_from_ms, resolve_execution_sources, resolve_perp_contexts, PerpBootstrapOptions,
};
use super::venue::validate_paper_source;
use super::{
    append_log_event, load_paper_session_export, load_paper_session_script,
    load_paper_session_snapshot, persist_session_manifest, persist_session_result,
    persist_session_snapshot, render_snapshot_from_result, ExecutionError, ExecutionSessionHealth,
    ExecutionSessionStatus, FeedSnapshotState, PaperSessionLogEvent, PaperSessionManifest,
};

pub(crate) struct LoadedPaperSession {
    compiled: crate::compiler::CompiledProgram,
    feed_plan: SessionFeedPlan,
    perp: Option<PerpBacktestConfig>,
    perp_context: Option<PerpBacktestContext>,
    portfolio_perp_contexts: BTreeMap<String, PerpBacktestContext>,
    perp_context_to_ms: i64,
}

impl LoadedPaperSession {
    fn load_blocking(manifest: &PaperSessionManifest, now_ms: i64) -> Result<Self, ExecutionError> {
        let source = load_paper_session_script(&manifest.session_id)?;
        let compiled = compile(&source).map_err(|err| ExecutionError::Compile(err.to_string()))?;
        let execution_sources =
            resolve_execution_sources(&compiled, &manifest.config.execution_source_aliases)?;
        for source in &execution_sources {
            validate_paper_source(source)?;
        }
        let feed_plan = build_session_feed_plan(
            &compiled,
            &manifest.config.execution_source_aliases,
            manifest.start_time_ms,
            &manifest.endpoints,
        )?;
        let base_interval = compiled
            .program
            .base_interval
            .ok_or(ExecutionError::MissingBaseInterval)?;
        let warmup_from_ms = compute_warmup_from_ms(&compiled, manifest.start_time_ms);
        let runtime_to_ms = base_interval
            .bucket_open_time(now_ms)
            .and_then(|open_time| {
                base_interval
                    .fixed_duration_ms()
                    .map(|step| open_time + step)
            })
            .unwrap_or(now_ms);
        let (perp, perp_context, portfolio_perp_contexts) = resolve_perp_contexts(
            &compiled,
            &manifest.config.execution_source_aliases,
            PerpBootstrapOptions {
                leverage: manifest.config.leverage,
                margin_mode: manifest
                    .config
                    .margin_mode
                    .unwrap_or(PerpMarginMode::Isolated),
                base_interval,
                from_ms: warmup_from_ms,
                to_ms: runtime_to_ms,
            },
            &manifest.endpoints,
        )?;
        Ok(Self {
            compiled,
            feed_plan,
            perp,
            perp_context,
            portfolio_perp_contexts,
            perp_context_to_ms: runtime_to_ms,
        })
    }

    pub(crate) fn feed_plan(&self) -> &SessionFeedPlan {
        &self.feed_plan
    }

    pub(crate) async fn load(
        manifest: &PaperSessionManifest,
        now_ms: i64,
    ) -> Result<Self, ExecutionError> {
        let manifest = manifest.clone();
        let session_id = manifest.session_id.clone();
        tokio::task::spawn_blocking({
            let manifest = manifest.clone();
            move || Self::load_blocking(&manifest, now_ms)
        })
        .await
        .map_err(|err| {
            ExecutionError::Runtime(format!(
                "paper session `{}` load task failed: {err}",
                session_id
            ))
        })?
    }

    async fn refresh_perp_contexts_if_needed(
        &mut self,
        manifest: &PaperSessionManifest,
        runtime_to_ms: i64,
    ) -> Result<(), ExecutionError> {
        if runtime_to_ms <= self.perp_context_to_ms
            || (self.perp.is_none() && self.portfolio_perp_contexts.is_empty())
        {
            return Ok(());
        }

        let compiled = self.compiled.clone();
        let execution_aliases = manifest.config.execution_source_aliases.clone();
        let endpoints = manifest.endpoints.clone();
        let options = PerpBootstrapOptions {
            leverage: manifest.config.leverage,
            margin_mode: manifest
                .config
                .margin_mode
                .unwrap_or(PerpMarginMode::Isolated),
            base_interval: self.feed_plan.base_interval,
            from_ms: self.feed_plan.warmup_from_ms,
            to_ms: runtime_to_ms,
        };
        let (perp, perp_context, portfolio_perp_contexts) =
            tokio::task::spawn_blocking(move || {
                resolve_perp_contexts(&compiled, &execution_aliases, options, &endpoints)
            })
            .await
            .map_err(|err| {
                ExecutionError::Runtime(format!(
                    "paper session `{}` perp context refresh task failed: {err}",
                    manifest.session_id
                ))
            })??;
        self.perp = perp;
        self.perp_context = perp_context;
        self.portfolio_perp_contexts = portfolio_perp_contexts;
        self.perp_context_to_ms = runtime_to_ms;
        Ok(())
    }
}

pub(crate) async fn process_paper_session(
    session: &mut LoadedPaperSession,
    manifest: &PaperSessionManifest,
    hub: &FeedHub,
    now_ms: i64,
) -> Result<PaperSessionManifest, ExecutionError> {
    let mut manifest = manifest.clone();
    if manifest.stop_requested {
        return mark_stopped(&mut manifest, "paper session stopped");
    }

    let feed_snapshots = hub.feed_snapshots_for_plan(&session.feed_plan, now_ms);
    let required_feeds = hub.required_feeds_for_plan(&session.feed_plan, now_ms);
    manifest.warmup_from_ms = Some(session.feed_plan.warmup_from_ms);
    manifest.feed_summary = hub.feed_summary_for_plan(&session.feed_plan);
    manifest.required_feeds = required_feeds;

    if manifest.feed_summary.history_ready_feeds < manifest.feed_summary.total_feeds {
        update_manifest_status(
            &mut manifest,
            ExecutionSessionStatus::ArmingHistory,
            ExecutionSessionHealth::Starting,
            None,
            "paper session bootstrapping history",
        )?;
        return Ok(manifest);
    }

    if !hub.fully_armed(&session.feed_plan) {
        update_manifest_status(
            &mut manifest,
            ExecutionSessionStatus::ArmingLive,
            ExecutionSessionHealth::WarmingUp,
            None,
            "paper session waiting for live feeds",
        )?;
        return Ok(manifest);
    }

    let Some((runtime, runtime_to_ms)) =
        hub.build_runtime(&session.compiled, &session.feed_plan, now_ms)?
    else {
        update_manifest_status(
            &mut manifest,
            ExecutionSessionStatus::ArmingLive,
            ExecutionSessionHealth::WarmingUp,
            None,
            "paper session waiting for closed candles",
        )?;
        return Ok(manifest);
    };

    session
        .refresh_perp_contexts_if_needed(&manifest, runtime_to_ms)
        .await?;

    let live_health = infer_health(&feed_snapshots);
    if manifest.latest_runtime_to_ms == Some(runtime_to_ms)
        && matches!(
            manifest.status,
            ExecutionSessionStatus::Live | ExecutionSessionStatus::ArmingLive
        )
        && load_paper_session_snapshot(&manifest.session_id).is_ok()
    {
        manifest.health = live_health;
        manifest.status = ExecutionSessionStatus::Live;
        manifest.updated_at_ms = now_ms;
        persist_session_manifest(&manifest)?;
        if let Some(export) = load_paper_session_export(&manifest.session_id)
            .ok()
            .and_then(|export| export.latest_result)
        {
            let snapshot = render_snapshot_from_result(
                &manifest,
                &export,
                runtime_to_ms,
                now_ms,
                &feed_snapshots,
            );
            persist_session_snapshot(&manifest.session_id, &snapshot)?;
        }
        return Ok(manifest);
    }

    let result = run_backtest_with_sources(
        &session.compiled,
        runtime,
        VmLimits {
            max_instructions_per_bar: manifest.config.vm_limits.max_instructions_per_bar,
            max_history_capacity: manifest.config.vm_limits.max_history_capacity,
        },
        BacktestConfig {
            execution_source_alias: manifest
                .config
                .execution_source_aliases
                .first()
                .cloned()
                .ok_or_else(|| ExecutionError::InvalidConfig {
                    message: "paper session has no execution aliases".to_string(),
                })?,
            portfolio_execution_aliases: if manifest.config.execution_source_aliases.len() > 1 {
                manifest.config.execution_source_aliases.clone()
            } else {
                Vec::new()
            },
            spot_virtual_rebalance: false,
            activation_time_ms: Some(manifest.start_time_ms),
            initial_capital: manifest.config.initial_capital,
            maker_fee_bps: manifest.config.maker_fee_bps,
            taker_fee_bps: manifest.config.taker_fee_bps,
            execution_fee_schedules: manifest.config.execution_fee_schedules.clone(),
            slippage_bps: manifest.config.slippage_bps,
            diagnostics_detail: manifest.config.diagnostics_detail,
            perp: session.perp.clone(),
            perp_context: session.perp_context.clone(),
            portfolio_perp_contexts: session.portfolio_perp_contexts.clone(),
        },
    );

    match result {
        Ok(result) => {
            manifest.latest_runtime_to_ms = Some(runtime_to_ms);
            update_manifest_status(
                &mut manifest,
                ExecutionSessionStatus::Live,
                live_health,
                None,
                "paper session is live",
            )?;
            let snapshot = render_snapshot_from_result(
                &manifest,
                &result,
                runtime_to_ms,
                now_ms,
                &feed_snapshots,
            );
            persist_session_result(&manifest.session_id, &result)?;
            persist_session_snapshot(&manifest.session_id, &snapshot)?;
            let update_message = format!(
                "paper session updated: runtime_to={} trades={} fills={} open_positions={} live_feeds={}/{}",
                runtime_to_ms,
                result.summary.trade_count,
                result.fills.len(),
                result.open_positions.len(),
                manifest.feed_summary.live_ready_feeds,
                manifest.feed_summary.total_feeds,
            );
            append_log_event(
                &manifest.session_id,
                &PaperSessionLogEvent {
                    time_ms: now_ms,
                    status: manifest.status,
                    health: manifest.health,
                    message: update_message,
                    latest_runtime_to_ms: manifest.latest_runtime_to_ms,
                },
            )?;
            debug_fields(
                "paper.session.updated",
                "Paper session processed a new runtime window",
                vec![
                    LogField::string("session_id", manifest.session_id.clone()),
                    LogField::i64("runtime_to_ms", runtime_to_ms),
                    LogField::u64("trade_count", result.summary.trade_count as u64),
                    LogField::u64("fill_count", result.fills.len() as u64),
                    LogField::u64("open_positions", result.open_positions.len() as u64),
                    LogField::u64(
                        "live_ready_feeds",
                        manifest.feed_summary.live_ready_feeds as u64,
                    ),
                    LogField::u64("total_feeds", manifest.feed_summary.total_feeds as u64),
                ],
            );
            Ok(manifest)
        }
        Err(BacktestError::MissingPerpMarkFeed { alias }) => {
            defer_perp_mark_alignment(session, &mut manifest, runtime_to_ms, &alias)
        }
        Err(err) => {
            let err = ExecutionError::Runtime(err.to_string());
            manifest.status = ExecutionSessionStatus::Failed;
            manifest.health = ExecutionSessionHealth::Failed;
            manifest.updated_at_ms = now_ms;
            manifest.failure_message = Some(err.to_string());
            persist_session_manifest(&manifest)?;
            error_fields(
                "paper.session.failed",
                "Paper session failed while processing runtime",
                vec![
                    LogField::string("session_id", manifest.session_id.clone()),
                    LogField::string("error", err.to_string()),
                    LogField::string(
                        "script_path",
                        manifest.script_path.clone().unwrap_or_default(),
                    ),
                    LogField::u64(
                        "live_ready_feeds",
                        manifest.feed_summary.live_ready_feeds as u64,
                    ),
                    LogField::u64("total_feeds", manifest.feed_summary.total_feeds as u64),
                ],
            );
            append_log_event(
                &manifest.session_id,
                &PaperSessionLogEvent {
                    time_ms: now_ms,
                    status: manifest.status,
                    health: manifest.health,
                    message: err.to_string(),
                    latest_runtime_to_ms: manifest.latest_runtime_to_ms,
                },
            )?;
            Ok(manifest)
        }
    }
}

fn defer_perp_mark_alignment(
    session: &mut LoadedPaperSession,
    manifest: &mut PaperSessionManifest,
    runtime_to_ms: i64,
    alias: &str,
) -> Result<PaperSessionManifest, ExecutionError> {
    session.perp_context_to_ms = session
        .perp_context_to_ms
        .min(runtime_to_ms.saturating_sub(1));
    let message =
        format!("paper session waiting for aligned perp mark bars for execution `{alias}`");
    update_manifest_status(
        manifest,
        ExecutionSessionStatus::ArmingLive,
        ExecutionSessionHealth::WarmingUp,
        None,
        &message,
    )?;
    warn_fields(
        "paper.session.waiting_perp_mark_bars",
        "Paper session waiting for aligned perp mark bars",
        vec![
            LogField::string("session_id", manifest.session_id.clone()),
            LogField::string("execution_alias", alias.to_string()),
            LogField::i64("runtime_to_ms", runtime_to_ms),
        ],
    );
    Ok(manifest.clone())
}

fn infer_health(feed_snapshots: &[super::PaperFeedSnapshot]) -> ExecutionSessionHealth {
    if feed_snapshots.is_empty() {
        return ExecutionSessionHealth::Live;
    }
    let healthy = feed_snapshots.iter().all(|feed| {
        feed.top_of_book
            .as_ref()
            .is_some_and(|snapshot| snapshot.state == FeedSnapshotState::Live)
    });
    if healthy {
        ExecutionSessionHealth::Live
    } else {
        ExecutionSessionHealth::Degraded
    }
}

fn mark_stopped(
    manifest: &mut PaperSessionManifest,
    message: &str,
) -> Result<PaperSessionManifest, ExecutionError> {
    manifest.status = ExecutionSessionStatus::Stopped;
    manifest.health = ExecutionSessionHealth::Stopped;
    manifest.updated_at_ms = super::now_ms();
    persist_session_manifest(manifest)?;
    append_log_event(
        &manifest.session_id,
        &PaperSessionLogEvent {
            time_ms: manifest.updated_at_ms,
            status: manifest.status,
            health: manifest.health,
            message: message.to_string(),
            latest_runtime_to_ms: manifest.latest_runtime_to_ms,
        },
    )?;
    warn_fields(
        "paper.session.stopped",
        "Paper session stopped",
        vec![LogField::string("session_id", manifest.session_id.clone())],
    );
    Ok(manifest.clone())
}

fn update_manifest_status(
    manifest: &mut PaperSessionManifest,
    status: ExecutionSessionStatus,
    health: ExecutionSessionHealth,
    failure_message: Option<String>,
    message: &str,
) -> Result<bool, ExecutionError> {
    let changed = manifest.status != status
        || manifest.health != health
        || manifest.failure_message != failure_message;
    manifest.status = status;
    manifest.health = health;
    manifest.failure_message = failure_message;
    manifest.updated_at_ms = super::now_ms();
    persist_session_manifest(manifest)?;
    if changed {
        append_log_event(
            &manifest.session_id,
            &PaperSessionLogEvent {
                time_ms: manifest.updated_at_ms,
                status,
                health,
                message: message.to_string(),
                latest_runtime_to_ms: manifest.latest_runtime_to_ms,
            },
        )?;
        info_fields(
            "paper.session.transition",
            "Paper session state changed",
            vec![
                LogField::string("session_id", manifest.session_id.clone()),
                LogField::string("status", format!("{:?}", status)),
                LogField::string("health", format!("{:?}", health)),
                LogField::string("message", message.to_string()),
            ],
        );
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::{defer_perp_mark_alignment, LoadedPaperSession};
    use crate::backtest::{DiagnosticsDetailMode, PerpMarginMode};
    use crate::compile;
    use crate::exchange::ExchangeEndpoints;
    use crate::execution::feed_hub::build_session_feed_plan;
    use crate::execution::market_data::{resolve_perp_contexts, PerpBootstrapOptions};
    use crate::execution::{
        ExecutionMode, ExecutionSessionHealth, ExecutionSessionStatus, PaperFeedSummary,
        PaperSessionConfig, PaperSessionManifest,
    };
    use crate::runtime::VmLimits;
    use mockito::{Matcher, Server};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn sample_manifest(endpoints: ExchangeEndpoints) -> PaperSessionManifest {
        PaperSessionManifest {
            session_id: "paper-test".to_string(),
            mode: ExecutionMode::Paper,
            created_at_ms: 1_704_067_200_000,
            updated_at_ms: 1_704_067_200_000,
            start_time_ms: 1_704_067_200_000,
            status: ExecutionSessionStatus::Queued,
            health: ExecutionSessionHealth::Starting,
            stop_requested: false,
            failure_message: None,
            script_path: None,
            script_sha256: "test".to_string(),
            base_interval: crate::Interval::Hour1,
            history_capacity: 16,
            endpoints,
            config: PaperSessionConfig {
                execution_source_aliases: vec!["exec".to_string()],
                initial_capital: 10_000.0,
                maker_fee_bps: 2.0,
                taker_fee_bps: 5.0,
                execution_fee_schedules: Default::default(),
                slippage_bps: 1.0,
                diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
                leverage: Some(10.0),
                margin_mode: Some(PerpMarginMode::Isolated),
                vm_limits: VmLimits::default(),
            },
            execution_sources: Vec::new(),
            feed_summary: PaperFeedSummary::default(),
            required_feeds: Vec::new(),
            warmup_from_ms: None,
            latest_runtime_to_ms: None,
        }
    }

    #[test]
    fn refresh_perp_contexts_extends_mark_bars_for_new_runtime_window() {
        let mut server = Server::new();
        let _mark_bars = server
            .mock("GET", "/fapi/v1/markPriceKlines")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("interval".into(), "1h".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [1704067200000_i64, "100.0", "101.0", "99.0", "100.5", "0"],
                    [1704070800000_i64, "100.5", "102.0", "100.0", "101.5", "0"],
                    [1704074400000_i64, "101.5", "103.0", "101.0", "102.5", "0"]
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
        let _server_time = server
            .mock("GET", "/fapi/v1/time")
            .with_status(200)
            .with_body(json!({ "serverTime": 1704074400000_i64 }).to_string())
            .create();
        let _leverage_bracket = server
            .mock("GET", "/fapi/v1/leverageBracket")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
                Matcher::UrlEncoded("timestamp".into(), "1704074400000".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([{
                    "symbol": "BTCUSDT",
                    "brackets": [{
                        "initialLeverage": 20,
                        "notionalFloor": "0",
                        "notionalCap": "100000",
                        "maintMarginRatio": "0.025",
                        "cum": "0"
                    }]
                }])
                .to_string(),
            )
            .create();

        let source = "interval 1h
source perp = binance.usdm(\"BTCUSDT\")
execution exec = binance.usdm(\"BTCUSDT\")
plot(perp.close)";
        let compiled = compile(source).expect("compile");
        let endpoints = ExchangeEndpoints {
            binance_usdm_base_url: server.url(),
            ..ExchangeEndpoints::default()
        };
        let manifest = sample_manifest(endpoints.clone());
        let feed_plan = build_session_feed_plan(
            &compiled,
            &manifest.config.execution_source_aliases,
            manifest.start_time_ms,
            &endpoints,
        )
        .expect("feed plan");
        let initial_runtime_to_ms = 1_704_070_800_000;
        let (perp, perp_context, portfolio_perp_contexts) = resolve_perp_contexts(
            &compiled,
            &manifest.config.execution_source_aliases,
            PerpBootstrapOptions {
                leverage: manifest.config.leverage,
                margin_mode: manifest.config.margin_mode.expect("margin mode"),
                base_interval: feed_plan.base_interval,
                from_ms: feed_plan.warmup_from_ms,
                to_ms: initial_runtime_to_ms,
            },
            &endpoints,
        )
        .expect("initial contexts");
        let initial_mark_bars = perp_context.as_ref().expect("perp context").mark_bars.len();
        assert_eq!(initial_mark_bars, 1);

        let mut session = LoadedPaperSession {
            compiled,
            feed_plan,
            perp,
            perp_context,
            portfolio_perp_contexts,
            perp_context_to_ms: initial_runtime_to_ms,
        };

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        runtime
            .block_on(session.refresh_perp_contexts_if_needed(&manifest, 1_704_078_000_000))
            .expect("refresh contexts");

        let refreshed = session.perp_context.expect("refreshed perp context");
        assert_eq!(refreshed.mark_bars.len(), 3);
        assert_eq!(refreshed.mark_bars[2].time as i64, 1_704_074_400_000);
        assert_eq!(session.perp_context_to_ms, 1_704_078_000_000);
    }

    #[test]
    fn defer_perp_mark_alignment_keeps_session_warming_and_retries_window() {
        let state_dir = tempfile::tempdir().expect("tempdir");
        std::env::set_var("PALMSCRIPT_EXECUTION_STATE_DIR", state_dir.path());

        let compiled = compile(
            "interval 1m
source perp = binance.usdm(\"BTCUSDT\")
execution exec = binance.usdm(\"BTCUSDT\")
plot(perp.close)",
        )
        .expect("compile");
        let manifest = sample_manifest(ExchangeEndpoints::default());
        let feed_plan = build_session_feed_plan(
            &compiled,
            &manifest.config.execution_source_aliases,
            manifest.start_time_ms,
            &manifest.endpoints,
        )
        .expect("feed plan");
        let mut session = LoadedPaperSession {
            compiled,
            feed_plan,
            perp: None,
            perp_context: None,
            portfolio_perp_contexts: BTreeMap::new(),
            perp_context_to_ms: 1_704_067_500_000,
        };
        let mut manifest = PaperSessionManifest {
            session_id: "paper-wait".to_string(),
            script_path: Some(PathBuf::from("strategy.ps").display().to_string()),
            ..manifest
        };

        let deferred =
            defer_perp_mark_alignment(&mut session, &mut manifest, 1_704_067_500_000, "exec")
                .expect("defer should succeed");

        assert_eq!(deferred.status, ExecutionSessionStatus::ArmingLive);
        assert_eq!(deferred.health, ExecutionSessionHealth::WarmingUp);
        assert_eq!(deferred.failure_message, None);
        assert_eq!(session.perp_context_to_ms, 1_704_067_499_999);

        std::env::remove_var("PALMSCRIPT_EXECUTION_STATE_DIR");
    }
}
