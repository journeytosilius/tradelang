use std::fs;
use std::path::Path;

use palmscript::{
    compile, fetch_source_runtime_config, run_backtest_with_sources, run_with_sources,
    BacktestConfig, CompiledProgram, ExchangeEndpoints, RuntimeError, VmLimits,
};

use crate::args::{
    BacktestRunArgs, BytecodeFormat, CheckArgs, Cli, Command, DumpBytecodeArgs, MarketRunArgs,
    OutputFormat, RunCommand,
};
use crate::diagnostics::{format_compile_error, format_runtime_error};
use crate::format::{render_backtest_text, render_bytecode_text, render_outputs_text};

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
    let execution_source_alias = resolve_execution_source_alias(&compiled, args.execution_source)?;
    let runtime = fetch_source_runtime_config(
        &compiled,
        args.from,
        args.to,
        &ExchangeEndpoints::from_env(),
    )
    .map_err(|err| format!("backtest mode error: {err}"))?;
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
            "backtest mode requires --execution-source when the script declares multiple `source`s"
                .to_string(),
        ),
    }
}

#[allow(dead_code)]
fn _runtime_error(_err: RuntimeError) -> String {
    unreachable!()
}
