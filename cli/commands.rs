use std::fs;
use std::path::Path;

use palmscript::{
    compile, fetch_perp_backtest_context, fetch_source_runtime_config, run_backtest_with_sources,
    run_walk_forward_sweep_with_source, run_walk_forward_with_sources, run_with_sources,
    BacktestConfig, CompiledProgram, ExchangeEndpoints, InputSweepDefinition, PerpBacktestConfig,
    PerpMarginMode, RuntimeError, SourceTemplate, VmLimits, WalkForwardConfig,
    WalkForwardSweepConfig, WalkForwardSweepError, WalkForwardSweepObjective,
};

use crate::args::{
    BacktestMarginMode, BacktestRunArgs, BytecodeFormat, CheckArgs, Cli, Command, DumpBytecodeArgs,
    MarketRunArgs, OutputFormat, RunCommand, WalkForwardRunArgs, WalkForwardSweepObjectiveArg,
    WalkForwardSweepRunArgs,
};
use crate::diagnostics::{format_compile_error, format_runtime_error};
use crate::format::{
    render_backtest_text, render_bytecode_text, render_outputs_text,
    render_walk_forward_sweep_text, render_walk_forward_text,
};

pub fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Run { mode } => run_mode(mode),
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
    let compiled = compile(&source).map_err(|err| format_compile_error(&args.script, &err))?;
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
    let compiled = compile(&source).map_err(|err| format_compile_error(&args.script, &err))?;
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
    let compiled = compile(&source).map_err(|err| format_compile_error(&args.script, &err))?;
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

fn load_source(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|err| format!("failed to read `{}`: {err}", path.display()))
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

fn map_sweep_objective(objective: WalkForwardSweepObjectiveArg) -> WalkForwardSweepObjective {
    match objective {
        WalkForwardSweepObjectiveArg::TotalReturn => WalkForwardSweepObjective::TotalReturn,
        WalkForwardSweepObjectiveArg::EndingEquity => WalkForwardSweepObjective::EndingEquity,
        WalkForwardSweepObjectiveArg::ReturnOverDrawdown => {
            WalkForwardSweepObjective::ReturnOverDrawdown
        }
    }
}

fn format_walk_forward_sweep_error(path: &Path, error: WalkForwardSweepError) -> String {
    match error {
        WalkForwardSweepError::Compile(err) => format_compile_error(path, &err),
        other => format!("walk-forward sweep mode error: {other}"),
    }
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
