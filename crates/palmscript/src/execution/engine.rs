use crate::backtest::{BacktestConfig, PerpMarginMode};
use crate::compiler::compile;
use crate::run_backtest_with_sources;
use crate::runtime::VmLimits;

use super::market_data::{bootstrap_runtime, resolve_execution_sources};
use super::venue::validate_paper_source;
use super::{
    append_log_event, load_paper_session_manifest, load_paper_session_script,
    load_paper_session_snapshot, persist_session_manifest, persist_session_result,
    persist_session_snapshot, render_snapshot_from_result, ExecutionError, ExecutionSessionHealth,
    ExecutionSessionStatus, FeedSnapshotState, PaperFeedSnapshot, PaperSessionLogEvent,
    PaperSessionManifest,
};

pub(crate) fn process_paper_session(
    session_id: &str,
    now_ms: i64,
    feed_snapshots: &[PaperFeedSnapshot],
) -> Result<PaperSessionManifest, ExecutionError> {
    let mut manifest = load_paper_session_manifest(session_id)?;
    if manifest.stop_requested {
        return mark_stopped(&mut manifest, "paper session stopped");
    }

    let source = load_paper_session_script(session_id)?;
    let compiled = compile(&source).map_err(|err| ExecutionError::Compile(err.to_string()))?;
    let execution_sources =
        resolve_execution_sources(&compiled, &manifest.config.execution_source_aliases)?;
    for source in &execution_sources {
        validate_paper_source(source)?;
    }

    update_manifest_status(
        &mut manifest,
        ExecutionSessionStatus::Starting,
        ExecutionSessionHealth::Starting,
        None,
        "paper session starting",
    )?;

    let bootstrap = bootstrap_runtime(
        &compiled,
        &manifest.config.execution_source_aliases,
        manifest.config.leverage,
        manifest
            .config
            .margin_mode
            .or(Some(PerpMarginMode::Isolated)),
        manifest.start_time_ms,
        now_ms,
        &manifest.endpoints,
    )?;

    manifest.warmup_from_ms = Some(bootstrap.warmup_from_ms);
    let live_health = infer_health(feed_snapshots);
    if manifest.latest_runtime_to_ms == Some(bootstrap.runtime_to_ms)
        && matches!(
            manifest.status,
            ExecutionSessionStatus::Live | ExecutionSessionStatus::WarmingUp
        )
        && load_paper_session_snapshot(session_id).is_ok()
    {
        manifest.health = live_health;
        manifest.status = ExecutionSessionStatus::Live;
        manifest.updated_at_ms = now_ms;
        persist_session_manifest(&manifest)?;
        return Ok(manifest);
    }

    update_manifest_status(
        &mut manifest,
        ExecutionSessionStatus::WarmingUp,
        ExecutionSessionHealth::WarmingUp,
        None,
        "paper session warmup fetched",
    )?;

    let result = run_backtest_with_sources(
        &compiled,
        bootstrap.runtime,
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
            perp: bootstrap.perp,
            perp_context: bootstrap.perp_context,
            portfolio_perp_contexts: bootstrap.portfolio_perp_contexts,
        },
    )
    .map_err(|err| ExecutionError::Runtime(err.to_string()));

    match result {
        Ok(result) => {
            manifest.latest_runtime_to_ms = Some(bootstrap.runtime_to_ms);
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
                bootstrap.runtime_to_ms,
                now_ms,
                feed_snapshots,
            );
            persist_session_result(session_id, &result)?;
            persist_session_snapshot(session_id, &snapshot)?;
            append_log_event(
                session_id,
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
                session_id,
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

fn infer_health(feed_snapshots: &[PaperFeedSnapshot]) -> ExecutionSessionHealth {
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
