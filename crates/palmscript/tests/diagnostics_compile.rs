use std::collections::BTreeMap;

use palmscript::{compile, compile_with_input_overrides, DiagnosticKind};

#[path = "support/mod.rs"]
mod support;

use support::{assert_compile_diagnostics, compile_diagnostics, ExpectedDiagnostic};

fn with_interval(source: &str) -> String {
    support::with_single_source_interval(source)
}

fn with_sources(source: &str) -> String {
    format!("interval 1m\nsource a = binance.spot(\"BTCUSDT\")\n{source}")
}

fn expected(kind: DiagnosticKind, message: &'static str) -> ExpectedDiagnostic {
    ExpectedDiagnostic { kind, message }
}

// Diagnostic inventory for public compiler/pre-execution coverage.
//
// Public lex diagnostics covered here:
// - unexpected character
// - unknown interval literal
// - unsupported string escape
// - unterminated string literal
//
// Public parse diagnostics covered here:
// - unsupported source template
// - malformed source declaration pieces
// - malformed source-qualified series
// - malformed `use <alias> <interval>`
// - malformed call expression
//
// Public type/semantic diagnostics covered here:
// - source alias errors
// - source interval declaration/reference errors
// - duplicate bindings
// - builtin/type/indexing/operator errors
// - function-body source-aware restrictions
// - ordered multi-diagnostic aggregation
//
// Internal-only diagnostics intentionally not part of the public compile contract:
// - function name `{...}` collides with a predefined binding
//   Reason: all current predefined bindings are also builtin names, so the builtin
//   collision path fires first.
// - string literals are not executable expressions
// - unknown identifier `{...}` during emission
// - unknown function `{...}` during emission
// - missing specialization for function `{...}`
// - unknown builtin `{...}`
// - builtin `{...}` is not callable in v0.1
// - unknown function specialization target
//   Reason: these are bytecode-emission consistency guards after semantic analysis.

#[test]
fn compile_diagnostic_catalog_matches_contract() {
    let cases: [(&str, String, Vec<ExpectedDiagnostic>); 25] = [
        (
            "lex_unexpected_character",
            with_interval("plot(@)"),
            vec![expected(DiagnosticKind::Lex, "unexpected character `@`")],
        ),
        (
            "lex_unknown_interval_literal",
            "interval 1q\nplot(close)".to_string(),
            vec![expected(DiagnosticKind::Lex, "unknown interval literal `1q`")],
        ),
        (
            "lex_unsupported_string_escape",
            "interval 1m\nsource a = binance.spot(\"BTC\\x\")\nplot(1)".to_string(),
            vec![expected(
                DiagnosticKind::Lex,
                "unsupported string escape `\\x`",
            )],
        ),
        (
            "lex_unterminated_string_literal",
            "interval 1m\nsource a = binance.spot(\"BTC\nplot(1)".to_string(),
            vec![expected(DiagnosticKind::Lex, "unterminated string literal")],
        ),
        (
            "parse_unsupported_source_template",
            "interval 1m\nsource a = foo.bar(\"BTC\")\nplot(1)".to_string(),
            vec![expected(DiagnosticKind::Parse, "unsupported source template")],
        ),
        (
            "parse_missing_source_template_dot",
            "interval 1m\nsource a = binance(\"BTC\")\nplot(1)".to_string(),
            vec![expected(DiagnosticKind::Parse, "expected `.` after exchange name")],
        ),
        (
            "parse_missing_source_symbol",
            "interval 1m\nsource a = binance.spot()\nplot(1)".to_string(),
            vec![expected(
                DiagnosticKind::Parse,
                "expected string literal source symbol",
            )],
        ),
        (
            "parse_malformed_source_series",
            with_sources("plot(a.)"),
            vec![expected(
                DiagnosticKind::Parse,
                "expected market field or interval after `.`",
            )],
        ),
        (
            "parse_malformed_source_use",
            "interval 1m\nsource a = binance.spot(\"BTCUSDT\")\nuse a\nplot(a.close)"
                .to_string(),
            vec![expected(
                DiagnosticKind::Parse,
                "expected interval literal after source alias",
            )],
        ),
        (
            "parse_malformed_call_expression",
            with_interval("plot(close,)"),
            vec![expected(DiagnosticKind::Parse, "expected expression")],
        ),
        (
            "type_duplicate_source_alias",
            "interval 1m\nsource a = binance.spot(\"BTCUSDT\")\nsource a = binance.usdm(\"BTCUSDT\")\nplot(a.close)"
                .to_string(),
            vec![expected(
                DiagnosticKind::Type,
                "duplicate source alias `a`",
            )],
        ),
        (
            "type_unknown_source_alias_in_use",
            "interval 1m\nsource a = binance.spot(\"BTCUSDT\")\nuse b 1h\nplot(a.close)"
                .to_string(),
            vec![expected(
                DiagnosticKind::Type,
                "unknown source alias `b`",
            )],
        ),
        (
            "type_unknown_source_alias_in_series",
            with_sources("plot(b.close)"),
            vec![expected(
                DiagnosticKind::Type,
                "unknown source alias `b`",
            )],
        ),
        (
            "type_missing_source_interval_use",
            "interval 1m\nsource a = bybit.usdt_perps(\"BTCUSDT\")\nplot(a.1h.close)"
                .to_string(),
            vec![expected(
                DiagnosticKind::Type,
                "source interval `1h` for `a` must be declared with `use a 1h`",
            )],
        ),
        (
            "type_duplicate_let_binding",
            with_interval("let x = close\nlet x = close[1]\nplot(x)"),
            vec![expected(
                DiagnosticKind::Type,
                "duplicate binding `x` in the same scope",
            )],
        ),
        (
            "type_duplicate_export_binding",
            with_interval("export trend = close > open\nexport trend = close < open\nplot(1)"),
            vec![expected(
                DiagnosticKind::Type,
                "duplicate binding `trend` in the same scope",
            )],
        ),
        (
            "type_if_condition_must_be_boolean_like",
            with_interval("if 1 { plot(1) } else { plot(0) }"),
            vec![expected(
                DiagnosticKind::Type,
                "if condition must be bool, series<bool>, or na",
            )],
        ),
        (
            "type_string_literals_only_in_source_declarations",
            with_interval("plot(\"x\")"),
            vec![expected(
                DiagnosticKind::Type,
                "string literals are only allowed in source declarations",
            )],
        ),
        (
            "type_unary_neg_requires_numeric_input",
            with_interval("plot(-true)"),
            vec![expected(
                DiagnosticKind::Type,
                "unary `-` requires numeric input",
            )],
        ),
        (
            "type_unary_not_requires_bool_input",
            with_interval("if !1 { plot(1) } else { plot(0) }"),
            vec![expected(DiagnosticKind::Type, "unary `!` requires bool input")],
        ),
        (
            "type_arithmetic_requires_numeric_operands",
            with_interval("plot(true + 1)"),
            vec![expected(
                DiagnosticKind::Type,
                "arithmetic operators require numeric operands",
            )],
        ),
        (
            "type_comparison_requires_numeric_operands",
            with_interval("if true < false { plot(1) } else { plot(0) }"),
            vec![expected(
                DiagnosticKind::Type,
                "comparison operators require numeric operands",
            )],
        ),
        (
            "type_market_data_builtins_are_not_callable",
            with_interval("plot(close())"),
            vec![expected(DiagnosticKind::Parse, "only identifiers can be called in v0.1")],
        ),
        (
            "type_plot_wrong_arity",
            with_interval("plot()"),
            vec![expected(
                DiagnosticKind::Type,
                "plot expects exactly one argument",
            )],
        ),
        (
            "type_plot_wrong_argument_type",
            with_interval("plot(true)"),
            vec![expected(
                DiagnosticKind::Type,
                "plot expects a numeric or series numeric value",
            )],
        ),
    ];

    for (name, source, expected_diags) in cases {
        assert_compile_diagnostics(name, &source, &expected_diags);
    }
}

#[test]
fn compile_with_input_overrides_rejects_unknown_inputs() {
    let source = with_interval("input fast_len = 10\nplot(fast_len)");
    let mut overrides = BTreeMap::new();
    overrides.insert("slow_len".to_string(), 21.0);
    let err = compile_with_input_overrides(&source, &overrides)
        .expect_err("unknown input override should fail");
    assert_eq!(err.diagnostics.len(), 1);
    assert_eq!(err.diagnostics[0].kind, DiagnosticKind::Compile);
    assert!(err.diagnostics[0]
        .message
        .contains("unknown input override `slow_len`"));
}

#[test]
fn auxiliary_binance_usdm_fields_reject_non_usdm_templates() {
    let err =
        compile("interval 1h\nsource spot = binance.spot(\"BTCUSDT\")\nplot(spot.funding_rate)")
            .expect_err("non-usdm auxiliary field should fail");
    assert!(err.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == DiagnosticKind::Type
            && diagnostic
                .message
                .contains("source field `funding_rate` is only available on `binance.usdm` sources")
    }));
}

#[test]
fn input_optimization_metadata_rejects_non_numeric_inputs() {
    let diagnostics = compile_diagnostics(&with_interval(
        "input enabled = true optimize(int, 0, 1)\nplot(close)",
    ));
    assert!(diagnostics.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag.1.contains(
                "input optimization metadata is only supported on numeric `input` bindings",
            )
    }));
}

#[test]
fn input_optimization_metadata_rejects_out_of_range_defaults() {
    let diagnostics = compile_diagnostics(&with_interval(
        "input fast = 21 optimize(int, 8, 20, 1)\nplot(close)",
    ));
    assert!(diagnostics.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag
                .1
                .contains("input `fast` default value 21 must lie inside optimize int range 8..=20")
    }));
}

#[test]
fn declarative_risk_controls_require_compile_time_numeric_scalars() {
    let diagnostics = compile_diagnostics(&with_interval("cooldown long = close\nplot(close)"));
    assert!(diagnostics.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag
                .1
                .contains("`cooldown` requires a compile-time numeric scalar expression")
    }));
}

#[test]
fn declarative_risk_controls_reject_negative_bar_counts() {
    let diagnostics =
        compile_diagnostics(&with_interval("max_bars_in_trade long = -1\nplot(close)"));
    assert!(diagnostics.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag
                .1
                .contains("`max_bars_in_trade` requires a non-negative whole number of bars")
    }));
}

#[test]
fn declarative_risk_controls_reject_duplicate_side_declarations() {
    let diagnostics = compile_diagnostics(&with_interval(
        "cooldown long = 2\ncooldown long = 3\nplot(close)",
    ));
    assert!(diagnostics.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag
                .1
                .contains("duplicate `cooldown` declaration for `long`")
    }));
}

#[test]
fn portfolio_controls_require_compile_time_numeric_scalars() {
    let diagnostics = compile_diagnostics(&with_interval("max_positions = close\nplot(close)"));
    assert!(diagnostics.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag
                .1
                .contains("`max_positions` requires a compile-time numeric scalar expression")
    }));
}

#[test]
fn portfolio_controls_reject_negative_or_fractional_counts() {
    let diagnostics = compile_diagnostics(&with_interval("max_long_positions = 1.5\nplot(close)"));
    assert!(diagnostics.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag
                .1
                .contains("`max_long_positions` requires a non-negative whole number")
    }));
}

#[test]
fn portfolio_group_rejects_unknown_aliases() {
    let diagnostics = compile_diagnostics(
        "interval 1m\nsource left = binance.spot(\"BTCUSDT\")\nportfolio_group \"majors\" = [left, missing]\nplot(left.close)",
    );
    assert!(diagnostics.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag
                .1
                .contains("portfolio group `majors` references unknown source alias `missing`")
    }));
}

#[test]
fn execution_aliases_must_be_unique_and_orders_must_reference_declared_execution_aliases() {
    let duplicate_execution = compile_diagnostics(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
entry long = left.close > left.close[1]
exit long = false
plot(left.close)",
    );
    assert!(duplicate_execution.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type && diag.1.contains("duplicate execution alias `exec`")
    }));

    let unknown_execution = compile_diagnostics(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
entry long = left.close > left.close[1]
exit long = false
order entry long = market(venue = missing_exec)
plot(left.close)",
    );
    assert!(unknown_execution.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type && diag.1.contains("unknown execution alias `missing_exec`")
    }));
}

#[test]
fn execution_scripts_require_explicit_orders_at_compile_time() {
    let missing_signal_order = compile_diagnostics(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
entry long = left.close > left.close[1]
exit long = false
plot(left.close)",
    );
    assert!(missing_signal_order.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag.1.contains(
                "signal declaration for `long_entry` requires a matching `order ...` declaration",
            )
    }));
    assert!(missing_signal_order.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag.1.contains(
                "signal declaration for `long_exit` requires a matching `order ...` declaration",
            )
    }));

    let legacy_trigger = compile_diagnostics(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
trigger long_entry = left.close > left.close[1]
plot(left.close)",
    );
    assert!(legacy_trigger.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag
                .1
                .contains("legacy trigger `long_entry` is no longer supported for execution")
    }));
}

#[test]
fn order_templates_must_be_declared_and_acyclic() {
    let unknown_template = compile_diagnostics(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
entry long = left.close > left.close[1]
exit long = false
order entry long = missing_template
order exit long = market(venue = exec)
plot(left.close)",
    );
    assert!(unknown_template.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag.1.contains("unknown order template `missing_template`")
    }));

    let duplicate_template = compile_diagnostics(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
order_template entry_order = market()
order_template entry_order = market()
plot(left.close)",
    );
    assert!(duplicate_template.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type && diag.1.contains("duplicate order template `entry_order`")
    }));

    let cyclic_template = compile_diagnostics(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
order_template first = second
order_template second = first
entry long = left.close > left.close[1]
exit long = false
order entry long = first
order exit long = market(venue = exec)
plot(left.close)",
    );
    assert!(cyclic_template.iter().any(|diag| {
        diag.0 == DiagnosticKind::Type
            && diag
                .1
                .contains("cyclic order template reference `first -> second -> first`")
    }));
}

#[test]
fn named_order_arguments_reject_unexpected_fields() {
    let diagnostics = compile_diagnostics(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
entry long = left.close > left.close[1]
exit long = false
order entry long = market(price = left.close[1], venue = exec)
plot(left.close)",
    );
    assert!(diagnostics.iter().any(|diag| {
        diag.0 == DiagnosticKind::Parse
            && diag
                .1
                .contains("unexpected `price` order argument for `market`")
    }));
}

#[test]
fn compile_accepts_new_exchange_backed_source_templates() {
    let cases = [
        "interval 1m\nsource a = bybit.spot(\"BTCUSDT\")\nplot(a.close)",
        "interval 1m\nsource a = bybit.usdt_perps(\"BTCUSDT\")\nplot(a.close)",
        "interval 1m\nsource a = gate.spot(\"BTC_USDT\")\nplot(a.close)",
        "interval 1m\nsource a = gate.usdt_perps(\"BTC_USDT\")\nplot(a.close)",
    ];

    for source in cases {
        compile(source).expect("new source template should compile");
    }
}

#[test]
fn rejects_position_namespace_outside_attached_exits() {
    let source = with_interval(
        "entry long = position.entry_price > 0\nexecution src = binance.spot(\"BTCUSDT\")\norder entry long = market(venue = src)\nplot(src.close)",
    );
    assert_compile_diagnostics(
        "position_namespace_outside_attached_exits",
        &source,
        &[expected(
            DiagnosticKind::Type,
            "`position.*` is only available inside `protect` and `target` declarations",
        )],
    );
}

#[test]
fn compile_source_specific_and_builtin_catalog_matches_contract() {
    let cases: [(&str, String, Vec<ExpectedDiagnostic>); 39] = [
        (
            "type_lower_source_interval_reports_both_use_and_reference",
            "interval 1h\nsource a = binance.spot(\"BTCUSDT\")\nuse a 1m\nplot(a.1m.close)"
                .to_string(),
            vec![
                expected(
                    DiagnosticKind::Type,
                    "lower interval reference `1m` is not allowed with base interval `1h`",
                ),
                expected(
                    DiagnosticKind::Type,
                    "lower interval reference `1m` is not allowed with base interval `1h`",
                ),
            ],
        ),
        (
            "type_indicator_wrong_arity",
            with_interval("plot(sma(close))"),
            vec![expected(
                DiagnosticKind::Type,
                "sma expects exactly two arguments",
            )],
        ),
        (
            "type_indicator_wrong_first_argument",
            with_interval("plot(sma(true, 5))"),
            vec![expected(
                DiagnosticKind::Type,
                "sma requires series<float> as the first argument",
            )],
        ),
        (
            "type_indicator_zero_window",
            with_interval("plot(sma(close, 0))"),
            vec![expected(
                DiagnosticKind::Type,
                "sma length must be greater than zero",
            )],
        ),
        (
            "type_non_series_indexing",
            with_interval("plot(1[0])"),
            vec![expected(
                DiagnosticKind::Type,
                "only series values can be indexed",
            )],
        ),
        (
            "type_source_aware_bare_series",
            with_sources("plot(close)"),
            vec![expected(
                DiagnosticKind::Type,
                "scripts require source-qualified market series; found `close`",
            )],
        ),
        (
            "type_source_aware_function_body_capture_variant",
            "interval 1m\nsource a = binance.spot(\"BTCUSDT\")\nlet basis = a.close\nfn helper() = basis\nplot(1)"
                .to_string(),
            vec![expected(
                DiagnosticKind::Type,
                "function bodies may only reference parameters or declared source series; found `basis`",
            )],
        ),
        (
            "type_function_returning_void_is_rejected_in_order",
            with_interval("fn bad(x) = plot(x)\nplot(bad(close))"),
            vec![
                expected(DiagnosticKind::Type, "function bodies may not call `plot`"),
                expected(DiagnosticKind::Type, "function bodies may not call `plot`"),
                expected(
                    DiagnosticKind::Type,
                    "function `bad` must not return void",
                ),
                expected(
                    DiagnosticKind::Type,
                    "plot expects a numeric or series numeric value",
                ),
            ],
        ),
        (
            "type_relation_helper_wrong_arity",
            with_interval("plot(above(close))"),
            vec![
                expected(DiagnosticKind::Type, "above expects exactly two arguments"),
                expected(
                    DiagnosticKind::Type,
                    "plot expects a numeric or series numeric value",
                ),
            ],
        ),
        (
            "type_relation_helper_wrong_input_kind",
            with_interval("plot(above(true, close))"),
            vec![
                expected(
                    DiagnosticKind::Type,
                    "above requires numeric or series numeric arguments",
                ),
                expected(
                    DiagnosticKind::Type,
                    "plot expects a numeric or series numeric value",
                ),
            ],
        ),
        (
            "type_cross_requires_series_operand",
            with_interval("if cross(1, 2) { plot(1) } else { plot(0) }"),
            vec![expected(
                DiagnosticKind::Type,
                "cross requires at least one series<float> argument",
            )],
        ),
        (
            "type_change_requires_series_float",
            with_interval("plot(change(1, 2))"),
            vec![expected(
                DiagnosticKind::Type,
                "change requires series<float> as the first argument",
            )],
        ),
        (
            "type_roc_zero_window",
            with_interval("plot(roc(close, 0))"),
            vec![expected(
                DiagnosticKind::Type,
                "roc length must be greater than zero",
            )],
        ),
        (
            "type_mom_requires_series_float",
            with_interval("plot(mom(1))"),
            vec![expected(
                DiagnosticKind::Type,
                "mom requires series<float> as the first argument",
            )],
        ),
        (
            "type_rocp_zero_window",
            with_interval("plot(rocp(close, 0))"),
            vec![expected(
                DiagnosticKind::Type,
                "rocp length must be greater than zero",
            )],
        ),
        (
            "type_highest_non_literal_window",
            with_interval("let n = 2\nplot(highest(close, n))"),
            vec![expected(
                DiagnosticKind::Type,
                "highest length must be a non-negative integer literal",
            )],
        ),
        (
            "type_barssince_requires_series_bool",
            with_interval("plot(barssince(close))"),
            vec![expected(
                DiagnosticKind::Type,
                "barssince requires series<bool> as the first argument",
            )],
        ),
        (
            "type_activated_requires_series_bool",
            with_interval("plot(activated(close))"),
            vec![
                expected(
                    DiagnosticKind::Type,
                    "activated requires series<bool> as the first argument",
                ),
                expected(
                    DiagnosticKind::Type,
                    "plot expects a numeric or series numeric value",
                ),
            ],
        ),
        (
            "type_deactivated_requires_series_bool",
            with_interval("plot(deactivated(close))"),
            vec![
                expected(
                    DiagnosticKind::Type,
                    "deactivated requires series<bool> as the first argument",
                ),
                expected(
                    DiagnosticKind::Type,
                    "plot expects a numeric or series numeric value",
                ),
            ],
        ),
        (
            "type_count_since_requires_series_bool",
            with_interval("plot(count_since(close > open, close))"),
            vec![expected(
                DiagnosticKind::Type,
                "count_since requires series<bool> as the second argument",
            )],
        ),
        (
            "type_valuewhen_requires_series_source_and_literal_occurrence",
            with_interval("plot(valuewhen(close > open, 1, -1))"),
            vec![
                expected(
                    DiagnosticKind::Type,
                    "valuewhen requires series<float> or series<bool> as the second argument",
                ),
                expected(
                    DiagnosticKind::Type,
                    "valuewhen occurrence must be a non-negative integer literal",
                ),
            ],
        ),
        (
            "type_ma_requires_typed_enum_argument",
            with_interval("plot(ma(close, 3, 1))"),
            vec![expected(
                DiagnosticKind::Type,
                "ma requires ma_type as the third argument",
            )],
        ),
        (
            "type_apo_requires_minimum_fast_window",
            with_interval("plot(apo(close, 1))"),
            vec![expected(
                DiagnosticKind::Type,
                "apo length must be greater than or equal to 2",
            )],
        ),
        (
            "type_ppo_requires_typed_enum_argument",
            with_interval("plot(ppo(close, 3, 5, 1))"),
            vec![expected(
                DiagnosticKind::Type,
                "ppo requires ma_type as the fourth argument",
            )],
        ),
        (
            "type_aroon_requires_minimum_window",
            with_interval("let (down, up) = aroon(high, low, 1)\nplot(up)"),
            vec![expected(
                DiagnosticKind::Type,
                "aroon length must be greater than or equal to 2",
            )],
        ),
        (
            "type_aroonosc_requires_series_low",
            with_interval("plot(aroonosc(high, 1))"),
            vec![expected(
                DiagnosticKind::Type,
                "aroonosc requires series<float> as the second argument",
            )],
        ),
        (
            "type_bop_requires_series_close",
            with_interval("plot(bop(open, high, low, 1))"),
            vec![expected(
                DiagnosticKind::Type,
                "bop requires series<float> as the fourth argument",
            )],
        ),
        (
            "type_cci_requires_minimum_window",
            with_interval("plot(cci(high, low, close, 1))"),
            vec![expected(
                DiagnosticKind::Type,
                "cci length must be greater than or equal to 2",
            )],
        ),
        (
            "type_cmo_requires_minimum_window",
            with_interval("plot(cmo(close, 1))"),
            vec![expected(
                DiagnosticKind::Type,
                "cmo length must be greater than or equal to 2",
            )],
        ),
        (
            "type_willr_requires_series_close",
            with_interval("plot(willr(high, low, 1))"),
            vec![expected(
                DiagnosticKind::Type,
                "willr requires series<float> as the third argument",
            )],
        ),
        (
            "type_unary_math_requires_numeric_input",
            with_interval("plot(sin(true))"),
            vec![expected(
                DiagnosticKind::Type,
                "sin requires numeric or series numeric arguments",
            )],
        ),
        (
            "type_sum_requires_minimum_window",
            with_interval("plot(sum(close, 1))"),
            vec![expected(
                DiagnosticKind::Type,
                "sum length must be greater than or equal to 2",
            )],
        ),
        (
            "type_minmax_requires_minimum_window",
            with_interval("let (lo, hi) = minmax(close, 1)\nplot(hi)"),
            vec![expected(
                DiagnosticKind::Type,
                "minmax length must be greater than or equal to 2",
            )],
        ),
        (
            "type_stddev_requires_scalar_deviation_factor",
            with_interval("plot(stddev(close, 5, high))"),
            vec![expected(
                DiagnosticKind::Type,
                "stddev deviations must be a numeric scalar value",
            )],
        ),
        (
            "type_beta_requires_series_second_argument",
            with_interval("plot(beta(close, 1))"),
            vec![expected(
                DiagnosticKind::Type,
                "beta requires series<float> as the second argument",
            )],
        ),
        (
            "type_correl_requires_minimum_window",
            with_interval("plot(correl(close, open, 0))"),
            vec![expected(
                DiagnosticKind::Type,
                "correl length must be greater than or equal to 1",
            )],
        ),
        (
            "type_obv_requires_series_volume",
            with_interval("plot(obv(close, 1))"),
            vec![expected(
                DiagnosticKind::Type,
                "obv requires series<float> as the second argument",
            )],
        ),
        (
            "type_tuple_builtin_requires_destructuring",
            with_interval("let x = macd(close, 3, 5, 2)\nplot(1)"),
            vec![expected(
                DiagnosticKind::Type,
                "tuple-valued expressions must be destructured with `let (...) = ...`",
            )],
        ),
        (
            "type_talib_reserved_name_collides_with_function",
            with_interval("fn ht_sine(x) = x\nplot(1)"),
            vec![expected(
                DiagnosticKind::Type,
                "function name `ht_sine` collides with a builtin",
            )],
        ),
    ];

    for (name, source, expected_diags) in cases {
        assert_compile_diagnostics(name, &source, &expected_diags);
    }
}

#[test]
fn compile_multi_diagnostic_order_is_stable() {
    assert_compile_diagnostics(
        "ordered_semantic_diagnostics",
        &with_interval("let x = close\nlet x = close[1]\nplot(true + 1)"),
        &[
            expected(
                DiagnosticKind::Type,
                "duplicate binding `x` in the same scope",
            ),
            expected(
                DiagnosticKind::Type,
                "arithmetic operators require numeric operands",
            ),
        ],
    );
}

#[test]
fn parse_diagnostics_aggregate_cleanly_without_panics() {
    assert_compile_diagnostics(
        "ordered_parse_diagnostics",
        "interval\nuse\nplot(",
        &[
            expected(
                DiagnosticKind::Parse,
                "expected interval literal after `interval`",
            ),
            expected(DiagnosticKind::Parse, "expected expression"),
        ],
    );
}

#[test]
fn rejects_risk_pct_for_target_size_declarations() {
    let source = with_interval(
        "entry long = close > close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry long = market(venue = src)
target long = take_profit_market(trigger_price = position.entry_price + 2, trigger_ref = trigger_ref.last, venue = src)
size target long = risk_pct(0.01, close)
plot(close)",
    );
    assert_compile_diagnostics(
        "risk_pct_target_size",
        &source,
        &[expected(
            DiagnosticKind::Type,
            "`risk_pct(...)` is only supported on staged entry size declarations in v1",
        )],
    );
}

#[test]
fn rejects_invalid_risk_pct_arity() {
    let source = with_interval(
        "entry long = close > close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry long = market(venue = src)
size entry long = risk_pct(0.01)
plot(close)",
    );
    assert_compile_diagnostics(
        "risk_pct_invalid_arity",
        &source,
        &[expected(
            DiagnosticKind::Type,
            "`risk_pct(...)` expects exactly two arguments: risk_pct(pct, stop_price)",
        )],
    );
}

#[test]
fn rejects_unknown_module_size_declarations() {
    let source = with_interval(
        "module breakout = entry long
entry long = close > close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry long = market(venue = src)
size module missing = 0.5
plot(close)",
    );
    assert_compile_diagnostics(
        "unknown_module_size",
        &source,
        &[expected(
            DiagnosticKind::Type,
            "unknown module `missing` in size declaration",
        )],
    );
}

#[test]
fn rejects_duplicate_module_names() {
    let source = with_interval(
        "module breakout = entry long
module breakout = entry2 long
entry long = close > close[1]
entry2 long = close > close[2]
execution src = binance.spot(\"BTCUSDT\")
order entry long = market(venue = src)
order entry2 long = market(venue = src)
plot(close)",
    );
    assert_compile_diagnostics(
        "duplicate_module_name",
        &source,
        &[expected(
            DiagnosticKind::Type,
            "duplicate module declaration name `breakout`",
        )],
    );
}

#[test]
fn compile_api_keeps_internal_compile_diagnostics_internal() {
    let cases = [
        "plot(\"x\")",
        "interval 1m\nplot(close())",
        "interval 1m\nfn bad(x) = plot(x)\nplot(bad(close))",
        "interval 1m\nif true { plot(1) }",
    ];

    for source in cases {
        let diagnostics = compile_diagnostics(source);
        assert!(
            diagnostics
                .iter()
                .all(|(kind, _)| *kind != DiagnosticKind::Compile),
            "{source}"
        );
    }
}

#[test]
fn rejects_duplicate_order_declarations_for_same_role() {
    assert_compile_diagnostics(
        "duplicate_order_role",
        &with_interval(
            "entry long = src.close > src.close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry long = market(venue = src)
order entry long = limit(price = src.close[1], tif = tif.gtc, post_only = false, venue = src)
plot(src.close)",
        ),
        &[expected(
            DiagnosticKind::Type,
            "duplicate order declaration for `long_entry`",
        )],
    );
}

#[test]
fn rejects_duplicate_staged_order_declarations_for_same_role() {
    assert_compile_diagnostics(
        "duplicate_staged_order_role",
        &with_interval(
            "entry1 long = src.close > src.close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry1 long = market(venue = src)
target2 long = take_profit_market(trigger_price = src.close + 1, trigger_ref = trigger_ref.last, venue = src)
target2 long = take_profit_market(trigger_price = src.close + 2, trigger_ref = trigger_ref.last, venue = src)
plot(src.close)",
        ),
        &[expected(
            DiagnosticKind::Type,
            "duplicate order declaration for `target_long2`",
        )],
    );
}

#[test]
fn rejects_invalid_order_constructor_argument_types() {
    assert_compile_diagnostics(
        "invalid_limit_tif",
        &with_interval(
            "entry long = src.close > src.close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry long = limit(price = src.close[1], tif = ma_type.ema, post_only = false, venue = src)
plot(src.close)",
        ),
        &[expected(
            DiagnosticKind::Type,
            "limit requires `tif.<variant>` as the second argument",
        )],
    );
}

#[test]
fn rejects_unknown_order_enum_variants() {
    assert_compile_diagnostics(
        "unknown_order_enum",
        &with_interval(
            "entry long = src.close > src.close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry long = stop_market(trigger_price = src.close + 1, trigger_ref = trigger_ref.foo, venue = src)
plot(src.close)",
        ),
        &[
            expected(
                DiagnosticKind::Type,
                "unknown enum variant `trigger_ref.foo`",
            ),
            expected(
                DiagnosticKind::Type,
                "stop_market requires `trigger_ref.<variant>` as the second argument",
            ),
        ],
    );
}

#[test]
fn anchored_vwap_requires_boolean_anchor_series() {
    assert_compile_diagnostics(
        "type_anchored_vwap_requires_series_bool_anchor",
        &with_interval("plot(anchored_vwap(close, close, volume))"),
        &[expected(
            DiagnosticKind::Type,
            "anchored_vwap requires series<bool> as the first argument",
        )],
    );
}

#[test]
fn percentile_requires_numeric_scalar_percentage() {
    assert_compile_diagnostics(
        "type_percentile_requires_numeric_percentage",
        &with_interval("plot(percentile(close, 20, close))"),
        &[expected(
            DiagnosticKind::Type,
            "percentile percentage must be a numeric scalar value",
        )],
    );
}

#[test]
fn arb_pair_fields_require_execution_aliases_and_numeric_size() {
    assert_compile_diagnostics(
        "arb_pair_requires_alias_and_numeric_size",
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution bn = binance.spot(\"BTCUSDT\")
execution gt = gate.spot(\"BTC_USDT\")
arb_entry = true
arb_order entry = market_pair(
    buy_venue = spot.close,
    sell_venue = gt,
    size = true
)
plot(spot.close)",
        &[expected(
            DiagnosticKind::Type,
            "arbitrage pair field `buy_venue` requires an execution_alias expression",
        )],
    );
}

#[test]
fn transfer_fields_require_execution_aliases_and_numeric_amounts() {
    assert_compile_diagnostics(
        "transfer_requires_alias_and_numeric_amount",
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution bn = binance.spot(\"BTCUSDT\")
execution gt = gate.spot(\"BTC_USDT\")
transfer quote = quote_transfer(
    from = spot.close,
    to = gt,
    amount = true
)
plot(spot.close)",
        &[expected(
            DiagnosticKind::Type,
            "transfer field `from` requires an execution_alias expression",
        )],
    );
}
