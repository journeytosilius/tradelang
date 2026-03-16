use palmscript::bytecode::{Constant, Instruction, LocalInfo, OpCode, Program};
use palmscript::compiler::CompiledProgram;
use palmscript::diagnostic::RuntimeError;
#[path = "support/mod.rs"]
mod support;

use palmscript::runtime::{Bar, VmLimits};
use palmscript::types::Value;
use palmscript::{
    run_with_sources, Interval, MarketBinding, MarketField, MarketSource, Outputs, SourceFeed,
    SourceRuntimeConfig, SourceTemplate, Type,
};

fn with_interval(source: &str) -> String {
    support::with_single_source_interval(source)
}

fn with_intervals(base: &str, supplemental: &[&str], source: &str) -> String {
    support::with_single_source_intervals(base, supplemental, source)
}

#[derive(Clone, Debug)]
struct IntervalFeed {
    interval: Interval,
    bars: Vec<Bar>,
}

#[derive(Clone, Debug)]
struct MultiIntervalConfig {
    base_interval: Interval,
    supplemental: Vec<IntervalFeed>,
}

fn run(
    compiled: &CompiledProgram,
    bars: &[Bar],
    limits: VmLimits,
) -> Result<Outputs, RuntimeError> {
    let base_interval = compiled
        .program
        .base_interval
        .expect("compiled strategy should declare a base interval");
    run_with_sources(
        compiled,
        SourceRuntimeConfig {
            base_interval,
            feeds: vec![SourceFeed {
                source_id: 0,
                interval: base_interval,
                bars: bars.to_vec(),
            }],
        },
        limits,
    )
}

fn run_multi_interval(
    compiled: &CompiledProgram,
    base_bars: &[Bar],
    config: MultiIntervalConfig,
    limits: VmLimits,
) -> Result<Outputs, RuntimeError> {
    let mut feeds = Vec::with_capacity(1 + config.supplemental.len());
    feeds.push(SourceFeed {
        source_id: 0,
        interval: config.base_interval,
        bars: base_bars.to_vec(),
    });
    feeds.extend(config.supplemental.into_iter().map(|feed| SourceFeed {
        source_id: 0,
        interval: feed.interval,
        bars: feed.bars,
    }));
    run_with_sources(
        compiled,
        SourceRuntimeConfig {
            base_interval: config.base_interval,
            feeds,
        },
        limits,
    )
}

fn empty_locals() -> Vec<LocalInfo> {
    vec![
        LocalInfo::series(
            Some("open".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Named {
                    source_id: 0,
                    interval: None,
                },
                field: MarketField::Open,
            }),
        ),
        LocalInfo::series(
            Some("high".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Named {
                    source_id: 0,
                    interval: None,
                },
                field: MarketField::High,
            }),
        ),
        LocalInfo::series(
            Some("low".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Named {
                    source_id: 0,
                    interval: None,
                },
                field: MarketField::Low,
            }),
        ),
        LocalInfo::series(
            Some("close".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Named {
                    source_id: 0,
                    interval: None,
                },
                field: MarketField::Close,
            }),
        ),
        LocalInfo::series(
            Some("volume".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Named {
                    source_id: 0,
                    interval: None,
                },
                field: MarketField::Volume,
            }),
        ),
        LocalInfo::series(
            Some("time".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Named {
                    source_id: 0,
                    interval: None,
                },
                field: MarketField::Time,
            }),
        ),
    ]
}

fn default_declared_sources() -> Vec<palmscript::DeclaredMarketSource> {
    vec![palmscript::DeclaredMarketSource {
        id: 0,
        alias: support::DEFAULT_SOURCE_ALIAS.to_string(),
        template: SourceTemplate::BinanceSpot,
        symbol: "BTCUSDT".to_string(),
    }]
}

fn bars() -> Vec<Bar> {
    vec![Bar {
        open: 1.0,
        high: 2.0,
        low: 0.5,
        close: 1.5,
        volume: 10.0,
        time: JAN_1_2024_UTC_MS as f64,
        funding_rate: None,
        open_interest: None,
        mark_price: None,
        index_price: None,
        premium_index: None,
        basis: None,
    }]
}

fn logic_literal(value: &str) -> &'static str {
    match value {
        "true" => "true",
        "false" => "false",
        "na" => "na",
        _ => panic!("unexpected logical literal"),
    }
}

fn evaluate_logic(op: &str, left: &str, right: &str) -> Option<f64> {
    let expr = format!("{} {} {}", logic_literal(left), op, logic_literal(right));
    let script = with_interval(&format!(
        "if {expr} {{ plot(1) }} else if !({expr}) {{ plot(2) }} else {{ plot(3) }}"
    ));
    let compiled = palmscript::compile(&script).expect("script should compile");
    let outputs = run(&compiled, &bars(), VmLimits::default()).expect("script should run");
    outputs.plots[0].points[0].value
}

#[test]
fn tiny_program_push_add_plot_executes() {
    let program = Program {
        instructions: vec![
            Instruction::new(OpCode::LoadConst).with_a(0),
            Instruction::new(OpCode::LoadConst).with_a(1),
            Instruction::new(OpCode::Add),
            Instruction::new(OpCode::CallBuiltin).with_a(9).with_b(1),
            Instruction::new(OpCode::Return),
        ],
        constants: vec![
            Constant::Value(Value::F64(1.0)),
            Constant::Value(Value::F64(2.0)),
        ],
        locals: empty_locals(),
        inputs: vec![],
        outputs: vec![],
        signal_modules: vec![],
        arb_signals: vec![],
        arb_orders: vec![],
        transfers: vec![],
        order_fields: vec![],
        position_fields: vec![],
        position_event_fields: vec![],
        last_exit_fields: vec![],
        ledger_fields: vec![],
        execution_price_fields: vec![],
        orders: vec![],
        risk_controls: vec![],
        portfolio_controls: vec![],
        portfolio_groups: vec![],
        base_interval: Some(Interval::Min1),
        declared_sources: default_declared_sources(),
        declared_executions: vec![],
        source_intervals: vec![],
        history_capacity: 2,
        plot_count: 1,
    };
    let compiled = CompiledProgram {
        program,
        source: String::new(),
    };
    let outputs = run(&compiled, &bars(), VmLimits::default()).expect("vm should run");
    assert_eq!(outputs.plots[0].points[0].value, Some(3.0));
}

#[test]
fn stack_underflow_is_reported() {
    let program = Program {
        instructions: vec![
            Instruction::new(OpCode::Add),
            Instruction::new(OpCode::Return),
        ],
        constants: vec![],
        locals: empty_locals(),
        inputs: vec![],
        outputs: vec![],
        signal_modules: vec![],
        arb_signals: vec![],
        arb_orders: vec![],
        transfers: vec![],
        order_fields: vec![],
        position_fields: vec![],
        position_event_fields: vec![],
        last_exit_fields: vec![],
        ledger_fields: vec![],
        execution_price_fields: vec![],
        orders: vec![],
        risk_controls: vec![],
        portfolio_controls: vec![],
        portfolio_groups: vec![],
        base_interval: Some(Interval::Min1),
        declared_sources: default_declared_sources(),
        declared_executions: vec![],
        source_intervals: vec![],
        history_capacity: 2,
        plot_count: 0,
    };
    let compiled = CompiledProgram {
        program,
        source: String::new(),
    };
    let err = run(&compiled, &bars(), VmLimits::default()).expect_err("expected stack underflow");
    assert!(matches!(err, RuntimeError::StackUnderflow { .. }));
}

#[test]
fn invalid_jump_is_reported() {
    let program = Program {
        instructions: vec![
            Instruction::new(OpCode::Jump).with_a(999),
            Instruction::new(OpCode::Return),
        ],
        constants: vec![],
        locals: empty_locals(),
        inputs: vec![],
        outputs: vec![],
        signal_modules: vec![],
        arb_signals: vec![],
        arb_orders: vec![],
        transfers: vec![],
        order_fields: vec![],
        position_fields: vec![],
        position_event_fields: vec![],
        last_exit_fields: vec![],
        ledger_fields: vec![],
        execution_price_fields: vec![],
        orders: vec![],
        risk_controls: vec![],
        portfolio_controls: vec![],
        portfolio_groups: vec![],
        base_interval: Some(Interval::Min1),
        declared_sources: default_declared_sources(),
        declared_executions: vec![],
        source_intervals: vec![],
        history_capacity: 2,
        plot_count: 0,
    };
    let compiled = CompiledProgram {
        program,
        source: String::new(),
    };
    let err = run(&compiled, &bars(), VmLimits::default()).expect_err("expected invalid jump");
    assert!(matches!(err, RuntimeError::InvalidJump { .. }));
}

#[test]
fn instruction_budget_exhaustion_is_reported() {
    let compiled =
        palmscript::compile(&with_interval("plot(sma(close, 5))")).expect("script should compile");
    let fixture = bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[1.0; 6]);
    let err = run(
        &compiled,
        &fixture,
        VmLimits {
            max_instructions_per_bar: 3,
            max_history_capacity: 32,
        },
    )
    .expect_err("budget should exhaust");
    assert!(matches!(
        err,
        RuntimeError::InstructionBudgetExceeded { .. }
    ));
}

#[test]
fn and_truth_table_matches_spec() {
    let cases = [
        (("true", "true"), Some(1.0)),
        (("true", "false"), Some(2.0)),
        (("true", "na"), Some(3.0)),
        (("false", "true"), Some(2.0)),
        (("false", "false"), Some(2.0)),
        (("false", "na"), Some(2.0)),
        (("na", "true"), Some(3.0)),
        (("na", "false"), Some(2.0)),
        (("na", "na"), Some(3.0)),
    ];

    for ((left, right), expected) in cases {
        let value = evaluate_logic("and", left, right);
        assert_eq!(value, expected, "unexpected result for {left} and {right}");
    }
}

#[test]
fn or_truth_table_matches_spec() {
    let cases = [
        (("true", "true"), Some(1.0)),
        (("true", "false"), Some(1.0)),
        (("true", "na"), Some(1.0)),
        (("false", "true"), Some(1.0)),
        (("false", "false"), Some(2.0)),
        (("false", "na"), Some(3.0)),
        (("na", "true"), Some(1.0)),
        (("na", "false"), Some(3.0)),
        (("na", "na"), Some(3.0)),
    ];

    for ((left, right), expected) in cases {
        let value = evaluate_logic("or", left, right);
        assert_eq!(value, expected, "unexpected result for {left} or {right}");
    }
}

#[test]
fn logical_precedence_matches_spec() {
    let compiled = palmscript::compile(&with_interval(
        "if true or false and false { plot(1) } else { plot(0) }",
    ))
    .expect("script should compile");
    let outputs = run(&compiled, &bars(), VmLimits::default()).expect("script should run");
    assert_eq!(outputs.plots[0].points[0].value, Some(1.0));
}

#[test]
fn division_operator_executes_with_numeric_results() {
    let compiled = palmscript::compile(&with_interval("plot((close - close[1]) / close[1])"))
        .expect("script should compile");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script should run");
    assert_eq!(outputs.plots[0].points[0].value, None);
    let second = outputs.plots[0].points[1].value.expect("second point");
    assert!((second - 0.01).abs() < 1e-12);
}

#[test]
fn else_if_selects_the_first_matching_branch() {
    let compiled = palmscript::compile(&with_interval(
        "if false { plot(0) } else if true { plot(1) } else { plot(2) }",
    ))
    .expect("script should compile");
    let outputs = run(&compiled, &bars(), VmLimits::default()).expect("script should run");
    assert_eq!(outputs.plots[0].points[0].value, Some(1.0));
}

fn fixture_bars() -> Vec<Bar> {
    (0..12)
        .map(|index| {
            let close = 100.0 + index as f64;
            Bar {
                open: close - 0.5,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1_000.0 + index as f64,
                time: JAN_1_2024_UTC_MS as f64 + index as f64 * 60_000.0,
                funding_rate: None,
                open_interest: None,
                mark_price: None,
                index_price: None,
                premium_index: None,
                basis: None,
            }
        })
        .collect()
}

const SECOND_MS: i64 = 1_000;
const MINUTE_MS: i64 = 60 * SECOND_MS;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;
const WEEK_MS: i64 = 7 * DAY_MS;
const JAN_1_2024_UTC_MS: i64 = 1_704_067_200_000;
const FEB_1_2024_UTC_MS: i64 = 1_706_745_600_000;

fn bars_with_spacing(start_ms: i64, spacing_ms: i64, closes: &[f64]) -> Vec<Bar> {
    closes
        .iter()
        .enumerate()
        .map(|(index, close)| Bar {
            open: *close - 0.5,
            high: *close + 1.0,
            low: *close - 1.0,
            close: *close,
            volume: 1_000.0 + index as f64,
            time: (start_ms + spacing_ms * index as i64) as f64,
            funding_rate: None,
            open_interest: None,
            mark_price: None,
            index_price: None,
            premium_index: None,
            basis: None,
        })
        .collect()
}

fn plot_values(source: &str, bars: &[Bar]) -> Vec<Option<f64>> {
    let compiled = palmscript::compile(&with_interval(source)).expect("script should compile");
    let outputs = run(&compiled, bars, VmLimits::default()).expect("script should run");
    outputs.plots[0]
        .points
        .iter()
        .map(|point| point.value)
        .collect()
}

#[test]
fn user_function_inlining_matches_inline_expression() {
    let helper = palmscript::compile(&with_interval(
        "fn is_rising(series) = series > series[1]\nif is_rising(close) { plot(1) } else { plot(0) }",
    ))
    .expect("helper script should compile");
    let inline = palmscript::compile(&with_interval(
        "if close > close[1] { plot(1) } else { plot(0) }",
    ))
    .expect("inline script should compile");
    let helper_outputs = run(&helper, &fixture_bars(), VmLimits::default()).expect("helper runs");
    let inline_outputs = run(&inline, &fixture_bars(), VmLimits::default()).expect("inline runs");
    assert_eq!(helper_outputs, inline_outputs);
}

#[test]
fn nested_user_functions_execute_over_indicators() {
    let compiled = palmscript::compile(
        &with_interval(
            "fn cross_signal(a, b) = a > b and a[1] <= b[1]\nfn long_signal(fast, slow) = cross_signal(fast, slow) or fast > slow\nlet fast = ema(close, 3)\nlet slow = ema(close, 5)\nif long_signal(fast, slow) { plot(1) } else { plot(0) }",
        ),
    )
    .expect("script should compile");
    let outputs = run(&compiled, &fixture_bars(), VmLimits::default()).expect("script should run");
    assert_eq!(outputs.plots[0].points.len(), 12);
    assert_eq!(outputs.plots[0].points[0].value, Some(0.0));
}

#[test]
fn user_function_with_na_result_preserves_null_plot() {
    let compiled = palmscript::compile(&with_interval("fn missing() = na\nplot(missing())"))
        .expect("script should compile");
    let outputs = run(&compiled, &bars(), VmLimits::default()).expect("script should run");
    assert_eq!(outputs.plots[0].points[0].value, None);
}

#[test]
fn referenced_source_intervals_require_matching_runtime_feeds() {
    let compiled = palmscript::compile(&with_intervals("1m", &["1h"], "plot(1h.close)"))
        .expect("script should compile");
    let err = run(&compiled, &fixture_bars(), VmLimits::default()).expect_err("feed required");
    assert!(matches!(
        err,
        RuntimeError::MissingSourceIntervalFeed {
            source_id: 0,
            interval: Interval::Hour1
        }
    ));
}

#[test]
fn lower_interval_references_are_rejected() {
    let err = palmscript::compile(&with_intervals("1h", &["1m"], "plot(1m.close)"))
        .expect_err("lower interval should reject");
    assert!(err
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.message.contains("lower interval reference `1m`")));
}

#[test]
fn hourly_series_only_updates_on_hour_close_boundaries() {
    let compiled = palmscript::compile(&with_intervals("1m", &["1h"], "plot(1h.close)"))
        .expect("script should compile");
    let base = bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[1.0; 120]);
    let hourly = bars_with_spacing(JAN_1_2024_UTC_MS, HOUR_MS, &[100.0, 200.0]);
    let outputs = run_multi_interval(
        &compiled,
        &base,
        MultiIntervalConfig {
            base_interval: Interval::Min1,
            supplemental: vec![IntervalFeed {
                interval: Interval::Hour1,
                bars: hourly,
            }],
        },
        VmLimits::default(),
    )
    .expect("script should run");
    assert_eq!(outputs.plots[0].points[58].value, None);
    assert_eq!(outputs.plots[0].points[59].value, Some(100.0));
    assert_eq!(outputs.plots[0].points[118].value, Some(100.0));
    assert_eq!(outputs.plots[0].points[119].value, Some(200.0));
}

#[test]
fn minute_series_only_updates_on_minute_close_boundaries_from_seconds() {
    let compiled = palmscript::compile(&with_intervals("1s", &["1m"], "plot(1m.close)"))
        .expect("script should compile");
    let base = bars_with_spacing(JAN_1_2024_UTC_MS, SECOND_MS, &[1.0; 120]);
    let minute = bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 20.0]);
    let outputs = run_multi_interval(
        &compiled,
        &base,
        MultiIntervalConfig {
            base_interval: Interval::Sec1,
            supplemental: vec![IntervalFeed {
                interval: Interval::Min1,
                bars: minute,
            }],
        },
        VmLimits::default(),
    )
    .expect("script should run");
    assert_eq!(outputs.plots[0].points[58].value, None);
    assert_eq!(outputs.plots[0].points[59].value, Some(10.0));
    assert_eq!(outputs.plots[0].points[119].value, Some(20.0));
}

#[test]
fn weekly_series_only_updates_on_week_close_boundaries() {
    let compiled = palmscript::compile(&with_intervals("1d", &["1w"], "plot(1w.close)"))
        .expect("script should compile");
    let base = bars_with_spacing(JAN_1_2024_UTC_MS, DAY_MS, &[1.0; 14]);
    let weekly = bars_with_spacing(JAN_1_2024_UTC_MS, WEEK_MS, &[10.0, 20.0]);
    let outputs = run_multi_interval(
        &compiled,
        &base,
        MultiIntervalConfig {
            base_interval: Interval::Day1,
            supplemental: vec![IntervalFeed {
                interval: Interval::Week1,
                bars: weekly,
            }],
        },
        VmLimits::default(),
    )
    .expect("script should run");
    assert_eq!(outputs.plots[0].points[5].value, None);
    assert_eq!(outputs.plots[0].points[6].value, Some(10.0));
    assert_eq!(outputs.plots[0].points[12].value, Some(10.0));
    assert_eq!(outputs.plots[0].points[13].value, Some(20.0));
}

#[test]
fn monthly_series_uses_calendar_close_boundaries() {
    let compiled = palmscript::compile(&with_intervals("1w", &["1M"], "plot(1M.close)"))
        .expect("script should compile");
    let base = bars_with_spacing(JAN_1_2024_UTC_MS, WEEK_MS, &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
    let monthly = bars_with_spacing(
        JAN_1_2024_UTC_MS,
        FEB_1_2024_UTC_MS - JAN_1_2024_UTC_MS,
        &[300.0, 400.0],
    );
    let outputs = run_multi_interval(
        &compiled,
        &base,
        MultiIntervalConfig {
            base_interval: Interval::Week1,
            supplemental: vec![IntervalFeed {
                interval: Interval::Month1,
                bars: monthly,
            }],
        },
        VmLimits::default(),
    )
    .expect("script should run");
    assert_eq!(outputs.plots[0].points[3].value, None);
    assert_eq!(outputs.plots[0].points[4].value, Some(300.0));
}

#[test]
fn missing_interval_bars_become_na_steps() {
    let compiled = palmscript::compile(&with_intervals("1h", &["1d"], "plot(1d.close)"))
        .expect("script should compile");
    let base = bars_with_spacing(JAN_1_2024_UTC_MS, HOUR_MS, &[1.0; 72]);
    let daily = vec![
        bars_with_spacing(JAN_1_2024_UTC_MS, DAY_MS, &[10.0])[0],
        Bar {
            time: (JAN_1_2024_UTC_MS + 2 * DAY_MS) as f64,
            ..bars_with_spacing(JAN_1_2024_UTC_MS + 2 * DAY_MS, DAY_MS, &[30.0])[0]
        },
    ];
    let outputs = run_multi_interval(
        &compiled,
        &base,
        MultiIntervalConfig {
            base_interval: Interval::Hour1,
            supplemental: vec![IntervalFeed {
                interval: Interval::Day1,
                bars: daily,
            }],
        },
        VmLimits::default(),
    )
    .expect("script should run");
    assert_eq!(outputs.plots[0].points[23].value, Some(10.0));
    assert_eq!(outputs.plots[0].points[47].value, None);
    assert_eq!(outputs.plots[0].points[71].value, Some(30.0));
}

#[test]
fn insufficient_history_capacity_rejects_at_engine_construction() {
    let compiled = palmscript::compile(&with_intervals("1d", &["1w"], "plot(1w.close[3])"))
        .expect("script should compile");
    let err = run_multi_interval(
        &compiled,
        &bars_with_spacing(JAN_1_2024_UTC_MS, DAY_MS, &[1.0; 28]),
        MultiIntervalConfig {
            base_interval: Interval::Day1,
            supplemental: vec![IntervalFeed {
                interval: Interval::Week1,
                bars: bars_with_spacing(JAN_1_2024_UTC_MS, WEEK_MS, &[1.0, 2.0, 3.0, 4.0]),
            }],
        },
        VmLimits {
            max_instructions_per_bar: 10_000,
            max_history_capacity: 2,
        },
    )
    .expect_err("history cap should reject");
    assert!(matches!(
        err,
        RuntimeError::HistoryCapacityExceeded {
            required: 4,
            limit: 2,
            ..
        }
    ));
}

#[test]
fn exports_are_recorded_each_bar() {
    let compiled = palmscript::compile(&with_interval("export trend = close > close[1]\nplot(0)"))
        .expect("script compiles");
    let outputs = run(
        &compiled,
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 11.0, 9.0]),
        VmLimits::default(),
    )
    .expect("script runs");
    assert_eq!(outputs.exports.len(), 1);
    assert_eq!(outputs.exports[0].name, "trend");
    assert_eq!(outputs.exports[0].points.len(), 3);
    assert!(matches!(
        outputs.exports[0].points[0].value,
        palmscript::OutputValue::NA
    ));
    assert!(matches!(
        outputs.exports[0].points[1].value,
        palmscript::OutputValue::Bool(true)
    ));
    assert!(matches!(
        outputs.exports[0].points[2].value,
        palmscript::OutputValue::Bool(false)
    ));
}

#[test]
fn state_builtin_persists_regime_transitions() {
    let compiled = palmscript::compile(&with_interval(
        "regime trend_long = state(close > close[1], close < close[1])\nexport entered = activated(trend_long)\nexport exited = deactivated(trend_long)\nplot(0)",
    ))
    .expect("script compiles");
    let outputs = run(
        &compiled,
        &bars_with_spacing(
            JAN_1_2024_UTC_MS,
            MINUTE_MS,
            &[10.0, 11.0, 12.0, 11.0, 10.0, 11.0],
        ),
        VmLimits::default(),
    )
    .expect("script runs");

    assert_eq!(outputs.exports[0].name, "trend_long");
    assert_eq!(outputs.exports[1].name, "entered");
    assert_eq!(outputs.exports[2].name, "exited");

    let trend_values = outputs.exports[0]
        .points
        .iter()
        .map(|point| point.value.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        trend_values,
        vec![
            palmscript::OutputValue::Bool(false),
            palmscript::OutputValue::Bool(true),
            palmscript::OutputValue::Bool(true),
            palmscript::OutputValue::Bool(false),
            palmscript::OutputValue::Bool(false),
            palmscript::OutputValue::Bool(true),
        ]
    );

    let entered_values = outputs.exports[1]
        .points
        .iter()
        .map(|point| point.value.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        entered_values,
        vec![
            palmscript::OutputValue::Bool(false),
            palmscript::OutputValue::Bool(true),
            palmscript::OutputValue::Bool(false),
            palmscript::OutputValue::Bool(false),
            palmscript::OutputValue::Bool(false),
            palmscript::OutputValue::Bool(true),
        ]
    );

    let exited_values = outputs.exports[2]
        .points
        .iter()
        .map(|point| point.value.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        exited_values,
        vec![
            palmscript::OutputValue::Bool(false),
            palmscript::OutputValue::Bool(false),
            palmscript::OutputValue::Bool(false),
            palmscript::OutputValue::Bool(true),
            palmscript::OutputValue::Bool(false),
            palmscript::OutputValue::Bool(false),
        ]
    );
}

#[test]
fn triggers_emit_samples_and_events() {
    let compiled = palmscript::compile(&with_interval(
        "trigger breakout = close > close[1]\nplot(0)",
    ))
    .expect("script compiles");
    let outputs = run(
        &compiled,
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 11.0, 9.0, 12.0]),
        VmLimits::default(),
    )
    .expect("script runs");
    assert_eq!(outputs.triggers.len(), 1);
    assert_eq!(outputs.triggers[0].points.len(), 4);
    assert_eq!(outputs.trigger_events.len(), 2);
    assert_eq!(outputs.trigger_events[0].bar_index, 1);
    assert_eq!(outputs.trigger_events[1].bar_index, 3);
}

#[test]
fn relation_helpers_follow_strict_semantics() {
    let values = plot_values(
        "if above(close, open) and between(close, low, high) and !outside(close, low, high) and below(low, close) { plot(1) } else { plot(0) }",
        &bars(),
    );
    assert_eq!(values, vec![Some(1.0)]);
}

#[test]
fn cross_helpers_use_strict_current_and_inclusive_prior_rules() {
    let crossover = plot_values(
        "if crossover(close, 10) { plot(1) } else { plot(0) }",
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[9.0, 10.0, 11.0, 8.0]),
    );
    assert_eq!(crossover, vec![Some(0.0), Some(0.0), Some(1.0), Some(0.0)]);

    let crossunder = plot_values(
        "if crossunder(close, 10) { plot(1) } else { plot(0) }",
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[11.0, 10.0, 9.0, 12.0]),
    );
    assert_eq!(crossunder, vec![Some(0.0), Some(0.0), Some(1.0), Some(0.0)]);

    let cross = plot_values(
        "if cross(close, 10) { plot(1) } else { plot(0) }",
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[9.0, 10.0, 11.0, 10.0, 9.0]),
    );
    assert_eq!(
        cross,
        vec![Some(0.0), Some(0.0), Some(1.0), Some(0.0), Some(1.0)]
    );
}

#[test]
fn change_and_roc_handle_history_and_zero_denominator() {
    let change = plot_values(
        "plot(change(close, 2))",
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 12.0, 15.0]),
    );
    assert_eq!(change, vec![None, None, Some(5.0)]);

    let roc = plot_values(
        "plot(roc(close, 1))",
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[0.0, 10.0, 5.0]),
    );
    assert_eq!(roc, vec![None, None, Some(-50.0)]);
}

#[test]
fn talib_roc_family_uses_default_window_and_expected_formulas() {
    let bars = bars_with_spacing(
        JAN_1_2024_UTC_MS,
        MINUTE_MS,
        &[
            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0, 108.0, 109.0, 110.0,
        ],
    );

    assert_eq!(plot_values("plot(mom(close))", &bars)[10], Some(10.0));
    assert_eq!(plot_values("plot(roc(close))", &bars)[10], Some(10.0));
    let rocp = plot_values("plot(rocp(close))", &bars)[10].expect("rocp value");
    let rocr = plot_values("plot(rocr(close))", &bars)[10].expect("rocr value");
    let rocr100 = plot_values("plot(rocr100(close))", &bars)[10].expect("rocr100 value");
    assert!((rocp - 0.1).abs() < 1e-12);
    assert!((rocr - 1.1).abs() < 1e-12);
    assert!((rocr100 - 110.0).abs() < 1e-12);
}

#[test]
fn extrema_and_direction_helpers_respect_window_and_na() {
    let bars = bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[1.0, 3.0, 2.0, 4.0]);
    assert_eq!(plot_values("plot(highest(close, 3))", &bars)[3], Some(4.0));
    assert_eq!(plot_values("plot(lowest(close, 3))", &bars)[3], Some(2.0));

    let rising = plot_values(
        "if rising(close, 2) { plot(1) } else { plot(0) }",
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[1.0, 2.0, 3.0, 2.0]),
    );
    assert_eq!(rising, vec![Some(0.0), Some(0.0), Some(1.0), Some(0.0)]);

    let falling = plot_values(
        "if falling(close, 2) { plot(1) } else { plot(0) }",
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[3.0, 2.0, 1.0, 2.0]),
    );
    assert_eq!(falling, vec![Some(0.0), Some(0.0), Some(1.0), Some(0.0)]);
}

#[test]
fn event_memory_helpers_track_matches() {
    let activated = plot_values(
        "plot(activated(close > close[1]) ? 1 : 0)",
        &bars_with_spacing(
            JAN_1_2024_UTC_MS,
            MINUTE_MS,
            &[10.0, 11.0, 12.0, 11.0, 12.0],
        ),
    );
    assert_eq!(
        activated,
        vec![Some(0.0), Some(1.0), Some(0.0), Some(0.0), Some(1.0)]
    );

    let deactivated = plot_values(
        "plot(deactivated(close > close[1]) ? 1 : 0)",
        &bars_with_spacing(
            JAN_1_2024_UTC_MS,
            MINUTE_MS,
            &[10.0, 11.0, 12.0, 11.0, 12.0],
        ),
    );
    assert_eq!(
        deactivated,
        vec![Some(0.0), Some(0.0), Some(0.0), Some(1.0), Some(0.0)]
    );

    let barssince = plot_values(
        "plot(barssince(close > close[1]))",
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 11.0, 9.0, 9.0, 12.0]),
    );
    assert_eq!(
        barssince,
        vec![None, Some(0.0), Some(1.0), Some(2.0), Some(0.0)]
    );

    let valuewhen = plot_values(
        "plot(valuewhen(close > close[1], close, 1))",
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 11.0, 9.0, 12.0, 8.0]),
    );
    assert_eq!(valuewhen, vec![None, None, None, Some(11.0), Some(11.0)]);

    let count_since = plot_values(
        "plot(count_since(close == 11, close > close[1]))",
        &bars_with_spacing(
            JAN_1_2024_UTC_MS,
            MINUTE_MS,
            &[10.0, 11.0, 12.0, 11.0, 10.0, 11.0, 12.0],
        ),
    );
    assert_eq!(
        count_since,
        vec![
            None,
            Some(1.0),
            Some(2.0),
            Some(0.0),
            Some(0.0),
            Some(1.0),
            Some(2.0),
        ]
    );
}

#[test]
fn null_conditional_and_state_helpers_execute() {
    let bars = bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 11.0, 9.0, 12.0]);

    assert_eq!(
        plot_values("plot(nz(close[1]))", &bars),
        vec![Some(0.0), Some(10.0), Some(11.0), Some(9.0)]
    );
    assert_eq!(
        plot_values("plot(coalesce(close[2], 7))", &bars),
        vec![Some(7.0), Some(7.0), Some(10.0), Some(11.0)]
    );
    assert_eq!(
        plot_values("plot(na(close[1]) ? 1 : 0)", &bars),
        vec![Some(1.0), Some(0.0), Some(0.0), Some(0.0)]
    );
    assert_eq!(
        plot_values("plot(cum(close - close[1]))", &bars),
        vec![None, Some(1.0), Some(-1.0), Some(2.0)]
    );
    assert_eq!(
        plot_values("plot(highestbars(close, 3))", &bars)[3],
        Some(0.0)
    );
    assert_eq!(
        plot_values("plot(lowestbars(close, 3))", &bars)[3],
        Some(1.0)
    );
}

#[test]
fn valuewhen_preserves_bool_source_type() {
    let compiled = palmscript::compile(&with_interval(
        "export remembered = valuewhen(close > close[1], close > open, 0)\nplot(0)",
    ))
    .expect("script compiles");
    let outputs = run(
        &compiled,
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 11.0, 9.0]),
        VmLimits::default(),
    )
    .expect("script runs");
    assert!(matches!(
        outputs.exports[0].points[1].value,
        palmscript::OutputValue::Bool(true)
    ));
}

#[test]
fn anchored_vwap_resets_from_anchor_bar_in_language_surface() {
    let compiled = palmscript::compile(&with_interval(
        "let anchor = activated(close > close[1])\nplot(anchored_vwap(anchor, close, volume))",
    ))
    .expect("script should compile");
    let outputs = run(
        &compiled,
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 11.0, 12.0, 11.0]),
        VmLimits::default(),
    )
    .expect("script should run");
    let values = outputs.plots[0]
        .points
        .iter()
        .map(|point| point.value)
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        vec![
            Some(10.0),
            Some(11.0),
            Some(11.500249625561658),
            Some(11.333333333333334),
        ]
    );
}

#[test]
fn supertrend_tuple_destructures_into_line_and_direction() {
    let compiled = palmscript::compile(&with_interval(
        "let (line, bullish) = supertrend(high, low, close, 3, 2.0)\nexport supertrend_bullish = bullish\nplot(line)",
    ))
    .expect("script should compile");
    let outputs = run(
        &compiled,
        &bars_with_spacing(
            JAN_1_2024_UTC_MS,
            MINUTE_MS,
            &[9.5, 10.5, 11.5, 12.5, 13.5, 12.2, 11.2, 10.8],
        ),
        VmLimits::default(),
    )
    .expect("script should run");
    assert_eq!(outputs.plots[0].points.len(), 8);
    assert!(outputs.plots[0]
        .points
        .iter()
        .any(|point| point.value.is_some()));
    assert!(outputs.exports[0]
        .points
        .iter()
        .any(|point| matches!(point.value, palmscript::OutputValue::Bool(_))));
}

#[test]
fn position_event_namespace_is_false_outside_backtests() {
    let compiled = palmscript::compile(&with_interval(
        "export entry_fill = position_event.long_entry_fill
export liquidation_fill = position_event.long_liquidation_fill
export trail = highest_since(position_event.long_entry_fill, close)
plot(0)",
    ))
    .expect("script compiles");
    let outputs = run(
        &compiled,
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 11.0, 12.0]),
        VmLimits::default(),
    )
    .expect("script runs");

    for point in &outputs.exports[0].points {
        assert_eq!(point.value, palmscript::OutputValue::Bool(false));
    }
    for point in &outputs.exports[1].points {
        assert_eq!(point.value, palmscript::OutputValue::Bool(false));
    }
    for point in &outputs.exports[2].points {
        assert_eq!(point.value, palmscript::OutputValue::NA);
    }
}

#[test]
fn last_exit_namespace_is_na_outside_backtests() {
    let compiled = palmscript::compile(&with_interval(
        "export target_fill = position_event.long_target_fill
export last_exit_price = last_long_exit.price
export was_target = last_long_exit.kind == exit_kind.target
export was_liquidation = last_long_exit.kind == exit_kind.liquidation
plot(0)",
    ))
    .expect("script compiles");
    let outputs = run(
        &compiled,
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 11.0, 12.0]),
        VmLimits::default(),
    )
    .expect("script runs");

    for point in &outputs.exports[0].points {
        assert_eq!(point.value, palmscript::OutputValue::Bool(false));
    }
    for point in &outputs.exports[1].points {
        assert_eq!(point.value, palmscript::OutputValue::NA);
    }
    for point in &outputs.exports[2].points {
        assert_eq!(point.value, palmscript::OutputValue::NA);
    }
    for point in &outputs.exports[3].points {
        assert_eq!(point.value, palmscript::OutputValue::NA);
    }
}

#[test]
fn ledger_namespace_is_na_outside_backtests() {
    let compiled = palmscript::compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution exec = binance.spot(\"BTCUSDT\")
export quote_free = ledger(exec).quote_free
export base_total = ledger(exec).base_total
export mark_value = ledger(exec).mark_value_quote
plot(0)",
    )
    .expect("script compiles");
    let outputs = run(
        &compiled,
        &bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[10.0, 11.0, 12.0]),
        VmLimits::default(),
    )
    .expect("script runs");

    for export in &outputs.exports {
        for point in &export.points {
            assert_eq!(point.value, palmscript::OutputValue::NA);
        }
    }
}

#[test]
fn venue_selection_builtins_use_execution_alias_prices_in_runtime_mode() {
    let compiled = palmscript::compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution bn = binance.spot(\"BTCUSDT\")
execution gt = gate.spot(\"BTC_USDT\")
export cheapest_is_gt = cheapest(bn, gt) == gt
export spread = spread_bps(cheapest(bn, gt), richest(bn, gt))
plot(spot.close)",
    )
    .expect("script compiles");

    let source_bars = bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[100.0, 101.0]);
    let bn_bars = bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[101.0, 103.0]);
    let gt_bars = bars_with_spacing(JAN_1_2024_UTC_MS, MINUTE_MS, &[100.0, 102.0]);
    let outputs = run_with_sources(
        &compiled,
        SourceRuntimeConfig {
            base_interval: Interval::Min1,
            feeds: vec![
                SourceFeed {
                    source_id: 0,
                    interval: Interval::Min1,
                    bars: source_bars,
                },
                SourceFeed {
                    source_id: 1,
                    interval: Interval::Min1,
                    bars: bn_bars,
                },
                SourceFeed {
                    source_id: 2,
                    interval: Interval::Min1,
                    bars: gt_bars,
                },
            ],
        },
        VmLimits::default(),
    )
    .expect("script runs");

    for point in &outputs.exports[0].points {
        assert_eq!(point.value, palmscript::OutputValue::Bool(true));
    }

    let first_spread = match outputs.exports[1].points[0].value {
        palmscript::OutputValue::F64(value) => value,
        ref other => panic!("expected numeric spread, found {other:?}"),
    };
    let second_spread = match outputs.exports[1].points[1].value {
        palmscript::OutputValue::F64(value) => value,
        ref other => panic!("expected numeric spread, found {other:?}"),
    };
    assert!((first_spread - 100.0).abs() < 1e-12);
    assert!((second_spread - ((103.0 - 102.0) / 102.0) * 10_000.0).abs() < 1e-12);
}
