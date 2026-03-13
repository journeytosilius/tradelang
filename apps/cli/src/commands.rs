use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::thread;

use palmscript::{
    bytecode::InputOptimizationDeclKind, compile, compile_with_input_overrides,
    fetch_perp_backtest_context, fetch_source_runtime_config, run_backtest_with_sources,
    run_optimize_with_source, run_walk_forward_sweep_with_source, run_walk_forward_with_sources,
    run_with_sources, BacktestConfig, CompiledProgram, DiagnosticsDetailMode, ExchangeEndpoints,
    InputSweepDefinition, OptimizeConfig, OptimizeError, OptimizeHoldoutConfig, OptimizeObjective,
    OptimizeParamSpace, OptimizePreset, OptimizeResult, OptimizeRunner, PerpBacktestConfig,
    PerpMarginMode, RuntimeError, SourceTemplate, VmLimits, WalkForwardConfig,
    WalkForwardSweepConfig, WalkForwardSweepError, WalkForwardSweepObjective,
};
use sha2::{Digest, Sha256};

use crate::args::{
    BacktestMarginMode, BacktestRunArgs, BytecodeFormat, CheckArgs, Cli, Command,
    DiagnosticsDetailArg, DocsArgs, DumpBytecodeArgs, MarketRunArgs, OptimizeObjectiveArg,
    OptimizeRunArgs, OptimizeRunnerArg, OutputFormat, RunCommand, WalkForwardRunArgs,
    WalkForwardSweepObjectiveArg, WalkForwardSweepRunArgs,
};
use crate::diagnostics::{format_compile_error, format_runtime_error};
use crate::docs;
use crate::format::{
    render_backtest_text, render_bytecode_text, render_optimize_text, render_outputs_text,
    render_walk_forward_sweep_text, render_walk_forward_text,
};

type ResolvedPerpContexts = (
    Option<PerpBacktestConfig>,
    Option<palmscript::PerpBacktestContext>,
    BTreeMap<String, palmscript::PerpBacktestContext>,
);

pub fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Docs(args) => print_docs(args),
        Command::Run { mode } => run_mode(*mode),
        Command::Check(args) => check_script(args),
        Command::DumpBytecode(args) => dump_bytecode(args),
    }
}

fn print_docs(args: DocsArgs) -> Result<(), String> {
    print!("{}", docs::render(&args)?);
    Ok(())
}

fn run_mode(mode: RunCommand) -> Result<(), String> {
    match mode {
        RunCommand::Market(args) => run_market(args),
        RunCommand::Backtest(args) => run_backtest(args),
        RunCommand::WalkForward(args) => run_walk_forward(args),
        RunCommand::WalkForwardSweep(args) => run_walk_forward_sweep(args),
        RunCommand::Optimize(args) => run_optimize(args),
    }
}

fn run_market(args: MarketRunArgs) -> Result<(), String> {
    let source = load_source(&args.script)?;
    let compiled = compile(&source).map_err(|err| format_compile_error(&args.script, &err))?;
    if compiled.program.declared_sources.is_empty() {
        return Err("market mode requires at least one `source` declaration".to_string());
    }
    let config = fetch_source_runtime_config(
        &compiled,
        args.from,
        args.to,
        &ExchangeEndpoints::from_env(),
    )
    .map_err(|err| format!("market mode error: {err}"))?;
    let outputs = run_with_sources(
        &compiled,
        config,
        VmLimits {
            max_instructions_per_bar: args.max_instructions_per_bar,
            max_history_capacity: args.max_history_capacity,
        },
    )
    .map_err(|err| format_runtime_error(&err))?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&outputs).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_outputs_text(&outputs)),
    }
    Ok(())
}

fn run_backtest(args: BacktestRunArgs) -> Result<(), String> {
    let source = load_source(&args.script)?;
    let preset = load_preset(&source, &args.script, args.preset.as_deref())?;
    let compiled = compile_with_preset_overrides(&source, &args.script, preset.as_ref())?;
    if compiled.program.declared_sources.is_empty() {
        return Err("backtest mode requires at least one `source` declaration".to_string());
    }
    let execution_source_aliases =
        resolve_execution_source_aliases(&compiled, &args.execution_source)?;
    let execution_source_alias = execution_source_aliases[0].clone();
    let endpoints = ExchangeEndpoints::from_env();
    let runtime = fetch_source_runtime_config(&compiled, args.from, args.to, &endpoints)
        .map_err(|err| format!("backtest mode error: {err}"))?;
    let (perp, perp_context, portfolio_perp_contexts) = resolve_backtest_perp_contexts(
        &compiled,
        &execution_source_aliases,
        PerpCliOptions {
            from: args.from,
            to: args.to,
            leverage: args.leverage,
            margin_mode: args.margin_mode,
        },
        &endpoints,
    )?;
    let result = run_backtest_with_sources(
        &compiled,
        runtime,
        VmLimits {
            max_instructions_per_bar: args.max_instructions_per_bar,
            max_history_capacity: args.max_history_capacity,
        },
        BacktestConfig {
            execution_source_alias,
            portfolio_execution_aliases: if execution_source_aliases.len() > 1 {
                execution_source_aliases.clone()
            } else {
                Vec::new()
            },
            initial_capital: args.initial_capital,
            fee_bps: args.fee_bps,
            slippage_bps: args.slippage_bps,
            diagnostics_detail: map_diagnostics_detail(args.diagnostics),
            perp,
            perp_context,
            portfolio_perp_contexts,
        },
    )
    .map_err(|err| format!("backtest mode error: {err}"))?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&result).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_backtest_text(&result)),
    }
    Ok(())
}

fn run_walk_forward(args: WalkForwardRunArgs) -> Result<(), String> {
    let source = load_source(&args.script)?;
    let preset = load_preset(&source, &args.script, args.preset.as_deref())?;
    let compiled = compile_with_preset_overrides(&source, &args.script, preset.as_ref())?;
    if compiled.program.declared_sources.is_empty() {
        return Err("walk-forward mode requires at least one `source` declaration".to_string());
    }
    let execution_source_aliases =
        resolve_execution_source_aliases(&compiled, &args.execution_source)?;
    let execution_source_alias = execution_source_aliases[0].clone();
    let endpoints = ExchangeEndpoints::from_env();
    let runtime = fetch_source_runtime_config(&compiled, args.from, args.to, &endpoints)
        .map_err(|err| format!("walk-forward mode error: {err}"))?;
    let (perp, perp_context, portfolio_perp_contexts) = resolve_backtest_perp_contexts(
        &compiled,
        &execution_source_aliases,
        PerpCliOptions {
            from: args.from,
            to: args.to,
            leverage: args.leverage,
            margin_mode: args.margin_mode,
        },
        &endpoints,
    )?;
    let result = run_walk_forward_with_sources(
        &compiled,
        runtime,
        VmLimits {
            max_instructions_per_bar: args.max_instructions_per_bar,
            max_history_capacity: args.max_history_capacity,
        },
        WalkForwardConfig {
            backtest: BacktestConfig {
                execution_source_alias,
                portfolio_execution_aliases: if execution_source_aliases.len() > 1 {
                    execution_source_aliases.clone()
                } else {
                    Vec::new()
                },
                initial_capital: args.initial_capital,
                fee_bps: args.fee_bps,
                slippage_bps: args.slippage_bps,
                diagnostics_detail: map_diagnostics_detail(args.diagnostics),
                perp,
                perp_context,
                portfolio_perp_contexts,
            },
            diagnostics_detail: map_diagnostics_detail(args.diagnostics),
            train_bars: args.train_bars,
            test_bars: args.test_bars,
            step_bars: args.step_bars.unwrap_or(args.test_bars),
        },
    )
    .map_err(|err| format!("walk-forward mode error: {err}"))?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&result).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_walk_forward_text(&result)),
    }
    Ok(())
}

fn run_walk_forward_sweep(args: WalkForwardSweepRunArgs) -> Result<(), String> {
    let source = load_source(&args.script)?;
    let preset = load_preset(&source, &args.script, args.preset.as_deref())?;
    let compiled = compile_with_preset_overrides(&source, &args.script, preset.as_ref())?;
    if compiled.program.declared_sources.is_empty() {
        return Err(
            "walk-forward sweep mode requires at least one `source` declaration".to_string(),
        );
    }
    let execution_source_aliases =
        resolve_execution_source_aliases(&compiled, &args.execution_source)?;
    let execution_source_alias = execution_source_aliases[0].clone();
    let endpoints = ExchangeEndpoints::from_env();
    let runtime = fetch_source_runtime_config(&compiled, args.from, args.to, &endpoints)
        .map_err(|err| format!("walk-forward sweep mode error: {err}"))?;
    let (perp, perp_context, portfolio_perp_contexts) = resolve_backtest_perp_contexts(
        &compiled,
        &execution_source_aliases,
        PerpCliOptions {
            from: args.from,
            to: args.to,
            leverage: args.leverage,
            margin_mode: args.margin_mode,
        },
        &endpoints,
    )?;
    let result = run_walk_forward_sweep_with_source(
        &source,
        runtime,
        VmLimits {
            max_instructions_per_bar: args.max_instructions_per_bar,
            max_history_capacity: args.max_history_capacity,
        },
        WalkForwardSweepConfig {
            walk_forward: WalkForwardConfig {
                backtest: BacktestConfig {
                    execution_source_alias,
                    portfolio_execution_aliases: if execution_source_aliases.len() > 1 {
                        execution_source_aliases.clone()
                    } else {
                        Vec::new()
                    },
                    initial_capital: args.initial_capital,
                    fee_bps: args.fee_bps,
                    slippage_bps: args.slippage_bps,
                    diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
                    perp,
                    perp_context,
                    portfolio_perp_contexts,
                },
                diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
                train_bars: args.train_bars,
                test_bars: args.test_bars,
                step_bars: args.step_bars.unwrap_or(args.test_bars),
            },
            inputs: parse_input_sweep_definitions(&args.sets)?,
            objective: map_sweep_objective(args.objective),
            top_n: args.top,
            base_input_overrides: preset
                .as_ref()
                .map(|preset| preset.best_input_overrides.clone())
                .unwrap_or_default(),
        },
    )
    .map_err(|err| format_walk_forward_sweep_error(&args.script, err))?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&result).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_walk_forward_sweep_text(&result)),
    }
    Ok(())
}

fn run_optimize(args: OptimizeRunArgs) -> Result<(), String> {
    let source = load_source(&args.script)?;
    let preset = load_preset(&source, &args.script, args.preset.as_deref())?;
    let preset_overrides = preset
        .as_ref()
        .map(|preset| preset.best_input_overrides.clone())
        .unwrap_or_default();
    let base_compiled = compile_with_input_overrides(&source, &preset_overrides)
        .map_err(|err| format_compile_error(&args.script, &err))?;
    if base_compiled.program.declared_sources.is_empty() {
        return Err("optimize mode requires at least one `source` declaration".to_string());
    }

    let params = resolve_optimize_params(&args, preset.as_ref(), &base_compiled)?;
    let execution_source_aliases =
        resolve_execution_source_aliases(&base_compiled, &args.execution_source)?;
    let execution_source_alias = execution_source_aliases[0].clone();
    let endpoints = ExchangeEndpoints::from_env();
    let runtime = fetch_source_runtime_config(&base_compiled, args.from, args.to, &endpoints)
        .map_err(|err| format!("optimize mode error: {err}"))?;
    let (perp, perp_context, portfolio_perp_contexts) = resolve_backtest_perp_contexts(
        &base_compiled,
        &execution_source_aliases,
        PerpCliOptions {
            from: args.from,
            to: args.to,
            leverage: args.leverage,
            margin_mode: args.margin_mode,
        },
        &endpoints,
    )?;
    let backtest = BacktestConfig {
        execution_source_alias,
        portfolio_execution_aliases: if execution_source_aliases.len() > 1 {
            execution_source_aliases.clone()
        } else {
            Vec::new()
        },
        initial_capital: args.initial_capital,
        fee_bps: args.fee_bps,
        slippage_bps: args.slippage_bps,
        diagnostics_detail: map_diagnostics_detail(args.diagnostics),
        perp,
        perp_context,
        portfolio_perp_contexts,
    };
    let walk_forward = match map_optimize_runner(args.runner) {
        OptimizeRunner::WalkForward => Some(WalkForwardConfig {
            backtest: backtest.clone(),
            diagnostics_detail: map_diagnostics_detail(args.diagnostics),
            train_bars: args
                .train_bars
                .ok_or_else(|| "optimize walk-forward runner requires --train-bars".to_string())?,
            test_bars: args
                .test_bars
                .ok_or_else(|| "optimize walk-forward runner requires --test-bars".to_string())?,
            step_bars: args.step_bars.unwrap_or_else(|| {
                args.test_bars
                    .expect("validated optimize walk-forward test_bars")
            }),
        }),
        OptimizeRunner::Backtest => None,
    };
    let trials = args.trials;
    let startup_trials = args
        .startup_trials
        .unwrap_or_else(|| default_startup_trials(trials));
    let workers = args.workers.unwrap_or_else(default_parallel_workers);
    let result = run_optimize_with_source(
        &source,
        runtime,
        VmLimits {
            max_instructions_per_bar: args.max_instructions_per_bar,
            max_history_capacity: args.max_history_capacity,
        },
        OptimizeConfig {
            runner: map_optimize_runner(args.runner),
            backtest: backtest.clone(),
            walk_forward: walk_forward.clone(),
            diagnostics_detail: map_diagnostics_detail(args.diagnostics),
            holdout: resolve_optimize_holdout(&args, preset.as_ref())?,
            params: params.clone(),
            objective: map_optimize_objective(args.objective),
            trials,
            startup_trials,
            seed: args.seed,
            workers,
            top_n: args.top,
            base_input_overrides: preset_overrides,
        },
    )
    .map_err(format_optimize_error)?;
    if let Some(path) = args.preset_out.as_deref() {
        write_optimize_preset(path, &args.script, &source, &result)?;
    }
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&result).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!(
            "{}",
            render_optimize_text(&result, args.preset_out.as_deref())
        ),
    }
    Ok(())
}

fn check_script(args: CheckArgs) -> Result<(), String> {
    let source = load_source(&args.script)?;
    compile_source(&source, &args.script)?;
    println!("{}: ok", args.script.display());
    Ok(())
}

fn dump_bytecode(args: DumpBytecodeArgs) -> Result<(), String> {
    let source = load_source(&args.script)?;
    let compiled = compile_source(&source, &args.script)?;
    match args.format {
        BytecodeFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&compiled).map_err(|err| err.to_string())?
        ),
        BytecodeFormat::Text => print!("{}", render_bytecode_text(&compiled)),
    }
    Ok(())
}

fn compile_source(source: &str, path: &Path) -> Result<CompiledProgram, String> {
    compile(source).map_err(|err| format_compile_error(path, &err))
}

pub(crate) fn compile_with_preset_overrides(
    source: &str,
    path: &Path,
    preset: Option<&OptimizePreset>,
) -> Result<CompiledProgram, String> {
    let overrides = preset
        .map(|preset| preset.best_input_overrides.clone())
        .unwrap_or_default();
    compile_with_input_overrides(source, &overrides).map_err(|err| format_compile_error(path, &err))
}

pub(crate) fn load_source(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("failed to read `{}`: {err}", path.display()))
}

pub(crate) fn load_preset(
    source: &str,
    script_path: &Path,
    preset_path: Option<&Path>,
) -> Result<Option<OptimizePreset>, String> {
    let Some(preset_path) = preset_path else {
        return Ok(None);
    };
    let raw = fs::read_to_string(preset_path)
        .map_err(|err| format!("failed to read preset `{}`: {err}", preset_path.display()))?;
    let preset = serde_json::from_str::<OptimizePreset>(&raw)
        .map_err(|err| format!("failed to parse preset `{}`: {err}", preset_path.display()))?;
    let script_hash = hash_source(source);
    if preset.script_sha256 != script_hash {
        return Err(format!(
            "preset `{}` does not match script `{}` (sha256 mismatch)",
            preset_path.display(),
            script_path.display()
        ));
    }
    Ok(Some(preset))
}

pub(crate) fn resolve_execution_source_aliases(
    compiled: &CompiledProgram,
    provided: &[String],
) -> Result<Vec<String>, String> {
    if !provided.is_empty() {
        return Ok(provided.to_vec());
    }
    match compiled.program.declared_sources.as_slice() {
        [source] => Ok(vec![source.alias.clone()]),
        _ => Err(
            "this mode requires --execution-source when the script declares multiple `source`s"
                .to_string(),
        ),
    }
}

fn resolve_backtest_perp_contexts(
    compiled: &CompiledProgram,
    execution_source_aliases: &[String],
    options: PerpCliOptions,
    endpoints: &ExchangeEndpoints,
) -> Result<ResolvedPerpContexts, String> {
    let mut shared_perp = None;
    let mut single_context = None;
    let mut portfolio_perp_contexts = BTreeMap::new();

    for alias in execution_source_aliases {
        let source = compiled
            .program
            .declared_sources
            .iter()
            .find(|source| source.alias == *alias)
            .ok_or_else(|| format!("unknown execution source `{alias}`"))?;
        let (perp, context) = resolve_perp_context(
            source.template,
            source,
            compiled.program.base_interval,
            options,
            endpoints,
        )?;
        if shared_perp.is_none() {
            shared_perp = perp.clone();
        }
        if let Some(context) = context {
            if execution_source_aliases.len() == 1 {
                single_context = Some(context.clone());
            }
            portfolio_perp_contexts.insert(alias.clone(), context);
        }
    }

    Ok((shared_perp, single_context, portfolio_perp_contexts))
}

fn parse_input_sweep_definitions(raw_sets: &[String]) -> Result<Vec<InputSweepDefinition>, String> {
    let mut inputs = Vec::with_capacity(raw_sets.len());
    for raw in raw_sets {
        let (name, values) = raw
            .split_once('=')
            .ok_or_else(|| format!("invalid `--set {raw}`: expected name=v1,v2,..."))?;
        if name.is_empty() {
            return Err(format!("invalid `--set {raw}`: missing input name"));
        }
        let mut parsed_values = Vec::new();
        for raw_value in values.split(',') {
            if raw_value.is_empty() {
                return Err(format!("invalid `--set {raw}`: empty value"));
            }
            let value = raw_value.parse::<f64>().map_err(|err| {
                format!("invalid `--set {raw}`: failed to parse `{raw_value}` as number: {err}")
            })?;
            if !value.is_finite() {
                return Err(format!(
                    "invalid `--set {raw}`: `{raw_value}` must be a finite number"
                ));
            }
            parsed_values.push(value);
        }
        inputs.push(InputSweepDefinition {
            name: name.to_string(),
            values: parsed_values,
        });
    }
    Ok(inputs)
}

pub(crate) fn resolve_optimize_params(
    args: &OptimizeRunArgs,
    preset: Option<&OptimizePreset>,
    compiled: &CompiledProgram,
) -> Result<Vec<OptimizeParamSpace>, String> {
    if !args.params.is_empty() {
        return args
            .params
            .iter()
            .map(|raw| parse_optimize_param_space(raw))
            .collect();
    }
    if let Some(preset) = preset {
        return Ok(preset.parameter_space.clone());
    }
    let inferred = infer_optimize_params(compiled);
    if !inferred.is_empty() {
        return Ok(inferred);
    }
    Err(
        "optimize mode requires at least one `--param`, a preset parameter space, or `input ... optimize(...)` metadata".to_string(),
    )
}

pub(crate) fn resolve_optimize_holdout(
    args: &OptimizeRunArgs,
    preset: Option<&OptimizePreset>,
) -> Result<Option<OptimizeHoldoutConfig>, String> {
    if args.no_holdout {
        return Ok(None);
    }
    if let Some(bars) = args.holdout_bars {
        return Ok(Some(OptimizeHoldoutConfig { bars }));
    }
    if let Some(holdout) = preset.and_then(|preset| preset.holdout.clone()) {
        return Ok(Some(holdout));
    }
    if matches!(
        map_optimize_runner(args.runner),
        OptimizeRunner::WalkForward
    ) {
        let inferred_test_bars = args
            .test_bars
            .or_else(|| preset.and_then(|value| value.walk_forward.as_ref().map(|wf| wf.test_bars)))
            .ok_or_else(|| {
                "optimize walk-forward runner requires --test-bars so the default holdout can be reserved"
                    .to_string()
            })?;
        return Ok(Some(OptimizeHoldoutConfig {
            bars: inferred_test_bars,
        }));
    }
    Ok(None)
}

pub(crate) fn parse_optimize_param_space(raw: &str) -> Result<OptimizeParamSpace, String> {
    let (kind, rest) = raw
        .split_once(':')
        .ok_or_else(|| format!("invalid `--param {raw}`: expected kind:name=..."))?;
    let (name, values) = rest
        .split_once('=')
        .ok_or_else(|| format!("invalid `--param {raw}`: expected kind:name=..."))?;
    if name.is_empty() {
        return Err(format!("invalid `--param {raw}`: missing input name"));
    }
    match kind {
        "int" => {
            let parts = values.split(':').collect::<Vec<_>>();
            if !(2..=3).contains(&parts.len()) {
                return Err(format!(
                    "invalid `--param {raw}`: expected int:name=low:high[:step]"
                ));
            }
            let low_raw = parts[0];
            let high_raw = parts[1];
            let low = low_raw.parse::<i64>().map_err(|err| {
                format!("invalid `--param {raw}`: failed to parse `{low_raw}` as integer: {err}")
            })?;
            let high = high_raw.parse::<i64>().map_err(|err| {
                format!("invalid `--param {raw}`: failed to parse `{high_raw}` as integer: {err}")
            })?;
            let step = match parts.get(2) {
                Some(step_raw) => step_raw.parse::<i64>().map_err(|err| {
                    format!(
                        "invalid `--param {raw}`: failed to parse `{step_raw}` as integer: {err}"
                    )
                })?,
                None => 1,
            };
            Ok(OptimizeParamSpace::IntegerRange {
                name: name.to_string(),
                low,
                high,
                step,
            })
        }
        "float" => {
            let parts = values.split(':').collect::<Vec<_>>();
            if !(2..=3).contains(&parts.len()) {
                return Err(format!(
                    "invalid `--param {raw}`: expected float:name=low:high[:step]"
                ));
            }
            let low_raw = parts[0];
            let high_raw = parts[1];
            let low = low_raw.parse::<f64>().map_err(|err| {
                format!("invalid `--param {raw}`: failed to parse `{low_raw}` as float: {err}")
            })?;
            let high = high_raw.parse::<f64>().map_err(|err| {
                format!("invalid `--param {raw}`: failed to parse `{high_raw}` as float: {err}")
            })?;
            let step = match parts.get(2) {
                Some(step_raw) => Some(step_raw.parse::<f64>().map_err(|err| {
                    format!("invalid `--param {raw}`: failed to parse `{step_raw}` as float: {err}")
                })?),
                None => None,
            };
            Ok(OptimizeParamSpace::FloatRange {
                name: name.to_string(),
                low,
                high,
                step,
            })
        }
        "choice" => {
            let mut parsed_values = Vec::new();
            for raw_value in values.split(',') {
                if raw_value.is_empty() {
                    return Err(format!("invalid `--param {raw}`: empty choice value"));
                }
                let value = raw_value.parse::<f64>().map_err(|err| {
                    format!(
                        "invalid `--param {raw}`: failed to parse `{raw_value}` as number: {err}"
                    )
                })?;
                if !value.is_finite() {
                    return Err(format!(
                        "invalid `--param {raw}`: `{raw_value}` must be a finite number"
                    ));
                }
                parsed_values.push(value);
            }
            Ok(OptimizeParamSpace::Choice {
                name: name.to_string(),
                values: parsed_values,
            })
        }
        _ => Err(format!(
            "invalid `--param {raw}`: kind must be one of int, float, choice"
        )),
    }
}

fn infer_optimize_params(compiled: &CompiledProgram) -> Vec<OptimizeParamSpace> {
    compiled
        .program
        .inputs
        .iter()
        .filter_map(|input| {
            let optimization = input.optimization.as_ref()?;
            Some(match &optimization.kind {
                InputOptimizationDeclKind::IntegerRange { low, high, step } => {
                    OptimizeParamSpace::IntegerRange {
                        name: input.name.clone(),
                        low: *low,
                        high: *high,
                        step: *step,
                    }
                }
                InputOptimizationDeclKind::FloatRange { low, high, step } => {
                    OptimizeParamSpace::FloatRange {
                        name: input.name.clone(),
                        low: *low,
                        high: *high,
                        step: *step,
                    }
                }
                InputOptimizationDeclKind::Choice { values } => OptimizeParamSpace::Choice {
                    name: input.name.clone(),
                    values: values.clone(),
                },
            })
        })
        .collect()
}

fn map_sweep_objective(objective: WalkForwardSweepObjectiveArg) -> WalkForwardSweepObjective {
    match objective {
        WalkForwardSweepObjectiveArg::TotalReturn => WalkForwardSweepObjective::TotalReturn,
        WalkForwardSweepObjectiveArg::EndingEquity => WalkForwardSweepObjective::EndingEquity,
        WalkForwardSweepObjectiveArg::ReturnOverDrawdown => {
            WalkForwardSweepObjective::ReturnOverDrawdown
        }
    }
}

pub(crate) fn map_optimize_runner(runner: OptimizeRunnerArg) -> OptimizeRunner {
    match runner {
        OptimizeRunnerArg::WalkForward => OptimizeRunner::WalkForward,
        OptimizeRunnerArg::Backtest => OptimizeRunner::Backtest,
    }
}

pub(crate) fn map_optimize_objective(objective: OptimizeObjectiveArg) -> OptimizeObjective {
    match objective {
        OptimizeObjectiveArg::RobustReturn => OptimizeObjective::RobustReturn,
        OptimizeObjectiveArg::TotalReturn => OptimizeObjective::TotalReturn,
        OptimizeObjectiveArg::EndingEquity => OptimizeObjective::EndingEquity,
        OptimizeObjectiveArg::ReturnOverDrawdown => OptimizeObjective::ReturnOverDrawdown,
    }
}

pub(crate) fn map_diagnostics_detail(detail: DiagnosticsDetailArg) -> DiagnosticsDetailMode {
    match detail {
        DiagnosticsDetailArg::Summary => DiagnosticsDetailMode::SummaryOnly,
        DiagnosticsDetailArg::FullTrace => DiagnosticsDetailMode::FullTrace,
    }
}

fn format_walk_forward_sweep_error(path: &Path, error: WalkForwardSweepError) -> String {
    match error {
        WalkForwardSweepError::Compile(err) => format_compile_error(path, &err),
        other => format!("walk-forward sweep mode error: {other}"),
    }
}

fn format_optimize_error(error: OptimizeError) -> String {
    match error {
        OptimizeError::Compile(err) => format!("optimize mode error: {err}"),
        OptimizeError::Backtest(err) => format!("optimize mode error: {err}"),
        other => format!("optimize mode error: {other}"),
    }
}

pub(crate) fn default_parallel_workers() -> usize {
    thread::available_parallelism()
        .map(|parallelism| parallelism.get().min(4))
        .unwrap_or(1)
}

pub(crate) fn default_startup_trials(trials: usize) -> usize {
    16.min((trials / 3).max(8)).min(trials)
}

pub(crate) fn write_optimize_preset(
    path: &Path,
    script_path: &Path,
    source: &str,
    result: &OptimizeResult,
) -> Result<(), String> {
    let preset = OptimizePreset {
        version: 1,
        script_path: Some(script_path.display().to_string()),
        script_sha256: hash_source(source),
        runner: result.config.runner,
        objective: result.config.objective,
        backtest: result.config.backtest.clone(),
        walk_forward: result.config.walk_forward.clone(),
        diagnostics_detail: result.config.diagnostics_detail,
        holdout: result.config.holdout.clone(),
        parameter_space: result.config.params.clone(),
        best_input_overrides: result.best_candidate.input_overrides.clone(),
        top_candidates: result.top_candidates.clone(),
    };
    let json = serde_json::to_string_pretty(&preset)
        .map_err(|err| format!("preset serialize error: {err}"))?;
    fs::write(path, json)
        .map_err(|err| format!("failed to write preset `{}`: {err}", path.display()))
}

pub(crate) fn hash_source(source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Clone, Copy)]
pub(crate) struct PerpCliOptions {
    pub(crate) from: i64,
    pub(crate) to: i64,
    pub(crate) leverage: Option<f64>,
    pub(crate) margin_mode: Option<BacktestMarginMode>,
}

pub(crate) fn resolve_perp_context(
    template: SourceTemplate,
    source: &palmscript::DeclaredMarketSource,
    base_interval: Option<palmscript::Interval>,
    options: PerpCliOptions,
    endpoints: &ExchangeEndpoints,
) -> Result<
    (
        Option<PerpBacktestConfig>,
        Option<palmscript::PerpBacktestContext>,
    ),
    String,
> {
    let margin_mode = match options.margin_mode.unwrap_or(BacktestMarginMode::Isolated) {
        BacktestMarginMode::Isolated => PerpMarginMode::Isolated,
    };
    match template {
        SourceTemplate::BinanceSpot | SourceTemplate::BybitSpot | SourceTemplate::GateSpot => {
            if options.leverage.is_some() || !matches!(margin_mode, PerpMarginMode::Isolated) {
                return Err(format!(
                    "spot source `{}` does not accept --leverage or --margin-mode",
                    source.alias
                ));
            }
            Ok((None, None))
        }
        SourceTemplate::BinanceUsdm
        | SourceTemplate::BybitUsdtPerps
        | SourceTemplate::GateUsdtPerps => {
            let interval = base_interval.ok_or_else(|| {
                format!(
                    "perp backtest for `{}` requires a base interval declaration",
                    source.alias
                )
            })?;
            let perp = PerpBacktestConfig {
                leverage: options.leverage.unwrap_or(1.0),
                margin_mode,
            };
            let context =
                fetch_perp_backtest_context(source, interval, options.from, options.to, endpoints)
                    .map_err(|err| format!("perp context error: {err}"))?;
            Ok((Some(perp), context))
        }
    }
}

#[allow(dead_code)]
fn _runtime_error(_err: RuntimeError) -> String {
    unreachable!()
}
