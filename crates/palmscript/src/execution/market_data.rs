use std::collections::BTreeMap;
use std::thread;

use crate::backtest::{PerpBacktestConfig, PerpBacktestContext, PerpMarginMode};
use crate::compiler::CompiledProgram;
use crate::exchange::{fetch_perp_backtest_context, ExchangeEndpoints};
use crate::interval::{DeclaredMarketSource, Interval, SourceTemplate};

use super::ExecutionError;

const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

pub(crate) type ResolvedPerpContexts = (
    Option<PerpBacktestConfig>,
    Option<PerpBacktestContext>,
    BTreeMap<String, PerpBacktestContext>,
);

pub(crate) struct PerpBootstrapOptions {
    pub(crate) leverage: Option<f64>,
    pub(crate) margin_mode: PerpMarginMode,
    pub(crate) base_interval: Interval,
    pub(crate) from_ms: i64,
    pub(crate) to_ms: i64,
}

pub(crate) fn resolve_perp_contexts(
    compiled: &CompiledProgram,
    execution_aliases: &[String],
    options: PerpBootstrapOptions,
    endpoints: &ExchangeEndpoints,
) -> Result<ResolvedPerpContexts, ExecutionError> {
    let mut shared_perp = None;
    let mut single_context = None;
    let mut portfolio_contexts = BTreeMap::new();
    let mut perp_specs = Vec::new();
    for (index, alias) in execution_aliases.iter().enumerate() {
        let source = compiled
            .program
            .declared_executions
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
                perp_specs.push((index, alias.clone(), source.clone()));
            }
        }
    }

    let worker_count = crate::exchange::historical_fetch_workers(perp_specs.len());
    let mut perp_results = Vec::with_capacity(perp_specs.len());
    for chunk in perp_specs.chunks(worker_count.max(1)) {
        let chunk_results = thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for (index, alias, source) in chunk {
                let alias = alias.clone();
                let source = source.clone();
                let endpoints = endpoints.clone();
                handles.push((
                    alias.clone(),
                    source.template.as_str(),
                    source.symbol.clone(),
                    scope.spawn(move || {
                        let perp = PerpBacktestConfig {
                            leverage: options.leverage.unwrap_or(1.0),
                            margin_mode: options.margin_mode,
                        };
                        let context = fetch_perp_backtest_context(
                            &source,
                            options.base_interval,
                            options.from_ms,
                            options.to_ms,
                            &endpoints,
                        )
                        .map_err(|err| ExecutionError::Fetch(err.to_string()))?
                        .ok_or_else(|| {
                            ExecutionError::Fetch(format!(
                                "missing perp backtest context for execution alias `{}`",
                                source.alias
                            ))
                        })?;
                        Ok::<_, ExecutionError>((*index, alias, perp, context))
                    }),
                ));
            }
            let mut results = Vec::with_capacity(handles.len());
            for (alias, template, symbol, handle) in handles {
                let result = handle.join().map_err(|_| {
                    ExecutionError::Runtime(format!(
                        "perp context worker panicked for `{alias}` ({template}) `{symbol}`"
                    ))
                })??;
                results.push(result);
            }
            Ok::<_, ExecutionError>(results)
        })?;
        perp_results.extend(chunk_results);
    }
    perp_results.sort_by_key(|(index, _, _, _)| *index);

    for (_, alias, perp, context) in perp_results {
        if shared_perp.is_none() {
            shared_perp = Some(perp.clone());
        }
        if execution_aliases.len() == 1 {
            single_context = Some(context.clone());
        }
        portfolio_contexts.insert(alias, context);
    }
    Ok((shared_perp, single_context, portfolio_contexts))
}

pub(crate) fn compute_warmup_from_ms(compiled: &CompiledProgram, start_time_ms: i64) -> i64 {
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

pub(crate) fn interval_duration_hint_ms(interval: Interval) -> i64 {
    interval.fixed_duration_ms().unwrap_or(31 * DAY_MS)
}

pub(crate) fn resolve_execution_sources<'a>(
    compiled: &'a CompiledProgram,
    aliases: &[String],
) -> Result<Vec<&'a DeclaredMarketSource>, ExecutionError> {
    if compiled.program.declared_executions.is_empty() {
        return Err(ExecutionError::InvalidConfig {
            message: "paper execution requires at least one declared `execution` target"
                .to_string(),
        });
    }
    aliases
        .iter()
        .map(|alias| {
            compiled
                .program
                .declared_executions
                .iter()
                .find(|source| source.alias == *alias)
                .ok_or_else(|| ExecutionError::InvalidConfig {
                    message: format!("unknown execution source `{alias}`"),
                })
        })
        .collect()
}
