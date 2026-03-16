use crate::backtest::{BacktestConfig, PerpBacktestConfig, PerpBacktestContext, PerpMarginMode};
use crate::compiler::compile;
use crate::run_backtest_with_sources;
use crate::runtime::VmLimits;

use std::collections::BTreeMap;

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
}

impl LoadedPaperSession {
    pub(crate) fn load(
        manifest: &PaperSessionManifest,
        now_ms: i64,
    ) -> Result<Self, ExecutionError> {
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
        })
    }

    pub(crate) fn feed_plan(&self) -> &SessionFeedPlan {
        &self.feed_plan
    }
}

pub(crate) fn process_paper_session(
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
    )
    .map_err(|err| ExecutionError::Runtime(err.to_string()));

    match result {
        Ok(result) => {
            manifest.latest_runtime_to_ms = Some(runtime_to_ms);
            update_manifest_status(
                &mut manifest,
                ExecutionSessionStatus::Live,
                live_health,
                None,
                "paper session updated",
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
            append_log_event(
                &manifest.session_id,
                &PaperSessionLogEvent {
                    time_ms: now_ms,
                    status: manifest.status,
                    health: manifest.health,
                    message: "paper session updated".to_string(),
                    latest_runtime_to_ms: manifest.latest_runtime_to_ms,
                },
            )?;
            Ok(manifest)
        }
        Err(err) => {
            manifest.status = ExecutionSessionStatus::Failed;
            manifest.health = ExecutionSessionHealth::Failed;
            manifest.updated_at_ms = now_ms;
            manifest.failure_message = Some(err.to_string());
            persist_session_manifest(&manifest)?;
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
    Ok(manifest.clone())
}

fn update_manifest_status(
    manifest: &mut PaperSessionManifest,
    status: ExecutionSessionStatus,
    health: ExecutionSessionHealth,
    failure_message: Option<String>,
    message: &str,
) -> Result<(), ExecutionError> {
    manifest.status = status;
    manifest.health = health;
    manifest.failure_message = failure_message;
    manifest.updated_at_ms = super::now_ms();
    persist_session_manifest(manifest)?;
    append_log_event(
        &manifest.session_id,
        &PaperSessionLogEvent {
            time_ms: manifest.updated_at_ms,
            status,
            health,
            message: message.to_string(),
            latest_runtime_to_ms: manifest.latest_runtime_to_ms,
        },
    )
}
