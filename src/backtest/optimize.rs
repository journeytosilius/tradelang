use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;
use std::thread;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, Normal};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::backtest::{
    run_backtest_with_sources, run_walk_forward_with_sources, BacktestCaptureSummary,
    BacktestConfig, BacktestError, BacktestSummary, WalkForwardConfig, WalkForwardResult,
    WalkForwardStitchedSummary,
};
use crate::compiler::compile_with_input_overrides;
use crate::diagnostic::CompileError;
use crate::runtime::{SourceRuntimeConfig, VmLimits};

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
    IntegerRange { name: String, low: i64, high: i64 },
    FloatRange { name: String, low: f64, high: f64 },
    Choice { name: String, values: Vec<f64> },
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
            Self::IntegerRange { low, high, .. } => rng.gen_range(*low..=*high) as f64,
            Self::FloatRange { low, high, .. } => rng.gen_range(*low..=*high),
            Self::Choice { values, .. } => {
                let index = rng.gen_range(0..values.len());
                values[index]
            }
        }
    }

    fn clamp(&self, value: f64) -> f64 {
        match self {
            Self::IntegerRange { low, high, .. } => value.round().clamp(*low as f64, *high as f64),
            Self::FloatRange { low, high, .. } => value.clamp(*low, *high),
            Self::Choice { values, .. } => closest_choice(values, value),
        }
    }

    fn span(&self) -> f64 {
        match self {
            Self::IntegerRange { low, high, .. } => (*high - *low).max(1) as f64,
            Self::FloatRange { low, high, .. } => (*high - *low).abs().max(MIN_BANDWIDTH),
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizeConfig {
    pub runner: OptimizeRunner,
    pub backtest: BacktestConfig,
    pub walk_forward: Option<WalkForwardConfig>,
    pub params: Vec<OptimizeParamSpace>,
    pub objective: OptimizeObjective,
    pub trials: usize,
    pub startup_trials: usize,
    pub seed: u64,
    pub workers: usize,
    pub top_n: usize,
    pub base_input_overrides: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "runner_summary_kind", rename_all = "snake_case")]
pub enum OptimizeEvaluationSummary {
    WalkForward {
        stitched_summary: WalkForwardStitchedSummary,
        zero_trade_segment_count: usize,
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
    pub parameter_space: Vec<OptimizeParamSpace>,
    pub best_input_overrides: BTreeMap<String, f64>,
    pub top_candidates: Vec<OptimizeCandidateSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptimizeResult {
    pub config: OptimizeConfig,
    pub candidate_count: usize,
    pub completed_trials: usize,
    pub best_candidate: OptimizeCandidateSummary,
    pub top_candidates: Vec<OptimizeCandidateSummary>,
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
    #[error("optimize trial count {count} exceeds max supported {limit}")]
    TooManyTrials { count: usize, limit: usize },
    #[error("optimize parameter `{name}` is defined more than once")]
    DuplicateParam { name: String },
    #[error("optimize integer parameter `{name}` must use low <= high")]
    InvalidIntegerRange { name: String },
    #[error("optimize float parameter `{name}` must use finite low/high with low <= high")]
    InvalidFloatRange { name: String },
    #[error("optimize choice parameter `{name}` must include at least one finite value")]
    EmptyChoice { name: String },
    #[error("optimizer worker thread panicked")]
    WorkerPanicked,
}

pub fn run_optimize_with_source(
    source: &str,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: OptimizeConfig,
) -> Result<OptimizeResult, OptimizeError> {
    validate_optimize_config(&config)?;
    let mut rng = StdRng::seed_from_u64(config.seed);
    let mut all_candidates = Vec::with_capacity(config.trials);
    let mut top_candidates = Vec::new();
    let mut seen_candidate_keys = BTreeSet::new();

    let mut next_trial_id = 0usize;
    while next_trial_id < config.trials {
        let batch_len = (config.trials - next_trial_id).min(TPE_UPDATE_BATCH_SIZE);
        let batch_inputs = suggest_batch(
            &config,
            &all_candidates,
            &mut seen_candidate_keys,
            &mut rng,
            next_trial_id,
            batch_len,
        );
        let mut batch_results = evaluate_batch(
            source,
            &runtime,
            vm_limits,
            &config,
            next_trial_id,
            batch_inputs,
        )?;
        batch_results.sort_by_key(|candidate| candidate.trial_id);
        for candidate in batch_results {
            insert_top_candidate(&mut top_candidates, candidate.clone(), config.top_n);
            all_candidates.push(candidate);
        }
        next_trial_id += batch_len;
    }

    top_candidates.sort_by(compare_candidates);
    let best_candidate = top_candidates
        .first()
        .cloned()
        .expect("validated optimize config always yields at least one trial");
    Ok(OptimizeResult {
        candidate_count: config.trials,
        completed_trials: all_candidates.len(),
        config,
        best_candidate,
        top_candidates,
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
            OptimizeParamSpace::IntegerRange { name, low, high } => {
                if low > high {
                    return Err(OptimizeError::InvalidIntegerRange { name: name.clone() });
                }
            }
            OptimizeParamSpace::FloatRange { name, low, high } => {
                if !low.is_finite() || !high.is_finite() || low > high {
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

fn evaluate_batch(
    source: &str,
    runtime: &SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: &OptimizeConfig,
    starting_trial_id: usize,
    batch_inputs: Vec<BTreeMap<String, f64>>,
) -> Result<Vec<OptimizeCandidateSummary>, OptimizeError> {
    let mut results = Vec::with_capacity(batch_inputs.len());
    let mut batch_offset = 0usize;
    for chunk in batch_inputs.chunks(config.workers.max(1)) {
        let chunk_results = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for (offset, overrides) in chunk.iter().enumerate() {
                let source = source.to_string();
                let runtime = runtime.clone();
                let config = config.clone();
                let overrides = overrides.clone();
                let trial_id = starting_trial_id + batch_offset + offset;
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
        results.extend(chunk_results);
        batch_offset += chunk.len();
    }
    Ok(results)
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
    let summary = match config.runner {
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
            summarize_walk_forward_candidate(&result)
        }
        OptimizeRunner::Backtest => {
            let result = run_backtest_with_sources(&compiled, runtime, vm_limits, config.backtest)?;
            OptimizeEvaluationSummary::Backtest {
                summary: result.summary,
                capture_summary: result.diagnostics.capture_summary,
            }
        }
    };
    let objective_score = score_candidate(config.objective, &summary);
    Ok(OptimizeCandidateSummary {
        trial_id,
        input_overrides: overrides,
        objective_score,
        summary,
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
    }
}

fn score_candidate(objective: OptimizeObjective, summary: &OptimizeEvaluationSummary) -> f64 {
    match summary {
        OptimizeEvaluationSummary::WalkForward {
            stitched_summary,
            zero_trade_segment_count,
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
        .objective_score
        .partial_cmp(&left.objective_score)
        .unwrap_or(Ordering::Equal)
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
