use palmscript::compile;

#[path = "support/mod.rs"]
mod support;

fn compile_err(source: &str) -> String {
    let err = compile(source).expect_err("expected compile error");
    err.diagnostics
        .into_iter()
        .map(|diag| diag.message)
        .collect::<Vec<_>>()
        .join(" | ")
}

fn with_interval(source: &str) -> String {
    support::with_single_source_interval(source)
}

fn with_intervals(source: &str, supplemental: &[&str]) -> String {
    support::with_single_source_intervals("1m", supplemental, source)
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
fn parses_na_predicate_call() {
    compile(&with_interval(
        "if na(close[1]) { plot(1) } else { plot(0) }",
    ))
    .expect("na(value) should compile");
}

#[test]
fn parses_regime_declarations_and_state_builtin() {
    compile(&with_interval(
        "regime trend_long = state(close > close[1], close < close[1])\nexport entered = activated(trend_long)\nplot(0)",
    ))
    .expect("regime declarations should compile");
}

#[test]
fn parses_ternary_conditional_expression() {
    compile(&with_interval("plot(close > close[1] ? 1 : 0)"))
        .expect("ternary conditional should compile");
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
fn parses_division_with_multiplicative_precedence() {
    compile(&with_interval("plot((close - close[1]) / close[1] * 100)"))
        .expect("division should parse with multiplicative precedence");
}

#[test]
fn parses_input_optimization_metadata() {
    compile(&with_interval(
        "input fast = 21 optimize(int, 8, 34, 1)\ninput threshold = 0.5 optimize(float, -2.0, 2.0, 0.1)\ninput selector = 21 optimize(choice, -13, 8, 13, 21)\nplot(close)",
    ))
    .expect("input optimization metadata should compile");
}

#[test]
fn parses_declarative_risk_control_statements() {
    compile(&with_interval(
        "cooldown long = 3\nmax_bars_in_trade short = 24\nplot(close)",
    ))
    .expect("risk control declarations should compile");
}

#[test]
fn parses_portfolio_control_and_group_declarations() {
    compile(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
source right = gate.spot(\"BTC_USDT\")
execution exec = binance.spot(\"BTCUSDT\")
max_positions = 2
max_long_positions = 1
max_short_positions = 1
max_gross_exposure_pct = 1.5
max_net_exposure_pct = 1.0
portfolio_group \"majors\" = [left, right]
entry long = left.close > right.close
entry short = left.close < right.close
exit long = false
exit short = false
order entry long = market(venue = exec)
order entry short = market(venue = exec)
order exit long = market(venue = exec)
order exit short = market(venue = exec)
plot(left.close)",
    )
    .expect("portfolio declarations should compile");
}

#[test]
fn parses_entry_module_declarations() {
    compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution spot = binance.spot(\"BTCUSDT\")
module breakout = entry long
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
order entry long = market(venue = spot)
order exit long = market(venue = spot)
plot(spot.close)",
    )
    .expect("entry module declarations should compile");
}

#[test]
fn parses_ledger_execution_namespace_fields() {
    compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution exec = binance.spot(\"BTCUSDT\")
export quote_free = ledger(exec).quote_free
export mark_value = ledger(exec).mark_value_quote
plot(spot.close)",
    )
    .expect("ledger namespace fields should compile");
}

#[test]
fn parses_execution_alias_venue_selection_builtins() {
    compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution bn = binance.spot(\"BTCUSDT\")
execution gt = gate.spot(\"BTC_USDT\")
export best_is_bn = cheapest(bn, gt) == bn
export worst_is_gt = richest(bn, gt) == gt
export spread = spread_bps(bn, gt)
plot(spot.close)",
    )
    .expect("venue-selection builtins should compile");
}

#[test]
fn parses_arb_signals_and_market_pair_orders() {
    compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution bn = binance.spot(\"BTCUSDT\")
execution gt = gate.spot(\"BTC_USDT\")
let cheap = cheapest(bn, gt)
let rich = richest(bn, gt)
arb_entry = spread_bps(cheap, rich) > 5
arb_exit = spread_bps(cheap, rich) < 1
arb_order entry = market_pair(
    buy_venue = cheap,
    sell_venue = rich,
    size = 0.25,
    abort_on_partial = true,
    max_leg_delay_bars = 1
)
arb_order exit = market_pair(
    buy_venue = rich,
    sell_venue = cheap,
    size = 0.25
)
plot(spot.close)",
    )
    .expect("arb syntax should compile");
}

#[test]
fn parses_quote_transfer_declarations() {
    compile(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution bn = binance.spot(\"BTCUSDT\")
execution gt = gate.spot(\"BTC_USDT\")
transfer quote = quote_transfer(
    from = gt,
    to = bn,
    amount = 100,
    fee = 1,
    delay_bars = 2
)
plot(spot.close)",
    )
    .expect("quote transfer declarations should compile");
}

#[test]
fn rejects_positional_arb_pair_constructor_arguments() {
    let message = compile_err(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution bn = binance.spot(\"BTCUSDT\")
execution gt = gate.spot(\"BTC_USDT\")
arb_entry = true
arb_order entry = market_pair(bn, gt, 0.25)
plot(spot.close)",
    );
    assert!(message.contains("arbitrage pair constructors must use named arguments"));
}

#[test]
fn rejects_non_execution_alias_venue_selection_arguments() {
    let message = compile_err(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution bn = binance.spot(\"BTCUSDT\")
execution gt = gate.spot(\"BTC_USDT\")
plot(cheapest(spot.close, bn))",
    );
    assert!(message.contains("cheapest requires declared execution-alias arguments"));
}

#[test]
fn rejects_unknown_ledger_execution_alias() {
    let message = compile_err(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution exec = binance.spot(\"BTCUSDT\")
plot(ledger(other).quote_free)",
    );
    assert!(message.contains("unknown execution alias `other`"));
}

#[test]
fn rejects_module_declarations_without_matching_signal() {
    let message = compile_err(
        "interval 1m
source spot = binance.spot(\"BTCUSDT\")
execution spot = binance.spot(\"BTCUSDT\")
module breakout = entry long
plot(spot.close)",
    );
    assert!(message.contains(
        "module declaration `breakout` requires a matching `entry long = ...` signal declaration"
    ));
}

#[test]
fn rejects_nested_risk_control_declarations() {
    let message = compile_err(&with_interval(
        "if true { cooldown long = 1 } else { plot(close) }",
    ));
    assert!(message.contains("risk control declarations are only allowed at the top level"));
}

#[test]
fn rejects_nested_portfolio_control_declarations() {
    let message = compile_err(&with_interval(
        "if true { max_positions = 2 } else { plot(close) }",
    ));
    assert!(message.contains("portfolio declarations are only allowed at the top level"));
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
        "fn cross_signal(a, b) = a > b and a[1] <= b[1]\nif cross_signal(close, ema(close, 3)) { plot(1) } else { plot(0) }",
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
fn parses_order_declarations_with_enum_literals() {
    compile(&with_interval(
        "entry long = src.close > src.close[1]
exit long = src.close < src.close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry long = limit(price = src.close[1], tif = tif.gtc, post_only = false, venue = src)
order exit long = stop_market(trigger_price = src.close[1], trigger_ref = trigger_ref.last, venue = src)
plot(src.close)",
    ))
    .expect("order declarations should compile");
}

#[test]
fn parses_execution_declarations_and_named_order_arguments() {
    compile(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
source right = gate.spot(\"BTC_USDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
entry long = left.close > right.close
exit long = left.close < right.close
order entry long = limit(price = left.close[1], tif = tif.gtc, post_only = false, venue = exec)
order exit long = stop_market(trigger_price = left.close[1], trigger_ref = trigger_ref.mark, venue = exec)
plot(left.close)",
    )
    .expect("execution declarations and named order arguments should compile");
}

#[test]
fn parses_reusable_order_templates() {
    compile(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
order_template maker_entry = limit(
    price = left.close[1],
    tif = tif.gtc,
    post_only = true,
    venue = exec
)
order_template maker_exit = maker_entry
entry long = left.close > left.close[1]
exit long = left.close < left.close[1]
order entry long = maker_entry
order exit long = maker_exit
plot(left.close)",
    )
    .expect("order templates should compile");
}

#[test]
fn parses_named_order_arguments_for_all_order_constructors() {
    compile(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
entry long = left.close > left.close[1]
exit long = left.close < left.close[1]
order entry long = market(venue = exec)
order exit long = stop_limit(
    trigger_price = left.close[1],
    limit_price = left.close[1],
    tif = tif.gtc,
    post_only = false,
    trigger_ref = trigger_ref.mark,
    expire_time_ms = 0,
    venue = exec
)
protect long = take_profit_market(
    trigger_price = left.close[1],
    trigger_ref = trigger_ref.mark,
    venue = exec
)
target long = take_profit_limit(
    trigger_price = left.close[1],
    limit_price = left.close[1],
    tif = tif.gtc,
    post_only = false,
    trigger_ref = trigger_ref.mark,
    expire_time_ms = 0,
    venue = exec
)
plot(left.close)",
    )
    .expect("all order constructors should accept named execution arguments");
}

#[test]
fn rejects_mixing_positional_and_named_order_arguments() {
    let message = compile_err(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
entry long = left.close > left.close[1]
exit long = false
order entry long = limit(left.close[1], tif = tif.gtc, post_only = false, venue = exec)
plot(left.close)",
    );
    assert!(message
        .contains("order constructors must use either positional arguments or named arguments"));
}

#[test]
fn rejects_non_identifier_execution_binding_in_order_arguments() {
    let message = compile_err(
        "interval 1m
source left = binance.spot(\"BTCUSDT\")
execution exec = bybit.usdt_perps(\"BTCUSDT\")
entry long = left.close > left.close[1]
exit long = false
order entry long = market(venue = left.close)
plot(left.close)",
    );
    assert!(message.contains("`venue` must reference an execution alias identifier"));
}

#[test]
fn parses_attached_exit_declarations_with_position_fields() {
    compile(
        "interval 1m
source src = binance.spot(\"BTCUSDT\")
entry long = src.close > src.close[1]
execution src = binance.spot(\"BTCUSDT\")
protect long = stop_market(
    trigger_price = position.side == position_side.long ? position.entry_price - 1 : position.entry_price - 2,
    trigger_ref = trigger_ref.last,
    venue = src
)
order entry long = market(venue = src)
target long = take_profit_market(trigger_price = position.entry_price + 2, trigger_ref = trigger_ref.last, venue = src)
plot(src.close)",
    )
    .expect("attached exits should compile");
}

#[test]
fn parses_partial_target_size_declarations() {
    compile(
        "interval 1m
source src = binance.spot(\"BTCUSDT\")
entry long = src.close > src.close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry long = market(venue = src)
target long = take_profit_market(trigger_price = position.entry_price + 2, trigger_ref = trigger_ref.last, venue = src)
size target long = 0.5
plot(src.close)",
    )
    .expect("partial target size declarations should compile");
}

#[test]
fn parses_partial_entry_size_declarations() {
    compile(
        "interval 1m
source src = binance.spot(\"BTCUSDT\")
entry long = src.close > src.close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry long = market(venue = src)
size entry long = 0.5
plot(src.close)",
    )
    .expect("partial entry size declarations should compile");
}

#[test]
fn parses_risk_based_entry_size_declarations() {
    compile(
        "interval 1m
source src = binance.spot(\"BTCUSDT\")
let stop_price = src.close - 2
entry long = src.close > src.close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry long = market(venue = src)
size entry long = risk_pct(0.01, stop_price)
protect long = stop_market(trigger_price = stop_price, trigger_ref = trigger_ref.last, venue = src)
plot(src.close)",
    )
    .expect("risk-based entry size declarations should compile");
}

#[test]
fn parses_module_based_entry_size_declarations() {
    compile(
        "interval 1m
source src = binance.spot(\"BTCUSDT\")
execution src = binance.spot(\"BTCUSDT\")
module breakout = entry long
entry long = src.close > src.close[1]
order entry long = market(venue = src)
size module breakout = 0.5
plot(src.close)",
    )
    .expect("module-based entry size declarations should compile");
}

#[test]
fn parses_regime_aware_module_entry_size_declarations() {
    compile(
        "interval 1m
source src = binance.spot(\"BTCUSDT\")
execution src = binance.spot(\"BTCUSDT\")
module breakout = entry long
regime strong = src.close > src.close[1]
entry long = src.close > src.close[1]
order entry long = market(venue = src)
size module breakout = strong ? 0.4 : 0.15
plot(src.close)",
    )
    .expect("regime-aware module-based entry size declarations should compile");
}

#[test]
fn parses_staged_entries_targets_and_protect_ratchets() {
    compile(
        "interval 1m
source src = binance.spot(\"BTCUSDT\")
entry1 long = src.close > src.close[1]
entry2 long = src.close > src.close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry1 long = market(venue = src)
order entry2 long = market(venue = src)
size entry1 long = 0.5
size entry2 long = 0.5
protect long = stop_market(trigger_price = position.entry_price - 2, trigger_ref = trigger_ref.last, venue = src)
protect_after_target1 long = stop_market(trigger_price = position.entry_price, trigger_ref = trigger_ref.last, venue = src)
target1 long = take_profit_market(trigger_price = position.entry_price + 2, trigger_ref = trigger_ref.last, venue = src)
target2 long = take_profit_market(trigger_price = position.entry_price + 4, trigger_ref = trigger_ref.last, venue = src)
size target1 long = 0.5
export target_stage = last_exit.stage
plot(src.close)",
    )
    .expect("staged declarations should compile");
}

#[test]
fn parses_position_event_anchors_with_since_helpers() {
    compile(
        "interval 1m
source src = binance.spot(\"BTCUSDT\")
entry long = src.close > src.close[1]
execution src = binance.spot(\"BTCUSDT\")
order entry long = market(venue = src)
protect long = stop_market(
    trigger_price = highest_since(position_event.long_entry_fill, src.high) - 2 * atr(src.high, src.low, src.close, 14),
    trigger_ref = trigger_ref.last,
    venue = src
)
export armed = position_event.long_entry_fill
plot(src.close)",
    )
    .expect("position event anchors should compile");
}

#[test]
fn parses_last_exit_namespaces_and_exit_kind_literals() {
    compile(
        "interval 1m
source src = binance.spot(\"BTCUSDT\")
entry long = last_long_exit.kind == exit_kind.target or last_exit.kind == exit_kind.liquidation or last_exit.side == position_side.long
execution src = binance.spot(\"BTCUSDT\")
order entry long = market(venue = src)
export last_short_price = last_short_exit.price
plot(src.close)",
    )
    .expect("last-exit namespaces should compile");
}

#[test]
fn rejects_unknown_last_exit_field() {
    let message = compile_err(&with_interval("plot(last_exit.foo)"));
    assert!(message.contains("unknown last-exit field"));
}

#[test]
fn rejects_unknown_exit_kind_variant() {
    let message = compile_err(&with_interval(
        "if last_exit.kind == exit_kind.foo { plot(1) } else { plot(0) }",
    ));
    assert!(message.contains("unknown enum variant `exit_kind.foo`"));
}

#[test]
fn rejects_order_declarations_inside_blocks() {
    let message = compile_err(&with_interval(
        "if true { order entry long = market() } else { plot(0) }",
    ));
    assert!(message.contains("order declarations are only allowed at the top level"));
}

#[test]
fn rejects_order_template_declarations_inside_blocks() {
    let message = compile_err(&with_interval(
        "if true { order_template market_entry = market() } else { plot(0) }",
    ));
    assert!(message.contains("order template declarations are only allowed at the top level"));
}

#[test]
fn rejects_attached_exit_declarations_inside_blocks() {
    let message = compile_err(&with_interval(
        "if true { protect long = stop_market(1, trigger_ref.last) } else { plot(0) }",
    ));
    assert!(message.contains("attached exit declarations are only allowed at the top level"));
}

#[test]
fn rejects_target_size_declarations_inside_blocks() {
    let message = compile_err(&with_interval(
        "if true { size target long = 0.5 } else { plot(0) }",
    ));
    assert!(message.contains("order size declarations are only allowed at the top level"));
}

#[test]
fn rejects_size_declarations_for_non_target_roles() {
    let message = compile_err(&with_interval(
        "entry long = src.close > src.close[1]\nsize exit long = 0.5\nplot(src.close)",
    ));
    assert!(message.contains(
        "expected `entry`, `target`, `module`, `entry1..3`, or `target1..3` after `size`"
    ));
}

#[test]
fn rejects_function_declarations_inside_blocks() {
    let message = compile_err(&with_interval(
        "if true { fn helper() = close > open } else { plot(0) }",
    ));
    assert!(message.contains("function declarations are only allowed at the top level"));
}

#[test]
fn rejects_regime_inside_blocks() {
    let message = compile_err(&with_interval(
        "if true { regime trend_long = close > close[1] } else { plot(0) }",
    ));
    assert!(message.contains("regime statements are only allowed at the top level"));
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
    assert!(
        message.contains("function bodies may only reference parameters or declared source series")
    );
}

#[test]
fn allows_function_body_const_and_input_captures() {
    compile(&with_interval(
        "input length = 5\nconst threshold = 2\nfn helper(x) = x > threshold ? x : ema(x, length)\nplot(helper(close))",
    ))
    .expect("functions should capture const/input bindings");
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
        "fn is_rising(x) = x > x[1]\nif is_rising(1d.close) { plot(1) } else { plot(0) }",
        &["1d"],
    ))
    .expect("qualified series should specialize");
}

#[test]
fn parses_talib_enum_literals() {
    compile(&with_interval("plot(ma(close, 3, ma_type.ema))"))
        .expect("ma_type enum literal should compile");
}

#[test]
fn parses_tuple_destructuring_from_builtin_calls() {
    compile(&with_interval(
        "let (macd_line, signal, hist) = macd(close, 3, 5, 2)\nplot(hist)",
    ))
    .expect("tuple destructuring should compile");
}

#[test]
fn rejects_bare_interval_literals() {
    let message = compile_err(&with_interval("plot(1w)"));
    assert!(message.contains("global interval-qualified series are not supported"));
}

#[test]
fn rejects_invalid_qualified_market_fields() {
    let message = compile_err(&with_intervals("plot(1w.foo)", &["1w"]));
    assert!(message.contains("global interval-qualified series are not supported"));
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
        "trigger breakout = close > high[1]\nplot(1)",
    ))
    .expect("trigger statements should compile");
}

#[test]
fn parses_const_input_and_signal_statements() {
    compile(&with_interval(
        "input length = 14\nconst limit = 50\nentry long = close > ema(close, length)\nexit long = close < ema(close, length)\nentry short = close < ema(close, length)\nexit short = close > ema(close, length)\nexecution src = binance.spot(\"BTCUSDT\")\norder entry long = market(venue = src)\norder exit long = market(venue = src)\norder entry short = market(venue = src)\norder exit short = market(venue = src)\nplot(limit)",
    ))
    .expect("const/input/signal statements should compile");
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
fn rejects_const_input_and_signal_inside_blocks() {
    let message = compile_err(&with_interval("if true { const x = 1 } else { plot(0) }"));
    assert!(message.contains("`const` declarations are only allowed at the top level"));

    let message = compile_err(&with_interval("if true { input x = 1 } else { plot(0) }"));
    assert!(message.contains("`input` declarations are only allowed at the top level"));

    let message = compile_err(&with_interval(
        "if true { entry long = close > open } else { plot(0) }",
    ));
    assert!(message.contains("signal declarations are only allowed at the top level"));
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
fn rejects_non_literal_input_values() {
    let message = compile_err(&with_interval("input x = 1 + 1\nplot(x)"));
    assert!(message.contains("`input` expressions may only use scalar literals or enum literals"));
}

#[test]
fn rejects_unknown_identifiers() {
    let message = compile_err(&with_interval("if trend { plot(1) } else { plot(0) }"));
    assert!(message.contains("unknown identifier `trend`"));
}

#[test]
fn rejects_missing_interval_directive() {
    let message = compile_err("plot(1)");
    assert!(message.contains("strategy must declare exactly one `interval <...>` directive"));
}

#[test]
fn parses_interval_and_use_directives() {
    compile(&support::with_single_source_intervals(
        "1m",
        &["1w", "1M"],
        "plot(1w.close)",
    ))
    .expect("interval directives should compile");
}

#[test]
fn rejects_duplicate_interval_directives() {
    let message = compile_err("interval 1m\ninterval 5m\nplot(1)");
    assert!(message.contains("strategy must declare exactly one `interval <...>` directive"));
}

#[test]
fn rejects_duplicate_use_directives() {
    let message = compile_err(
        "interval 1m\nsource src = binance.spot(\"BTCUSDT\")\nuse src 1w\nuse src 1w\nplot(src.1w.close)",
    );
    assert!(message.contains("duplicate `use src 1w` declaration"));
}

#[test]
fn allows_use_of_base_interval_for_named_sources() {
    compile("interval 1m\nsource src = binance.spot(\"BTCUSDT\")\nuse src 1m\nplot(src.close)")
        .expect("source-scoped base interval use should compile");
}

#[test]
fn rejects_undeclared_qualified_intervals() {
    let message =
        compile_err("interval 1m\nsource src = binance.spot(\"BTCUSDT\")\nplot(src.1w.close)");
    assert!(message.contains("source interval `1w` for `src` must be declared with `use src 1w`"));
}

#[test]
fn rejects_bare_interval_directives() {
    let message = compile_err("interval\nplot(1)");
    assert!(message.contains("expected interval literal after `interval`"));
}

#[test]
fn rejects_bare_use_directives() {
    let message =
        compile_err("interval 1m\nsource src = binance.spot(\"BTCUSDT\")\nuse\nplot(src.close)");
    assert!(message.contains("expected source alias after `use`"));
}

#[test]
fn rejects_interval_directives_inside_blocks() {
    let message = compile_err(&with_interval("if true { interval 5m } else { plot(0) }"));
    assert!(message.contains("interval directives are only allowed at the top level"));
}

#[test]
fn rejects_use_directives_inside_blocks() {
    let message = compile_err(&with_interval("if true { use src 1w } else { plot(0) }"));
    assert!(message.contains("interval directives are only allowed at the top level"));
}
