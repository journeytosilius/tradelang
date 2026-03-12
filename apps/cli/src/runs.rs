use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use palmscript::{
    compile_with_input_overrides, fetch_source_runtime_config, run_optimize_with_source_resume,
    BacktestConfig, CompiledProgram, ExchangeEndpoints, OptimizeCandidateSummary, OptimizeConfig,
    OptimizeEvaluationSummary, OptimizeHoldoutConfig, OptimizeHoldoutResult, OptimizeObjective,
    OptimizeParamSpace, OptimizePreset, OptimizeProgressEvent, OptimizeProgressListener,
    OptimizeProgressState, OptimizeResult, OptimizeResumeState, OptimizeRunner,
    OptimizeScheduledBatch, PerpBacktestConfig, PerpBacktestContext, PerpMarginMode, VmLimits,
    WalkForwardConfig,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::args::{
    BacktestMarginMode, OptimizeRunArgs, RunLookupArgs, RunSubmitCommand, RunsBestArgs,
    RunsCommand, RunsListArgs, RunsServeArgs,
};
use crate::commands::{
    default_parallel_workers, default_startup_trials, hash_source, load_preset, load_source,
    map_optimize_objective, map_optimize_runner, resolve_execution_source_alias,
    resolve_optimize_holdout, resolve_optimize_params, resolve_perp_context, write_optimize_preset,
    PerpCliOptions,
};
use crate::diagnostics::format_compile_error;
use crate::format::render_optimize_text;

const RUN_KIND_OPTIMIZE: &str = "optimize";
const RUN_STATUS_QUEUED: &str = "queued";
const RUN_STATUS_RUNNING: &str = "running";
const RUN_STATUS_COMPLETED: &str = "completed";
const RUN_STATUS_FAILED: &str = "failed";
const RUN_STATUS_CANCELED: &str = "canceled";
const DAEMON_RECOVERY_STALE_MS: i64 = 30_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OptimizeJobSpec {
    script_path: Option<String>,
    script_sha256: String,
    git_commit: Option<String>,
    from: i64,
    to: i64,
    execution_source_alias: String,
    initial_capital: f64,
    fee_bps: f64,
    slippage_bps: f64,
    leverage: Option<f64>,
    margin_mode: Option<PerpMarginMode>,
    train_bars: Option<usize>,
    test_bars: Option<usize>,
    step_bars: Option<usize>,
    #[serde(default)]
    holdout: Option<OptimizeHoldoutConfig>,
    params: Vec<OptimizeParamSpace>,
    runner: OptimizeRunner,
    objective: OptimizeObjective,
    trials: usize,
    startup_trials: usize,
    seed: u64,
    workers: usize,
    top_n: usize,
    base_input_overrides: BTreeMap<String, f64>,
    max_instructions_per_bar: usize,
    max_history_capacity: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RunManifest {
    run_id: String,
    run_kind: String,
    status: String,
    artifact_dir: String,
    script_path: Option<String>,
    script_sha256: String,
    git_commit: Option<String>,
    created_at_ms: i64,
    started_at_ms: Option<i64>,
    updated_at_ms: i64,
    heartbeat_at_ms: Option<i64>,
    completed_at_ms: Option<i64>,
    worker_pid: Option<u32>,
    error_message: Option<String>,
    candidate_count: usize,
    completed_trials: usize,
    best_candidate: Option<OptimizeCandidateSummary>,
    top_candidates: Vec<OptimizeCandidateSummary>,
    #[serde(default)]
    holdout_result: Option<OptimizeHoldoutResult>,
    pending_batch: Option<OptimizeScheduledBatch>,
    job: OptimizeJobSpec,
}

#[derive(Clone, Debug)]
struct RunRecord {
    run_id: String,
    status: String,
    artifact_dir: String,
    script_sha256: String,
    config_json: String,
    created_at_ms: i64,
    started_at_ms: Option<i64>,
    updated_at_ms: i64,
    heartbeat_at_ms: Option<i64>,
    completed_at_ms: Option<i64>,
    worker_pid: Option<u32>,
    error_message: Option<String>,
    candidate_count: usize,
    completed_trials: usize,
    best_objective_score: Option<f64>,
    best_overrides_json: Option<String>,
    cancel_requested: bool,
    pending_batch_json: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CandidatePayload {
    trial_id: usize,
    input_overrides: BTreeMap<String, f64>,
    objective_score: f64,
    summary_kind: &'static str,
    ending_equity: f64,
    total_return: f64,
    max_drawdown: f64,
    entered_top_n: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EventLine<T> {
    sequence: i64,
    event_type: String,
    created_at_ms: i64,
    payload: T,
}

struct RunProgressWriter {
    conn: Connection,
    run_id: String,
    artifact_dir: PathBuf,
    job: OptimizeJobSpec,
    config: OptimizeConfig,
    event_sequence: i64,
}

impl OptimizeProgressListener for RunProgressWriter {
    fn on_event(
        &mut self,
        event: OptimizeProgressEvent,
        state: &OptimizeProgressState,
    ) -> Result<(), String> {
        let now = now_ms();
        self.touch_running_row(now, state)?;
        match event {
            OptimizeProgressEvent::BatchScheduled { batch } => {
                let payload = serde_json::to_string(&batch).map_err(|err| err.to_string())?;
                self.conn
                    .execute(
                        "UPDATE runs SET pending_batch_json = ?, updated_at_ms = ?, heartbeat_at_ms = ? WHERE id = ?",
                        params![payload, now, now, self.run_id],
                    )
                    .map_err(|err| err.to_string())?;
                self.append_event("batch_scheduled", &batch, now)?;
            }
            OptimizeProgressEvent::CandidateCompleted {
                candidate,
                entered_top_n,
            } => {
                let payload = candidate_payload(&candidate, entered_top_n);
                let summary_json =
                    serde_json::to_string(&candidate.summary).map_err(|err| err.to_string())?;
                let overrides_json = serde_json::to_string(&candidate.input_overrides)
                    .map_err(|err| err.to_string())?;
                self.conn
                    .execute(
                        "INSERT OR REPLACE INTO run_candidates (run_id, trial_id, input_overrides_json, objective_score, summary_kind, summary_json, entered_top_n, created_at_ms)
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                        params![
                            self.run_id,
                            candidate.trial_id as i64,
                            overrides_json,
                            candidate.objective_score,
                            summary_kind(&candidate.summary),
                            summary_json,
                            entered_top_n as i64,
                            now
                        ],
                    )
                    .map_err(|err| err.to_string())?;
                self.append_event("candidate_completed", &payload, now)?;
            }
            OptimizeProgressEvent::BestCandidateImproved { candidate } => {
                self.write_json_artifact("best_candidate.json", &candidate)?;
                self.write_json_artifact("top_candidates.json", &state.top_candidates)?;
                self.write_best_preset(&candidate, &state.top_candidates)?;
                let overrides_json = serde_json::to_string(&candidate.input_overrides)
                    .map_err(|err| err.to_string())?;
                self.conn
                    .execute(
                        "UPDATE runs SET best_objective_score = ?, best_overrides_json = ?, updated_at_ms = ?, heartbeat_at_ms = ? WHERE id = ?",
                        params![
                            candidate.objective_score,
                            overrides_json,
                            now,
                            now,
                            self.run_id
                        ],
                    )
                    .map_err(|err| err.to_string())?;
                self.append_event("best_candidate_improved", &candidate, now)?;
            }
            OptimizeProgressEvent::CheckpointWritten => {
                self.write_json_artifact("top_candidates.json", &state.top_candidates)?;
                self.write_manifest(RUN_STATUS_RUNNING, state, now, None)?;
                self.conn
                    .execute(
                        "UPDATE runs SET pending_batch_json = NULL, updated_at_ms = ?, heartbeat_at_ms = ? WHERE id = ?",
                        params![now, now, self.run_id],
                    )
                    .map_err(|err| err.to_string())?;
                self.append_event("checkpoint_written", &state, now)?;
            }
            OptimizeProgressEvent::Canceled => {
                self.append_event("canceled", &state, now)?;
            }
        }
        Ok(())
    }

    fn should_cancel(&mut self) -> Result<bool, String> {
        self.conn
            .query_row(
                "SELECT cancel_requested FROM runs WHERE id = ?",
                params![self.run_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value != 0)
            .map_err(|err| err.to_string())
    }
}

pub fn run(mode: RunsCommand) -> Result<(), String> {
    match mode {
        RunsCommand::Serve(args) => run_serve(args),
        RunsCommand::Submit { job } => run_submit(*job),
        RunsCommand::Status(args) => run_status(args),
        RunsCommand::Show(args) => run_show(args),
        RunsCommand::Tail(args) => run_tail(args),
        RunsCommand::List(args) => run_list(args),
        RunsCommand::Cancel(args) => run_cancel(args),
        RunsCommand::Resume(args) => run_resume(args),
        RunsCommand::Best(args) => run_best(args),
    }
}

fn run_submit(job: RunSubmitCommand) -> Result<(), String> {
    match job {
        RunSubmitCommand::Optimize(args) => submit_optimize(args),
    }
}

fn submit_optimize(args: OptimizeRunArgs) -> Result<(), String> {
    let state_root = ensure_state_root()?;
    let conn = open_runs_db(&state_root)?;
    init_schema(&conn)?;

    let source = load_source(&args.script)?;
    let script_sha256 = hash_source(&source);
    let preset = load_preset(&source, &args.script, args.preset.as_deref())?;
    let base_input_overrides = preset
        .as_ref()
        .map(|preset| preset.best_input_overrides.clone())
        .unwrap_or_default();
    let compiled = compile_with_input_overrides(&source, &base_input_overrides)
        .map_err(|err| format_compile_error(&args.script, &err))?;
    if compiled.program.declared_sources.is_empty() {
        return Err("runs submit optimize requires at least one `source` declaration".to_string());
    }
    let params = resolve_optimize_params(&args, preset.as_ref(), &compiled)?;
    let execution_source_alias =
        resolve_execution_source_alias(&compiled, args.execution_source.clone())?;
    let spec = OptimizeJobSpec {
        script_path: Some(args.script.display().to_string()),
        script_sha256: script_sha256.clone(),
        git_commit: git_commit(),
        from: args.from,
        to: args.to,
        execution_source_alias,
        initial_capital: args.initial_capital,
        fee_bps: args.fee_bps,
        slippage_bps: args.slippage_bps,
        leverage: args.leverage,
        margin_mode: args.margin_mode.map(map_margin_mode),
        train_bars: args.train_bars,
        test_bars: args.test_bars,
        step_bars: args.step_bars,
        holdout: resolve_optimize_holdout(&args, preset.as_ref())?,
        params,
        runner: map_optimize_runner(args.runner),
        objective: map_optimize_objective(args.objective),
        trials: args.trials,
        startup_trials: args
            .startup_trials
            .unwrap_or_else(|| default_startup_trials(args.trials)),
        seed: args.seed,
        workers: args.workers.unwrap_or_else(default_parallel_workers),
        top_n: args.top,
        base_input_overrides,
        max_instructions_per_bar: args.max_instructions_per_bar,
        max_history_capacity: args.max_history_capacity,
    };
    let run_id = generate_run_id(&script_sha256);
    let artifact_dir = state_root.join("artifacts").join(&run_id);
    fs::create_dir_all(&artifact_dir).map_err(|err| err.to_string())?;
    fs::write(artifact_dir.join("script.ps"), &source).map_err(|err| err.to_string())?;
    File::create(artifact_dir.join("events.jsonl")).map_err(|err| err.to_string())?;
    let now = now_ms();
    conn.execute(
        "INSERT INTO runs (
            id, run_kind, status, artifact_dir, script_sha256, config_json, created_at_ms, queued_at_ms,
            updated_at_ms, candidate_count, completed_trials, worker_pid, cancel_requested, script_path
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, NULL, 0, ?)",
        params![
            run_id,
            RUN_KIND_OPTIMIZE,
            RUN_STATUS_QUEUED,
            artifact_dir.display().to_string(),
            script_sha256,
            serde_json::to_string(&spec).map_err(|err| err.to_string())?,
            now,
            now,
            now,
            spec.trials as i64,
            spec.script_path.clone(),
        ],
    )
    .map_err(|err| err.to_string())?;
    upsert_artifact(&conn, &run_id, "script.ps", &artifact_dir.join("script.ps"))?;
    upsert_artifact(
        &conn,
        &run_id,
        "events.jsonl",
        &artifact_dir.join("events.jsonl"),
    )?;
    write_manifest_file(
        &artifact_dir,
        &RunManifest {
            run_id: run_id.clone(),
            run_kind: RUN_KIND_OPTIMIZE.to_string(),
            status: RUN_STATUS_QUEUED.to_string(),
            artifact_dir: artifact_dir.display().to_string(),
            script_path: spec.script_path.clone(),
            script_sha256: spec.script_sha256.clone(),
            git_commit: spec.git_commit.clone(),
            created_at_ms: now,
            started_at_ms: None,
            updated_at_ms: now,
            heartbeat_at_ms: None,
            completed_at_ms: None,
            worker_pid: None,
            error_message: None,
            candidate_count: spec.trials,
            completed_trials: 0,
            best_candidate: None,
            top_candidates: Vec::new(),
            holdout_result: None,
            pending_batch: None,
            job: spec.clone(),
        },
    )?;
    upsert_artifact(
        &conn,
        &run_id,
        "manifest.json",
        &artifact_dir.join("manifest.json"),
    )?;
    append_event_record(&conn, &artifact_dir, &run_id, "queued", &run_id, now)?;
    println!("run_id={run_id}");
    println!("status={RUN_STATUS_QUEUED}");
    println!("artifact_dir={}", artifact_dir.display());
    if autostart_enabled() {
        let _ = spawn_background_server();
    }
    Ok(())
}

fn run_serve(args: RunsServeArgs) -> Result<(), String> {
    let state_root = ensure_state_root()?;
    let mut idle_loops = 0usize;
    loop {
        let processed = process_next_run(&state_root)?;
        if args.once {
            break;
        }
        if processed {
            idle_loops = 0;
        } else {
            idle_loops += 1;
            if idle_loops > 0 {
                thread::sleep(Duration::from_millis(args.poll_ms.max(50)));
            }
        }
    }
    Ok(())
}

fn process_next_run(state_root: &Path) -> Result<bool, String> {
    let conn = open_runs_db(state_root)?;
    init_schema(&conn)?;
    recover_stale_running_runs(&conn)?;
    let run = claim_next_queued_run(&conn)?;
    let Some(run) = run else {
        return Ok(false);
    };
    execute_optimize_run(state_root, run)?;
    Ok(true)
}

fn execute_optimize_run(state_root: &Path, run: RunRecord) -> Result<(), String> {
    let artifact_dir = PathBuf::from(&run.artifact_dir);
    let source_path = artifact_dir.join("script.ps");
    let source = fs::read_to_string(&source_path).map_err(|err| {
        format!(
            "failed to read run source `{}`: {err}",
            source_path.display()
        )
    })?;
    let source_hash = hash_source(&source);
    if source_hash != run.script_sha256 {
        mark_run_failed(
            state_root,
            &run.run_id,
            &artifact_dir,
            &format!(
                "stored script hash mismatch: expected {}, found {}",
                run.script_sha256, source_hash
            ),
        )?;
        return Ok(());
    }

    let job: OptimizeJobSpec =
        serde_json::from_str(&run.config_json).map_err(|err| err.to_string())?;
    let compiled =
        compile_with_input_overrides(&source, &job.base_input_overrides).map_err(|err| {
            format_compile_error(
                Path::new(job.script_path.as_deref().unwrap_or("script.ps")),
                &err,
            )
        })?;
    let endpoints = ExchangeEndpoints::from_env();
    let runtime = fetch_source_runtime_config(&compiled, job.from, job.to, &endpoints)
        .map_err(|err| format!("runs optimize runtime error: {err}"))?;
    let (perp, perp_context) = resolve_perp_from_job(&compiled, &job, &endpoints)?;
    let backtest = BacktestConfig {
        execution_source_alias: job.execution_source_alias.clone(),
        initial_capital: job.initial_capital,
        fee_bps: job.fee_bps,
        slippage_bps: job.slippage_bps,
        perp,
        perp_context,
    };
    let walk_forward = match job.runner {
        OptimizeRunner::WalkForward => Some(WalkForwardConfig {
            backtest: backtest.clone(),
            train_bars: job.train_bars.ok_or_else(|| {
                "stored optimize walk-forward run is missing train_bars".to_string()
            })?,
            test_bars: job.test_bars.ok_or_else(|| {
                "stored optimize walk-forward run is missing test_bars".to_string()
            })?,
            step_bars: job.step_bars.or(job.test_bars).ok_or_else(|| {
                "stored optimize walk-forward run is missing step_bars/test_bars".to_string()
            })?,
        }),
        OptimizeRunner::Backtest => None,
    };
    let config = OptimizeConfig {
        runner: job.runner,
        backtest: backtest.clone(),
        walk_forward,
        holdout: job.holdout.clone(),
        params: job.params.clone(),
        objective: job.objective,
        trials: job.trials,
        startup_trials: job.startup_trials,
        seed: job.seed,
        workers: job.workers,
        top_n: job.top_n,
        base_input_overrides: job.base_input_overrides.clone(),
    };
    let resume = load_resume_state(state_root, &run.run_id)?;
    let mut writer = RunProgressWriter::new(
        state_root,
        run.clone(),
        artifact_dir.clone(),
        job.clone(),
        config.clone(),
    )?;
    let result = run_optimize_with_source_resume(
        &source,
        runtime,
        VmLimits {
            max_instructions_per_bar: job.max_instructions_per_bar,
            max_history_capacity: job.max_history_capacity,
        },
        config,
        resume,
        Some(&mut writer),
    );
    match result {
        Ok(result) => finalize_completed_run(
            state_root,
            &run.run_id,
            &artifact_dir,
            &job,
            &source,
            &result,
        ),
        Err(palmscript::OptimizeError::Canceled) => {
            finalize_canceled_run(state_root, &run.run_id, &artifact_dir, &job, &source)
        }
        Err(err) => mark_run_failed(state_root, &run.run_id, &artifact_dir, &err.to_string()),
    }
}

fn run_status(args: RunLookupArgs) -> Result<(), String> {
    let state_root = ensure_state_root()?;
    let conn = open_runs_db(&state_root)?;
    let run = load_run(&conn, &args.run_id)?;
    print_status(&run);
    Ok(())
}

fn run_show(args: RunLookupArgs) -> Result<(), String> {
    let state_root = ensure_state_root()?;
    let conn = open_runs_db(&state_root)?;
    let run = load_run(&conn, &args.run_id)?;
    let manifest = load_manifest(Path::new(&run.artifact_dir))?;
    print_manifest(&manifest);
    Ok(())
}

fn run_tail(args: RunLookupArgs) -> Result<(), String> {
    let state_root = ensure_state_root()?;
    let conn = open_runs_db(&state_root)?;
    let mut last_sequence = 0i64;
    loop {
        let rows = load_events_since(&conn, &args.run_id, last_sequence)?;
        for (sequence, event_type, created_at_ms, payload_json) in rows {
            println!(
                "seq={sequence} time={created_at_ms} event={event_type} payload={payload_json}"
            );
            last_sequence = sequence;
        }
        let run = load_run(&conn, &args.run_id)?;
        if matches!(
            run.status.as_str(),
            RUN_STATUS_COMPLETED | RUN_STATUS_FAILED | RUN_STATUS_CANCELED
        ) {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

fn run_list(_args: RunsListArgs) -> Result<(), String> {
    let state_root = ensure_state_root()?;
    let conn = open_runs_db(&state_root)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, status, updated_at_ms, completed_trials, candidate_count, artifact_dir
             FROM runs ORDER BY created_at_ms DESC",
        )
        .map_err(|err| err.to_string())?;
    let mut rows = stmt.query([]).map_err(|err| err.to_string())?;
    while let Some(row) = rows.next().map_err(|err| err.to_string())? {
        let run_id: String = row.get(0).map_err(|err| err.to_string())?;
        let status: String = row.get(1).map_err(|err| err.to_string())?;
        let updated_at_ms: i64 = row.get(2).map_err(|err| err.to_string())?;
        let completed_trials: i64 = row.get(3).map_err(|err| err.to_string())?;
        let candidate_count: i64 = row.get(4).map_err(|err| err.to_string())?;
        let artifact_dir: String = row.get(5).map_err(|err| err.to_string())?;
        println!(
            "run_id={run_id} status={status} progress={completed_trials}/{candidate_count} updated_at_ms={updated_at_ms} artifact_dir={artifact_dir}"
        );
    }
    Ok(())
}

fn run_cancel(args: RunLookupArgs) -> Result<(), String> {
    let state_root = ensure_state_root()?;
    let conn = open_runs_db(&state_root)?;
    let run = load_run(&conn, &args.run_id)?;
    let now = now_ms();
    match run.status.as_str() {
        RUN_STATUS_QUEUED => {
            conn.execute(
                "UPDATE runs SET status = ?, updated_at_ms = ?, completed_at_ms = ?, cancel_requested = 1 WHERE id = ?",
                params![RUN_STATUS_CANCELED, now, now, args.run_id],
            )
            .map_err(|err| err.to_string())?;
            append_event_record(
                &conn,
                Path::new(&run.artifact_dir),
                &run.run_id,
                "canceled",
                &run.run_id,
                now,
            )?;
        }
        RUN_STATUS_RUNNING => {
            conn.execute(
                "UPDATE runs SET cancel_requested = 1, updated_at_ms = ? WHERE id = ?",
                params![now, args.run_id],
            )
            .map_err(|err| err.to_string())?;
            append_event_record(
                &conn,
                Path::new(&run.artifact_dir),
                &run.run_id,
                "cancel_requested",
                &run.run_id,
                now,
            )?;
        }
        _ => return Err(format!("run `{}` is already {}", args.run_id, run.status)),
    }
    println!("run_id={} status={}", args.run_id, RUN_STATUS_CANCELED);
    Ok(())
}

fn run_resume(args: RunLookupArgs) -> Result<(), String> {
    let state_root = ensure_state_root()?;
    let conn = open_runs_db(&state_root)?;
    let run = load_run(&conn, &args.run_id)?;
    let source = fs::read_to_string(Path::new(&run.artifact_dir).join("script.ps"))
        .map_err(|err| err.to_string())?;
    let source_hash = hash_source(&source);
    if source_hash != run.script_sha256 {
        return Err(format!(
            "run `{}` cannot be resumed because the stored script hash does not match its source snapshot",
            args.run_id
        ));
    }
    let now = now_ms();
    conn.execute(
        "UPDATE runs SET status = ?, updated_at_ms = ?, completed_at_ms = NULL, error_message = NULL, cancel_requested = 0 WHERE id = ?",
        params![RUN_STATUS_QUEUED, now, args.run_id],
    )
    .map_err(|err| err.to_string())?;
    append_event_record(
        &conn,
        Path::new(&run.artifact_dir),
        &run.run_id,
        "resumed",
        &run.run_id,
        now,
    )?;
    if autostart_enabled() {
        let _ = spawn_background_server();
    }
    println!("run_id={} status={}", args.run_id, RUN_STATUS_QUEUED);
    Ok(())
}

fn run_best(args: RunsBestArgs) -> Result<(), String> {
    let state_root = ensure_state_root()?;
    let conn = open_runs_db(&state_root)?;
    let run = load_run(&conn, &args.run_id)?;
    let manifest = load_manifest(Path::new(&run.artifact_dir))?;
    let best = manifest
        .best_candidate
        .clone()
        .ok_or_else(|| format!("run `{}` does not have a best candidate yet", args.run_id))?;
    if let Some(path) = args.preset_out.as_deref() {
        let source = fs::read_to_string(Path::new(&run.artifact_dir).join("script.ps"))
            .map_err(|err| err.to_string())?;
        write_best_preset_from_manifest(path, &manifest, &source)?;
        println!("preset_out={}", path.display());
    } else {
        println!(
            "trial_id={} objective_score={:.6} overrides={}",
            best.trial_id,
            best.objective_score,
            serde_json::to_string(&best.input_overrides).map_err(|err| err.to_string())?
        );
    }
    Ok(())
}

impl RunProgressWriter {
    fn new(
        state_root: &Path,
        run: RunRecord,
        artifact_dir: PathBuf,
        job: OptimizeJobSpec,
        config: OptimizeConfig,
    ) -> Result<Self, String> {
        let conn = open_runs_db(state_root)?;
        let event_sequence = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_no), 0) FROM run_events WHERE run_id = ?",
                params![run.run_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|err| err.to_string())?;
        Ok(Self {
            conn,
            run_id: run.run_id,
            artifact_dir,
            job,
            config,
            event_sequence,
        })
    }

    fn touch_running_row(&self, now: i64, state: &OptimizeProgressState) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE runs SET completed_trials = ?, updated_at_ms = ?, heartbeat_at_ms = ?, worker_pid = ? WHERE id = ?",
                params![
                    state.completed_trials as i64,
                    now,
                    now,
                    std::process::id(),
                    self.run_id
                ],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    fn append_event<T: Serialize>(
        &mut self,
        event_type: &str,
        payload: &T,
        now: i64,
    ) -> Result<(), String> {
        self.event_sequence += 1;
        let payload_json = serde_json::to_string(payload).map_err(|err| err.to_string())?;
        self.conn
            .execute(
                "INSERT INTO run_events (run_id, sequence_no, event_type, created_at_ms, payload_json)
                 VALUES (?, ?, ?, ?, ?)",
                params![self.run_id, self.event_sequence, event_type, now, payload_json],
            )
            .map_err(|err| err.to_string())?;
        let mut file = OpenOptions::new()
            .append(true)
            .open(self.artifact_dir.join("events.jsonl"))
            .map_err(|err| err.to_string())?;
        let line = EventLine {
            sequence: self.event_sequence,
            event_type: event_type.to_string(),
            created_at_ms: now,
            payload,
        };
        serde_json::to_writer(&mut file, &line).map_err(|err| err.to_string())?;
        writeln!(file).map_err(|err| err.to_string())?;
        Ok(())
    }

    fn write_json_artifact<T: Serialize>(&self, name: &str, value: &T) -> Result<(), String> {
        let path = self.artifact_dir.join(name);
        let json = serde_json::to_string_pretty(value).map_err(|err| err.to_string())?;
        fs::write(&path, json).map_err(|err| err.to_string())?;
        upsert_artifact(&self.conn, &self.run_id, name, &path)?;
        Ok(())
    }

    fn write_best_preset(
        &self,
        best_candidate: &OptimizeCandidateSummary,
        top_candidates: &[OptimizeCandidateSummary],
    ) -> Result<(), String> {
        let preset = OptimizePreset {
            version: 1,
            script_path: self.job.script_path.clone(),
            script_sha256: self.job.script_sha256.clone(),
            runner: self.config.runner,
            objective: self.config.objective,
            backtest: self.config.backtest.clone(),
            walk_forward: self.config.walk_forward.clone(),
            holdout: self.config.holdout.clone(),
            parameter_space: self.config.params.clone(),
            best_input_overrides: best_candidate.input_overrides.clone(),
            top_candidates: top_candidates.to_vec(),
        };
        self.write_json_artifact("best_preset.json", &preset)
    }

    fn write_manifest(
        &self,
        status: &str,
        state: &OptimizeProgressState,
        now: i64,
        error_message: Option<String>,
    ) -> Result<(), String> {
        let manifest = RunManifest {
            run_id: self.run_id.clone(),
            run_kind: RUN_KIND_OPTIMIZE.to_string(),
            status: status.to_string(),
            artifact_dir: self.artifact_dir.display().to_string(),
            script_path: self.job.script_path.clone(),
            script_sha256: self.job.script_sha256.clone(),
            git_commit: self.job.git_commit.clone(),
            created_at_ms: load_run(&self.conn, &self.run_id)?.created_at_ms,
            started_at_ms: load_run(&self.conn, &self.run_id)?.started_at_ms,
            updated_at_ms: now,
            heartbeat_at_ms: Some(now),
            completed_at_ms: None,
            worker_pid: Some(std::process::id()),
            error_message,
            candidate_count: self.job.trials,
            completed_trials: state.completed_trials,
            best_candidate: state.best_candidate.clone(),
            top_candidates: state.top_candidates.clone(),
            holdout_result: None,
            pending_batch: state.pending_batch.clone(),
            job: self.job.clone(),
        };
        write_manifest_file(&self.artifact_dir, &manifest)
    }
}

fn ensure_state_root() -> Result<PathBuf, String> {
    if let Ok(override_dir) = std::env::var("PALMSCRIPT_RUNS_STATE_DIR") {
        let path = PathBuf::from(override_dir);
        fs::create_dir_all(&path).map_err(|err| err.to_string())?;
        return Ok(path);
    }
    let mut path = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .ok_or_else(|| "could not determine a local state directory".to_string())?;
    path.push("palmscript");
    path.push("runs");
    fs::create_dir_all(&path).map_err(|err| err.to_string())?;
    Ok(path)
}

fn open_runs_db(state_root: &Path) -> Result<Connection, String> {
    let db_path = state_root.join("runs.sqlite");
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
        .map_err(|err| err.to_string())?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS runs (
            id TEXT PRIMARY KEY,
            run_kind TEXT NOT NULL,
            status TEXT NOT NULL,
            artifact_dir TEXT NOT NULL,
            script_sha256 TEXT NOT NULL,
            config_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            queued_at_ms INTEGER NOT NULL,
            started_at_ms INTEGER,
            updated_at_ms INTEGER NOT NULL,
            heartbeat_at_ms INTEGER,
            completed_at_ms INTEGER,
            worker_pid INTEGER,
            error_message TEXT,
            candidate_count INTEGER NOT NULL,
            completed_trials INTEGER NOT NULL,
            best_objective_score REAL,
            best_overrides_json TEXT,
            cancel_requested INTEGER NOT NULL DEFAULT 0,
            pending_batch_json TEXT,
            script_path TEXT
        );
        CREATE TABLE IF NOT EXISTS run_events (
            run_id TEXT NOT NULL,
            sequence_no INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            payload_json TEXT NOT NULL,
            PRIMARY KEY (run_id, sequence_no)
        );
        CREATE TABLE IF NOT EXISTS run_candidates (
            run_id TEXT NOT NULL,
            trial_id INTEGER NOT NULL,
            input_overrides_json TEXT NOT NULL,
            objective_score REAL NOT NULL,
            summary_kind TEXT NOT NULL,
            summary_json TEXT NOT NULL,
            entered_top_n INTEGER NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY (run_id, trial_id)
        );
        CREATE TABLE IF NOT EXISTS run_artifacts (
            run_id TEXT NOT NULL,
            artifact_name TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (run_id, artifact_name)
        );",
    )
    .map_err(|err| err.to_string())
}

fn recover_stale_running_runs(conn: &Connection) -> Result<(), String> {
    let stale_before = now_ms() - DAEMON_RECOVERY_STALE_MS;
    conn.execute(
        "UPDATE runs SET status = ?, updated_at_ms = ?, worker_pid = NULL
         WHERE status = ? AND heartbeat_at_ms IS NOT NULL AND heartbeat_at_ms < ?",
        params![
            RUN_STATUS_QUEUED,
            now_ms(),
            RUN_STATUS_RUNNING,
            stale_before
        ],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn claim_next_queued_run(conn: &Connection) -> Result<Option<RunRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id FROM runs WHERE status = ? ORDER BY queued_at_ms ASC, created_at_ms ASC LIMIT 1",
        )
        .map_err(|err| err.to_string())?;
    let run_id = stmt
        .query_row(params![RUN_STATUS_QUEUED], |row| row.get::<_, String>(0))
        .optional()
        .map_err(|err| err.to_string())?;
    let Some(run_id) = run_id else {
        return Ok(None);
    };
    let now = now_ms();
    conn.execute(
        "UPDATE runs SET status = ?, started_at_ms = COALESCE(started_at_ms, ?), updated_at_ms = ?, heartbeat_at_ms = ?, worker_pid = ?, cancel_requested = 0 WHERE id = ?",
        params![RUN_STATUS_RUNNING, now, now, now, std::process::id(), run_id],
    )
    .map_err(|err| err.to_string())?;
    let run = load_run(conn, &run_id)?;
    append_event_record(
        conn,
        Path::new(&run.artifact_dir),
        &run.run_id,
        "started",
        &run.run_id,
        now,
    )?;
    Ok(Some(run))
}

fn resolve_perp_from_job(
    compiled: &CompiledProgram,
    job: &OptimizeJobSpec,
    endpoints: &ExchangeEndpoints,
) -> Result<(Option<PerpBacktestConfig>, Option<PerpBacktestContext>), String> {
    let source = compiled
        .program
        .declared_sources
        .iter()
        .find(|source| source.alias == job.execution_source_alias)
        .ok_or_else(|| format!("unknown execution source `{}`", job.execution_source_alias))?;
    resolve_perp_context(
        source.template,
        source,
        compiled.program.base_interval,
        PerpCliOptions {
            from: job.from,
            to: job.to,
            leverage: job.leverage,
            margin_mode: job.margin_mode.map(map_margin_mode_back),
        },
        endpoints,
    )
}

fn load_resume_state(state_root: &Path, run_id: &str) -> Result<OptimizeResumeState, String> {
    let conn = open_runs_db(state_root)?;
    let pending_batch_json: Option<String> = conn
        .query_row(
            "SELECT pending_batch_json FROM runs WHERE id = ?",
            params![run_id],
            |row| row.get(0),
        )
        .map_err(|err| err.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT trial_id, objective_score, summary_json, input_overrides_json
             FROM run_candidates WHERE run_id = ? ORDER BY trial_id ASC",
        )
        .map_err(|err| err.to_string())?;
    let mut rows = stmt.query(params![run_id]).map_err(|err| err.to_string())?;
    let mut completed_candidates = Vec::new();
    while let Some(row) = rows.next().map_err(|err| err.to_string())? {
        let trial_id: i64 = row.get(0).map_err(|err| err.to_string())?;
        let objective_score: f64 = row.get(1).map_err(|err| err.to_string())?;
        let summary_json: String = row.get(2).map_err(|err| err.to_string())?;
        let overrides_json: String = row.get(3).map_err(|err| err.to_string())?;
        completed_candidates.push(OptimizeCandidateSummary {
            trial_id: trial_id as usize,
            input_overrides: serde_json::from_str(&overrides_json)
                .map_err(|err| err.to_string())?,
            objective_score,
            summary: serde_json::from_str(&summary_json).map_err(|err| err.to_string())?,
        });
    }
    Ok(OptimizeResumeState {
        completed_candidates,
        pending_batch: pending_batch_json
            .map(|json| serde_json::from_str(&json).map_err(|err| err.to_string()))
            .transpose()?,
    })
}

fn finalize_completed_run(
    state_root: &Path,
    run_id: &str,
    artifact_dir: &Path,
    job: &OptimizeJobSpec,
    source: &str,
    result: &OptimizeResult,
) -> Result<(), String> {
    let conn = open_runs_db(state_root)?;
    let now = now_ms();
    let best_overrides_json = serde_json::to_string(&result.best_candidate.input_overrides)
        .map_err(|err| err.to_string())?;
    conn.execute(
        "UPDATE runs SET status = ?, updated_at_ms = ?, heartbeat_at_ms = ?, completed_at_ms = ?, completed_trials = ?, best_objective_score = ?, best_overrides_json = ?, pending_batch_json = NULL, error_message = NULL WHERE id = ?",
        params![
            RUN_STATUS_COMPLETED,
            now,
            now,
            now,
            result.completed_trials as i64,
            result.best_candidate.objective_score,
            best_overrides_json,
            run_id
        ],
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        artifact_dir.join("result.json"),
        serde_json::to_string_pretty(result).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    upsert_artifact(
        &conn,
        run_id,
        "result.json",
        &artifact_dir.join("result.json"),
    )?;
    write_optimize_preset(
        &artifact_dir.join("best_preset.json"),
        Path::new(job.script_path.as_deref().unwrap_or("script.ps")),
        source,
        result,
    )?;
    upsert_artifact(
        &conn,
        run_id,
        "best_preset.json",
        &artifact_dir.join("best_preset.json"),
    )?;
    write_manifest_file(
        artifact_dir,
        &RunManifest {
            run_id: run_id.to_string(),
            run_kind: RUN_KIND_OPTIMIZE.to_string(),
            status: RUN_STATUS_COMPLETED.to_string(),
            artifact_dir: artifact_dir.display().to_string(),
            script_path: job.script_path.clone(),
            script_sha256: job.script_sha256.clone(),
            git_commit: job.git_commit.clone(),
            created_at_ms: load_run(&conn, run_id)?.created_at_ms,
            started_at_ms: load_run(&conn, run_id)?.started_at_ms,
            updated_at_ms: now,
            heartbeat_at_ms: Some(now),
            completed_at_ms: Some(now),
            worker_pid: Some(std::process::id()),
            error_message: None,
            candidate_count: result.candidate_count,
            completed_trials: result.completed_trials,
            best_candidate: Some(result.best_candidate.clone()),
            top_candidates: result.top_candidates.clone(),
            holdout_result: result.holdout.clone(),
            pending_batch: None,
            job: job.clone(),
        },
    )?;
    append_event_record(&conn, artifact_dir, run_id, "completed", result, now)?;
    println!(
        "{}",
        render_optimize_text(result, Some(&artifact_dir.join("best_preset.json")))
    );
    Ok(())
}

fn finalize_canceled_run(
    state_root: &Path,
    run_id: &str,
    artifact_dir: &Path,
    job: &OptimizeJobSpec,
    source: &str,
) -> Result<(), String> {
    let conn = open_runs_db(state_root)?;
    let now = now_ms();
    let manifest = load_manifest(artifact_dir)?;
    conn.execute(
        "UPDATE runs SET status = ?, updated_at_ms = ?, heartbeat_at_ms = ?, completed_at_ms = ?, pending_batch_json = NULL WHERE id = ?",
        params![RUN_STATUS_CANCELED, now, now, now, run_id],
    )
    .map_err(|err| err.to_string())?;
    if manifest.best_candidate.is_some() {
        let canceled_manifest = RunManifest {
            status: RUN_STATUS_CANCELED.to_string(),
            completed_at_ms: Some(now),
            updated_at_ms: now,
            heartbeat_at_ms: Some(now),
            error_message: None,
            job: job.clone(),
            ..manifest.clone()
        };
        write_best_preset_from_manifest(
            &artifact_dir.join("best_preset.json"),
            &canceled_manifest,
            source,
        )?;
    }
    write_manifest_file(
        artifact_dir,
        &RunManifest {
            status: RUN_STATUS_CANCELED.to_string(),
            completed_at_ms: Some(now),
            updated_at_ms: now,
            heartbeat_at_ms: Some(now),
            error_message: None,
            job: job.clone(),
            ..manifest
        },
    )?;
    append_event_record(&conn, artifact_dir, run_id, "canceled", run_id, now)?;
    Ok(())
}

fn mark_run_failed(
    state_root: &Path,
    run_id: &str,
    artifact_dir: &Path,
    message: &str,
) -> Result<(), String> {
    let conn = open_runs_db(state_root)?;
    let now = now_ms();
    conn.execute(
        "UPDATE runs SET status = ?, updated_at_ms = ?, heartbeat_at_ms = ?, completed_at_ms = ?, error_message = ?, pending_batch_json = NULL WHERE id = ?",
        params![RUN_STATUS_FAILED, now, now, now, message, run_id],
    )
    .map_err(|err| err.to_string())?;
    if let Ok(manifest) = load_manifest(artifact_dir) {
        write_manifest_file(
            artifact_dir,
            &RunManifest {
                status: RUN_STATUS_FAILED.to_string(),
                updated_at_ms: now,
                heartbeat_at_ms: Some(now),
                completed_at_ms: Some(now),
                error_message: Some(message.to_string()),
                ..manifest
            },
        )?;
    }
    append_event_record(
        &conn,
        artifact_dir,
        run_id,
        "failed",
        &message.to_string(),
        now,
    )?;
    Ok(())
}

fn build_config_from_job(job: &OptimizeJobSpec) -> Result<OptimizeConfig, String> {
    let backtest = BacktestConfig {
        execution_source_alias: job.execution_source_alias.clone(),
        initial_capital: job.initial_capital,
        fee_bps: job.fee_bps,
        slippage_bps: job.slippage_bps,
        perp: job.leverage.map(|leverage| PerpBacktestConfig {
            leverage,
            margin_mode: job.margin_mode.unwrap_or(PerpMarginMode::Isolated),
        }),
        perp_context: None,
    };
    Ok(OptimizeConfig {
        runner: job.runner,
        backtest: backtest.clone(),
        walk_forward: match job.runner {
            OptimizeRunner::WalkForward => Some(WalkForwardConfig {
                backtest,
                train_bars: job
                    .train_bars
                    .ok_or_else(|| "missing train_bars".to_string())?,
                test_bars: job
                    .test_bars
                    .ok_or_else(|| "missing test_bars".to_string())?,
                step_bars: job
                    .step_bars
                    .or(job.test_bars)
                    .ok_or_else(|| "missing step_bars".to_string())?,
            }),
            OptimizeRunner::Backtest => None,
        },
        holdout: job.holdout.clone(),
        params: job.params.clone(),
        objective: job.objective,
        trials: job.trials,
        startup_trials: job.startup_trials,
        seed: job.seed,
        workers: job.workers,
        top_n: job.top_n,
        base_input_overrides: job.base_input_overrides.clone(),
    })
}

fn load_run(conn: &Connection, run_id: &str) -> Result<RunRecord, String> {
    conn.query_row(
        "SELECT id, status, artifact_dir, script_sha256, config_json, created_at_ms, started_at_ms,
                updated_at_ms, heartbeat_at_ms, completed_at_ms, worker_pid, error_message,
                candidate_count, completed_trials, best_objective_score, best_overrides_json,
                cancel_requested, pending_batch_json
         FROM runs WHERE id = ?",
        params![run_id],
        |row| {
            Ok(RunRecord {
                run_id: row.get(0)?,
                status: row.get(1)?,
                artifact_dir: row.get(2)?,
                script_sha256: row.get(3)?,
                config_json: row.get(4)?,
                created_at_ms: row.get(5)?,
                started_at_ms: row.get(6)?,
                updated_at_ms: row.get(7)?,
                heartbeat_at_ms: row.get(8)?,
                completed_at_ms: row.get(9)?,
                worker_pid: row.get(10)?,
                error_message: row.get(11)?,
                candidate_count: row.get::<_, i64>(12)? as usize,
                completed_trials: row.get::<_, i64>(13)? as usize,
                best_objective_score: row.get(14)?,
                best_overrides_json: row.get(15)?,
                cancel_requested: row.get::<_, i64>(16)? != 0,
                pending_batch_json: row.get(17)?,
            })
        },
    )
    .map_err(|err| err.to_string())
}

fn load_manifest(artifact_dir: &Path) -> Result<RunManifest, String> {
    let raw =
        fs::read_to_string(artifact_dir.join("manifest.json")).map_err(|err| err.to_string())?;
    serde_json::from_str(&raw).map_err(|err| err.to_string())
}

fn write_best_preset_from_manifest(
    path: &Path,
    manifest: &RunManifest,
    source: &str,
) -> Result<(), String> {
    let best_candidate = manifest.best_candidate.clone().ok_or_else(|| {
        format!(
            "run `{}` does not have a best candidate yet",
            manifest.run_id
        )
    })?;
    let source_hash = hash_source(source);
    if source_hash != manifest.job.script_sha256 {
        return Err(format!(
            "stored script hash mismatch while writing preset for run `{}`: expected {}, found {}",
            manifest.run_id, manifest.job.script_sha256, source_hash
        ));
    }
    let config = build_config_from_job(&manifest.job)?;
    let preset = OptimizePreset {
        version: 1,
        script_path: manifest.job.script_path.clone(),
        script_sha256: manifest.job.script_sha256.clone(),
        runner: manifest.job.runner,
        objective: manifest.job.objective,
        backtest: config.backtest.clone(),
        walk_forward: config.walk_forward.clone(),
        holdout: config.holdout.clone(),
        parameter_space: manifest.job.params.clone(),
        best_input_overrides: best_candidate.input_overrides,
        top_candidates: manifest.top_candidates.clone(),
    };
    fs::write(
        path,
        serde_json::to_string_pretty(&preset).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())
}

fn write_manifest_file(artifact_dir: &Path, manifest: &RunManifest) -> Result<(), String> {
    fs::write(
        artifact_dir.join("manifest.json"),
        serde_json::to_string_pretty(manifest).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())
}

fn append_event_record<T: Serialize + ?Sized>(
    conn: &Connection,
    artifact_dir: &Path,
    run_id: &str,
    event_type: &str,
    payload: &T,
    now: i64,
) -> Result<(), String> {
    let sequence = conn
        .query_row(
            "SELECT COALESCE(MAX(sequence_no), 0) + 1 FROM run_events WHERE run_id = ?",
            params![run_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|err| err.to_string())?;
    let payload_json = serde_json::to_string(payload).map_err(|err| err.to_string())?;
    conn.execute(
        "INSERT INTO run_events (run_id, sequence_no, event_type, created_at_ms, payload_json)
         VALUES (?, ?, ?, ?, ?)",
        params![run_id, sequence, event_type, now, payload_json],
    )
    .map_err(|err| err.to_string())?;
    let mut file = OpenOptions::new()
        .append(true)
        .open(artifact_dir.join("events.jsonl"))
        .map_err(|err| err.to_string())?;
    let line = EventLine {
        sequence,
        event_type: event_type.to_string(),
        created_at_ms: now,
        payload,
    };
    serde_json::to_writer(&mut file, &line).map_err(|err| err.to_string())?;
    writeln!(file).map_err(|err| err.to_string())
}

fn load_events_since(
    conn: &Connection,
    run_id: &str,
    after_sequence: i64,
) -> Result<Vec<(i64, String, i64, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT sequence_no, event_type, created_at_ms, payload_json
             FROM run_events WHERE run_id = ? AND sequence_no > ? ORDER BY sequence_no ASC",
        )
        .map_err(|err| err.to_string())?;
    let mut rows = stmt
        .query(params![run_id, after_sequence])
        .map_err(|err| err.to_string())?;
    let mut events = Vec::new();
    while let Some(row) = rows.next().map_err(|err| err.to_string())? {
        events.push((
            row.get(0).map_err(|err| err.to_string())?,
            row.get(1).map_err(|err| err.to_string())?,
            row.get(2).map_err(|err| err.to_string())?,
            row.get(3).map_err(|err| err.to_string())?,
        ));
    }
    Ok(events)
}

fn upsert_artifact(conn: &Connection, run_id: &str, name: &str, path: &Path) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO run_artifacts (run_id, artifact_name, relative_path, updated_at_ms)
         VALUES (?, ?, ?, ?)",
        params![run_id, name, path.display().to_string(), now_ms()],
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

fn print_status(run: &RunRecord) {
    println!("run_id={}", run.run_id);
    println!("status={}", run.status);
    println!("progress={}/{}", run.completed_trials, run.candidate_count);
    println!("created_at_ms={}", run.created_at_ms);
    if let Some(started_at_ms) = run.started_at_ms {
        println!("started_at_ms={started_at_ms}");
    }
    if let Some(heartbeat_at_ms) = run.heartbeat_at_ms {
        println!("heartbeat_at_ms={heartbeat_at_ms}");
    }
    if let Some(completed_at_ms) = run.completed_at_ms {
        println!("completed_at_ms={completed_at_ms}");
    }
    if let Some(worker_pid) = run.worker_pid {
        println!("worker_pid={worker_pid}");
    }
    if let Some(score) = run.best_objective_score {
        println!("best_objective_score={score:.6}");
    }
    if let Some(overrides) = &run.best_overrides_json {
        println!("best_overrides={overrides}");
    }
    println!("cancel_requested={}", run.cancel_requested);
    println!("has_pending_batch={}", run.pending_batch_json.is_some());
    if let Some(error_message) = &run.error_message {
        println!("error={error_message}");
    }
    println!("updated_at_ms={}", run.updated_at_ms);
    println!("artifact_dir={}", run.artifact_dir);
}

fn print_manifest(manifest: &RunManifest) {
    println!("run_id={}", manifest.run_id);
    println!("status={}", manifest.status);
    println!("candidate_count={}", manifest.candidate_count);
    println!("completed_trials={}", manifest.completed_trials);
    println!("artifact_dir={}", manifest.artifact_dir);
    if let Some(best) = &manifest.best_candidate {
        println!("best_trial_id={}", best.trial_id);
        println!("best_objective_score={:.6}", best.objective_score);
        println!(
            "best_overrides={}",
            serde_json::to_string(&best.input_overrides).unwrap_or_else(|_| "{}".to_string())
        );
    }
    if let Some(holdout) = &manifest.job.holdout {
        println!("holdout_bars={}", holdout.bars);
    }
    if let Some(holdout) = &manifest.holdout_result {
        println!("holdout_from={}", holdout.from);
        println!("holdout_to={}", holdout.to);
        println!("holdout_trade_count={}", holdout.summary.trade_count);
        println!(
            "holdout_total_return_pct={:.2}",
            holdout.summary.total_return * 100.0
        );
        println!("holdout_max_drawdown={:.2}", holdout.summary.max_drawdown);
    }
    if let Some(error) = &manifest.error_message {
        println!("error={error}");
    }
    println!("top_candidates={}", manifest.top_candidates.len());
}

fn autostart_enabled() -> bool {
    std::env::var("PALMSCRIPT_RUNS_NO_AUTOSTART")
        .map(|value| value != "1")
        .unwrap_or(true)
}

fn spawn_background_server() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    ProcessCommand::new(exe)
        .args(["runs", "serve", "--once"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| err.to_string())?;
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn generate_run_id(script_sha256: &str) -> String {
    let pid = std::process::id();
    let now = now_ms();
    format!(
        "run-{}-{now:x}-{pid:x}",
        &script_sha256[..12.min(script_sha256.len())]
    )
}

fn git_commit() -> Option<String> {
    ProcessCommand::new("git")
        .args(["rev-parse", "HEAD"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
}

fn map_margin_mode(mode: BacktestMarginMode) -> PerpMarginMode {
    match mode {
        BacktestMarginMode::Isolated => PerpMarginMode::Isolated,
    }
}

fn map_margin_mode_back(mode: PerpMarginMode) -> BacktestMarginMode {
    match mode {
        PerpMarginMode::Isolated => BacktestMarginMode::Isolated,
    }
}

fn summary_kind(summary: &OptimizeEvaluationSummary) -> &'static str {
    match summary {
        OptimizeEvaluationSummary::WalkForward { .. } => "walk_forward",
        OptimizeEvaluationSummary::Backtest { .. } => "backtest",
    }
}

fn candidate_payload(
    candidate: &OptimizeCandidateSummary,
    entered_top_n: bool,
) -> CandidatePayload {
    let (ending_equity, total_return, max_drawdown) = match &candidate.summary {
        OptimizeEvaluationSummary::WalkForward {
            stitched_summary, ..
        } => (
            stitched_summary.ending_equity,
            stitched_summary.total_return,
            stitched_summary.max_drawdown,
        ),
        OptimizeEvaluationSummary::Backtest { summary, .. } => (
            summary.ending_equity,
            summary.total_return,
            summary.max_drawdown,
        ),
    };
    CandidatePayload {
        trial_id: candidate.trial_id,
        input_overrides: candidate.input_overrides.clone(),
        objective_score: candidate.objective_score,
        summary_kind: summary_kind(&candidate.summary),
        ending_equity,
        total_return,
        max_drawdown,
        entered_top_n,
    }
}
