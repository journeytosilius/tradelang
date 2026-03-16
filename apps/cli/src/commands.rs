use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::thread;

use palmscript::{
    bytecode::InputOptimizationDeclKind, compile, compile_with_input_overrides,
    execution_daemon_status, fetch_perp_backtest_context, fetch_source_runtime_config,
    list_paper_sessions, load_paper_session_export, load_paper_session_logs,
    load_paper_session_snapshot, request_execution_daemon_stop, run_backtest_with_sources,
    run_optimize_with_source, run_walk_forward_sweep_with_source, run_walk_forward_with_sources,
    run_with_sources, serve_execution_daemon, stop_paper_session, submit_paper_session,
    BacktestConfig, CompiledProgram, DiagnosticsDetailMode, ExchangeEndpoints,
    ExecutionDaemonConfig, ExecutionError, FeeSchedule, InputSweepDefinition, OptimizeConfig,
    OptimizeError, OptimizeHoldoutConfig, OptimizeObjective, OptimizeParamSpace, OptimizePreset,
    OptimizeResult, OptimizeRunner, OverfittingRiskLevel, PaperSessionConfig, PerpBacktestConfig,
    PerpMarginMode, RuntimeError, SourceTemplate, SubmitPaperSession, ValidationConstraintConfig,
    VmLimits, WalkForwardConfig, WalkForwardSweepConfig, WalkForwardSweepError,
    WalkForwardSweepObjective,
};
use sha2::{Digest, Sha256};

use crate::args::{
    BacktestMarginMode, BacktestRunArgs, BytecodeFormat, CheckArgs, Cli, Command,
    DiagnosticsDetailArg, DocsArgs, DumpBytecodeArgs, ExecutionCommand, ExecutionServeArgs,
    ExecutionStatusArgs, InspectCommand, InspectExportArgs, InspectExportsArgs, InspectOverlapArgs,
    MarketRunArgs, OptimizeObjectiveArg, OptimizeRunArgs, OptimizeRunnerArg, OutputFormat,
    OverfittingRiskArg, PaperExportArgs, PaperFillsArgs, PaperListArgs, PaperLogsArgs,
    PaperOrdersArgs, PaperPositionsArgs, PaperRunArgs, PaperStatusArgs, PaperStopArgs, RunCommand,
    WalkForwardRunArgs, WalkForwardSweepObjectiveArg, WalkForwardSweepRunArgs,
};
use crate::diagnostics::{format_compile_error, format_runtime_error};
use crate::docs;
use crate::format::{
    render_backtest_text, render_bytecode_text, render_execution_daemon_status_text,
    render_export_list_text, render_export_overlap_text, render_export_summary_text,
    render_optimize_text, render_outputs_text, render_paper_export_text,
    render_paper_export_text_full, render_paper_logs_text, render_paper_manifest_text,
    render_paper_positions_text, render_paper_snapshot_text, render_walk_forward_sweep_text,
    render_walk_forward_text,
};
use crate::inspect::{inspect_export, inspect_exports, inspect_overlap};

type ResolvedPerpContexts = (
    Option<PerpBacktestConfig>,
    Option<palmscript::PerpBacktestContext>,
    BTreeMap<String, palmscript::PerpBacktestContext>,
);

pub fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Docs(args) => print_docs(args),
        Command::Inspect { command } => run_inspect(*command),
        Command::Run { mode } => run_mode(*mode),
        Command::Execution { command } => run_execution(*command),
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
        RunCommand::Optimize(args) => run_optimize(*args),
        RunCommand::Paper(args) => run_paper(args),
        RunCommand::PaperStatus(args) => run_paper_status(args),
        RunCommand::PaperList(args) => run_paper_list(args),
        RunCommand::PaperStop(args) => run_paper_stop(args),
        RunCommand::PaperLogs(args) => run_paper_logs(args),
        RunCommand::PaperPositions(args) => run_paper_positions(args),
        RunCommand::PaperOrders(args) => run_paper_orders(args),
        RunCommand::PaperFills(args) => run_paper_fills(args),
        RunCommand::PaperExport(args) => run_paper_export(args),
    }
}

fn run_inspect(command: InspectCommand) -> Result<(), String> {
    match command {
        InspectCommand::Exports(args) => run_inspect_exports(args),
        InspectCommand::Export(args) => run_inspect_export(args),
        InspectCommand::Overlap(args) => run_inspect_overlap(args),
    }
}

fn run_execution(command: ExecutionCommand) -> Result<(), String> {
    match command {
        ExecutionCommand::Serve(args) => run_execution_serve(args),
        ExecutionCommand::Status(args) => run_execution_status(args),
        ExecutionCommand::Stop => run_execution_stop(),
    }
}

fn run_inspect_exports(args: InspectExportsArgs) -> Result<(), String> {
    let summary = inspect_exports(&args.artifact)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&summary).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_export_list_text(&summary)),
    }
    Ok(())
}

fn run_inspect_export(args: InspectExportArgs) -> Result<(), String> {
    let summary = inspect_export(&args.artifact, &args.name)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&summary).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_export_summary_text(&summary)),
    }
    Ok(())
}

fn run_inspect_overlap(args: InspectOverlapArgs) -> Result<(), String> {
    let summary = inspect_overlap(&args.artifact, &args.left, &args.right)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&summary).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_export_overlap_text(&summary)),
    }
    Ok(())
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
    let compiled = compile_with_preset_overrides(
        &source,
        &args.script,
        preset.as_ref(),
        args.preset_trial_id,
        &args.set_overrides,
    )?;
    if compiled.program.declared_sources.is_empty() {
        return Err("backtest mode requires at least one `source` declaration".to_string());
    }
    let execution_source_aliases =
        resolve_execution_source_aliases(&compiled, &args.execution_source)?;
    let execution_source_alias = execution_source_aliases[0].clone();
    let global_fee_schedule = global_fee_schedule(args.maker_fee_bps, args.taker_fee_bps);
    let execution_fee_schedules =
        parse_execution_fee_schedules(&args.fee_schedule, &execution_source_aliases)?;
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
            spot_virtual_rebalance: args.spot_virtual_rebalance,
            activation_time_ms: None,
            initial_capital: args.initial_capital,
            maker_fee_bps: global_fee_schedule.maker_bps,
            taker_fee_bps: global_fee_schedule.taker_bps,
            execution_fee_schedules,
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
    let compiled = compile_with_preset_overrides(
        &source,
        &args.script,
        preset.as_ref(),
        args.preset_trial_id,
        &args.set_overrides,
    )?;
    if compiled.program.declared_sources.is_empty() {
        return Err("walk-forward mode requires at least one `source` declaration".to_string());
    }
    let execution_source_aliases =
        resolve_execution_source_aliases(&compiled, &args.execution_source)?;
    let execution_source_alias = execution_source_aliases[0].clone();
    let global_fee_schedule = global_fee_schedule(args.maker_fee_bps, args.taker_fee_bps);
    let execution_fee_schedules =
        parse_execution_fee_schedules(&args.fee_schedule, &execution_source_aliases)?;
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
                spot_virtual_rebalance: args.spot_virtual_rebalance,
                activation_time_ms: None,
                initial_capital: args.initial_capital,
                maker_fee_bps: global_fee_schedule.maker_bps,
                taker_fee_bps: global_fee_schedule.taker_bps,
                execution_fee_schedules,
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
            constraints: ValidationConstraintConfig {
                min_trade_count: args.min_trades,
                min_sharpe_ratio: args.min_sharpe,
                min_holdout_trade_count: None,
                require_positive_holdout: false,
                max_zero_trade_segments: args.max_zero_trade_segments,
                min_holdout_pass_rate: None,
                min_date_perturbation_positive_ratio: None,
                min_date_perturbation_outperform_ratio: None,
                max_overfitting_risk: None,
            },
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
    let compiled =
        compile_with_preset_overrides(&source, &args.script, preset.as_ref(), None, &[])?;
    if compiled.program.declared_sources.is_empty() {
        return Err(
            "walk-forward sweep mode requires at least one `source` declaration".to_string(),
        );
    }
    let execution_source_aliases =
        resolve_execution_source_aliases(&compiled, &args.execution_source)?;
    let execution_source_alias = execution_source_aliases[0].clone();
    let global_fee_schedule = global_fee_schedule(args.maker_fee_bps, args.taker_fee_bps);
    let execution_fee_schedules =
        parse_execution_fee_schedules(&args.fee_schedule, &execution_source_aliases)?;
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
                    spot_virtual_rebalance: args.spot_virtual_rebalance,
                    activation_time_ms: None,
                    initial_capital: args.initial_capital,
                    maker_fee_bps: global_fee_schedule.maker_bps,
                    taker_fee_bps: global_fee_schedule.taker_bps,
                    execution_fee_schedules: execution_fee_schedules.clone(),
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
                constraints: ValidationConstraintConfig::default(),
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
    let global_fee_schedule = global_fee_schedule(args.maker_fee_bps, args.taker_fee_bps);
    let execution_fee_schedules =
        parse_execution_fee_schedules(&args.fee_schedule, &execution_source_aliases)?;
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
        spot_virtual_rebalance: args.spot_virtual_rebalance,
        activation_time_ms: None,
        initial_capital: args.initial_capital,
        maker_fee_bps: global_fee_schedule.maker_bps,
        taker_fee_bps: global_fee_schedule.taker_bps,
        execution_fee_schedules,
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
            constraints: ValidationConstraintConfig {
                min_trade_count: args.min_trades,
                min_sharpe_ratio: args.min_sharpe,
                min_holdout_trade_count: None,
                require_positive_holdout: false,
                max_zero_trade_segments: args.max_zero_trade_segments,
                min_holdout_pass_rate: None,
                min_date_perturbation_positive_ratio: None,
                min_date_perturbation_outperform_ratio: None,
                max_overfitting_risk: None,
            },
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
            direct_validation_top_n: args.direct_validate_top.unwrap_or(0),
            base_input_overrides: preset_overrides,
            constraints: ValidationConstraintConfig {
                min_trade_count: args.min_trades,
                min_sharpe_ratio: args.min_sharpe,
                min_holdout_trade_count: args.min_holdout_trades,
                require_positive_holdout: args.require_positive_holdout,
                max_zero_trade_segments: args.max_zero_trade_segments,
                min_holdout_pass_rate: args.min_holdout_pass_rate,
                min_date_perturbation_positive_ratio: args.min_date_perturbation_positive_ratio,
                min_date_perturbation_outperform_ratio: args.min_date_perturbation_outperform_ratio,
                max_overfitting_risk: args.max_overfitting_risk.map(map_overfitting_risk),
            },
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

fn run_paper(args: PaperRunArgs) -> Result<(), String> {
    let source = load_source(&args.script)?;
    let compiled = compile_source(&source, &args.script)?;
    if compiled.program.declared_sources.is_empty() {
        return Err("paper mode requires at least one `source` declaration".to_string());
    }
    let execution_source_aliases =
        resolve_execution_source_aliases(&compiled, &args.execution_source)?;
    let global_fee_schedule = global_fee_schedule(args.maker_fee_bps, args.taker_fee_bps);
    let execution_fee_schedules =
        parse_execution_fee_schedules(&args.fee_schedule, &execution_source_aliases)?;
    let manifest = submit_paper_session(SubmitPaperSession {
        source,
        script_path: Some(args.script.clone()),
        config: PaperSessionConfig {
            execution_source_aliases,
            initial_capital: args.initial_capital,
            maker_fee_bps: global_fee_schedule.maker_bps,
            taker_fee_bps: global_fee_schedule.taker_bps,
            execution_fee_schedules,
            slippage_bps: args.slippage_bps,
            diagnostics_detail: map_diagnostics_detail(args.diagnostics),
            leverage: args.leverage,
            margin_mode: args.margin_mode.map(|mode| match mode {
                BacktestMarginMode::Isolated => PerpMarginMode::Isolated,
            }),
            vm_limits: VmLimits {
                max_instructions_per_bar: args.max_instructions_per_bar,
                max_history_capacity: args.max_history_capacity,
            },
        },
        start_time_ms: current_unix_ms(),
        endpoints: ExchangeEndpoints::from_env(),
    })
    .map_err(format_execution_error)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&manifest).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_paper_manifest_text(&manifest)),
    }
    Ok(())
}

fn run_paper_status(args: PaperStatusArgs) -> Result<(), String> {
    let snapshot = load_paper_session_snapshot(&args.session_id).map_err(format_execution_error)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&snapshot).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_paper_snapshot_text(&snapshot)),
    }
    Ok(())
}

fn run_paper_list(args: PaperListArgs) -> Result<(), String> {
    let sessions = list_paper_sessions().map_err(format_execution_error)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&sessions).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => {
            for session in &sessions {
                print!("{}", render_paper_manifest_text(session));
            }
        }
    }
    Ok(())
}

fn run_paper_stop(args: PaperStopArgs) -> Result<(), String> {
    let manifest = stop_paper_session(&args.session_id).map_err(format_execution_error)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&manifest).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_paper_manifest_text(&manifest)),
    }
    Ok(())
}

fn run_paper_logs(args: PaperLogsArgs) -> Result<(), String> {
    let logs = load_paper_session_logs(&args.session_id).map_err(format_execution_error)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&logs).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_paper_logs_text(&args.session_id, &logs)),
    }
    Ok(())
}

fn run_paper_positions(args: PaperPositionsArgs) -> Result<(), String> {
    let export = load_paper_session_export(&args.session_id).map_err(format_execution_error)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(
                &export
                    .latest_result
                    .as_ref()
                    .map(|result| &result.open_positions)
            )
            .map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!(
            "{}",
            render_paper_positions_text(
                &args.session_id,
                export
                    .latest_result
                    .as_ref()
                    .map(|result| result.open_positions.as_slice())
                    .unwrap_or(&[])
            )
        ),
    }
    Ok(())
}

fn run_paper_orders(args: PaperOrdersArgs) -> Result<(), String> {
    let export = load_paper_session_export(&args.session_id).map_err(format_execution_error)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(
                &export.latest_result.as_ref().map(|result| &result.orders)
            )
            .map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!(
            "{}",
            render_paper_export_text(
                &args.session_id,
                export
                    .latest_result
                    .as_ref()
                    .map(|result| result.orders.as_slice())
                    .unwrap_or(&[]),
                &[]
            )
        ),
    }
    Ok(())
}

fn run_paper_fills(args: PaperFillsArgs) -> Result<(), String> {
    let export = load_paper_session_export(&args.session_id).map_err(format_execution_error)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(
                &export.latest_result.as_ref().map(|result| &result.fills)
            )
            .map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!(
            "{}",
            render_paper_export_text(
                &args.session_id,
                &[],
                export
                    .latest_result
                    .as_ref()
                    .map(|result| result.fills.as_slice())
                    .unwrap_or(&[])
            )
        ),
    }
    Ok(())
}

fn run_paper_export(args: PaperExportArgs) -> Result<(), String> {
    let export = load_paper_session_export(&args.session_id).map_err(format_execution_error)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&export).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => print!("{}", render_paper_export_text_full(&export)),
    }
    Ok(())
}

fn run_execution_serve(args: ExecutionServeArgs) -> Result<(), String> {
    let status = serve_execution_daemon(ExecutionDaemonConfig {
        poll_interval_ms: args.poll_interval_ms,
        once: args.once,
    })
    .map_err(format_execution_error)?;
    print!("{}", render_execution_daemon_status_text(&status));
    Ok(())
}

fn run_execution_status(args: ExecutionStatusArgs) -> Result<(), String> {
    let status = execution_daemon_status().map_err(format_execution_error)?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&status).map_err(|err| err.to_string())?
        ),
        OutputFormat::Text => {
            if let Some(status) = status {
                print!("{}", render_execution_daemon_status_text(&status));
            } else {
                println!("execution daemon status unavailable");
            }
        }
    }
    Ok(())
}

fn run_execution_stop() -> Result<(), String> {
    let stop_path = request_execution_daemon_stop().map_err(format_execution_error)?;
    println!(
        "execution daemon stop requested via {}",
        stop_path.display()
    );
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
    preset_trial_id: Option<usize>,
    raw_input_overrides: &[String],
) -> Result<CompiledProgram, String> {
    let overrides = resolve_preset_input_overrides(preset, preset_trial_id, raw_input_overrides)?;
    compile_with_input_overrides(source, &overrides).map_err(|err| format_compile_error(path, &err))
}

pub(crate) fn resolve_preset_input_overrides(
    preset: Option<&OptimizePreset>,
    preset_trial_id: Option<usize>,
    raw_input_overrides: &[String],
) -> Result<BTreeMap<String, f64>, String> {
    if preset_trial_id.is_some() && preset.is_none() {
        return Err("`--preset-trial-id` requires `--preset <path>`".to_string());
    }

    let mut overrides = match (preset, preset_trial_id) {
        (Some(preset), Some(trial_id)) => preset
            .top_candidates
            .iter()
            .find(|candidate| candidate.trial_id == trial_id)
            .map(|candidate| candidate.input_overrides.clone())
            .ok_or_else(|| {
                format!(
                    "preset does not contain top-candidate trial_id {}; replay is limited to saved top candidates",
                    trial_id
                )
            })?,
        (Some(preset), None) => preset.best_input_overrides.clone(),
        (None, None) => BTreeMap::new(),
        (None, Some(_)) => unreachable!("validated above"),
    };

    let explicit_overrides = parse_input_override_assignments(raw_input_overrides)?;
    overrides.extend(explicit_overrides);
    Ok(overrides)
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
    match compiled.program.declared_executions.as_slice() {
        [execution] => Ok(vec![execution.alias.clone()]),
        [] => Err(
            "this mode requires at least one declared `execution` target; add `execution <alias> = exchange.market(\"SYMBOL\")` to the script"
                .to_string(),
        ),
        _ => Err(
            "this mode requires --execution-source when the script declares multiple `execution` targets"
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
            .find_execution_target(alias)
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

pub(crate) fn parse_input_override_assignments(
    raw_assignments: &[String],
) -> Result<BTreeMap<String, f64>, String> {
    let mut overrides = BTreeMap::new();
    for raw in raw_assignments {
        let (name, raw_value) = raw
            .split_once('=')
            .ok_or_else(|| format!("invalid `--set {raw}`: expected name=value"))?;
        if name.is_empty() {
            return Err(format!("invalid `--set {raw}`: missing input name"));
        }
        let value = raw_value.parse::<f64>().map_err(|err| {
            format!("invalid `--set {raw}`: failed to parse `{raw_value}` as number: {err}")
        })?;
        if !value.is_finite() {
            return Err(format!(
                "invalid `--set {raw}`: `{raw_value}` must be a finite number"
            ));
        }
        if overrides.insert(name.to_string(), value).is_some() {
            return Err(format!("duplicate `--set` override for input `{name}`"));
        }
    }
    Ok(overrides)
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

pub(crate) fn map_overfitting_risk(risk: OverfittingRiskArg) -> OverfittingRiskLevel {
    match risk {
        OverfittingRiskArg::Low => OverfittingRiskLevel::Low,
        OverfittingRiskArg::Moderate => OverfittingRiskLevel::Moderate,
        OverfittingRiskArg::High => OverfittingRiskLevel::High,
    }
}

pub(crate) fn global_fee_schedule(maker_fee_bps: f64, taker_fee_bps: f64) -> FeeSchedule {
    FeeSchedule {
        maker_bps: maker_fee_bps,
        taker_bps: taker_fee_bps,
    }
}

pub(crate) fn parse_execution_fee_schedules(
    specs: &[String],
    execution_source_aliases: &[String],
) -> Result<BTreeMap<String, FeeSchedule>, String> {
    let known_aliases = execution_source_aliases
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut schedules = BTreeMap::new();
    for spec in specs {
        let mut parts = spec.split(':');
        let alias = parts
            .next()
            .filter(|alias| !alias.is_empty())
            .ok_or_else(|| {
                format!(
                    "invalid --fee-schedule `{spec}`; expected <execution_alias>:<maker_bps>:<taker_bps>"
                )
            })?;
        let maker_bps = parts
            .next()
            .ok_or_else(|| {
                format!(
                    "invalid --fee-schedule `{spec}`; expected <execution_alias>:<maker_bps>:<taker_bps>"
                )
            })?
            .parse::<f64>()
            .map_err(|_| format!("invalid maker fee in --fee-schedule `{spec}`"))?;
        let taker_bps = parts
            .next()
            .ok_or_else(|| {
                format!(
                    "invalid --fee-schedule `{spec}`; expected <execution_alias>:<maker_bps>:<taker_bps>"
                )
            })?
            .parse::<f64>()
            .map_err(|_| format!("invalid taker fee in --fee-schedule `{spec}`"))?;
        if parts.next().is_some() {
            return Err(format!(
                "invalid --fee-schedule `{spec}`; expected <execution_alias>:<maker_bps>:<taker_bps>"
            ));
        }
        if !known_aliases.contains(alias) {
            return Err(format!(
                "fee schedule alias `{alias}` is not one of the selected execution aliases"
            ));
        }
        if schedules
            .insert(
                alias.to_string(),
                FeeSchedule {
                    maker_bps,
                    taker_bps,
                },
            )
            .is_some()
        {
            return Err(format!(
                "duplicate --fee-schedule for execution alias `{alias}`"
            ));
        }
    }
    Ok(schedules)
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

fn format_execution_error(error: ExecutionError) -> String {
    format!("execution error: {error}")
}

fn current_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
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
        constraints: result.config.constraints.clone(),
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

#[cfg(test)]
mod tests {
    use super::{
        global_fee_schedule, parse_execution_fee_schedules, parse_input_override_assignments,
        resolve_preset_input_overrides,
    };
    use palmscript::{
        FeeSchedule, OptimizeCandidateSummary, OptimizeEvaluationSummary, OptimizeObjective,
        OptimizePreset, OptimizeRunner,
    };
    use std::collections::BTreeMap;

    #[test]
    fn global_fee_schedule_uses_explicit_values() {
        assert_eq!(
            global_fee_schedule(2.0, 5.0),
            FeeSchedule {
                maker_bps: 2.0,
                taker_bps: 5.0,
            }
        );
    }

    #[test]
    fn parse_execution_fee_schedules_rejects_unknown_aliases() {
        let err = parse_execution_fee_schedules(&["missing:1:2".to_string()], &["spot".into()])
            .expect_err("unknown alias should fail");
        assert!(err.contains("fee schedule alias `missing`"));
    }

    #[test]
    fn parse_execution_fee_schedules_collects_alias_rates() {
        let schedules = parse_execution_fee_schedules(
            &["spot:1.5:4.5".to_string(), "perp:2:5".to_string()],
            &["spot".into(), "perp".into()],
        )
        .expect("valid fee schedules should parse");
        assert_eq!(
            schedules,
            BTreeMap::from([
                (
                    "perp".to_string(),
                    FeeSchedule {
                        maker_bps: 2.0,
                        taker_bps: 5.0,
                    },
                ),
                (
                    "spot".to_string(),
                    FeeSchedule {
                        maker_bps: 1.5,
                        taker_bps: 4.5,
                    },
                ),
            ])
        );
    }

    #[test]
    fn parse_input_override_assignments_rejects_duplicate_names() {
        let err = parse_input_override_assignments(&[
            "threshold=1".to_string(),
            "threshold=2".to_string(),
        ])
        .expect_err("duplicate names should fail");
        assert!(err.contains("duplicate `--set` override"));
    }

    #[test]
    fn resolve_preset_input_overrides_selects_trial_and_applies_mutations() {
        let preset = OptimizePreset {
            version: 1,
            script_path: Some("strategy.ps".to_string()),
            script_sha256: "hash".to_string(),
            runner: OptimizeRunner::Backtest,
            objective: OptimizeObjective::EndingEquity,
            backtest: palmscript::BacktestConfig {
                execution_source_alias: "spot".to_string(),
                portfolio_execution_aliases: Vec::new(),
                spot_virtual_rebalance: false,
                activation_time_ms: None,
                initial_capital: 1_000.0,
                maker_fee_bps: 0.0,
                taker_fee_bps: 0.0,
                execution_fee_schedules: BTreeMap::new(),
                slippage_bps: 0.0,
                diagnostics_detail: palmscript::DiagnosticsDetailMode::SummaryOnly,
                perp: None,
                perp_context: None,
                portfolio_perp_contexts: BTreeMap::new(),
            },
            walk_forward: None,
            diagnostics_detail: palmscript::DiagnosticsDetailMode::SummaryOnly,
            holdout: None,
            constraints: palmscript::ValidationConstraintConfig::default(),
            parameter_space: Vec::new(),
            best_input_overrides: BTreeMap::from([
                ("threshold".to_string(), 0.0),
                ("atr_mult".to_string(), 2.0),
            ]),
            top_candidates: vec![
                OptimizeCandidateSummary {
                    trial_id: 7,
                    input_overrides: BTreeMap::from([
                        ("threshold".to_string(), 100.0),
                        ("atr_mult".to_string(), 2.0),
                    ]),
                    objective_score: 1.0,
                    summary: OptimizeEvaluationSummary::Backtest {
                        summary: palmscript::BacktestSummary {
                            starting_equity: 1_000.0,
                            ending_equity: 1_000.0,
                            realized_pnl: 0.0,
                            unrealized_pnl: 0.0,
                            total_return: 0.0,
                            sharpe_ratio: None,
                            trade_count: 0,
                            winning_trade_count: 0,
                            losing_trade_count: 0,
                            win_rate: 0.0,
                            max_drawdown: 0.0,
                            max_gross_exposure: 0.0,
                            max_net_exposure: 0.0,
                            peak_open_position_count: 0,
                        },
                        capture_summary: palmscript::BacktestCaptureSummary::default(),
                        arbitrage: palmscript::ArbitrageDiagnosticsSummary::default(),
                        transfer_summary: palmscript::TransferDiagnosticsSummary::default(),
                    },
                    time_bucket_cohorts: Vec::new(),
                    constraints: palmscript::ValidationConstraintSummary::default(),
                },
                OptimizeCandidateSummary {
                    trial_id: 9,
                    input_overrides: BTreeMap::from([
                        ("threshold".to_string(), 0.0),
                        ("atr_mult".to_string(), 3.0),
                    ]),
                    objective_score: 2.0,
                    summary: OptimizeEvaluationSummary::Backtest {
                        summary: palmscript::BacktestSummary {
                            starting_equity: 1_000.0,
                            ending_equity: 1_100.0,
                            realized_pnl: 100.0,
                            unrealized_pnl: 0.0,
                            total_return: 0.1,
                            sharpe_ratio: Some(1.0),
                            trade_count: 2,
                            winning_trade_count: 2,
                            losing_trade_count: 0,
                            win_rate: 1.0,
                            max_drawdown: 0.0,
                            max_gross_exposure: 0.0,
                            max_net_exposure: 0.0,
                            peak_open_position_count: 1,
                        },
                        capture_summary: palmscript::BacktestCaptureSummary::default(),
                        arbitrage: palmscript::ArbitrageDiagnosticsSummary::default(),
                        transfer_summary: palmscript::TransferDiagnosticsSummary::default(),
                    },
                    time_bucket_cohorts: Vec::new(),
                    constraints: palmscript::ValidationConstraintSummary::default(),
                },
            ],
        };

        let overrides = resolve_preset_input_overrides(
            Some(&preset),
            Some(7),
            &["atr_mult=4".to_string(), "risk=1.5".to_string()],
        )
        .expect("trial selection should succeed");

        assert_eq!(
            overrides,
            BTreeMap::from([
                ("atr_mult".to_string(), 4.0),
                ("risk".to_string(), 1.5),
                ("threshold".to_string(), 100.0),
            ])
        );
    }
}
