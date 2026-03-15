use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;
use std::thread;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::backtest::diagnostics::aggregate_time_bucket_summaries;
use crate::backtest::overfitting::build_optimize_overfitting_risk;
use crate::backtest::{
    bridge, run_backtest_with_sources, run_backtest_with_sources_internal,
    run_walk_forward_with_sources, BacktestCaptureSummary, BacktestConfig, BacktestError,
    BacktestSummary, BaselineComparisonSummary, ConstraintFailureBreakdown,
    DatePerturbationDiagnostics, DiagnosticsDetailMode, ImprovementHint, ImprovementHintKind,
    OverfittingRiskLevel, OverfittingRiskSummary, TimeBucketUtcDiagnosticSummary,
    ValidationConstraintConfig, ValidationConstraintKind, ValidationConstraintSummary,
    ValidationConstraintViolation, WalkForwardConfig, WalkForwardResult,
    WalkForwardSegmentDiagnostics, WalkForwardStitchedSummary, WalkForwardWindowSummary,
};
use crate::compiler::compile_with_input_overrides;
use crate::diagnostic::CompileError;
use crate::runtime::{slice_runtime_window, SourceRuntimeConfig, VmLimits};

const MAX_OPTIMIZE_TRIALS: usize = 1_000;
const TPE_UPDATE_BATCH_SIZE: usize = 4;
const TPE_CANDIDATES_PER_TRIAL: usize = 24;
const GOOD_TRIAL_FRACTION: f64 = 0.2;
const MIN_DENSITY: f64 = 1.0e-12;
const MIN_BANDWIDTH: f64 = 1.0e-6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizeRunner {
    WalkForward,
    Backtest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizeObjective {
    RobustReturn,
    TotalReturn,
    EndingEquity,
    ReturnOverDrawdown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "space_kind", rename_all = "snake_case")]
pub enum OptimizeParamSpace {
    IntegerRange {
        name: String,
        low: i64,
        high: i64,
        #[serde(default = "default_integer_step")]
        step: i64,
    },
    FloatRange {
        name: String,
        low: f64,
        high: f64,
        #[serde(default)]
        step: Option<f64>,
    },
    Choice {
        name: String,
        values: Vec<f64>,
    },
}

impl OptimizeParamSpace {
    pub fn name(&self) -> &str {
        match self {
            Self::IntegerRange { name, .. }
            | Self::FloatRange { name, .. }
            | Self::Choice { name, .. } => name,
        }
    }

    fn sample_random(&self, rng: &mut StdRng) -> f64 {
        match self {
            Self::IntegerRange {
                low, high, step, ..
            } => {
                let slots = integer_slot_count(*low, *high, *step);
                (*low + rng.gen_range(0..slots) * *step) as f64
            }
            Self::FloatRange {
                low, high, step, ..
            } => {
                let sampled = rng.gen_range(*low..=*high);
                match step {
                    Some(step) => quantize_float_step(*low, *high, *step, sampled),
                    None => sampled,
                }
            }
            Self::Choice { values, .. } => {
                let index = rng.gen_range(0..values.len());
                values[index]
            }
        }
    }

    fn clamp(&self, value: f64) -> f64 {
        match self {
            Self::IntegerRange {
                low, high, step, ..
            } => quantize_integer_step(*low, *high, *step, value),
            Self::FloatRange {
                low, high, step, ..
            } => match step {
                Some(step) => quantize_float_step(*low, *high, *step, value),
                None => value.clamp(*low, *high),
            },
            Self::Choice { values, .. } => closest_choice(values, value),
        }
    }

    fn span(&self) -> f64 {
        match self {
            Self::IntegerRange {
                low, high, step, ..
            } => ((*high - *low).max(*step).max(1)) as f64,
            Self::FloatRange {
                low, high, step, ..
            } => match step {
                Some(step) => (*high - *low).abs().max(*step).max(MIN_BANDWIDTH),
                None => (*high - *low).abs().max(MIN_BANDWIDTH),
            },
            Self::Choice { values, .. } => {
                if values.len() <= 1 {
                    1.0
                } else {
                    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
                    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    (max - min).abs().max(MIN_BANDWIDTH)
                }
            }
        }
    }
}

const fn default_integer_step() -> i64 {
    1
}

fn integer_slot_count(low: i64, high: i64, step: i64) -> i64 {
    ((high - low) / step) + 1
}

fn quantize_integer_step(low: i64, high: i64, step: i64, value: f64) -> f64 {
    let value = value.clamp(low as f64, high as f64);
    let offset = ((value - low as f64) / step as f64).round();
    let clamped_offset = offset.clamp(0.0, (integer_slot_count(low, high, step) - 1) as f64);
    (low + (clamped_offset as i64) * step) as f64
}

fn quantize_float_step(low: f64, high: f64, step: f64, value: f64) -> f64 {
    let value = value.clamp(low, high);
    let offset = ((value - low) / step).round();
    let max_offset = ((high - low) / step).round().max(0.0);
    (low + offset.clamp(0.0, max_offset) * step).clamp(low, high)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizeConfig {
    pub runner: OptimizeRunner,
    pub backtest: BacktestConfig,
    pub walk_forward: Option<WalkForwardConfig>,
    #[serde(default)]
    pub diagnostics_detail: DiagnosticsDetailMode,
    #[serde(default)]
    pub holdout: Option<OptimizeHoldoutConfig>,
    pub params: Vec<OptimizeParamSpace>,
    pub objective: OptimizeObjective,
    pub trials: usize,
    pub startup_trials: usize,
    pub seed: u64,
    pub workers: usize,
    pub top_n: usize,
    #[serde(default)]
    pub direct_validation_top_n: usize,
    pub base_input_overrides: BTreeMap<String, f64>,
    #[serde(default)]
    pub constraints: ValidationConstraintConfig,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizeHoldoutConfig {
    pub bars: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizeHoldoutResult {
    pub bars: usize,
    pub from: i64,
    pub to: i64,
    pub summary: WalkForwardWindowSummary,
    pub diagnostics: WalkForwardSegmentDiagnostics,
    pub drift: HoldoutDriftSummary,
    #[serde(default)]
    pub constraints: ValidationConstraintSummary,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HoldoutDriftSummary {
    pub total_return_delta: f64,
    pub execution_asset_return_delta: f64,
    pub trade_count_delta: i64,
    pub win_rate_delta: f64,
    pub max_drawdown_delta: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DirectValidationDriftSummary {
    pub total_return_delta: f64,
    pub execution_asset_return_delta: f64,
    pub trade_count_delta: i64,
    pub win_rate_delta: f64,
    pub max_drawdown_delta: f64,
    pub sharpe_ratio_delta: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizeDirectValidationResult {
    pub survivor_rank: usize,
    pub trial_id: usize,
    pub input_overrides: BTreeMap<String, f64>,
    pub objective_score: f64,
    pub summary: BacktestSummary,
    pub capture_summary: BacktestCaptureSummary,
    #[serde(default)]
    pub baseline_comparison: BaselineComparisonSummary,
    #[serde(default)]
    pub date_perturbation: DatePerturbationDiagnostics,
    #[serde(default)]
    pub overfitting_risk: OverfittingRiskSummary,
    #[serde(default)]
    pub time_bucket_cohorts: Vec<TimeBucketUtcDiagnosticSummary>,
    pub drift: DirectValidationDriftSummary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "runner_summary_kind", rename_all = "snake_case")]
pub enum OptimizeEvaluationSummary {
    WalkForward {
        stitched_summary: WalkForwardStitchedSummary,
        zero_trade_segment_count: usize,
        trade_count: usize,
        winning_trade_count: usize,
        losing_trade_count: usize,
        win_rate: f64,
    },
    Backtest {
        summary: BacktestSummary,
        capture_summary: BacktestCaptureSummary,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizeCandidateSummary {
    pub trial_id: usize,
    pub input_overrides: BTreeMap<String, f64>,
    pub objective_score: f64,
    pub summary: OptimizeEvaluationSummary,
    #[serde(default)]
    pub time_bucket_cohorts: Vec<TimeBucketUtcDiagnosticSummary>,
    #[serde(default)]
    pub constraints: ValidationConstraintSummary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizeScheduledTrial {
    pub trial_id: usize,
    pub input_overrides: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OptimizeScheduledBatch {
    pub trials: Vec<OptimizeScheduledTrial>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OptimizeResumeState {
    pub completed_candidates: Vec<OptimizeCandidateSummary>,
    pub pending_batch: Option<OptimizeScheduledBatch>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizeProgressState {
    pub candidate_count: usize,
    pub completed_trials: usize,
    pub best_candidate: Option<OptimizeCandidateSummary>,
    pub top_candidates: Vec<OptimizeCandidateSummary>,
    pub pending_batch: Option<OptimizeScheduledBatch>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event_kind", rename_all = "snake_case")]
pub enum OptimizeProgressEvent {
    BatchScheduled {
        batch: OptimizeScheduledBatch,
    },
    CandidateCompleted {
        candidate: OptimizeCandidateSummary,
        entered_top_n: bool,
    },
    BestCandidateImproved {
        candidate: OptimizeCandidateSummary,
    },
    CheckpointWritten,
    Canceled,
}

pub trait OptimizeProgressListener {
    fn on_event(
        &mut self,
        event: OptimizeProgressEvent,
        state: &OptimizeProgressState,
    ) -> Result<(), String>;

    fn should_cancel(&mut self) -> Result<bool, String> {
        Ok(false)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizePreset {
    pub version: u32,
    pub script_path: Option<String>,
    pub script_sha256: String,
    pub runner: OptimizeRunner,
    pub objective: OptimizeObjective,
    pub backtest: BacktestConfig,
    pub walk_forward: Option<WalkForwardConfig>,
    #[serde(default)]
    pub diagnostics_detail: DiagnosticsDetailMode,
    #[serde(default)]
    pub holdout: Option<OptimizeHoldoutConfig>,
    #[serde(default)]
    pub constraints: ValidationConstraintConfig,
    pub parameter_space: Vec<OptimizeParamSpace>,
    pub best_input_overrides: BTreeMap<String, f64>,
    pub top_candidates: Vec<OptimizeCandidateSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizeResult {
    pub config: OptimizeConfig,
    pub candidate_count: usize,
    pub completed_trials: usize,
    pub validated_candidate_count: usize,
    pub feasible_candidate_count: usize,
    pub infeasible_candidate_count: usize,
    pub best_candidate: OptimizeCandidateSummary,
    #[serde(default)]
    pub best_infeasible_candidate: Option<OptimizeCandidateSummary>,
    pub top_candidates: Vec<OptimizeCandidateSummary>,
    #[serde(default)]
    pub holdout: Option<OptimizeHoldoutResult>,
    #[serde(default)]
    pub robustness: OptimizationRobustnessSummary,
    #[serde(default)]
    pub constraints: ValidationConstraintSummary,
    #[serde(default)]
    pub constraint_failure_breakdown: Vec<ConstraintFailureBreakdown>,
    #[serde(default)]
    pub direct_validation: Vec<OptimizeDirectValidationResult>,
    #[serde(default)]
    pub hints: Vec<ImprovementHint>,
    #[serde(default)]
    pub overfitting_risk: OverfittingRiskSummary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoldoutCandidateEvaluation {
    pub trial_id: usize,
    pub input_overrides: BTreeMap<String, f64>,
    pub passed: bool,
    pub summary: WalkForwardWindowSummary,
    pub drift: HoldoutDriftSummary,
    #[serde(default)]
    pub date_perturbation: Option<DatePerturbationDiagnostics>,
    #[serde(default)]
    pub overfitting_risk: Option<OverfittingRiskSummary>,
    #[serde(default)]
    pub constraints: ValidationConstraintSummary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParameterRobustnessSummary {
    pub name: String,
    pub best_value: Option<f64>,
    pub top_ranked_min: Option<f64>,
    pub top_ranked_max: Option<f64>,
    pub holdout_passing_min: Option<f64>,
    pub holdout_passing_max: Option<f64>,
    pub distinct_sampled_value_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OptimizationRobustnessSummary {
    pub top_candidate_count: usize,
    pub holdout_evaluated_count: usize,
    pub holdout_pass_count: usize,
    pub holdout_fail_count: usize,
    pub best_candidate_holdout_rank: Option<usize>,
    pub holdout_return_min: Option<f64>,
    pub holdout_return_max: Option<f64>,
    pub holdout_return_mean: Option<f64>,
    pub holdout_pass_rate: Option<f64>,
    pub evaluations: Vec<HoldoutCandidateEvaluation>,
    pub parameter_stability: Vec<ParameterRobustnessSummary>,
}

#[derive(Debug, Error)]
pub enum OptimizeError {
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error(transparent)]
    Backtest(#[from] BacktestError),
    #[error("optimize requires at least one `--param` search space or a preset parameter space")]
    MissingParams,
    #[error("optimize `trials` must be > 0, found {value}")]
    InvalidTrials { value: usize },
    #[error("optimize `startup_trials` must be > 0 and <= trials, found {value}")]
    InvalidStartupTrials { value: usize },
    #[error("optimize `workers` must be > 0, found {value}")]
    InvalidWorkers { value: usize },
    #[error("optimize `top_n` must be > 0, found {value}")]
    InvalidTopN { value: usize },
    #[error("optimize holdout `bars` must be > 0, found {value}")]
    InvalidHoldoutBars { value: usize },
    #[error("optimize `min_trade_count` must be > 0 when set, found {value}")]
    InvalidMinTradeCount { value: usize },
    #[error("optimize `min_sharpe_ratio` must be finite when set, found {value}")]
    InvalidMinSharpeRatio { value: f64 },
    #[error("optimize `min_holdout_trade_count` must be > 0 when set, found {value}")]
    InvalidMinHoldoutTradeCount { value: usize },
    #[error("optimize `min_holdout_pass_rate` must be finite and in [0, 1], found {value}")]
    InvalidMinHoldoutPassRate { value: f64 },
    #[error("optimize `min_date_perturbation_positive_ratio` must be finite and in [0, 1], found {value}")]
    InvalidMinDatePerturbationPositiveRatio { value: f64 },
    #[error("optimize `min_date_perturbation_outperform_ratio` must be finite and in [0, 1], found {value}")]
    InvalidMinDatePerturbationOutperformRatio { value: f64 },
    #[error("optimize constraint `{name}` requires holdout validation to be enabled")]
    HoldoutConstraintRequiresHoldout { name: String },
    #[error("optimize trial count {count} exceeds max supported {limit}")]
    TooManyTrials { count: usize, limit: usize },
    #[error("optimize holdout requires fewer reserved bars than the available execution bars; requested {requested}, available {available}")]
    InvalidHoldoutWindow { requested: usize, available: usize },
    #[error("optimize holdout would leave only {available} execution bars for tuning but {required} are required")]
    HoldoutLeavesTooFewBars { available: usize, required: usize },
    #[error("optimize parameter `{name}` is defined more than once")]
    DuplicateParam { name: String },
    #[error(
        "optimize integer parameter `{name}` must use low <= high with step > 0 and aligned bounds"
    )]
    InvalidIntegerRange { name: String },
    #[error("optimize float parameter `{name}` must use finite low/high with low <= high and a finite step when present")]
    InvalidFloatRange { name: String },
    #[error("optimize choice parameter `{name}` must include at least one finite value")]
    EmptyChoice { name: String },
    #[error("optimizer worker thread panicked")]
    WorkerPanicked,
    #[error("optimize progress callback failed: {message}")]
    ProgressCallback { message: String },
    #[error("optimize resume state is invalid: {message}")]
    InvalidResumeState { message: String },
    #[error("optimize run canceled")]
    Canceled,
}

pub fn run_optimize_with_source(
    source: &str,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: OptimizeConfig,
) -> Result<OptimizeResult, OptimizeError> {
    run_optimize_with_source_resume(
        source,
        runtime,
        vm_limits,
        config,
        OptimizeResumeState::default(),
        None,
    )
}

pub fn run_optimize_with_source_resume(
    source: &str,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: OptimizeConfig,
    resume: OptimizeResumeState,
    mut listener: Option<&mut dyn OptimizeProgressListener>,
) -> Result<OptimizeResult, OptimizeError> {
    validate_optimize_config(&config)?;
    let holdout_plan = prepare_holdout_plan(source, &runtime, &config)?;
    let optimize_runtime = holdout_plan
        .as_ref()
        .map(|plan| plan.optimize_runtime.clone())
        .unwrap_or_else(|| runtime.clone());
    let mut all_candidates = sorted_candidates(resume.completed_candidates);
    validate_resume_state(&config, &all_candidates, resume.pending_batch.as_ref())?;
    let mut top_candidates = build_top_candidates(&all_candidates, config.top_n);
    let mut pending_batch = resume.pending_batch.clone();
    let replay = replay_generation_state(&config, &all_candidates, pending_batch.as_ref())?;
    let mut next_trial_id = replay.generated_trial_count;
    let mut seen_candidate_keys = replay.seen_candidate_keys;
    let mut rng = replay.rng;

    if let Some(batch) = pending_batch.clone() {
        let state = progress_state(
            &config,
            &all_candidates,
            &top_candidates,
            Some(batch.clone()),
        );
        emit_progress_event(
            &mut listener,
            OptimizeProgressEvent::BatchScheduled {
                batch: batch.clone(),
            },
            &state,
        )?;
        let completed_trial_ids = all_candidates
            .iter()
            .map(|candidate| candidate.trial_id)
            .collect::<BTreeSet<_>>();
        evaluate_scheduled_batch(
            source,
            &optimize_runtime,
            vm_limits,
            &config,
            &batch,
            &completed_trial_ids,
            &mut |candidate| {
                handle_candidate_completion(
                    &config,
                    &mut all_candidates,
                    &mut top_candidates,
                    &pending_batch,
                    candidate,
                    &mut listener,
                )
            },
        )?;
        all_candidates.sort_by_key(|candidate| candidate.trial_id);
        emit_progress_event(
            &mut listener,
            OptimizeProgressEvent::CheckpointWritten,
            &progress_state(&config, &all_candidates, &top_candidates, None),
        )?;
    }

    while next_trial_id < config.trials {
        if should_cancel(&mut listener)? {
            emit_progress_event(
                &mut listener,
                OptimizeProgressEvent::Canceled,
                &progress_state(&config, &all_candidates, &top_candidates, None),
            )?;
            return Err(OptimizeError::Canceled);
        }
        let batch_len = (config.trials - next_trial_id).min(TPE_UPDATE_BATCH_SIZE);
        let batch_inputs = suggest_batch(
            &config,
            &all_candidates,
            &mut seen_candidate_keys,
            &mut rng,
            next_trial_id,
            batch_len,
        );
        let batch = scheduled_batch(next_trial_id, batch_inputs);
        pending_batch = Some(batch.clone());
        emit_progress_event(
            &mut listener,
            OptimizeProgressEvent::BatchScheduled {
                batch: batch.clone(),
            },
            &progress_state(
                &config,
                &all_candidates,
                &top_candidates,
                pending_batch.clone(),
            ),
        )?;
        let completed_trial_ids = all_candidates
            .iter()
            .map(|candidate| candidate.trial_id)
            .collect::<BTreeSet<_>>();
        evaluate_scheduled_batch(
            source,
            &optimize_runtime,
            vm_limits,
            &config,
            &batch,
            &completed_trial_ids,
            &mut |candidate| {
                handle_candidate_completion(
                    &config,
                    &mut all_candidates,
                    &mut top_candidates,
                    &pending_batch,
                    candidate,
                    &mut listener,
                )
            },
        )?;
        all_candidates.sort_by_key(|candidate| candidate.trial_id);
        emit_progress_event(
            &mut listener,
            OptimizeProgressEvent::CheckpointWritten,
            &progress_state(&config, &all_candidates, &top_candidates, None),
        )?;
        next_trial_id += batch_len;
    }

    top_candidates.sort_by(compare_candidates);
    let ValidatedCandidateSelection {
        best_holdout: holdout,
        robustness,
        best_overfitting_risk: overfitting_risk,
        best_infeasible_candidate,
        constraint_failure_breakdown,
    } = validate_ranked_candidates(
        source,
        &runtime,
        vm_limits,
        &config,
        &all_candidates,
        &mut top_candidates,
        holdout_plan.as_ref(),
    )?;
    let best_candidate = top_candidates
        .first()
        .cloned()
        .expect("validated optimize config always yields at least one trial");
    let feasible_candidate_count = top_candidates
        .iter()
        .filter(|candidate| candidate.constraints.passed)
        .count();
    let infeasible_candidate_count = top_candidates
        .len()
        .saturating_sub(feasible_candidate_count);
    let constraints = best_candidate.constraints.clone();
    let direct_validation =
        run_direct_validations(source, &runtime, vm_limits, &config, &top_candidates)?;
    let hints = build_optimize_hints(&best_candidate, holdout.as_ref(), &robustness);
    Ok(OptimizeResult {
        candidate_count: config.trials,
        completed_trials: all_candidates.len(),
        validated_candidate_count: top_candidates.len(),
        feasible_candidate_count,
        infeasible_candidate_count,
        config,
        best_candidate,
        best_infeasible_candidate,
        top_candidates,
        holdout,
        robustness,
        constraints,
        constraint_failure_breakdown,
        direct_validation,
        hints,
        overfitting_risk,
    })
}

struct HoldoutPlan {
    optimize_runtime: SourceRuntimeConfig,
    execution_bars: Vec<crate::runtime::Bar>,
    split_index: usize,
    from: i64,
    to: i64,
    bars: usize,
}

struct ReplayGenerationState {
    rng: StdRng,
    seen_candidate_keys: BTreeSet<String>,
    generated_trial_count: usize,
}

#[derive(Clone)]
struct ValidatedCandidateArtifacts {
    holdout: Option<OptimizeHoldoutResult>,
    overfitting_risk: OverfittingRiskSummary,
}

struct ValidatedCandidateSelection {
    best_holdout: Option<OptimizeHoldoutResult>,
    robustness: OptimizationRobustnessSummary,
    best_overfitting_risk: OverfittingRiskSummary,
    best_infeasible_candidate: Option<OptimizeCandidateSummary>,
    constraint_failure_breakdown: Vec<ConstraintFailureBreakdown>,
}

fn sorted_candidates(
    mut candidates: Vec<OptimizeCandidateSummary>,
) -> Vec<OptimizeCandidateSummary> {
    candidates.sort_by_key(|candidate| candidate.trial_id);
    candidates
}

fn build_top_candidates(
    candidates: &[OptimizeCandidateSummary],
    top_n: usize,
) -> Vec<OptimizeCandidateSummary> {
    let mut top = Vec::new();
    for candidate in candidates {
        insert_top_candidate(&mut top, candidate.clone(), top_n);
    }
    top.sort_by(compare_candidates);
    top
}

fn scheduled_batch(
    starting_trial_id: usize,
    batch_inputs: Vec<BTreeMap<String, f64>>,
) -> OptimizeScheduledBatch {
    OptimizeScheduledBatch {
        trials: batch_inputs
            .into_iter()
            .enumerate()
            .map(|(offset, input_overrides)| OptimizeScheduledTrial {
                trial_id: starting_trial_id + offset,
                input_overrides,
            })
            .collect(),
    }
}

fn progress_state(
    config: &OptimizeConfig,
    all_candidates: &[OptimizeCandidateSummary],
    top_candidates: &[OptimizeCandidateSummary],
    pending_batch: Option<OptimizeScheduledBatch>,
) -> OptimizeProgressState {
    OptimizeProgressState {
        candidate_count: config.trials,
        completed_trials: all_candidates.len(),
        best_candidate: top_candidates.first().cloned(),
        top_candidates: top_candidates.to_vec(),
        pending_batch,
    }
}

fn emit_progress_event(
    listener: &mut Option<&mut dyn OptimizeProgressListener>,
    event: OptimizeProgressEvent,
    state: &OptimizeProgressState,
) -> Result<(), OptimizeError> {
    if let Some(listener) = listener.as_deref_mut() {
        listener
            .on_event(event, state)
            .map_err(|message| OptimizeError::ProgressCallback { message })?;
    }
    Ok(())
}

fn should_cancel(
    listener: &mut Option<&mut dyn OptimizeProgressListener>,
) -> Result<bool, OptimizeError> {
    match listener.as_deref_mut() {
        Some(listener) => listener
            .should_cancel()
            .map_err(|message| OptimizeError::ProgressCallback { message }),
        None => Ok(false),
    }
}

fn handle_candidate_completion(
    config: &OptimizeConfig,
    all_candidates: &mut Vec<OptimizeCandidateSummary>,
    top_candidates: &mut Vec<OptimizeCandidateSummary>,
    pending_batch: &Option<OptimizeScheduledBatch>,
    candidate: OptimizeCandidateSummary,
    listener: &mut Option<&mut dyn OptimizeProgressListener>,
) -> Result<(), OptimizeError> {
    let prior_best = top_candidates.first().cloned();
    insert_top_candidate(top_candidates, candidate.clone(), config.top_n);
    let entered_top_n = top_candidates
        .iter()
        .any(|existing| existing.trial_id == candidate.trial_id);
    all_candidates.push(candidate.clone());
    all_candidates.sort_by_key(|existing| existing.trial_id);
    let state = progress_state(
        config,
        all_candidates,
        top_candidates,
        pending_batch.clone(),
    );
    emit_progress_event(
        listener,
        OptimizeProgressEvent::CandidateCompleted {
            candidate: candidate.clone(),
            entered_top_n,
        },
        &state,
    )?;
    if top_candidates.first().map(|best| best.trial_id)
        != prior_best.as_ref().map(|best| best.trial_id)
    {
        emit_progress_event(
            listener,
            OptimizeProgressEvent::BestCandidateImproved { candidate },
            &state,
        )?;
    }
    Ok(())
}

fn validate_resume_state(
    config: &OptimizeConfig,
    completed_candidates: &[OptimizeCandidateSummary],
    pending_batch: Option<&OptimizeScheduledBatch>,
) -> Result<(), OptimizeError> {
    let mut seen_trial_ids = BTreeSet::new();
    for candidate in completed_candidates {
        if candidate.trial_id >= config.trials {
            return Err(OptimizeError::InvalidResumeState {
                message: format!(
                    "completed candidate trial_id {} exceeds configured trials {}",
                    candidate.trial_id, config.trials
                ),
            });
        }
        if !seen_trial_ids.insert(candidate.trial_id) {
            return Err(OptimizeError::InvalidResumeState {
                message: format!("duplicate completed trial_id {}", candidate.trial_id),
            });
        }
    }

    if let Some(batch) = pending_batch {
        if batch.trials.is_empty() {
            return Err(OptimizeError::InvalidResumeState {
                message: "pending batch must include at least one trial".to_string(),
            });
        }
        if batch.trials.len() > TPE_UPDATE_BATCH_SIZE {
            return Err(OptimizeError::InvalidResumeState {
                message: format!(
                    "pending batch has {} trials but batch size is {}",
                    batch.trials.len(),
                    TPE_UPDATE_BATCH_SIZE
                ),
            });
        }
        let mut pending_ids = batch
            .trials
            .iter()
            .map(|trial| trial.trial_id)
            .collect::<Vec<_>>();
        pending_ids.sort_unstable();
        pending_ids.dedup();
        if pending_ids.len() != batch.trials.len() {
            return Err(OptimizeError::InvalidResumeState {
                message: "pending batch contains duplicate trial ids".to_string(),
            });
        }
        let batch_start = *pending_ids.first().expect("pending batch is non-empty");
        let batch_end = pending_ids.last().expect("pending batch is non-empty") + 1;
        if batch_end > config.trials {
            return Err(OptimizeError::InvalidResumeState {
                message: format!(
                    "pending batch ends at trial {} but configured trials are {}",
                    batch_end, config.trials
                ),
            });
        }
        for expected in batch_start..batch_end {
            if !pending_ids.contains(&expected) {
                return Err(OptimizeError::InvalidResumeState {
                    message: "pending batch trial ids must be contiguous".to_string(),
                });
            }
        }
        for expected in 0..batch_start {
            if !seen_trial_ids.contains(&expected) {
                return Err(OptimizeError::InvalidResumeState {
                    message: format!(
                        "completed candidates must be contiguous before pending batch; missing trial {}",
                        expected
                    ),
                });
            }
        }
        for candidate in completed_candidates {
            if candidate.trial_id >= batch_end {
                return Err(OptimizeError::InvalidResumeState {
                    message: format!(
                        "completed trial {} appears after pending batch ending at {}",
                        candidate.trial_id, batch_end
                    ),
                });
            }
        }
    } else {
        for expected in 0..completed_candidates.len() {
            if !seen_trial_ids.contains(&expected) {
                return Err(OptimizeError::InvalidResumeState {
                    message: format!(
                        "completed candidates must be contiguous without pending batch; missing trial {}",
                        expected
                    ),
                });
            }
        }
    }

    Ok(())
}

fn replay_generation_state(
    config: &OptimizeConfig,
    completed_candidates: &[OptimizeCandidateSummary],
    pending_batch: Option<&OptimizeScheduledBatch>,
) -> Result<ReplayGenerationState, OptimizeError> {
    let mut rng = StdRng::seed_from_u64(config.seed);
    let mut seen_candidate_keys = BTreeSet::new();
    let mut generated_trial_count = 0usize;

    while generated_trial_count < completed_candidates.len() {
        let batch_len = (config.trials - generated_trial_count).min(TPE_UPDATE_BATCH_SIZE);
        let completed_before_batch = completed_candidates
            .iter()
            .filter(|candidate| candidate.trial_id < generated_trial_count)
            .cloned()
            .collect::<Vec<_>>();
        let batch_inputs = suggest_batch(
            config,
            &completed_before_batch,
            &mut seen_candidate_keys,
            &mut rng,
            generated_trial_count,
            batch_len,
        );
        generated_trial_count += batch_inputs.len();
    }

    if let Some(batch) = pending_batch {
        let batch_start = batch
            .trials
            .first()
            .map(|trial| trial.trial_id)
            .expect("validated pending batch is non-empty");
        let completed_before_batch = completed_candidates
            .iter()
            .filter(|candidate| candidate.trial_id < batch_start)
            .cloned()
            .collect::<Vec<_>>();
        let batch_len = batch.trials.len();
        let replayed = scheduled_batch(
            batch_start,
            suggest_batch(
                config,
                &completed_before_batch,
                &mut seen_candidate_keys,
                &mut rng,
                batch_start,
                batch_len,
            ),
        );
        if replayed != *batch {
            return Err(OptimizeError::InvalidResumeState {
                message: "pending batch does not match deterministic optimizer replay".to_string(),
            });
        }
        generated_trial_count = batch_start + batch_len;
    }

    Ok(ReplayGenerationState {
        rng,
        seen_candidate_keys,
        generated_trial_count,
    })
}

fn validate_optimize_config(config: &OptimizeConfig) -> Result<(), OptimizeError> {
    if config.params.is_empty() {
        return Err(OptimizeError::MissingParams);
    }
    if config.trials == 0 {
        return Err(OptimizeError::InvalidTrials {
            value: config.trials,
        });
    }
    if config.trials > MAX_OPTIMIZE_TRIALS {
        return Err(OptimizeError::TooManyTrials {
            count: config.trials,
            limit: MAX_OPTIMIZE_TRIALS,
        });
    }
    if config.startup_trials == 0 || config.startup_trials > config.trials {
        return Err(OptimizeError::InvalidStartupTrials {
            value: config.startup_trials,
        });
    }
    if config.workers == 0 {
        return Err(OptimizeError::InvalidWorkers {
            value: config.workers,
        });
    }
    if config.top_n == 0 {
        return Err(OptimizeError::InvalidTopN {
            value: config.top_n,
        });
    }
    if let Some(holdout) = &config.holdout {
        if holdout.bars == 0 {
            return Err(OptimizeError::InvalidHoldoutBars {
                value: holdout.bars,
            });
        }
    }
    if config.constraints.min_trade_count == Some(0) {
        return Err(OptimizeError::InvalidMinTradeCount { value: 0 });
    }
    if let Some(value) = config.constraints.min_sharpe_ratio {
        if !value.is_finite() {
            return Err(OptimizeError::InvalidMinSharpeRatio { value });
        }
    }
    if config.constraints.min_holdout_trade_count == Some(0) {
        return Err(OptimizeError::InvalidMinHoldoutTradeCount { value: 0 });
    }
    if let Some(rate) = config.constraints.min_holdout_pass_rate {
        if !rate.is_finite() || !(0.0..=1.0).contains(&rate) {
            return Err(OptimizeError::InvalidMinHoldoutPassRate { value: rate });
        }
    }
    if let Some(rate) = config.constraints.min_date_perturbation_positive_ratio {
        if !rate.is_finite() || !(0.0..=1.0).contains(&rate) {
            return Err(OptimizeError::InvalidMinDatePerturbationPositiveRatio { value: rate });
        }
    }
    if let Some(rate) = config.constraints.min_date_perturbation_outperform_ratio {
        if !rate.is_finite() || !(0.0..=1.0).contains(&rate) {
            return Err(OptimizeError::InvalidMinDatePerturbationOutperformRatio { value: rate });
        }
    }
    if config.holdout.is_none()
        && (config.constraints.min_holdout_trade_count.is_some()
            || config.constraints.require_positive_holdout
            || config.constraints.min_holdout_pass_rate.is_some())
    {
        let name = if config.constraints.min_holdout_trade_count.is_some() {
            "min_holdout_trade_count"
        } else if config.constraints.require_positive_holdout {
            "require_positive_holdout"
        } else {
            "min_holdout_pass_rate"
        };
        return Err(OptimizeError::HoldoutConstraintRequiresHoldout {
            name: name.to_string(),
        });
    }
    if matches!(config.runner, OptimizeRunner::WalkForward) && config.walk_forward.is_none() {
        return Err(OptimizeError::MissingParams);
    }

    let mut names = BTreeSet::new();
    for param in &config.params {
        if !names.insert(param.name().to_string()) {
            return Err(OptimizeError::DuplicateParam {
                name: param.name().to_string(),
            });
        }
        match param {
            OptimizeParamSpace::IntegerRange {
                name,
                low,
                high,
                step,
            } => {
                if low > high || *step <= 0 || (*high - *low) % *step != 0 {
                    return Err(OptimizeError::InvalidIntegerRange { name: name.clone() });
                }
            }
            OptimizeParamSpace::FloatRange {
                name,
                low,
                high,
                step,
            } => {
                let valid_step = step.is_none_or(|value| value.is_finite() && value > 0.0);
                if !low.is_finite() || !high.is_finite() || low > high || !valid_step {
                    return Err(OptimizeError::InvalidFloatRange { name: name.clone() });
                }
            }
            OptimizeParamSpace::Choice { name, values } => {
                if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
                    return Err(OptimizeError::EmptyChoice { name: name.clone() });
                }
            }
        }
    }

    Ok(())
}

fn build_candidate_constraint_summary(
    constraints: &ValidationConstraintConfig,
    summary: &OptimizeEvaluationSummary,
) -> ValidationConstraintSummary {
    let mut violations = Vec::new();
    let trade_count = candidate_trade_count(summary);
    if let Some(required) = constraints.min_trade_count {
        if trade_count < required {
            violations.push(ValidationConstraintViolation {
                kind: ValidationConstraintKind::MinTradeCount,
                actual: Some(trade_count as f64),
                required: Some(required as f64),
            });
        }
    }
    if let Some(required) = constraints.min_sharpe_ratio {
        match candidate_sharpe_ratio(summary) {
            Some(actual) if actual + crate::backtest::EPSILON >= required => {}
            actual => violations.push(ValidationConstraintViolation {
                kind: ValidationConstraintKind::MinSharpeRatio,
                actual,
                required: Some(required),
            }),
        }
    }
    if let (Some(required), Some(actual)) = (
        constraints.max_zero_trade_segments,
        candidate_zero_trade_segment_count(summary),
    ) {
        if actual > required {
            violations.push(ValidationConstraintViolation {
                kind: ValidationConstraintKind::MaxZeroTradeSegments,
                actual: Some(actual as f64),
                required: Some(required as f64),
            });
        }
    }
    ValidationConstraintSummary {
        passed: violations.is_empty(),
        violations,
    }
}

fn build_holdout_constraint_summary(
    constraints: &ValidationConstraintConfig,
    summary: &WalkForwardWindowSummary,
) -> ValidationConstraintSummary {
    let mut violations = Vec::new();
    if let Some(required) = constraints.min_holdout_trade_count {
        if summary.trade_count < required {
            violations.push(ValidationConstraintViolation {
                kind: ValidationConstraintKind::MinHoldoutTradeCount,
                actual: Some(summary.trade_count as f64),
                required: Some(required as f64),
            });
        }
    }
    if constraints.require_positive_holdout && summary.total_return <= 0.0 {
        violations.push(ValidationConstraintViolation {
            kind: ValidationConstraintKind::RequirePositiveHoldout,
            actual: Some(summary.total_return),
            required: Some(0.0),
        });
    }
    ValidationConstraintSummary {
        passed: violations.is_empty(),
        violations,
    }
}

fn build_detailed_candidate_constraint_summary(
    constraints: &ValidationConstraintConfig,
    candidate: &OptimizeCandidateSummary,
    holdout: Option<&OptimizeHoldoutResult>,
    date_perturbation: Option<&DatePerturbationDiagnostics>,
    overfitting_risk: &OverfittingRiskSummary,
    holdout_pass_rate: Option<f64>,
) -> ValidationConstraintSummary {
    let mut violations = candidate.constraints.violations.clone();
    if let Some(holdout) = holdout {
        violations.extend(holdout.constraints.violations.iter().cloned());
    }
    if let Some(required_rate) = constraints.min_holdout_pass_rate {
        let actual_rate = holdout_pass_rate.unwrap_or(0.0);
        if actual_rate + crate::backtest::EPSILON < required_rate {
            violations.push(ValidationConstraintViolation {
                kind: ValidationConstraintKind::MinHoldoutPassRate,
                actual: Some(actual_rate),
                required: Some(required_rate),
            });
        }
    }
    if let Some(required_ratio) = constraints.min_date_perturbation_positive_ratio {
        let actual_ratio = date_perturbation.and_then(date_perturbation_positive_ratio);
        match actual_ratio {
            Some(actual) if actual + crate::backtest::EPSILON >= required_ratio => {}
            actual => violations.push(ValidationConstraintViolation {
                kind: ValidationConstraintKind::MinDatePerturbationPositiveRatio,
                actual,
                required: Some(required_ratio),
            }),
        }
    }
    if let Some(required_ratio) = constraints.min_date_perturbation_outperform_ratio {
        let actual_ratio = date_perturbation.and_then(date_perturbation_outperform_ratio);
        match actual_ratio {
            Some(actual) if actual + crate::backtest::EPSILON >= required_ratio => {}
            actual => violations.push(ValidationConstraintViolation {
                kind: ValidationConstraintKind::MinDatePerturbationOutperformRatio,
                actual,
                required: Some(required_ratio),
            }),
        }
    }
    if let Some(max_allowed) = constraints.max_overfitting_risk {
        if !overfitting_risk_within_limit(overfitting_risk.level, max_allowed) {
            violations.push(ValidationConstraintViolation {
                kind: ValidationConstraintKind::MaxOverfittingRisk,
                actual: Some(overfitting_risk_level_value(overfitting_risk.level)),
                required: Some(overfitting_risk_level_value(max_allowed)),
            });
        }
    }
    ValidationConstraintSummary {
        passed: violations.is_empty(),
        violations,
    }
}

fn date_perturbation_positive_ratio(diagnostics: &DatePerturbationDiagnostics) -> Option<f64> {
    (!diagnostics.scenarios.is_empty())
        .then_some(diagnostics.positive_scenario_count as f64 / diagnostics.scenarios.len() as f64)
}

fn date_perturbation_outperform_ratio(diagnostics: &DatePerturbationDiagnostics) -> Option<f64> {
    (!diagnostics.scenarios.is_empty()).then_some(
        diagnostics.outperformed_execution_asset_count as f64 / diagnostics.scenarios.len() as f64,
    )
}

fn overfitting_risk_level_value(level: OverfittingRiskLevel) -> f64 {
    match level {
        OverfittingRiskLevel::Unknown => 3.0,
        OverfittingRiskLevel::Low => 0.0,
        OverfittingRiskLevel::Moderate => 1.0,
        OverfittingRiskLevel::High => 2.0,
    }
}

fn overfitting_risk_within_limit(
    actual: OverfittingRiskLevel,
    max_allowed: OverfittingRiskLevel,
) -> bool {
    if matches!(actual, OverfittingRiskLevel::Unknown) {
        return matches!(max_allowed, OverfittingRiskLevel::Unknown);
    }
    overfitting_risk_level_value(actual) <= overfitting_risk_level_value(max_allowed)
}

fn candidate_trade_count(summary: &OptimizeEvaluationSummary) -> usize {
    match summary {
        OptimizeEvaluationSummary::WalkForward { trade_count, .. } => *trade_count,
        OptimizeEvaluationSummary::Backtest { summary, .. } => summary.trade_count,
    }
}

fn candidate_zero_trade_segment_count(summary: &OptimizeEvaluationSummary) -> Option<usize> {
    match summary {
        OptimizeEvaluationSummary::WalkForward {
            zero_trade_segment_count,
            ..
        } => Some(*zero_trade_segment_count),
        OptimizeEvaluationSummary::Backtest { .. } => None,
    }
}

fn candidate_sharpe_ratio(summary: &OptimizeEvaluationSummary) -> Option<f64> {
    match summary {
        OptimizeEvaluationSummary::WalkForward {
            stitched_summary, ..
        } => stitched_summary.sharpe_ratio,
        OptimizeEvaluationSummary::Backtest { summary, .. } => summary.sharpe_ratio,
    }
}

fn prepare_holdout_plan(
    source: &str,
    runtime: &SourceRuntimeConfig,
    config: &OptimizeConfig,
) -> Result<Option<HoldoutPlan>, OptimizeError> {
    let Some(holdout) = &config.holdout else {
        return Ok(None);
    };
    let compiled = compile_with_input_overrides(source, &config.base_input_overrides)?;
    let execution =
        bridge::resolve_execution_source(&compiled, &config.backtest.execution_source_alias)?;
    let execution_bars = super::execution_bars(
        runtime,
        execution.source_id,
        &config.backtest.execution_source_alias,
    )?;
    if holdout.bars >= execution_bars.len() {
        return Err(OptimizeError::InvalidHoldoutWindow {
            requested: holdout.bars,
            available: execution_bars.len(),
        });
    }
    let split_index = execution_bars.len() - holdout.bars;
    let available = split_index;
    let required = match config.runner {
        OptimizeRunner::WalkForward => {
            let walk_forward = config
                .walk_forward
                .as_ref()
                .expect("validated walk-forward config");
            walk_forward.train_bars + walk_forward.test_bars
        }
        OptimizeRunner::Backtest => 1,
    };
    if available < required {
        return Err(OptimizeError::HoldoutLeavesTooFewBars {
            available,
            required,
        });
    }
    let from = execution_bars[split_index].time as i64;
    let to = execution_bars
        .last()
        .map(|bar| bar.time as i64 + 1)
        .unwrap_or(from);
    Ok(Some(HoldoutPlan {
        optimize_runtime: slice_runtime_window(runtime, i64::MIN, from),
        execution_bars,
        split_index,
        from,
        to,
        bars: holdout.bars,
    }))
}

fn evaluate_holdout(
    source: &str,
    runtime: &SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: &OptimizeConfig,
    plan: &HoldoutPlan,
    overrides: &BTreeMap<String, f64>,
) -> Result<OptimizeHoldoutResult, OptimizeError> {
    let compiled = compile_with_input_overrides(source, overrides)?;
    let prepared = bridge::prepare_backtest(
        &compiled,
        &config.backtest.execution_source_alias,
        bridge::resolve_execution_source(&compiled, &config.backtest.execution_source_alias)?
            .template,
    )?;
    let result = run_backtest_with_sources_internal(
        &compiled,
        runtime.clone(),
        vm_limits,
        config.backtest.clone(),
        false,
    )?;
    let starting_equity = if plan.split_index == 0 {
        config.backtest.initial_capital
    } else {
        result
            .equity_curve
            .get(plan.split_index - 1)
            .map(|point| point.equity)
            .unwrap_or(config.backtest.initial_capital)
    };
    let summary = super::walk_forward::summarize_window(
        &result.equity_curve,
        &result.trades,
        plan.split_index,
        result.equity_curve.len(),
        starting_equity,
    );
    let diagnostics = super::walk_forward::summarize_segment_diagnostics(
        &prepared.exports,
        &result,
        &plan.execution_bars[plan.split_index..],
        plan.split_index,
        result.equity_curve.len(),
        summary.total_return,
    );
    let constraints = build_holdout_constraint_summary(&config.constraints, &summary);
    Ok(OptimizeHoldoutResult {
        bars: plan.bars,
        from: plan.from,
        to: plan.to,
        summary,
        diagnostics,
        drift: HoldoutDriftSummary::default(),
        constraints,
    })
}

fn evaluate_top_candidate_holdouts(
    source: &str,
    runtime: &SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: &OptimizeConfig,
    plan: &HoldoutPlan,
    top_candidates: &[OptimizeCandidateSummary],
) -> Result<Vec<HoldoutCandidateEvaluation>, OptimizeError> {
    let mut evaluations = Vec::new();
    for candidate in top_candidates {
        let holdout = evaluate_holdout(
            source,
            runtime,
            vm_limits,
            config,
            plan,
            &candidate.input_overrides,
        )?;
        let drift = build_holdout_drift(&candidate.summary, &holdout.summary);
        let constraints = holdout.constraints.clone();
        evaluations.push(HoldoutCandidateEvaluation {
            trial_id: candidate.trial_id,
            input_overrides: candidate.input_overrides.clone(),
            passed: constraints.passed
                && holdout.summary.trade_count > 0
                && holdout.summary.total_return > 0.0
                && holdout.summary.win_rate >= 0.5,
            summary: holdout.summary,
            drift,
            date_perturbation: None,
            overfitting_risk: None,
            constraints,
        });
    }
    Ok(evaluations)
}

fn build_holdout_drift(
    candidate_summary: &OptimizeEvaluationSummary,
    holdout_summary: &WalkForwardWindowSummary,
) -> HoldoutDriftSummary {
    match candidate_summary {
        OptimizeEvaluationSummary::WalkForward {
            stitched_summary,
            trade_count,
            win_rate,
            ..
        } => HoldoutDriftSummary {
            total_return_delta: holdout_summary.total_return - stitched_summary.total_return,
            execution_asset_return_delta: holdout_summary.execution_asset_return
                - stitched_summary.average_execution_asset_return,
            trade_count_delta: holdout_summary.trade_count as i64 - *trade_count as i64,
            win_rate_delta: holdout_summary.win_rate - *win_rate,
            max_drawdown_delta: holdout_summary.max_drawdown - stitched_summary.max_drawdown,
        },
        OptimizeEvaluationSummary::Backtest { summary, .. } => HoldoutDriftSummary {
            total_return_delta: holdout_summary.total_return - summary.total_return,
            execution_asset_return_delta: holdout_summary.execution_asset_return
                - summary.total_return,
            trade_count_delta: holdout_summary.trade_count as i64 - summary.trade_count as i64,
            win_rate_delta: holdout_summary.win_rate - summary.win_rate,
            max_drawdown_delta: holdout_summary.max_drawdown - summary.max_drawdown,
        },
    }
}

fn build_robustness_summary(
    config: &OptimizeConfig,
    all_candidates: &[OptimizeCandidateSummary],
    best_candidate: &OptimizeCandidateSummary,
    top_candidates: &[OptimizeCandidateSummary],
    evaluations: Vec<HoldoutCandidateEvaluation>,
) -> OptimizationRobustnessSummary {
    let holdout_pass_count = evaluations
        .iter()
        .filter(|evaluation| evaluation.passed)
        .count();
    let holdout_returns = evaluations
        .iter()
        .map(|evaluation| evaluation.summary.total_return)
        .collect::<Vec<_>>();
    let holdout_pass_rate = if evaluations.is_empty() {
        None
    } else {
        Some(holdout_pass_count as f64 / evaluations.len() as f64)
    };
    OptimizationRobustnessSummary {
        top_candidate_count: top_candidates.len(),
        holdout_evaluated_count: evaluations.len(),
        holdout_pass_count,
        holdout_fail_count: evaluations.len().saturating_sub(holdout_pass_count),
        best_candidate_holdout_rank: evaluations
            .iter()
            .filter(|evaluation| evaluation.passed)
            .position(|evaluation| evaluation.trial_id == best_candidate.trial_id)
            .map(|index| index + 1),
        holdout_return_min: holdout_returns.iter().copied().reduce(f64::min),
        holdout_return_max: holdout_returns.iter().copied().reduce(f64::max),
        holdout_return_mean: if holdout_returns.is_empty() {
            None
        } else {
            Some(holdout_returns.iter().sum::<f64>() / holdout_returns.len() as f64)
        },
        holdout_pass_rate,
        parameter_stability: build_parameter_stability(
            &config.params,
            all_candidates,
            best_candidate,
            top_candidates,
            &evaluations,
        ),
        evaluations,
    }
}

fn validate_ranked_candidates(
    source: &str,
    runtime: &SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: &OptimizeConfig,
    all_candidates: &[OptimizeCandidateSummary],
    top_candidates: &mut [OptimizeCandidateSummary],
    holdout_plan: Option<&HoldoutPlan>,
) -> Result<ValidatedCandidateSelection, OptimizeError> {
    let date_validation_required = config
        .constraints
        .min_date_perturbation_positive_ratio
        .is_some()
        || config
            .constraints
            .min_date_perturbation_outperform_ratio
            .is_some();
    let base_evaluations = if let Some(plan) = holdout_plan {
        evaluate_top_candidate_holdouts(source, runtime, vm_limits, config, plan, top_candidates)?
    } else {
        Vec::new()
    };
    let holdout_pass_rate = if base_evaluations.is_empty() {
        None
    } else {
        Some(
            base_evaluations
                .iter()
                .filter(|evaluation| evaluation.passed)
                .count() as f64
                / base_evaluations.len() as f64,
        )
    };

    let mut evaluation_by_trial = base_evaluations
        .iter()
        .cloned()
        .map(|evaluation| (evaluation.trial_id, evaluation))
        .collect::<BTreeMap<_, _>>();
    let ranked_snapshot = top_candidates.to_vec();
    let mut artifacts = BTreeMap::new();
    for candidate in top_candidates.iter_mut() {
        let holdout = if let Some(plan) = holdout_plan {
            Some(evaluate_holdout(
                source,
                runtime,
                vm_limits,
                config,
                plan,
                &candidate.input_overrides,
            )?)
        } else {
            None
        };
        let date_perturbation = if date_validation_required {
            Some(run_candidate_backtest_validation(
                source,
                runtime.clone(),
                vm_limits,
                config,
                &candidate.input_overrides,
            )?)
        } else {
            None
        };
        let candidate_robustness = if holdout_plan.is_some() {
            build_robustness_summary(
                config,
                all_candidates,
                candidate,
                &ranked_snapshot,
                base_evaluations.clone(),
            )
        } else {
            OptimizationRobustnessSummary::default()
        };
        let holdout_ref = holdout.as_ref();
        let overfitting_risk =
            build_optimize_overfitting_risk(config, candidate, holdout_ref, &candidate_robustness);
        let constraints = build_detailed_candidate_constraint_summary(
            &config.constraints,
            candidate,
            holdout_ref,
            date_perturbation.as_ref(),
            &overfitting_risk,
            holdout_pass_rate,
        );
        candidate.constraints = constraints.clone();
        if let Some(evaluation) = evaluation_by_trial.get_mut(&candidate.trial_id) {
            evaluation.date_perturbation = date_perturbation.clone();
            evaluation.overfitting_risk = Some(overfitting_risk.clone());
            evaluation.constraints = constraints.clone();
        }
        artifacts.insert(
            candidate.trial_id,
            ValidatedCandidateArtifacts {
                holdout,
                overfitting_risk,
            },
        );
    }

    top_candidates.sort_by(compare_candidates);
    let best_candidate = top_candidates
        .first()
        .cloned()
        .expect("top candidates are non-empty after validation");
    let best_infeasible_candidate = top_candidates
        .iter()
        .find(|candidate| !candidate.constraints.passed)
        .cloned();
    let final_evaluations = top_candidates
        .iter()
        .filter_map(|candidate| evaluation_by_trial.remove(&candidate.trial_id))
        .collect::<Vec<_>>();
    let robustness = if holdout_plan.is_some() {
        build_robustness_summary(
            config,
            all_candidates,
            &best_candidate,
            top_candidates,
            final_evaluations,
        )
    } else {
        OptimizationRobustnessSummary::default()
    };
    let best_holdout = artifacts
        .get(&best_candidate.trial_id)
        .and_then(|artifact| artifact.holdout.clone());
    let best_overfitting_risk = artifacts
        .get(&best_candidate.trial_id)
        .map(|artifact| artifact.overfitting_risk.clone())
        .unwrap_or_default();
    let constraint_failure_breakdown = build_constraint_failure_breakdown(top_candidates);
    Ok(ValidatedCandidateSelection {
        best_holdout,
        robustness,
        best_overfitting_risk,
        best_infeasible_candidate,
        constraint_failure_breakdown,
    })
}

fn run_candidate_backtest_validation(
    source: &str,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: &OptimizeConfig,
    overrides: &BTreeMap<String, f64>,
) -> Result<DatePerturbationDiagnostics, OptimizeError> {
    let compiled = compile_with_input_overrides(source, overrides)?;
    let result = run_backtest_with_sources(&compiled, runtime, vm_limits, config.backtest.clone())?;
    Ok(result.diagnostics.date_perturbation)
}

fn run_direct_validations(
    source: &str,
    runtime: &SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: &OptimizeConfig,
    top_candidates: &[OptimizeCandidateSummary],
) -> Result<Vec<OptimizeDirectValidationResult>, OptimizeError> {
    if config.direct_validation_top_n == 0 {
        return Ok(Vec::new());
    }

    top_candidates
        .iter()
        .filter(|candidate| candidate.constraints.passed)
        .take(config.direct_validation_top_n)
        .enumerate()
        .map(|(index, candidate)| {
            let compiled = compile_with_input_overrides(source, &candidate.input_overrides)?;
            let result = run_backtest_with_sources(
                &compiled,
                runtime.clone(),
                vm_limits,
                config.backtest.clone(),
            )?;
            Ok(OptimizeDirectValidationResult {
                survivor_rank: index + 1,
                trial_id: candidate.trial_id,
                input_overrides: candidate.input_overrides.clone(),
                objective_score: candidate.objective_score,
                drift: build_direct_validation_drift(
                    &candidate.summary,
                    &result.summary,
                    &result.diagnostics.capture_summary,
                ),
                summary: result.summary,
                capture_summary: result.diagnostics.capture_summary,
                baseline_comparison: result.diagnostics.baseline_comparison,
                date_perturbation: result.diagnostics.date_perturbation,
                overfitting_risk: result.diagnostics.overfitting_risk,
                time_bucket_cohorts: result.diagnostics.cohorts.by_time_bucket_utc,
            })
        })
        .collect()
}

fn build_direct_validation_drift(
    candidate_summary: &OptimizeEvaluationSummary,
    direct_summary: &BacktestSummary,
    direct_capture_summary: &BacktestCaptureSummary,
) -> DirectValidationDriftSummary {
    match candidate_summary {
        OptimizeEvaluationSummary::WalkForward {
            stitched_summary,
            trade_count,
            win_rate,
            ..
        } => DirectValidationDriftSummary {
            total_return_delta: direct_summary.total_return - stitched_summary.total_return,
            execution_asset_return_delta: direct_capture_summary.execution_asset_return
                - stitched_summary.average_execution_asset_return,
            trade_count_delta: direct_summary.trade_count as i64 - *trade_count as i64,
            win_rate_delta: direct_summary.win_rate - *win_rate,
            max_drawdown_delta: direct_summary.max_drawdown - stitched_summary.max_drawdown,
            sharpe_ratio_delta: option_delta(
                direct_summary.sharpe_ratio,
                stitched_summary.sharpe_ratio,
            ),
        },
        OptimizeEvaluationSummary::Backtest {
            summary,
            capture_summary,
        } => DirectValidationDriftSummary {
            total_return_delta: direct_summary.total_return - summary.total_return,
            execution_asset_return_delta: direct_capture_summary.execution_asset_return
                - capture_summary.execution_asset_return,
            trade_count_delta: direct_summary.trade_count as i64 - summary.trade_count as i64,
            win_rate_delta: direct_summary.win_rate - summary.win_rate,
            max_drawdown_delta: direct_summary.max_drawdown - summary.max_drawdown,
            sharpe_ratio_delta: option_delta(direct_summary.sharpe_ratio, summary.sharpe_ratio),
        },
    }
}

fn option_delta(lhs: Option<f64>, rhs: Option<f64>) -> Option<f64> {
    match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => Some(lhs - rhs),
        _ => None,
    }
}

fn build_constraint_failure_breakdown(
    candidates: &[OptimizeCandidateSummary],
) -> Vec<ConstraintFailureBreakdown> {
    let mut counts = BTreeMap::<ValidationConstraintKind, usize>::new();
    for candidate in candidates {
        for violation in &candidate.constraints.violations {
            *counts.entry(violation.kind).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|(kind, count)| ConstraintFailureBreakdown { kind, count })
        .collect()
}

fn build_parameter_stability(
    params: &[OptimizeParamSpace],
    all_candidates: &[OptimizeCandidateSummary],
    best_candidate: &OptimizeCandidateSummary,
    top_candidates: &[OptimizeCandidateSummary],
    evaluations: &[HoldoutCandidateEvaluation],
) -> Vec<ParameterRobustnessSummary> {
    params
        .iter()
        .map(|param| {
            let top_values = top_candidates
                .iter()
                .filter_map(|candidate| candidate.input_overrides.get(param.name()).copied())
                .collect::<Vec<_>>();
            let passing_values = evaluations
                .iter()
                .filter(|evaluation| evaluation.passed)
                .filter_map(|evaluation| evaluation.input_overrides.get(param.name()).copied())
                .collect::<Vec<_>>();
            let distinct_sampled_value_count = all_candidates
                .iter()
                .filter_map(|candidate| candidate.input_overrides.get(param.name()).copied())
                .map(ordered_f64_key)
                .collect::<BTreeSet<_>>()
                .len();
            ParameterRobustnessSummary {
                name: param.name().to_string(),
                best_value: best_candidate.input_overrides.get(param.name()).copied(),
                top_ranked_min: top_values.iter().copied().reduce(f64::min),
                top_ranked_max: top_values.iter().copied().reduce(f64::max),
                holdout_passing_min: passing_values.iter().copied().reduce(f64::min),
                holdout_passing_max: passing_values.iter().copied().reduce(f64::max),
                distinct_sampled_value_count,
            }
        })
        .collect()
}

fn build_optimize_hints(
    best_candidate: &OptimizeCandidateSummary,
    holdout: Option<&OptimizeHoldoutResult>,
    robustness: &OptimizationRobustnessSummary,
) -> Vec<ImprovementHint> {
    let mut hints = Vec::new();
    if let Some(holdout) = holdout {
        if holdout.summary.trade_count == 0 || holdout.summary.total_return <= 0.0 {
            hints.push(ImprovementHint {
                kind: ImprovementHintKind::HoldoutCollapse,
                metric: Some("holdout_total_return".to_string()),
                value: Some(holdout.summary.total_return),
            });
        }
    }
    if robustness.holdout_evaluated_count > 0 && robustness.holdout_pass_count <= 1 {
        hints.push(ImprovementHint {
            kind: ImprovementHintKind::EdgeConcentrated,
            metric: Some("holdout_pass_count".to_string()),
            value: Some(robustness.holdout_pass_count as f64),
        });
    }
    let trade_count = match &best_candidate.summary {
        OptimizeEvaluationSummary::WalkForward { trade_count, .. } => *trade_count,
        OptimizeEvaluationSummary::Backtest { summary, .. } => summary.trade_count,
    };
    if trade_count < 5 {
        hints.push(ImprovementHint {
            kind: ImprovementHintKind::TooFewTrades,
            metric: Some("trade_count".to_string()),
            value: Some(trade_count as f64),
        });
    }
    hints
}

fn evaluate_scheduled_batch<F>(
    source: &str,
    runtime: &SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: &OptimizeConfig,
    batch: &OptimizeScheduledBatch,
    completed_trial_ids: &BTreeSet<usize>,
    on_candidate: &mut F,
) -> Result<(), OptimizeError>
where
    F: FnMut(OptimizeCandidateSummary) -> Result<(), OptimizeError>,
{
    for chunk in batch.trials.chunks(config.workers.max(1)) {
        let chunk_results = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for trial in chunk.iter() {
                if completed_trial_ids.contains(&trial.trial_id) {
                    continue;
                }
                let source = source.to_string();
                let runtime = runtime.clone();
                let config = config.clone();
                let overrides = trial.input_overrides.clone();
                let trial_id = trial.trial_id;
                handles.push(scope.spawn(move || {
                    evaluate_candidate(&source, runtime, vm_limits, config, trial_id, overrides)
                }));
            }
            let mut chunk_results = Vec::with_capacity(handles.len());
            for handle in handles {
                let candidate = handle.join().map_err(|_| OptimizeError::WorkerPanicked)??;
                chunk_results.push(candidate);
            }
            Ok::<_, OptimizeError>(chunk_results)
        })?;
        for candidate in chunk_results {
            on_candidate(candidate)?;
        }
    }
    Ok(())
}

fn evaluate_candidate(
    source: &str,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: OptimizeConfig,
    trial_id: usize,
    overrides: BTreeMap<String, f64>,
) -> Result<OptimizeCandidateSummary, OptimizeError> {
    let compiled = compile_with_input_overrides(source, &overrides)?;
    let (summary, time_bucket_cohorts) = match config.runner {
        OptimizeRunner::WalkForward => {
            let result = run_walk_forward_with_sources(
                &compiled,
                runtime,
                vm_limits,
                config
                    .walk_forward
                    .clone()
                    .expect("validated walk-forward config"),
            )?;
            (
                summarize_walk_forward_candidate(&result),
                aggregate_time_bucket_summaries(
                    result
                        .segments
                        .iter()
                        .map(|segment| &segment.out_of_sample_diagnostics.cohorts),
                ),
            )
        }
        OptimizeRunner::Backtest => {
            let result = run_backtest_with_sources_internal(
                &compiled,
                runtime,
                vm_limits,
                config.backtest,
                false,
            )?;
            (
                OptimizeEvaluationSummary::Backtest {
                    summary: result.summary,
                    capture_summary: result.diagnostics.capture_summary,
                },
                result.diagnostics.cohorts.by_time_bucket_utc,
            )
        }
    };
    let constraints = build_candidate_constraint_summary(&config.constraints, &summary);
    let objective_score = score_candidate(config.objective, &summary);
    Ok(OptimizeCandidateSummary {
        trial_id,
        input_overrides: overrides,
        objective_score,
        summary,
        time_bucket_cohorts,
        constraints,
    })
}

fn summarize_walk_forward_candidate(result: &WalkForwardResult) -> OptimizeEvaluationSummary {
    let zero_trade_segment_count = result
        .segments
        .iter()
        .filter(|segment| segment.out_of_sample.trade_count == 0)
        .count();
    OptimizeEvaluationSummary::WalkForward {
        stitched_summary: result.stitched_summary.clone(),
        zero_trade_segment_count,
        trade_count: result.stitched_summary.trade_count,
        winning_trade_count: result.stitched_summary.winning_trade_count,
        losing_trade_count: result.stitched_summary.losing_trade_count,
        win_rate: result.stitched_summary.win_rate,
    }
}

fn score_candidate(objective: OptimizeObjective, summary: &OptimizeEvaluationSummary) -> f64 {
    match summary {
        OptimizeEvaluationSummary::WalkForward {
            stitched_summary,
            zero_trade_segment_count,
            trade_count: _,
            winning_trade_count: _,
            losing_trade_count: _,
            win_rate: _,
        } => score_walk_forward_candidate(objective, stitched_summary, *zero_trade_segment_count),
        OptimizeEvaluationSummary::Backtest {
            summary,
            capture_summary: _,
        } => score_backtest_candidate(objective, summary),
    }
}

fn score_walk_forward_candidate(
    objective: OptimizeObjective,
    summary: &WalkForwardStitchedSummary,
    zero_trade_segment_count: usize,
) -> f64 {
    let drawdown_pct = if summary.starting_equity > 0.0 {
        summary.max_drawdown / summary.starting_equity
    } else {
        0.0
    };
    let segment_count = summary.segment_count.max(1) as f64;
    let negative_segment_ratio = summary.negative_segment_count as f64 / segment_count;
    let zero_trade_segment_ratio = zero_trade_segment_count as f64 / segment_count;
    match objective {
        OptimizeObjective::RobustReturn => {
            summary.total_return
                - 0.50 * drawdown_pct
                - 0.25 * negative_segment_ratio
                - 0.10 * zero_trade_segment_ratio
        }
        OptimizeObjective::TotalReturn => summary.total_return,
        OptimizeObjective::EndingEquity => summary.ending_equity,
        OptimizeObjective::ReturnOverDrawdown => {
            if summary.max_drawdown <= 0.0 {
                summary.total_return
            } else {
                summary.total_return / drawdown_pct.max(MIN_DENSITY)
            }
        }
    }
}

fn score_backtest_candidate(objective: OptimizeObjective, summary: &BacktestSummary) -> f64 {
    let drawdown_pct = if summary.starting_equity > 0.0 {
        summary.max_drawdown / summary.starting_equity
    } else {
        0.0
    };
    match objective {
        OptimizeObjective::RobustReturn => summary.total_return - 0.50 * drawdown_pct,
        OptimizeObjective::TotalReturn => summary.total_return,
        OptimizeObjective::EndingEquity => summary.ending_equity,
        OptimizeObjective::ReturnOverDrawdown => {
            if summary.max_drawdown <= 0.0 {
                summary.total_return
            } else {
                summary.total_return / drawdown_pct.max(MIN_DENSITY)
            }
        }
    }
}

fn suggest_batch(
    config: &OptimizeConfig,
    completed: &[OptimizeCandidateSummary],
    seen_candidate_keys: &mut BTreeSet<String>,
    rng: &mut StdRng,
    _starting_trial_id: usize,
    batch_len: usize,
) -> Vec<BTreeMap<String, f64>> {
    let mut batch = Vec::with_capacity(batch_len);
    let existing_keys = completed
        .iter()
        .map(|candidate| candidate_key(&candidate.input_overrides))
        .collect::<BTreeSet<_>>();
    for _ in 0..batch_len {
        let candidate = if completed.len() < config.startup_trials {
            sample_unique_candidate(config, rng, seen_candidate_keys, &existing_keys)
        } else {
            sample_tpe_candidate(config, completed, rng, seen_candidate_keys, &existing_keys)
        };
        seen_candidate_keys.insert(candidate_key(&candidate));
        batch.push(candidate);
    }
    batch
}

fn sample_unique_candidate(
    config: &OptimizeConfig,
    rng: &mut StdRng,
    batch_seen: &BTreeSet<String>,
    existing: &BTreeSet<String>,
) -> BTreeMap<String, f64> {
    let mut last = BTreeMap::new();
    for _ in 0..64 {
        let candidate = random_candidate(config, rng);
        let key = candidate_key(&candidate);
        if !batch_seen.contains(&key) && !existing.contains(&key) {
            return candidate;
        }
        last = candidate;
    }
    last
}

fn sample_tpe_candidate(
    config: &OptimizeConfig,
    completed: &[OptimizeCandidateSummary],
    rng: &mut StdRng,
    batch_seen: &BTreeSet<String>,
    existing: &BTreeSet<String>,
) -> BTreeMap<String, f64> {
    let (good, bad) = split_trials(completed);
    let mut best_candidate = None;
    let mut best_score = f64::NEG_INFINITY;
    for _ in 0..TPE_CANDIDATES_PER_TRIAL {
        let mut candidate = config.base_input_overrides.clone();
        let mut acquisition = 0.0;
        for param in &config.params {
            let (value, contribution) = sample_param_value(param, &good, &bad, rng);
            candidate.insert(param.name().to_string(), value);
            acquisition += contribution;
        }
        let key = candidate_key(&candidate);
        if batch_seen.contains(&key) || existing.contains(&key) {
            continue;
        }
        if acquisition > best_score {
            best_score = acquisition;
            best_candidate = Some(candidate);
        }
    }
    best_candidate.unwrap_or_else(|| sample_unique_candidate(config, rng, batch_seen, existing))
}

fn random_candidate(config: &OptimizeConfig, rng: &mut StdRng) -> BTreeMap<String, f64> {
    let mut candidate = config.base_input_overrides.clone();
    for param in &config.params {
        candidate.insert(param.name().to_string(), param.sample_random(rng));
    }
    candidate
}

fn split_trials(
    completed: &[OptimizeCandidateSummary],
) -> (
    Vec<&OptimizeCandidateSummary>,
    Vec<&OptimizeCandidateSummary>,
) {
    let mut sorted = completed.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| compare_candidates(left, right));
    let good_len = ((sorted.len() as f64) * GOOD_TRIAL_FRACTION).ceil() as usize;
    let good_len = good_len.clamp(1, sorted.len().max(1));
    let good = sorted[..good_len].to_vec();
    let bad = if good_len < sorted.len() {
        sorted[good_len..].to_vec()
    } else {
        good.clone()
    };
    (good, bad)
}

fn sample_param_value(
    param: &OptimizeParamSpace,
    good: &[&OptimizeCandidateSummary],
    bad: &[&OptimizeCandidateSummary],
    rng: &mut StdRng,
) -> (f64, f64) {
    match param {
        OptimizeParamSpace::Choice { name, values } => {
            sample_choice_value(name, values, good, bad, rng)
        }
        OptimizeParamSpace::IntegerRange { name, .. }
        | OptimizeParamSpace::FloatRange { name, .. } => {
            sample_numeric_value(name, param, good, bad, rng)
        }
    }
}

fn sample_choice_value(
    name: &str,
    values: &[f64],
    good: &[&OptimizeCandidateSummary],
    bad: &[&OptimizeCandidateSummary],
    rng: &mut StdRng,
) -> (f64, f64) {
    let probabilities = smoothed_choice_probabilities(name, values, good);
    let value = sample_discrete(values, &probabilities, rng);
    let p_good = choice_probability(name, value, values, good);
    let p_bad = choice_probability(name, value, values, bad);
    (
        value,
        (p_good + MIN_DENSITY).ln() - (p_bad + MIN_DENSITY).ln(),
    )
}

fn sample_numeric_value(
    name: &str,
    param: &OptimizeParamSpace,
    good: &[&OptimizeCandidateSummary],
    bad: &[&OptimizeCandidateSummary],
    rng: &mut StdRng,
) -> (f64, f64) {
    let good_values = trial_values(name, good);
    let bad_values = trial_values(name, bad);
    if good_values.is_empty() {
        let value = param.sample_random(rng);
        return (value, 0.0);
    }
    let bandwidth_good = kernel_bandwidth(&good_values, param.span());
    let bandwidth_bad = kernel_bandwidth(&bad_values, param.span());
    let pivot = good_values[rng.gen_range(0..good_values.len())];
    let sampled = sample_normal_clamped(rng, pivot, bandwidth_good, param);
    let l = gaussian_mixture_density(sampled, &good_values, bandwidth_good);
    let g = gaussian_mixture_density(sampled, &bad_values, bandwidth_bad);
    (sampled, (l + MIN_DENSITY).ln() - (g + MIN_DENSITY).ln())
}

fn trial_values(name: &str, trials: &[&OptimizeCandidateSummary]) -> Vec<f64> {
    trials
        .iter()
        .filter_map(|trial| trial.input_overrides.get(name).copied())
        .collect()
}

fn smoothed_choice_probabilities(
    name: &str,
    values: &[f64],
    trials: &[&OptimizeCandidateSummary],
) -> Vec<f64> {
    let mut counts = vec![1.0; values.len()];
    for trial in trials {
        if let Some(value) = trial.input_overrides.get(name) {
            if let Some(index) = values.iter().position(|candidate| candidate == value) {
                counts[index] += 1.0;
            }
        }
    }
    let total = counts.iter().sum::<f64>();
    counts.into_iter().map(|count| count / total).collect()
}

fn choice_probability(
    name: &str,
    value: f64,
    values: &[f64],
    trials: &[&OptimizeCandidateSummary],
) -> f64 {
    let probabilities = smoothed_choice_probabilities(name, values, trials);
    values
        .iter()
        .position(|candidate| *candidate == value)
        .map(|index| probabilities[index])
        .unwrap_or(MIN_DENSITY)
}

fn sample_discrete(values: &[f64], probabilities: &[f64], rng: &mut StdRng) -> f64 {
    let mut cumulative = 0.0;
    let target = rng.gen_range(0.0..1.0);
    for (index, probability) in probabilities.iter().enumerate() {
        cumulative += *probability;
        if target <= cumulative {
            return values[index];
        }
    }
    *values.last().expect("choice values are non-empty")
}

fn kernel_bandwidth(values: &[f64], span: f64) -> f64 {
    if values.len() <= 1 {
        return (span / 6.0).max(MIN_BANDWIDTH);
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| {
            let delta = *value - mean;
            delta * delta
        })
        .sum::<f64>()
        / values.len() as f64;
    let sigma = variance.sqrt();
    let silverman = 1.06 * sigma * (values.len() as f64).powf(-0.2);
    silverman.max((span / 20.0).max(MIN_BANDWIDTH))
}

fn sample_normal_clamped(
    rng: &mut StdRng,
    mean: f64,
    std_dev: f64,
    param: &OptimizeParamSpace,
) -> f64 {
    let distribution = Normal::new(mean, std_dev.max(MIN_BANDWIDTH))
        .expect("validated normal distribution parameters");
    param.clamp(distribution.sample(rng))
}

fn gaussian_mixture_density(value: f64, samples: &[f64], bandwidth: f64) -> f64 {
    if samples.is_empty() {
        return MIN_DENSITY;
    }
    let sigma = bandwidth.max(MIN_BANDWIDTH);
    let coefficient = 1.0 / (sigma * (2.0 * PI).sqrt());
    let density = samples
        .iter()
        .map(|sample| {
            let z = (value - *sample) / sigma;
            coefficient * (-0.5 * z * z).exp()
        })
        .sum::<f64>();
    density / samples.len() as f64
}

fn insert_top_candidate(
    top_candidates: &mut Vec<OptimizeCandidateSummary>,
    candidate: OptimizeCandidateSummary,
    top_n: usize,
) {
    top_candidates.push(candidate);
    top_candidates.sort_by(compare_candidates);
    if top_candidates.len() > top_n {
        top_candidates.truncate(top_n);
    }
}

fn compare_candidates(
    left: &OptimizeCandidateSummary,
    right: &OptimizeCandidateSummary,
) -> Ordering {
    right
        .constraints
        .passed
        .cmp(&left.constraints.passed)
        .then_with(|| {
            right
                .objective_score
                .partial_cmp(&left.objective_score)
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| {
            candidate_ending_equity(&right.summary)
                .partial_cmp(&candidate_ending_equity(&left.summary))
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| left.trial_id.cmp(&right.trial_id))
}

fn candidate_ending_equity(summary: &OptimizeEvaluationSummary) -> f64 {
    match summary {
        OptimizeEvaluationSummary::WalkForward {
            stitched_summary, ..
        } => stitched_summary.ending_equity,
        OptimizeEvaluationSummary::Backtest { summary, .. } => summary.ending_equity,
    }
}

fn candidate_key(overrides: &BTreeMap<String, f64>) -> String {
    overrides
        .iter()
        .map(|(name, value)| format!("{name}:{:016x}", value.to_bits()))
        .collect::<Vec<_>>()
        .join("|")
}

fn ordered_f64_key(value: f64) -> u64 {
    value.to_bits()
}

fn closest_choice(values: &[f64], target: f64) -> f64 {
    values
        .iter()
        .copied()
        .min_by(|left, right| {
            (left - target)
                .abs()
                .partial_cmp(&(right - target).abs())
                .unwrap_or(Ordering::Equal)
        })
        .unwrap_or(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::DatePerturbationScenarioSummary;

    fn backtest_candidate(
        trial_id: usize,
        objective_score: f64,
        ending_equity: f64,
        constraints_passed: bool,
    ) -> OptimizeCandidateSummary {
        OptimizeCandidateSummary {
            trial_id,
            input_overrides: BTreeMap::new(),
            objective_score,
            summary: OptimizeEvaluationSummary::Backtest {
                summary: BacktestSummary {
                    starting_equity: 1_000.0,
                    ending_equity,
                    realized_pnl: ending_equity - 1_000.0,
                    unrealized_pnl: 0.0,
                    total_return: (ending_equity - 1_000.0) / 1_000.0,
                    sharpe_ratio: None,
                    trade_count: 1,
                    winning_trade_count: usize::from(ending_equity >= 1_000.0),
                    losing_trade_count: usize::from(ending_equity < 1_000.0),
                    win_rate: if ending_equity >= 1_000.0 { 1.0 } else { 0.0 },
                    max_drawdown: 0.0,
                    max_gross_exposure: 0.0,
                    max_net_exposure: 0.0,
                    peak_open_position_count: 1,
                },
                capture_summary: BacktestCaptureSummary::default(),
            },
            time_bucket_cohorts: Vec::new(),
            constraints: ValidationConstraintSummary {
                passed: constraints_passed,
                violations: Vec::new(),
            },
        }
    }

    #[test]
    fn compare_candidates_prefers_constraint_passing_entries() {
        let mut candidates = [
            backtest_candidate(1, 10.0, 1_100.0, false),
            backtest_candidate(2, 5.0, 1_050.0, true),
        ];

        candidates.sort_by(compare_candidates);

        assert_eq!(candidates[0].trial_id, 2);
        assert_eq!(candidates[1].trial_id, 1);
    }

    #[test]
    fn detailed_constraints_include_date_perturbation_ratios() {
        let candidate = backtest_candidate(1, 1.0, 1_100.0, true);
        let diagnostics = DatePerturbationDiagnostics {
            offset_bars: 10,
            scenarios: vec![
                DatePerturbationScenarioSummary {
                    kind: crate::backtest::DatePerturbationKind::LateStart,
                    from: 0,
                    to: 1,
                    total_return: 0.02,
                    execution_asset_return: 0.0,
                    excess_return_vs_execution_asset: 0.02,
                    trade_count: 1,
                    max_drawdown: 1.0,
                },
                DatePerturbationScenarioSummary {
                    kind: crate::backtest::DatePerturbationKind::EarlyEnd,
                    from: 0,
                    to: 1,
                    total_return: -0.01,
                    execution_asset_return: 0.0,
                    excess_return_vs_execution_asset: -0.01,
                    trade_count: 1,
                    max_drawdown: 1.0,
                },
                DatePerturbationScenarioSummary {
                    kind: crate::backtest::DatePerturbationKind::TrimmedBoth,
                    from: 0,
                    to: 1,
                    total_return: 0.03,
                    execution_asset_return: 0.01,
                    excess_return_vs_execution_asset: 0.02,
                    trade_count: 1,
                    max_drawdown: 1.0,
                },
            ],
            positive_scenario_count: 2,
            outperformed_execution_asset_count: 2,
            ..DatePerturbationDiagnostics::default()
        };
        let constraints = build_detailed_candidate_constraint_summary(
            &ValidationConstraintConfig {
                min_trade_count: None,
                min_sharpe_ratio: None,
                min_holdout_trade_count: None,
                require_positive_holdout: false,
                max_zero_trade_segments: None,
                min_holdout_pass_rate: None,
                min_date_perturbation_positive_ratio: Some(0.9),
                min_date_perturbation_outperform_ratio: Some(0.6),
                max_overfitting_risk: None,
            },
            &candidate,
            None,
            Some(&diagnostics),
            &OverfittingRiskSummary::default(),
            None,
        );

        assert!(!constraints.passed);
        assert!(constraints.violations.iter().any(|violation| {
            violation.kind == ValidationConstraintKind::MinDatePerturbationPositiveRatio
        }));
        assert!(!constraints.violations.iter().any(|violation| {
            violation.kind == ValidationConstraintKind::MinDatePerturbationOutperformRatio
        }));
    }
}
