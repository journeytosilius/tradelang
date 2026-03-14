# Grammar

This page is the normative grammar for PalmScript.

The productions below define the accepted parser surface. Rules that depend on name resolution, interval ordering, or typing are defined in later reference chapters.

## Program

```text
program                ::= separator* item* EOF
item                   ::= interval_decl
                         | source_decl
                         | execution_decl
                         | use_decl
                         | function_decl
                         | stmt

interval_decl          ::= "interval" interval
source_decl            ::= "source" ident "=" source_template "(" string_literal ")"
execution_decl         ::= "execution" ident "=" source_template "(" string_literal ")"
source_template        ::= ident "." ident
use_decl               ::= "use" interval
                         | "use" ident interval
function_decl          ::= "fn" ident "(" param_list? ")" "=" expr
param_list             ::= ident ("," ident)*
```

## Statements

```text
stmt                   ::= let_stmt
                         | const_stmt
                         | input_stmt
                         | export_stmt
                         | regime_stmt
                         | trigger_stmt
                         | risk_control_stmt
                         | portfolio_control_stmt
                         | portfolio_group_stmt
                         | signal_stmt
                         | attached_exit_stmt
                         | order_template_stmt
                         | order_stmt
                         | if_stmt
                         | expr_stmt

let_stmt               ::= "let" ident "=" expr
                         | "let" "(" ident ("," ident)+ ")" "=" expr
const_stmt             ::= "const" ident "=" expr
input_stmt             ::= "input" ident "=" expr input_optimize?
input_optimize         ::= "optimize" "(" optimize_space ")"
optimize_space         ::= "int" "," number "," number ("," number)?
                         | "float" "," number "," number ("," number)?
                         | "choice" "," number ("," number)*
export_stmt            ::= "export" ident "=" expr
regime_stmt            ::= "regime" ident "=" expr
trigger_stmt           ::= "trigger" ident "=" expr
risk_control_stmt      ::= "cooldown" signal_side "=" expr
                         | "max_bars_in_trade" signal_side "=" expr
portfolio_control_stmt ::= "max_positions" "=" expr
                         | "max_long_positions" "=" expr
                         | "max_short_positions" "=" expr
                         | "max_gross_exposure_pct" "=" expr
                         | "max_net_exposure_pct" "=" expr
portfolio_group_stmt   ::= "portfolio_group" string_literal "=" "[" ident ("," ident)* "]"
signal_stmt            ::= "entry" signal_side "=" expr
                         | "exit" signal_side "=" expr
attached_exit_stmt     ::= "protect" signal_side "=" order_spec
                         | "target" signal_side "=" order_spec
order_template_stmt    ::= "order_template" ident "=" order_spec
order_stmt             ::= "order" ("entry" | "exit") signal_side "=" order_spec
signal_side            ::= "long" | "short"
order_spec             ::= ident
                         | "market" "(" order_args? ")"
                         | "limit" "(" order_args ")"
                         | "stop_market" "(" order_args ")"
                         | "stop_limit" "(" order_args ")"
                         | "take_profit_market" "(" order_args ")"
                         | "take_profit_limit" "(" order_args ")"
order_args             ::= expr ("," expr)*
                         | named_order_arg ("," named_order_arg)*
named_order_arg        ::= ident "=" (expr | ident)
if_stmt                ::= "if" expr block "else" else_tail
else_tail              ::= if_stmt
                         | block
expr_stmt              ::= expr
block                  ::= "{" separator* stmt* "}"
```

## Expressions

```text
expr                   ::= conditional_expr
conditional_expr       ::= or_expr ("?" expr ":" conditional_expr)?
or_expr                ::= and_expr ("or" and_expr)*
and_expr               ::= cmp_expr ("and" cmp_expr)*
cmp_expr               ::= add_expr (cmp_op add_expr)*
cmp_op                 ::= "==" | "!=" | "<" | "<=" | ">" | ">="
add_expr               ::= mul_expr (("+" | "-") mul_expr)*
mul_expr               ::= unary_expr (("*" | "/") unary_expr)*
unary_expr             ::= ("-" | "!") unary_expr
                         | postfix_expr
postfix_expr           ::= primary_expr postfix*
postfix                ::= call_suffix
                         | index_suffix
                         | source_suffix
call_suffix            ::= "(" arg_list? ")"
index_suffix           ::= "[" expr "]"
source_suffix          ::= "." ident
                         | "." interval "." ident
arg_list               ::= expr ("," expr)*
```

## Primary Expressions

```text
primary_expr           ::= number
                         | "true"
                         | "false"
                         | "na"
                         | string_literal
                         | ident
                         | ident "." ident
                         | interval "." market_field
                         | "(" expr ")"
```

## Lexical Nonterminals

```text
market_field           ::= "open" | "high" | "low" | "close" | "volume" | "time"
interval               ::= one of the literals listed in [Interval Table](intervals.md)
ident                  ::= identifier token
string_literal         ::= string token
number                 ::= numeric literal token
separator              ::= newline | ";"
```

## Binding And Precedence

PalmScript parses binary operators with the following precedence, from lowest to highest:

1. `or`
2. `and`
3. `==`, `!=`, `<`, `<=`, `>`, `>=`
4. `+`, `-`
5. `*`, `/`
6. unary `-`, unary `!`
7. call `(...)`, indexing `[...]`, and source/field qualification with `.`

Operators within one precedence level associate left-to-right, except the ternary conditional which associates right-to-left.

## Required Semantic Restrictions

The grammar does not by itself make a program valid. The implementation additionally requires:

- a script must declare exactly one base `interval`
- a script must declare at least one `source`
- `interval`, `source`, `execution`, `use`, `fn`, `const`, `input`, `export`, `regime`, `trigger`, `cooldown`, `max_bars_in_trade`, `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, `portfolio_group`, `entry`, `exit`, `protect`, `target`, `order_template`, `order`, and `size` must appear only at the top level
- bare market identifiers such as `close` are rejected and market series must be source-qualified
- higher source interval references require `use <alias> <interval>`
- every `if` must have an `else`
- string literals are accepted lexically but are semantically valid only inside `source` declarations
- `execution` declarations share the same exchange-backed template syntax as `source` declarations but do not create market series bindings
- only identifiers may be called
- series indexing must use a non-negative integer literal or a top-level immutable numeric binding
- tuple-valued builtins must be bound with tuple destructuring before use
- `input ... optimize(...)` metadata is only valid on numeric `input` declarations and must pass the range/choice validation rules described in [Declarations and Scope](declarations-and-scope.md)
- `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>`, and `exit_kind.<variant>` are typed enum namespaces
- `position.*` is valid only inside `protect` and `target` declarations
- `position_event.*` is a backtest-driven `series<bool>` namespace
- `last_exit.*`, `last_long_exit.*`, and `last_short_exit.*` are backtest-driven latest-closed-trade namespaces
- `entry1..3 long|short`, `target1..3 long|short`, and `protect_after_target1..3 long|short` are valid staged declarations in v1
- `entry long` and `target long|short` remain compatibility aliases for stage 1
- `cooldown long|short` and `max_bars_in_trade long|short` require a compile-time non-negative whole-number scalar expression
- `max_positions`, `max_long_positions`, and `max_short_positions` require a compile-time non-negative whole-number scalar expression
- `max_gross_exposure_pct` and `max_net_exposure_pct` require a compile-time non-negative finite numeric scalar expression
- `portfolio_group` aliases must refer to declared `source` bindings and group names must be unique
- matching `source` and `execution` aliases may mirror each other when the template and symbol are the same; other aliases must remain unique
- order constructors accept either the legacy positional form or the named-argument form, but not both at once
- `venue = <execution_alias>` requires a declared `execution` alias
- trading scripts require at least one declared `execution` target
- `size entry1..3 long|short` and `size target1..3 long|short` are valid staged `size` declarations in v1
- staged entry sizes accept either a legacy bare numeric fraction, `capital_fraction(x)`, or `risk_pct(pct, stop_price)`
- `size entry ...` requires a matching staged `order entry ...` declaration for the same role
- `size target ...` requires a matching staged `target ...` declaration for the same role
- `risk_pct(...)` is only valid on staged entry size declarations
- user-defined functions are expression-bodied, top-level only, non-recursive, and may not capture surrounding `let` bindings
- user-defined functions may capture top-level immutable `const` and `input` bindings
- source, interval, scope, and type rules are enforced as described in the other `Reference` pages
