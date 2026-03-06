use palmscript::bytecode::{Constant, Instruction, LocalInfo, OpCode, Program};
use palmscript::compiler::CompiledProgram;
use palmscript::diagnostic::RuntimeError;
use palmscript::runtime::{
    run, run_multi_interval, Bar, IntervalFeed, MultiIntervalConfig, VmLimits,
};
use palmscript::types::Value;
use palmscript::{Interval, MarketBinding, MarketField, MarketSource, Type};

fn with_interval(source: &str) -> String {
    format!("interval 1m\n{source}")
}

fn with_intervals(base: &str, supplemental: &[&str], source: &str) -> String {
    let mut script = format!("interval {base}\n");
    for interval in supplemental {
        script.push_str("use ");
        script.push_str(interval);
        script.push('\n');
    }
    script.push_str(source);
    script
}

fn empty_locals() -> Vec<LocalInfo> {
    vec![
        LocalInfo::series(
            Some("open".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Base,
                field: MarketField::Open,
            }),
        ),
        LocalInfo::series(
            Some("high".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Base,
                field: MarketField::High,
            }),
        ),
        LocalInfo::series(
            Some("low".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Base,
                field: MarketField::Low,
            }),
        ),
        LocalInfo::series(
            Some("close".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Base,
                field: MarketField::Close,
            }),
        ),
        LocalInfo::series(
            Some("volume".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Base,
                field: MarketField::Volume,
            }),
        ),
        LocalInfo::series(
            Some("time".into()),
            Type::SeriesF64,
            false,
            1,
            Some(MarketBinding {
                source: MarketSource::Base,
                field: MarketField::Time,
            }),
        ),
    ]
}

fn bars() -> Vec<Bar> {
    vec![Bar {
        open: 1.0,
        high: 2.0,
        low: 0.5,
        close: 1.5,
        volume: 10.0,
        time: 1_700_000_000_000.0,
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
        outputs: vec![],
        base_interval: None,
        declared_intervals: vec![],
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
        outputs: vec![],
        base_interval: None,
        declared_intervals: vec![],
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
        outputs: vec![],
        base_interval: None,
        declared_intervals: vec![],
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
    let fixture = vec![
        Bar {
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume: 1.0,
            time: 1.0,
        };
        6
    ];
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
                time: 1_700_000_000_000.0 + index as f64 * 60_000.0,
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
        })
        .collect()
}

#[test]
fn user_function_inlining_matches_inline_expression() {
    let helper = palmscript::compile(&with_interval(
        "fn rising(series) = series > series[1]\nif rising(close) { plot(1) } else { plot(0) }",
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
            "fn crossover(a, b) = a > b and a[1] <= b[1]\nfn long_signal(fast, slow) = crossover(fast, slow) or fast > slow\nlet fast = ema(close, 3)\nlet slow = ema(close, 5)\nif long_signal(fast, slow) { plot(1) } else { plot(0) }",
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
fn qualified_series_requires_multi_interval_config() {
    let compiled = palmscript::compile(&with_intervals("1m", &["1h"], "plot(1h.close)"))
        .expect("script should compile");
    let err = run(&compiled, &fixture_bars(), VmLimits::default()).expect_err("config required");
    assert!(matches!(err, RuntimeError::MissingIntervalConfig));
}

#[test]
fn lower_interval_references_are_rejected() {
    let compiled = palmscript::compile(&with_intervals("1h", &["1m"], "plot(1m.close)"))
        .expect("script should compile");
    let err = run_multi_interval(
        &compiled,
        &bars_with_spacing(JAN_1_2024_UTC_MS, HOUR_MS, &[1.0, 2.0]),
        MultiIntervalConfig {
            base_interval: Interval::Hour1,
            supplemental: Vec::new(),
        },
        VmLimits::default(),
    )
    .expect_err("lower interval should reject");
    assert!(matches!(
        err,
        RuntimeError::LowerIntervalReference {
            base: Interval::Hour1,
            referenced: Interval::Min1
        }
    ));
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
fn triggers_emit_samples_and_events() {
    let compiled = palmscript::compile(&with_interval(
        "trigger long_entry = close > close[1]\nplot(0)",
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
