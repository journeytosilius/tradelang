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
    Market(MarketRunArgs),
    Backtest(BacktestRunArgs),
    WalkForward(WalkForwardRunArgs),
    WalkForwardSweep(WalkForwardSweepRunArgs),
}

#[derive(Debug, clap::Args)]
pub struct MarketRunArgs {
    pub script: PathBuf,
    #[arg(long)]
    pub from: i64,
    #[arg(long)]
    pub to: i64,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long, default_value_t = 10_000)]
    pub max_instructions_per_bar: usize,
    #[arg(long, default_value_t = 1_024)]
    pub max_history_capacity: usize,
}

#[derive(Debug, clap::Args)]
pub struct BacktestRunArgs {
    pub script: PathBuf,
    #[arg(long)]
    pub from: i64,
    #[arg(long)]
    pub to: i64,
    #[arg(long)]
    pub execution_source: Option<String>,
    #[arg(long, default_value_t = 10_000.0)]
    pub initial_capital: f64,
    #[arg(long, default_value_t = 5.0)]
    pub fee_bps: f64,
    #[arg(long, default_value_t = 2.0)]
    pub slippage_bps: f64,
    #[arg(long)]
    pub leverage: Option<f64>,
    #[arg(long, value_enum)]
    pub margin_mode: Option<BacktestMarginMode>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long, default_value_t = 10_000)]
    pub max_instructions_per_bar: usize,
    #[arg(long, default_value_t = 1_024)]
    pub max_history_capacity: usize,
}

#[derive(Debug, clap::Args)]
pub struct WalkForwardRunArgs {
    pub script: PathBuf,
    #[arg(long)]
    pub from: i64,
    #[arg(long)]
    pub to: i64,
    #[arg(long)]
    pub execution_source: Option<String>,
    #[arg(long, default_value_t = 10_000.0)]
    pub initial_capital: f64,
    #[arg(long, default_value_t = 5.0)]
    pub fee_bps: f64,
    #[arg(long, default_value_t = 2.0)]
    pub slippage_bps: f64,
    #[arg(long)]
    pub leverage: Option<f64>,
    #[arg(long, value_enum)]
    pub margin_mode: Option<BacktestMarginMode>,
    #[arg(long)]
    pub train_bars: usize,
    #[arg(long)]
    pub test_bars: usize,
    #[arg(long)]
    pub step_bars: Option<usize>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long, default_value_t = 10_000)]
    pub max_instructions_per_bar: usize,
    #[arg(long, default_value_t = 1_024)]
    pub max_history_capacity: usize,
}

#[derive(Debug, clap::Args)]
pub struct WalkForwardSweepRunArgs {
    pub script: PathBuf,
    #[arg(long)]
    pub from: i64,
    #[arg(long)]
    pub to: i64,
    #[arg(long)]
    pub execution_source: Option<String>,
    #[arg(long, default_value_t = 10_000.0)]
    pub initial_capital: f64,
    #[arg(long, default_value_t = 5.0)]
    pub fee_bps: f64,
    #[arg(long, default_value_t = 2.0)]
    pub slippage_bps: f64,
    #[arg(long)]
    pub leverage: Option<f64>,
    #[arg(long, value_enum)]
    pub margin_mode: Option<BacktestMarginMode>,
    #[arg(long)]
    pub train_bars: usize,
    #[arg(long)]
    pub test_bars: usize,
    #[arg(long)]
    pub step_bars: Option<usize>,
    #[arg(long = "set", required = true)]
    pub sets: Vec<String>,
    #[arg(long, value_enum, default_value_t = WalkForwardSweepObjectiveArg::TotalReturn)]
    pub objective: WalkForwardSweepObjectiveArg,
    #[arg(long, default_value_t = 10)]
    pub top: usize,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum BacktestMarginMode {
    Isolated,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum WalkForwardSweepObjectiveArg {
    #[default]
    TotalReturn,
    EndingEquity,
    ReturnOverDrawdown,
}
