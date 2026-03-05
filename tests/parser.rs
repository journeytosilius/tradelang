use tradelang::compile;

fn compile_err(source: &str) -> String {
    let err = compile(source).expect_err("expected compile error");
    err.diagnostics
        .into_iter()
        .map(|diag| diag.message)
        .collect::<Vec<_>>()
        .join(" | ")
}

#[test]
fn rejects_missing_right_bracket() {
    let message = compile_err("plot(close[1)");
    assert!(message.contains("expected `]` after index"));
}

#[test]
fn rejects_negative_index() {
    let message = compile_err("plot(close[-1])");
    assert!(message.contains("series indexing requires a non-negative integer literal"));
}

#[test]
fn rejects_non_literal_index() {
    let message = compile_err("let n = 1\nplot(close[n])");
    assert!(message.contains("series indexing requires a non-negative integer literal"));
}

#[test]
fn rejects_non_literal_window_length() {
    let message = compile_err("let n = 14\nplot(sma(close, n))");
    assert!(message.contains("sma length must be a non-negative integer literal"));
}

#[test]
fn rejects_if_without_else() {
    let message = compile_err("if true { plot(1) }");
    assert!(message.contains("expected `else` after `if` block"));
}

#[test]
fn rejects_numeric_logical_operands() {
    let message = compile_err("if 1 and 2 { plot(1) } else { plot(0) }");
    assert!(message.contains("logical operators require bool, series<bool>, or na operands"));
}

#[test]
fn rejects_series_float_logical_operands() {
    let message = compile_err("if close and true { plot(1) } else { plot(0) }");
    assert!(message.contains("logical operators require bool, series<bool>, or na operands"));
}

#[test]
fn rejects_indicator_logical_operands() {
    let message = compile_err("if sma(close, 5) or false { plot(1) } else { plot(0) }");
    assert!(message.contains("logical operators require bool, series<bool>, or na operands"));
}

#[test]
fn allows_shadowing_in_inner_scope() {
    compile("let x = close\nif close > close[1] { let x = close[1]\nplot(x) } else { plot(x) }")
        .expect("shadowing should compile");
}

#[test]
fn parses_na_literal() {
    compile("plot(na)").expect("na literal should compile");
}

#[test]
fn supports_newline_and_semicolon_separators() {
    compile("let x = close;\nplot(x)").expect("mixed separators should compile");
}

#[test]
fn parses_logical_operators_with_expected_precedence() {
    compile("if true or false and false { plot(1) } else { plot(0) }")
        .expect("logical precedence should parse");
}

#[test]
fn parses_else_if_chains() {
    compile("if false { plot(0) } else if true { plot(1) } else { plot(2) }")
        .expect("else if chains should compile");
}

#[test]
fn supports_newlines_around_else_if() {
    compile("if false { plot(0) } else\nif true { plot(1) } else { plot(2) }")
        .expect("newline-separated else if should compile");
}

#[test]
fn reserves_logical_keywords() {
    let message = compile_err("let and = true\nplot(and)");
    assert!(message.contains("expected identifier after `let`"));
}

#[test]
fn parses_top_level_function_declarations() {
    compile(
        "fn crossover(a, b) = a > b and a[1] <= b[1]\nif crossover(close, ema(close, 3)) { plot(1) } else { plot(0) }",
    )
    .expect("function declarations should compile");
}

#[test]
fn parses_zero_argument_functions() {
    compile("fn bullish_bar() = close > open\nif bullish_bar() { plot(1) } else { plot(0) }")
        .expect("zero-argument functions should compile");
}

#[test]
fn rejects_function_declarations_inside_blocks() {
    let message = compile_err("if true { fn helper() = close > open } else { plot(0) }");
    assert!(message.contains("function declarations are only allowed at the top level"));
}

#[test]
fn rejects_duplicate_function_names() {
    let message = compile_err("fn helper() = true\nfn helper() = false\nplot(1)");
    assert!(message.contains("duplicate function `helper`"));
}

#[test]
fn rejects_duplicate_function_parameters() {
    let message = compile_err("fn helper(x, x) = x\nplot(1)");
    assert!(message.contains("duplicate parameter `x` in function `helper`"));
}

#[test]
fn rejects_builtin_function_name_collisions() {
    let message = compile_err("fn plot(x) = x\nplot(1)");
    assert!(message.contains("function name `plot` collides with a builtin"));
}

#[test]
fn rejects_wrong_user_function_arity() {
    let message = compile_err("fn helper(x) = x\nplot(helper())");
    assert!(message.contains("function `helper` expects 1 argument(s), found 0"));
}

#[test]
fn rejects_function_body_captures() {
    let message = compile_err("let basis = close\nfn helper() = basis\nplot(1)");
    assert!(message.contains("function bodies may only reference parameters or predefined series"));
}

#[test]
fn rejects_recursive_functions() {
    let message = compile_err("fn recurse(x) = recurse(x)\nplot(1)");
    assert!(message.contains("recursive and cyclic function definitions are not allowed"));
}

#[test]
fn rejects_mutually_recursive_functions() {
    let message = compile_err("fn a() = b()\nfn b() = a()\nplot(1)");
    assert!(message.contains("recursive and cyclic function definitions are not allowed"));
}

#[test]
fn rejects_plot_calls_inside_function_bodies() {
    let message = compile_err("fn bad(x) = plot(x)\nplot(1)");
    assert!(message.contains("function bodies may not call `plot`"));
}

#[test]
fn supports_multiple_function_specializations() {
    compile(
        "fn add1(x) = x + 1\nlet one = add1(1)\nif add1(close) > one { plot(1) } else { plot(0) }",
    )
    .expect("function specializations should compile");
}

#[test]
fn parses_interval_qualified_series() {
    compile("plot(1w.close)").expect("qualified series should compile");
}

#[test]
fn parses_interval_series_in_calls_and_indexing() {
    compile("plot(ema(4h.high, 5)[1])").expect("qualified series should compose");
}

#[test]
fn supports_qualified_series_in_user_functions() {
    compile("fn rising(x) = x > x[1]\nif rising(1d.close) { plot(1) } else { plot(0) }")
        .expect("qualified series should specialize");
}

#[test]
fn rejects_bare_interval_literals() {
    let message = compile_err("plot(1w)");
    assert!(message.contains("expected `.` after interval literal"));
}

#[test]
fn rejects_invalid_qualified_market_fields() {
    let message = compile_err("plot(1w.foo)");
    assert!(message.contains("expected market field after `.`"));
}

#[test]
fn rejects_calling_interval_qualified_series() {
    let message = compile_err("plot(1w.close())");
    assert!(message.contains("only identifiers can be called in v0.1"));
}
