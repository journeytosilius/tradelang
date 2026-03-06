use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "palmscript")]
#[command(about = "PalmScript CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Run {
        #[command(subcommand)]
        mode: RunCommand,
    },
    Check(CheckArgs),
    DumpBytecode(DumpBytecodeArgs),
}

#[derive(Debug, Subcommand)]
pub enum RunCommand {
    Csv(CsvRunArgs),
}

#[derive(Debug, clap::Args)]
pub struct CsvRunArgs {
    pub script: PathBuf,
    #[arg(long)]
    pub bars: PathBuf,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long, default_value_t = 10_000)]
    pub max_instructions_per_bar: usize,
    #[arg(long, default_value_t = 1_024)]
    pub max_history_capacity: usize,
}

#[derive(Debug, clap::Args)]
pub struct CheckArgs {
    pub script: PathBuf,
}

#[derive(Debug, clap::Args)]
pub struct DumpBytecodeArgs {
    pub script: PathBuf,
    #[arg(long, value_enum, default_value_t = BytecodeFormat::Text)]
    pub format: BytecodeFormat,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Json,
    Text,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum BytecodeFormat {
    #[default]
    Text,
    Json,
}
