use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::backtest::{
    run_walk_forward_with_sources, BacktestError, WalkForwardConfig, WalkForwardStitchedSummary,
};
use crate::compiler::compile_with_input_overrides;
use crate::diagnostic::CompileError;
use crate::runtime::{SourceRuntimeConfig, VmLimits};

const MAX_SWEEP_CANDIDATES: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalkForwardSweepObjective {
    TotalReturn,
    EndingEquity,
    ReturnOverDrawdown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InputSweepDefinition {
    pub name: String,
    pub values: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WalkForwardSweepConfig {
    pub walk_forward: WalkForwardConfig,
    pub inputs: Vec<InputSweepDefinition>,
    pub objective: WalkForwardSweepObjective,
    pub top_n: usize,
    pub base_input_overrides: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WalkForwardSweepCandidateSummary {
    pub input_overrides: BTreeMap<String, f64>,
    pub stitched_summary: WalkForwardStitchedSummary,
    pub objective_score: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WalkForwardSweepResult {
    pub config: WalkForwardSweepConfig,
    pub candidate_count: usize,
    pub best_candidate: WalkForwardSweepCandidateSummary,
    pub top_candidates: Vec<WalkForwardSweepCandidateSummary>,
}

#[derive(Debug, Error)]
pub enum WalkForwardSweepError {
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error(transparent)]
    Backtest(#[from] BacktestError),
    #[error("walk-forward sweep requires at least one `--set` input grid")]
    MissingInputs,
    #[error("walk-forward sweep top_n must be > 0, found {value}")]
    InvalidTopN { value: usize },
    #[error("walk-forward sweep input `{name}` must include at least one value")]
    EmptyInputValues { name: String },
    #[error("walk-forward sweep input `{name}` is defined more than once")]
    DuplicateInput { name: String },
    #[error("walk-forward sweep candidate count {count} exceeds max supported {limit}")]
    TooManyCandidates { count: usize, limit: usize },
}

pub fn run_walk_forward_sweep_with_source(
    source: &str,
    runtime: SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: WalkForwardSweepConfig,
) -> Result<WalkForwardSweepResult, WalkForwardSweepError> {
    let candidate_count = validate_sweep_config(&config)?;
    let mut current = BTreeMap::new();
    let mut top_candidates = Vec::new();
    enumerate_candidates(
        source,
        &runtime,
        vm_limits,
        &config,
        0,
        &mut current,
        &mut top_candidates,
    )?;
    top_candidates.sort_by(compare_candidates);
    let best_candidate = top_candidates
        .first()
        .cloned()
        .expect("validated sweep config always yields at least one candidate");
    Ok(WalkForwardSweepResult {
        config,
        candidate_count,
        best_candidate,
        top_candidates,
    })
}

fn validate_sweep_config(config: &WalkForwardSweepConfig) -> Result<usize, WalkForwardSweepError> {
    if config.inputs.is_empty() {
        return Err(WalkForwardSweepError::MissingInputs);
    }
    if config.top_n == 0 {
        return Err(WalkForwardSweepError::InvalidTopN {
            value: config.top_n,
        });
    }
    let mut names = std::collections::BTreeSet::new();
    let mut candidate_count = 1usize;
    for input in &config.inputs {
        if !names.insert(input.name.clone()) {
            return Err(WalkForwardSweepError::DuplicateInput {
                name: input.name.clone(),
            });
        }
        if input.values.is_empty() {
            return Err(WalkForwardSweepError::EmptyInputValues {
                name: input.name.clone(),
            });
        }
        candidate_count = candidate_count.checked_mul(input.values.len()).ok_or(
            WalkForwardSweepError::TooManyCandidates {
                count: usize::MAX,
                limit: MAX_SWEEP_CANDIDATES,
            },
        )?;
        if candidate_count > MAX_SWEEP_CANDIDATES {
            return Err(WalkForwardSweepError::TooManyCandidates {
                count: candidate_count,
                limit: MAX_SWEEP_CANDIDATES,
            });
        }
    }
    Ok(candidate_count)
}

fn enumerate_candidates(
    source: &str,
    runtime: &SourceRuntimeConfig,
    vm_limits: VmLimits,
    config: &WalkForwardSweepConfig,
    input_index: usize,
    current: &mut BTreeMap<String, f64>,
    top_candidates: &mut Vec<WalkForwardSweepCandidateSummary>,
) -> Result<(), WalkForwardSweepError> {
    if input_index == config.inputs.len() {
        let mut overrides = config.base_input_overrides.clone();
        for (name, value) in current.iter() {
            overrides.insert(name.clone(), *value);
        }
        let compiled = compile_with_input_overrides(source, &overrides)?;
        let result = run_walk_forward_with_sources(
            &compiled,
            runtime.clone(),
            vm_limits,
            config.walk_forward.clone(),
        )?;
        let candidate = WalkForwardSweepCandidateSummary {
            input_overrides: overrides,
            objective_score: score_candidate(config.objective, &result.stitched_summary),
            stitched_summary: result.stitched_summary,
        };
        insert_top_candidate(top_candidates, candidate, config.top_n);
        return Ok(());
    }

    let input = &config.inputs[input_index];
    for value in &input.values {
        current.insert(input.name.clone(), *value);
        enumerate_candidates(
            source,
            runtime,
            vm_limits,
            config,
            input_index + 1,
            current,
            top_candidates,
        )?;
    }
    current.remove(&input.name);
    Ok(())
}

fn score_candidate(
    objective: WalkForwardSweepObjective,
    summary: &WalkForwardStitchedSummary,
) -> f64 {
    match objective {
        WalkForwardSweepObjective::TotalReturn => summary.total_return,
        WalkForwardSweepObjective::EndingEquity => summary.ending_equity,
        WalkForwardSweepObjective::ReturnOverDrawdown => {
            if summary.max_drawdown <= 0.0 {
                summary.total_return
            } else {
                summary.total_return / (summary.max_drawdown / summary.starting_equity)
            }
        }
    }
}

fn insert_top_candidate(
    top_candidates: &mut Vec<WalkForwardSweepCandidateSummary>,
    candidate: WalkForwardSweepCandidateSummary,
    top_n: usize,
) {
    top_candidates.push(candidate);
    top_candidates.sort_by(compare_candidates);
    if top_candidates.len() > top_n {
        top_candidates.truncate(top_n);
    }
}

fn compare_candidates(
    left: &WalkForwardSweepCandidateSummary,
    right: &WalkForwardSweepCandidateSummary,
) -> Ordering {
    right
        .objective_score
        .partial_cmp(&left.objective_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| {
            right
                .stitched_summary
                .ending_equity
                .partial_cmp(&left.stitched_summary.ending_equity)
                .unwrap_or(Ordering::Equal)
        })
}
