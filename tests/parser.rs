use palmscript::compile;

fn compile_err(source: &str) -> String {
    let err = compile(source).expect_err("expected compile error");
    err.diagnostics
        .into_iter()
        .map(|diag| diag.message)
        .collect::<Vec<_>>()
        .join(" | ")
}

fn with_interval(source: &str) -> String {
    format!("interval 1m\n{source}")
}

fn with_intervals(source: &str, supplemental: &[&str]) -> String {
    let mut script = String::from("interval 1m\n");
    for interval in supplemental {
        script.push_str("use ");
        script.push_str(interval);
        script.push('\n');
    }
    script.push_str(source);
    script
}

#[test]
fn rejects_missing_right_bracket() {
    let message = compile_err(&with_interval("plot(close[1)"));
    assert!(message.contains("expected `]` after index"));
}

#[test]
fn rejects_negative_index() {
    let message = compile_err(&with_interval("plot(close[-1])"));
    assert!(message.contains("series indexing requires a non-negative integer literal"));
}

#[test]
fn rejects_non_literal_index() {
    let message = compile_err(&with_interval("let n = 1\nplot(close[n])"));
    assert!(message.contains("series indexing requires a non-negative integer literal"));
}

#[test]
fn rejects_non_literal_window_length() {
    let message = compile_err(&with_interval("let n = 14\nplot(sma(close, n))"));
    assert!(message.contains("sma length must be a non-negative integer literal"));
}

#[test]
fn rejects_if_without_else() {
    let message = compile_err(&with_interval("if true { plot(1) }"));
    assert!(message.contains("expected `else` after `if` block"));
}

#[test]
fn rejects_numeric_logical_operands() {
    let message = compile_err(&with_interval("if 1 and 2 { plot(1) } else { plot(0) }"));
    assert!(message.contains("logical operators require bool, series<bool>, or na operands"));
}

#[test]
fn rejects_series_float_logical_operands() {
    let message = compile_err(&with_interval(
        "if close and true { plot(1) } else { plot(0) }",
    ));
    assert!(message.contains("logical operators require bool, series<bool>, or na operands"));
}

#[test]
fn rejects_indicator_logical_operands() {
    let message = compile_err(&with_interval(
        "if sma(close, 5) or false { plot(1) } else { plot(0) }",
    ));
    assert!(message.contains("logical operators require bool, series<bool>, or na operands"));
}

#[test]
fn allows_shadowing_in_inner_scope() {
    compile(&with_interval(
        "let x = close\nif close > close[1] { let x = close[1]\nplot(x) } else { plot(x) }",
    ))
    .expect("shadowing should compile");
}

#[test]
fn parses_na_literal() {
    compile(&with_interval("plot(na)")).expect("na literal should compile");
}

#[test]
fn supports_newline_and_semicolon_separators() {
    compile(&with_interval("let x = close;\nplot(x)")).expect("mixed separators should compile");
}

#[test]
fn parses_logical_operators_with_expected_precedence() {
    compile(&with_interval(
        "if true or false and false { plot(1) } else { plot(0) }",
    ))
    .expect("logical precedence should parse");
}

#[test]
fn parses_else_if_chains() {
    compile(&with_interval(
        "if false { plot(0) } else if true { plot(1) } else { plot(2) }",
    ))
    .expect("else if chains should compile");
}

#[test]
fn supports_newlines_around_else_if() {
    compile(&with_interval(
        "if false { plot(0) } else\nif true { plot(1) } else { plot(2) }",
    ))
    .expect("newline-separated else if should compile");
}

#[test]
fn reserves_logical_keywords() {
    let message = compile_err(&with_interval("let and = true\nplot(and)"));
    assert!(message.contains("expected identifier after `let`"));
}

#[test]
fn parses_top_level_function_declarations() {
    compile(&with_interval(
        "fn crossover(a, b) = a > b and a[1] <= b[1]\nif crossover(close, ema(close, 3)) { plot(1) } else { plot(0) }",
    ))
    .expect("function declarations should compile");
}

#[test]
fn parses_zero_argument_functions() {
    compile(&with_interval(
        "fn bullish_bar() = close > open\nif bullish_bar() { plot(1) } else { plot(0) }",
    ))
    .expect("zero-argument functions should compile");
}

#[test]
fn rejects_function_declarations_inside_blocks() {
    let message = compile_err(&with_interval(
        "if true { fn helper() = close > open } else { plot(0) }",
    ));
    assert!(message.contains("function declarations are only allowed at the top level"));
}

#[test]
fn rejects_duplicate_function_names() {
    let message = compile_err(&with_interval(
        "fn helper() = true\nfn helper() = false\nplot(1)",
    ));
    assert!(message.contains("duplicate function `helper`"));
}

#[test]
fn rejects_duplicate_function_parameters() {
    let message = compile_err(&with_interval("fn helper(x, x) = x\nplot(1)"));
    assert!(message.contains("duplicate parameter `x` in function `helper`"));
}

#[test]
fn rejects_builtin_function_name_collisions() {
    let message = compile_err(&with_interval("fn plot(x) = x\nplot(1)"));
    assert!(message.contains("function name `plot` collides with a builtin"));
}

#[test]
fn rejects_wrong_user_function_arity() {
    let message = compile_err(&with_interval("fn helper(x) = x\nplot(helper())"));
    assert!(message.contains("function `helper` expects 1 argument(s), found 0"));
}

#[test]
fn rejects_function_body_captures() {
    let message = compile_err(&with_interval(
        "let basis = close\nfn helper() = basis\nplot(1)",
    ));
    assert!(message.contains("function bodies may only reference parameters or predefined series"));
}

#[test]
fn rejects_recursive_functions() {
    let message = compile_err(&with_interval("fn recurse(x) = recurse(x)\nplot(1)"));
    assert!(message.contains("recursive and cyclic function definitions are not allowed"));
}

#[test]
fn rejects_mutually_recursive_functions() {
    let message = compile_err(&with_interval("fn a() = b()\nfn b() = a()\nplot(1)"));
    assert!(message.contains("recursive and cyclic function definitions are not allowed"));
}

#[test]
fn rejects_plot_calls_inside_function_bodies() {
    let message = compile_err(&with_interval("fn bad(x) = plot(x)\nplot(1)"));
    assert!(message.contains("function bodies may not call `plot`"));
}

#[test]
fn supports_multiple_function_specializations() {
    compile(&with_interval(
        "fn add1(x) = x + 1\nlet one = add1(1)\nif add1(close) > one { plot(1) } else { plot(0) }",
    ))
    .expect("function specializations should compile");
}

#[test]
fn parses_interval_qualified_series() {
    compile(&with_intervals("plot(1w.close)", &["1w"])).expect("qualified series should compile");
}

#[test]
fn parses_interval_series_in_calls_and_indexing() {
    compile(&with_intervals("plot(ema(4h.high, 5)[1])", &["4h"]))
        .expect("qualified series should compose");
}

#[test]
fn supports_qualified_series_in_user_functions() {
    compile(&with_intervals(
        "fn rising(x) = x > x[1]\nif rising(1d.close) { plot(1) } else { plot(0) }",
        &["1d"],
    ))
    .expect("qualified series should specialize");
}

#[test]
fn rejects_bare_interval_literals() {
    let message = compile_err(&with_interval("plot(1w)"));
    assert!(message.contains("expected `.` after interval literal"));
}

#[test]
fn rejects_invalid_qualified_market_fields() {
    let message = compile_err(&with_intervals("plot(1w.foo)", &["1w"]));
    assert!(message.contains("expected market field after `.`"));
}

#[test]
fn rejects_calling_interval_qualified_series() {
    let message = compile_err(&with_intervals("plot(1w.close())", &["1w"]));
    assert!(message.contains("only identifiers can be called in v0.1"));
}

#[test]
fn parses_export_statements() {
    compile(&with_interval(
        "export trend = close > ema(close, 20)\nplot(1)",
    ))
    .expect("export statements should compile");
}

#[test]
fn parses_trigger_statements() {
    compile(&with_interval(
        "trigger long_entry = close > high[1]\nplot(1)",
    ))
    .expect("trigger statements should compile");
}

#[test]
fn reserves_export_and_trigger_keywords() {
    let message = compile_err(&with_interval(
        "let export = true\nlet trigger = false\nplot(1)",
    ));
    assert!(message.contains("expected identifier after `let`"));
}

#[test]
fn rejects_export_inside_blocks() {
    let message = compile_err(&with_interval(
        "if true { export trend = close } else { plot(0) }",
    ));
    assert!(message.contains("export statements are only allowed at the top level"));
}

#[test]
fn rejects_trigger_inside_blocks() {
    let message = compile_err(&with_interval(
        "if true { trigger long = close > open } else { plot(0) }",
    ));
    assert!(message.contains("trigger statements are only allowed at the top level"));
}

#[test]
fn rejects_void_exports() {
    let message = compile_err(&with_interval("export x = plot(close)\nplot(1)"));
    assert!(message
        .contains("export requires a numeric, bool, series numeric, series bool, or na value"));
}

#[test]
fn rejects_numeric_triggers() {
    let message = compile_err(&with_interval("trigger x = 1\nplot(1)"));
    assert!(message.contains("trigger requires bool, series<bool>, or na"));
}

#[test]
fn rejects_unknown_identifiers() {
    let message = compile_err(&with_interval("if trend { plot(1) } else { plot(0) }"));
    assert!(message.contains("unknown identifier `trend`"));
}

#[test]
fn rejects_missing_interval_directive() {
    let message = compile_err("plot(close)");
    assert!(message.contains("strategy must declare exactly one `interval <...>` directive"));
}

#[test]
fn parses_interval_and_use_directives() {
    compile("interval 1m\nuse 1w\nuse 1M\nplot(1w.close)")
        .expect("interval directives should compile");
}

#[test]
fn rejects_duplicate_interval_directives() {
    let message = compile_err("interval 1m\ninterval 5m\nplot(close)");
    assert!(message.contains("strategy must declare exactly one `interval <...>` directive"));
}

#[test]
fn rejects_duplicate_use_directives() {
    let message = compile_err("interval 1m\nuse 1w\nuse 1w\nplot(1w.close)");
    assert!(message.contains("duplicate `use 1w` declaration"));
}

#[test]
fn rejects_use_of_base_interval() {
    let message = compile_err("interval 1m\nuse 1m\nplot(close)");
    assert!(message.contains("duplicates the base interval"));
}

#[test]
fn rejects_undeclared_qualified_intervals() {
    let message = compile_err("interval 1m\nplot(1w.close)");
    assert!(message.contains("interval `1w` must be declared with `use 1w`"));
}

#[test]
fn rejects_bare_interval_directives() {
    let message = compile_err("interval\nplot(close)");
    assert!(message.contains("expected interval literal after `interval`"));
}

#[test]
fn rejects_bare_use_directives() {
    let message = compile_err("interval 1m\nuse\nplot(close)");
    assert!(message.contains("expected interval literal after `use`"));
}

#[test]
fn rejects_interval_directives_inside_blocks() {
    let message = compile_err(&with_interval("if true { interval 5m } else { plot(0) }"));
    assert!(message.contains("interval directives are only allowed at the top level"));
}

#[test]
fn rejects_use_directives_inside_blocks() {
    let message = compile_err(&with_interval("if true { use 1w } else { plot(0) }"));
    assert!(message.contains("interval directives are only allowed at the top level"));
}
