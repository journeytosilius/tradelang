use std::fs;
use std::path::Path;

use palmscript::{
    compile, prepare_csv_inputs_for_program, run_multi_interval, CompiledProgram, RuntimeError,
    VmLimits,
};

use crate::args::{
    BytecodeFormat, CheckArgs, Cli, Command, CsvRunArgs, DumpBytecodeArgs, OutputFormat, RunCommand,
};
use crate::data::load_bars_csv;
use crate::diagnostics::{format_compile_error, format_data_prep_error, format_runtime_error};
use crate::format::{render_bytecode_text, render_outputs_text};

pub fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Run { mode } => run_mode(mode),
        Command::Check(args) => check_script(args),
        Command::DumpBytecode(args) => dump_bytecode(args),
    }
}

fn run_mode(mode: RunCommand) -> Result<(), String> {
    match mode {
        RunCommand::Csv(args) => run_csv(args),
    }
}

fn run_csv(args: CsvRunArgs) -> Result<(), String> {
    let source = load_source(&args.script)?;
    let compiled = compile(&source).map_err(|err| format_compile_error(&args.script, &err))?;
    let raw_bars = load_bars_csv(&args.bars)?;
    let prepared = prepare_csv_inputs_for_program(&compiled, raw_bars)
        .map_err(|err| format_data_prep_error(&err))?;
    let outputs = run_multi_interval(
        &compiled,
        &prepared.base_bars,
        prepared.config,
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

#[allow(dead_code)]
fn _runtime_error(_err: RuntimeError) -> String {
    unreachable!()
}
