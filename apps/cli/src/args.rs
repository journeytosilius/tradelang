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
    Docs(DocsArgs),
    Inspect {
        #[command(subcommand)]
        command: Box<InspectCommand>,
    },
    Run {
        #[command(subcommand)]
        mode: Box<RunCommand>,
    },
    Execution {
        #[command(subcommand)]
        command: Box<ExecutionCommand>,
    },
    Check(CheckArgs),
    DumpBytecode(DumpBytecodeArgs),
}

#[derive(Debug, Subcommand)]
pub enum InspectCommand {
    Exports(InspectExportsArgs),
    Export(InspectExportArgs),
    Overlap(InspectOverlapArgs),
}

#[derive(Debug, clap::Args)]
pub struct DocsArgs {
    pub topic: Option<String>,
    #[arg(long, conflicts_with_all = ["topic", "list"])]
    pub all: bool,
    #[arg(long, conflicts_with_all = ["topic", "all"])]
    pub list: bool,
}

#[derive(Debug, clap::Args)]
pub struct InspectExportsArgs {
    pub artifact: PathBuf,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Debug, clap::Args)]
pub struct InspectExportArgs {
    pub artifact: PathBuf,
    pub name: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Debug, clap::Args)]
pub struct InspectOverlapArgs {
    pub artifact: PathBuf,
    pub left: String,
    pub right: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Debug, Subcommand)]
pub enum RunCommand {
    Market(MarketRunArgs),
    Backtest(BacktestRunArgs),
    WalkForward(WalkForwardRunArgs),
    WalkForwardSweep(WalkForwardSweepRunArgs),
    Optimize(Box<OptimizeRunArgs>),
    Paper(PaperRunArgs),
    PaperStatus(PaperStatusArgs),
    PaperList(PaperListArgs),
    PaperStop(PaperStopArgs),
    PaperLogs(PaperLogsArgs),
    PaperPositions(PaperPositionsArgs),
    PaperOrders(PaperOrdersArgs),
    PaperFills(PaperFillsArgs),
    PaperExport(PaperExportArgs),
}

#[derive(Debug, Subcommand)]
pub enum ExecutionCommand {
    Serve(ExecutionServeArgs),
    Status(ExecutionStatusArgs),
    Stop,
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
    pub preset: Option<PathBuf>,
    #[arg(long, requires = "preset")]
    pub preset_trial_id: Option<usize>,
    #[arg(long)]
    pub from: i64,
    #[arg(long)]
    pub to: i64,
    #[arg(long = "execution-source")]
    pub execution_source: Vec<String>,
    #[arg(long, default_value_t = 10_000.0)]
    pub initial_capital: f64,
    #[arg(long)]
    pub spot_virtual_rebalance: bool,
    #[arg(long)]
    pub maker_fee_bps: f64,
    #[arg(long)]
    pub taker_fee_bps: f64,
    #[arg(long = "fee-schedule")]
    pub fee_schedule: Vec<String>,
    #[arg(long = "set")]
    pub set_overrides: Vec<String>,
    #[arg(long, default_value_t = 2.0)]
    pub slippage_bps: f64,
    #[arg(long)]
    pub leverage: Option<f64>,
    #[arg(long, value_enum)]
    pub margin_mode: Option<BacktestMarginMode>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long, value_enum, default_value_t = DiagnosticsDetailArg::Summary)]
    pub diagnostics: DiagnosticsDetailArg,
    #[arg(long, default_value_t = 10_000)]
    pub max_instructions_per_bar: usize,
    #[arg(long, default_value_t = 1_024)]
    pub max_history_capacity: usize,
}

#[derive(Debug, clap::Args)]
pub struct WalkForwardRunArgs {
    pub script: PathBuf,
    #[arg(long)]
    pub preset: Option<PathBuf>,
    #[arg(long, requires = "preset")]
    pub preset_trial_id: Option<usize>,
    #[arg(long)]
    pub from: i64,
    #[arg(long)]
    pub to: i64,
    #[arg(long = "execution-source")]
    pub execution_source: Vec<String>,
    #[arg(long, default_value_t = 10_000.0)]
    pub initial_capital: f64,
    #[arg(long)]
    pub spot_virtual_rebalance: bool,
    #[arg(long)]
    pub maker_fee_bps: f64,
    #[arg(long)]
    pub taker_fee_bps: f64,
    #[arg(long = "fee-schedule")]
    pub fee_schedule: Vec<String>,
    #[arg(long = "set")]
    pub set_overrides: Vec<String>,
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
    #[arg(long)]
    pub min_trades: Option<usize>,
    #[arg(long)]
    pub min_sharpe: Option<f64>,
    #[arg(long)]
    pub max_zero_trade_segments: Option<usize>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long, value_enum, default_value_t = DiagnosticsDetailArg::Summary)]
    pub diagnostics: DiagnosticsDetailArg,
    #[arg(long, default_value_t = 10_000)]
    pub max_instructions_per_bar: usize,
    #[arg(long, default_value_t = 1_024)]
    pub max_history_capacity: usize,
}

#[derive(Debug, clap::Args)]
pub struct WalkForwardSweepRunArgs {
    pub script: PathBuf,
    #[arg(long)]
    pub preset: Option<PathBuf>,
    #[arg(long)]
    pub from: i64,
    #[arg(long)]
    pub to: i64,
    #[arg(long = "execution-source")]
    pub execution_source: Vec<String>,
    #[arg(long, default_value_t = 10_000.0)]
    pub initial_capital: f64,
    #[arg(long)]
    pub spot_virtual_rebalance: bool,
    #[arg(long)]
    pub maker_fee_bps: f64,
    #[arg(long)]
    pub taker_fee_bps: f64,
    #[arg(long = "fee-schedule")]
    pub fee_schedule: Vec<String>,
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
pub struct OptimizeRunArgs {
    pub script: PathBuf,
    #[arg(long)]
    pub preset: Option<PathBuf>,
    #[arg(long)]
    pub from: i64,
    #[arg(long)]
    pub to: i64,
    #[arg(long = "execution-source")]
    pub execution_source: Vec<String>,
    #[arg(long, default_value_t = 10_000.0)]
    pub initial_capital: f64,
    #[arg(long)]
    pub spot_virtual_rebalance: bool,
    #[arg(long)]
    pub maker_fee_bps: f64,
    #[arg(long)]
    pub taker_fee_bps: f64,
    #[arg(long = "fee-schedule")]
    pub fee_schedule: Vec<String>,
    #[arg(long, default_value_t = 2.0)]
    pub slippage_bps: f64,
    #[arg(long)]
    pub leverage: Option<f64>,
    #[arg(long, value_enum)]
    pub margin_mode: Option<BacktestMarginMode>,
    #[arg(long)]
    pub train_bars: Option<usize>,
    #[arg(long)]
    pub test_bars: Option<usize>,
    #[arg(long)]
    pub step_bars: Option<usize>,
    #[arg(long)]
    pub holdout_bars: Option<usize>,
    #[arg(long, default_value_t = false, conflicts_with = "holdout_bars")]
    pub no_holdout: bool,
    #[arg(long)]
    pub min_trades: Option<usize>,
    #[arg(long)]
    pub min_sharpe: Option<f64>,
    #[arg(long)]
    pub min_holdout_trades: Option<usize>,
    #[arg(long, default_value_t = false)]
    pub require_positive_holdout: bool,
    #[arg(long)]
    pub max_zero_trade_segments: Option<usize>,
    #[arg(long)]
    pub min_holdout_pass_rate: Option<f64>,
    #[arg(long)]
    pub min_date_perturbation_positive_ratio: Option<f64>,
    #[arg(long)]
    pub min_date_perturbation_outperform_ratio: Option<f64>,
    #[arg(long, value_enum)]
    pub max_overfitting_risk: Option<OverfittingRiskArg>,
    #[arg(long = "param")]
    pub params: Vec<String>,
    #[arg(long, value_enum, default_value_t = OptimizeRunnerArg::WalkForward)]
    pub runner: OptimizeRunnerArg,
    #[arg(long, value_enum, default_value_t = OptimizeObjectiveArg::RobustReturn)]
    pub objective: OptimizeObjectiveArg,
    #[arg(long, default_value_t = 50)]
    pub trials: usize,
    #[arg(long)]
    pub startup_trials: Option<usize>,
    #[arg(long, default_value_t = 0)]
    pub seed: u64,
    #[arg(long)]
    pub workers: Option<usize>,
    #[arg(long, default_value_t = 10)]
    pub top: usize,
    #[arg(long)]
    pub direct_validate_top: Option<usize>,
    #[arg(long)]
    pub preset_out: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long, value_enum, default_value_t = DiagnosticsDetailArg::Summary)]
    pub diagnostics: DiagnosticsDetailArg,
    #[arg(long, default_value_t = 10_000)]
    pub max_instructions_per_bar: usize,
    #[arg(long, default_value_t = 1_024)]
    pub max_history_capacity: usize,
}

#[derive(Debug, clap::Args)]
pub struct PaperRunArgs {
    pub script: PathBuf,
    #[arg(long = "execution-source")]
    pub execution_source: Vec<String>,
    #[arg(long, default_value_t = 10_000.0)]
    pub initial_capital: f64,
    #[arg(long)]
    pub maker_fee_bps: f64,
    #[arg(long)]
    pub taker_fee_bps: f64,
    #[arg(long = "fee-schedule")]
    pub fee_schedule: Vec<String>,
    #[arg(long, default_value_t = 2.0)]
    pub slippage_bps: f64,
    #[arg(long)]
    pub leverage: Option<f64>,
    #[arg(long, value_enum)]
    pub margin_mode: Option<BacktestMarginMode>,
    #[arg(long, value_enum, default_value_t = DiagnosticsDetailArg::Summary)]
    pub diagnostics: DiagnosticsDetailArg,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(long, default_value_t = 10_000)]
    pub max_instructions_per_bar: usize,
    #[arg(long, default_value_t = 1_024)]
    pub max_history_capacity: usize,
}

#[derive(Debug, clap::Args)]
pub struct PaperStatusArgs {
    pub session_id: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Debug, clap::Args)]
pub struct PaperListArgs {
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Debug, clap::Args)]
pub struct PaperStopArgs {
    pub session_id: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Debug, clap::Args)]
pub struct PaperLogsArgs {
    pub session_id: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Debug, clap::Args)]
pub struct PaperPositionsArgs {
    pub session_id: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Debug, clap::Args)]
pub struct PaperOrdersArgs {
    pub session_id: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Debug, clap::Args)]
pub struct PaperFillsArgs {
    pub session_id: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Debug, clap::Args)]
pub struct PaperExportArgs {
    pub session_id: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
}

#[derive(Debug, clap::Args)]
pub struct ExecutionServeArgs {
    #[arg(long, default_value_t = 30_000)]
    pub poll_interval_ms: u64,
    #[arg(long, default_value_t = false)]
    pub once: bool,
}

#[derive(Debug, clap::Args)]
pub struct ExecutionStatusArgs {
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OptimizeRunnerArg {
    #[default]
    WalkForward,
    Backtest,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum DiagnosticsDetailArg {
    #[default]
    Summary,
    FullTrace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OverfittingRiskArg {
    Low,
    Moderate,
    High,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum OptimizeObjectiveArg {
    #[default]
    RobustReturn,
    TotalReturn,
    EndingEquity,
    ReturnOverDrawdown,
}
