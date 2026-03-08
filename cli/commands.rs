use std::fs;
use std::path::Path;
use std::thread;

use palmscript::{
    compile, compile_with_input_overrides, fetch_perp_backtest_context,
    fetch_source_runtime_config, run_backtest_with_sources, run_optimize_with_source,
    run_walk_forward_sweep_with_source, run_walk_forward_with_sources, run_with_sources,
    BacktestConfig, CompiledProgram, ExchangeEndpoints, InputSweepDefinition, OptimizeConfig,
    OptimizeError, OptimizeObjective, OptimizeParamSpace, OptimizePreset, OptimizeResult,
    OptimizeRunner, PerpBacktestConfig, PerpMarginMode, RuntimeError, SourceTemplate, VmLimits,
    WalkForwardConfig, WalkForwardSweepConfig, WalkForwardSweepError, WalkForwardSweepObjective,
};
use sha2::{Digest, Sha256};

use crate::args::{
    BacktestMarginMode, BacktestRunArgs, BytecodeFormat, CheckArgs, Cli, Command, DumpBytecodeArgs,
    MarketRunArgs, OptimizeObjectiveArg, OptimizeRunArgs, OptimizeRunnerArg, OutputFormat,
    RunCommand, WalkForwardRunArgs, WalkForwardSweepObjectiveArg, WalkForwardSweepRunArgs,
};
use crate::diagnostics::{format_compile_error, format_runtime_error};
use crate::format::{
    render_backtest_text, render_bytecode_text, render_optimize_text, render_outputs_text,
    render_walk_forward_sweep_text, render_walk_forward_text,
};

pub fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Run { mode } => run_mode(*mode),
        Command::Check(args) => check_script(args),
        Command::DumpBytecode(args) => dump_bytecode(args),
    }
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
    let execution_source_alias =
        resolve_execution_source_alias(&compiled, args.execution_source.clone())?;
    let endpoints = ExchangeEndpoints::from_env();
    let runtime = fetch_source_runtime_config(&compiled, args.from, args.to, &endpoints)
        .map_err(|err| format!("backtest mode error: {err}"))?;
    let (perp, perp_context) =
        resolve_perp_backtest_context(&compiled, &execution_source_alias, &args, &endpoints)?;
    let result = run_backtest_with_sources(
        &compiled,
        runtime,
        VmLimits {
            max_instructions_per_bar: args.max_instructions_per_bar,
            max_history_capacity: args.max_history_capacity,
        },
        BacktestConfig {
            execution_source_alias,
            initial_capital: args.initial_capital,
            fee_bps: args.fee_bps,
            slippage_bps: args.slippage_bps,
            perp,
            perp_context,
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
    let execution_source_alias =
        resolve_execution_source_alias(&compiled, args.execution_source.clone())?;
    let endpoints = ExchangeEndpoints::from_env();
    let runtime = fetch_source_runtime_config(&compiled, args.from, args.to, &endpoints)
        .map_err(|err| format!("walk-forward mode error: {err}"))?;
    let (perp, perp_context) =
        resolve_walk_forward_perp_context(&compiled, &execution_source_alias, &args, &endpoints)?;
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
                initial_capital: args.initial_capital,
                fee_bps: args.fee_bps,
                slippage_bps: args.slippage_bps,
                perp,
                perp_context,
            },
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
    let execution_source_alias =
        resolve_execution_source_alias(&compiled, args.execution_source.clone())?;
    let endpoints = ExchangeEndpoints::from_env();
    let runtime = fetch_source_runtime_config(&compiled, args.from, args.to, &endpoints)
        .map_err(|err| format!("walk-forward sweep mode error: {err}"))?;
    let (perp, perp_context) = resolve_walk_forward_sweep_perp_context(
        &compiled,
        &execution_source_alias,
        &args,
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
                    initial_capital: args.initial_capital,
                    fee_bps: args.fee_bps,
                    slippage_bps: args.slippage_bps,
                    perp,
                    perp_context,
                },
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

    let params = resolve_optimize_params(&args, preset.as_ref())?;
    let execution_source_alias =
        resolve_execution_source_alias(&base_compiled, args.execution_source.clone())?;
    let endpoints = ExchangeEndpoints::from_env();
    let runtime = fetch_source_runtime_config(&base_compiled, args.from, args.to, &endpoints)
        .map_err(|err| format!("optimize mode error: {err}"))?;
    let (perp, perp_context) =
        resolve_optimize_perp_context(&base_compiled, &execution_source_alias, &args, &endpoints)?;
    let backtest = BacktestConfig {
        execution_source_alias,
        initial_capital: args.initial_capital,
        fee_bps: args.fee_bps,
        slippage_bps: args.slippage_bps,
        perp,
        perp_context,
    };
    let walk_forward = match map_optimize_runner(args.runner) {
        OptimizeRunner::WalkForward => Some(WalkForwardConfig {
            backtest: backtest.clone(),
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

fn compile_with_preset_overrides(
    source: &str,
    path: &Path,
    preset: Option<&OptimizePreset>,
) -> Result<CompiledProgram, String> {
    let overrides = preset
        .map(|preset| preset.best_input_overrides.clone())
        .unwrap_or_default();
    compile_with_input_overrides(source, &overrides).map_err(|err| format_compile_error(path, &err))
}

fn load_source(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("failed to read `{}`: {err}", path.display()))
}

fn load_preset(
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

fn resolve_execution_source_alias(
    compiled: &CompiledProgram,
    provided: Option<String>,
) -> Result<String, String> {
    if let Some(alias) = provided {
        return Ok(alias);
    }
    match compiled.program.declared_sources.as_slice() {
        [source] => Ok(source.alias.clone()),
        _ => Err(
            "this mode requires --execution-source when the script declares multiple `source`s"
                .to_string(),
        ),
    }
}

fn resolve_perp_backtest_context(
    compiled: &CompiledProgram,
    execution_source_alias: &str,
    args: &BacktestRunArgs,
    endpoints: &ExchangeEndpoints,
) -> Result<
    (
        Option<PerpBacktestConfig>,
        Option<palmscript::PerpBacktestContext>,
    ),
    String,
> {
    let source = compiled
        .program
        .declared_sources
        .iter()
        .find(|source| source.alias == execution_source_alias)
        .ok_or_else(|| format!("unknown execution source `{execution_source_alias}`"))?;
    resolve_perp_context(
        source.template,
        source,
        compiled.program.base_interval,
        PerpCliOptions {
            from: args.from,
            to: args.to,
            leverage: args.leverage,
            margin_mode: args.margin_mode,
        },
        endpoints,
    )
}

fn resolve_walk_forward_perp_context(
    compiled: &CompiledProgram,
    execution_source_alias: &str,
    args: &WalkForwardRunArgs,
    endpoints: &ExchangeEndpoints,
) -> Result<
    (
        Option<PerpBacktestConfig>,
        Option<palmscript::PerpBacktestContext>,
    ),
    String,
> {
    let source = compiled
        .program
        .declared_sources
        .iter()
        .find(|source| source.alias == execution_source_alias)
        .ok_or_else(|| format!("unknown execution source `{execution_source_alias}`"))?;
    resolve_perp_context(
        source.template,
        source,
        compiled.program.base_interval,
        PerpCliOptions {
            from: args.from,
            to: args.to,
            leverage: args.leverage,
            margin_mode: args.margin_mode,
        },
        endpoints,
    )
}

fn resolve_walk_forward_sweep_perp_context(
    compiled: &CompiledProgram,
    execution_source_alias: &str,
    args: &WalkForwardSweepRunArgs,
    endpoints: &ExchangeEndpoints,
) -> Result<
    (
        Option<PerpBacktestConfig>,
        Option<palmscript::PerpBacktestContext>,
    ),
    String,
> {
    let source = compiled
        .program
        .declared_sources
        .iter()
        .find(|source| source.alias == execution_source_alias)
        .ok_or_else(|| format!("unknown execution source `{execution_source_alias}`"))?;
    resolve_perp_context(
        source.template,
        source,
        compiled.program.base_interval,
        PerpCliOptions {
            from: args.from,
            to: args.to,
            leverage: args.leverage,
            margin_mode: args.margin_mode,
        },
        endpoints,
    )
}

fn resolve_optimize_perp_context(
    compiled: &CompiledProgram,
    execution_source_alias: &str,
    args: &OptimizeRunArgs,
    endpoints: &ExchangeEndpoints,
) -> Result<
    (
        Option<PerpBacktestConfig>,
        Option<palmscript::PerpBacktestContext>,
    ),
    String,
> {
    let source = compiled
        .program
        .declared_sources
        .iter()
        .find(|source| source.alias == execution_source_alias)
        .ok_or_else(|| format!("unknown execution source `{execution_source_alias}`"))?;
    resolve_perp_context(
        source.template,
        source,
        compiled.program.base_interval,
        PerpCliOptions {
            from: args.from,
            to: args.to,
            leverage: args.leverage,
            margin_mode: args.margin_mode,
        },
        endpoints,
    )
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

fn resolve_optimize_params(
    args: &OptimizeRunArgs,
    preset: Option<&OptimizePreset>,
) -> Result<Vec<OptimizeParamSpace>, String> {
    if !args.params.is_empty() {
        return args
            .params
            .iter()
            .map(|raw| parse_optimize_param_space(raw))
            .collect();
    }
    preset
        .map(|preset| preset.parameter_space.clone())
        .ok_or_else(|| {
            "optimize mode requires at least one `--param` or a preset parameter space".to_string()
        })
}

fn parse_optimize_param_space(raw: &str) -> Result<OptimizeParamSpace, String> {
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
            let (low, high) = values
                .split_once(':')
                .ok_or_else(|| format!("invalid `--param {raw}`: expected int:name=low:high"))?;
            let low = low.parse::<i64>().map_err(|err| {
                format!("invalid `--param {raw}`: failed to parse `{low}` as integer: {err}")
            })?;
            let high = high.parse::<i64>().map_err(|err| {
                format!("invalid `--param {raw}`: failed to parse `{high}` as integer: {err}")
            })?;
            Ok(OptimizeParamSpace::IntegerRange {
                name: name.to_string(),
                low,
                high,
            })
        }
        "float" => {
            let (low, high) = values
                .split_once(':')
                .ok_or_else(|| format!("invalid `--param {raw}`: expected float:name=low:high"))?;
            let low = low.parse::<f64>().map_err(|err| {
                format!("invalid `--param {raw}`: failed to parse `{low}` as float: {err}")
            })?;
            let high = high.parse::<f64>().map_err(|err| {
                format!("invalid `--param {raw}`: failed to parse `{high}` as float: {err}")
            })?;
            Ok(OptimizeParamSpace::FloatRange {
                name: name.to_string(),
                low,
                high,
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

fn map_sweep_objective(objective: WalkForwardSweepObjectiveArg) -> WalkForwardSweepObjective {
    match objective {
        WalkForwardSweepObjectiveArg::TotalReturn => WalkForwardSweepObjective::TotalReturn,
        WalkForwardSweepObjectiveArg::EndingEquity => WalkForwardSweepObjective::EndingEquity,
        WalkForwardSweepObjectiveArg::ReturnOverDrawdown => {
            WalkForwardSweepObjective::ReturnOverDrawdown
        }
    }
}

fn map_optimize_runner(runner: OptimizeRunnerArg) -> OptimizeRunner {
    match runner {
        OptimizeRunnerArg::WalkForward => OptimizeRunner::WalkForward,
        OptimizeRunnerArg::Backtest => OptimizeRunner::Backtest,
    }
}

fn map_optimize_objective(objective: OptimizeObjectiveArg) -> OptimizeObjective {
    match objective {
        OptimizeObjectiveArg::RobustReturn => OptimizeObjective::RobustReturn,
        OptimizeObjectiveArg::TotalReturn => OptimizeObjective::TotalReturn,
        OptimizeObjectiveArg::EndingEquity => OptimizeObjective::EndingEquity,
        OptimizeObjectiveArg::ReturnOverDrawdown => OptimizeObjective::ReturnOverDrawdown,
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

fn default_parallel_workers() -> usize {
    thread::available_parallelism()
        .map(|parallelism| parallelism.get().min(4))
        .unwrap_or(1)
}

fn default_startup_trials(trials: usize) -> usize {
    16.min((trials / 3).max(8)).min(trials)
}

fn write_optimize_preset(
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
        parameter_space: result.config.params.clone(),
        best_input_overrides: result.best_candidate.input_overrides.clone(),
        top_candidates: result.top_candidates.clone(),
    };
    let json = serde_json::to_string_pretty(&preset)
        .map_err(|err| format!("preset serialize error: {err}"))?;
    fs::write(path, json)
        .map_err(|err| format!("failed to write preset `{}`: {err}", path.display()))
}

fn hash_source(source: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Clone, Copy)]
struct PerpCliOptions {
    from: i64,
    to: i64,
    leverage: Option<f64>,
    margin_mode: Option<BacktestMarginMode>,
}

fn resolve_perp_context(
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
        SourceTemplate::BinanceSpot | SourceTemplate::HyperliquidSpot => {
            if options.leverage.is_some() || !matches!(margin_mode, PerpMarginMode::Isolated) {
                return Err(format!(
                    "spot source `{}` does not accept --leverage or --margin-mode",
                    source.alias
                ));
            }
            Ok((None, None))
        }
        SourceTemplate::BinanceUsdm | SourceTemplate::HyperliquidPerps => {
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
