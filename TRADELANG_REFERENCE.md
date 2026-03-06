# TradeLang Language Reference

This document describes the TradeLang language as implemented in this
repository today.

It is intended to be the working reference for both humans and agents. If the
implementation changes, this file should be updated to match the actual parser,
compiler, and runtime behavior.

## Status

TradeLang is currently a small deterministic DSL for financial time-series
programs compiled to bytecode and executed by a VM. The repository now ships
the library crate, the official `tradelang` CLI binary, and a first-party
editor stack built from the same library APIs:

- `tradelang`: CLI wrapper around the compiler and runtime
- `tradelang-lsp`: language server binary for editor integrations
- `editors/vscode/`: VS Code extension that launches `tradelang-lsp`

The implemented language supports:

- numeric, boolean, and `na` literals
- one mandatory top-level `interval <...>` execution directive
- top-level `use <...>` declarations for additional referenced intervals
- `let` bindings
- top-level `export` and `trigger` statements
- top-level user-defined functions
- `if`, `else if`, and `else` statements
- logical `and` and `or` expressions
- arithmetic and comparison expressions
- unary negation and boolean negation
- series indexing with literal lookback offsets
- builtins: `sma`, `ema`, `rsi`, `plot`
- predefined market data series: `open`, `high`, `low`, `close`, `volume`,
  `time`
- interval-qualified market series such as `1w.close` and `4h.volume`
- host-managed strategy composition through external series inputs

## Execution Model

TradeLang scripts are compiled once and then executed once per declared base
interval bar.

At each bar:

1. base-interval market data values are loaded into predefined series
2. referenced higher/equal interval feeds are advanced up to the current
   fully closed base-bar boundary
3. host-provided external inputs, if any, are loaded into predefined series
   slots
4. the compiled bytecode runs
5. current values may be stored into bounded series buffers
6. `plot(...)`, `export`, and `trigger` outputs are emitted for the current
   bar

The base execution interval is declared in source with `interval <binance-interval>`.

For multi-interval scripts:

- `Bar.time` is interpreted as candle open time in Unix milliseconds UTC
- every strategy must declare exactly one base interval with `interval <...>`
- every referenced non-base interval must be declared with `use <...>`
- interval-qualified series expose the last fully closed candle for that
  interval
- no partial higher-timeframe candle is ever visible
- lower-than-base interval references are rejected when the runtime binds feeds

For composed pipelines:

- external inputs are injected by the host in a fixed slot order determined at
  compile time
- external inputs behave like base-clock predefined series
- pipelines are DAGs and execute in topological order
- downstream nodes may observe upstream outputs from the same bar only if the
  upstream node already ran earlier in that topological order

TradeLang is deterministic:

- no IO
- no filesystem access
- no network access
- no wall clock access
- no randomness

## Source File Structure

A program is a sequence of top-level items.

Supported top-level item forms:

- `interval <interval>`
- `use <interval>`
- `fn name(params...) = expr`
- `let name = expr`
- `export name = expr`
- `trigger name = expr`
- `if condition { ... } else if other_condition { ... } else { ... }`
- expression statements such as `plot(close)`

Function declarations are hoisted into the callable namespace and do not emit
bytecode directly.

Example:

```tradelang
interval 1m

fn crossover(a, b) = a > b and a[1] <= b[1]

let fast = ema(close, 5)
let slow = sma(close, 10)

if crossover(fast, slow) {
    plot(1)
} else {
    plot(0)
}
```

## Lexical Structure

### Whitespace

Spaces, tabs, and carriage returns are ignored.

### Statement Separators

Statements are separated by:

- newlines
- semicolons

Newlines inside parentheses or brackets do not terminate statements.

Example:

```tradelang
let x = close;
plot(
    sma(close, 5)
)
```

### Comments

Single-line comments are supported:

```tradelang
// this is a comment
plot(close)
```

### Identifiers

Identifiers start with a letter or `_`, and may contain letters, digits, and
`_`.

Examples:

- `x`
- `close`
- `_temp1`

### Keywords

Reserved keywords:

- `fn`
- `let`
- `interval`
- `use`
- `export`
- `trigger`
- `if`
- `else`
- `and`
- `or`
- `true`
- `false`
- `na`

### Numbers

Numeric literals are parsed as `f64`.

Supported forms:

- `1`
- `14`
- `3.5`

Current constraints:

- no exponent syntax like `1e6`
- no leading-dot form like `.5`
- negative numbers are written with unary `-`, not as signed literals

### Interval Literals

TradeLang supports Binance interval literals for interval-qualified market
series.

Supported literals are case-sensitive:

- `1s`
- `1m`
- `3m`
- `5m`
- `15m`
- `30m`
- `1h`
- `2h`
- `4h`
- `6h`
- `8h`
- `12h`
- `1d`
- `3d`
- `1w`
- `1M`

Important cases:

- weekly is `1w`
- monthly is `1M`
- `1W` is invalid

## Grammar Sketch

This is a practical sketch of the implemented grammar, not a formal grammar.

```text
program      := item*
item         := interval_decl | use_decl | fn_decl | stmt
interval_decl:= "interval" interval
use_decl     := "use" interval
fn_decl      := "fn" ident "(" params? ")" "=" expr
params       := ident ("," ident)*
stmt         := let_stmt | if_stmt | expr_stmt
let_stmt     := "let" ident "=" expr
if_stmt      := "if" expr block ("else" "if" expr block)* "else" block
expr_stmt    := expr
block        := "{" stmt* "}"

expr         := or_expr
or_expr      := and_expr ("or" and_expr)*
and_expr     := cmp_expr ("and" cmp_expr)*
cmp_expr     := prefix (postfix | infix expr)*
prefix       := number
             | "true"
             | "false"
             | "na"
             | ident
             | interval "." market_field
             | "-" expr
             | "!" expr
             | "(" expr ")"

postfix      := "(" args? ")"
             | "[" expr "]"

args         := expr ("," expr)*
interval     := "1s" | "1m" | "3m" | "5m" | "15m" | "30m"
             | "1h" | "2h" | "4h" | "6h" | "8h" | "12h"
             | "1d" | "3d" | "1w" | "1M"
market_field := "open" | "high" | "low" | "close" | "volume" | "time"
```

## Operator Precedence

From lowest to highest:

1. logical `or`
2. logical `and`
3. equality and comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`
4. addition and subtraction: `+`, `-`
5. multiplication: `*`
6. unary operators: `-`, `!`
7. postfix call and indexing: `f(...)`, `x[...]`

Associativity:

- infix operators are left-associative

Examples:

```tradelang
1 + 2 * 3
true or false and false
close > sma(close, 14)
!(close > close[1])
```

## Types

TradeLang currently uses these internal source-level types:

- `f64`
- `bool`
- `series<f64>`
- `series<bool>`
- `void`
- `na`

Type annotations are not part of the language yet. Types are inferred by the
compiler.

## Values

### Numeric Values

Numbers are `f64`.

### Boolean Values

Booleans are `true` and `false`.

### `na`

`na` is the missing-value sentinel.

It is used when:

- there is insufficient series history
- an indicator is not seeded yet
- an operation propagates a missing input

Example:

```tradelang
plot(na)
```

## Market Series Access

The following identifiers are predefined and available in every script:

- `open`
- `high`
- `low`
- `close`
- `volume`
- `time`

These are series values, not ordinary functions.

TradeLang also supports interval-qualified market series:

- `1s.close`
- `4h.high`
- `1w.volume`
- `1M.time`

Rules:

- exactly one `interval <...>` directive is required per strategy
- every non-base interval reference must be declared with `use <...>`
- interval-qualified market series are `series<f64>`
- the visible value is the last fully closed candle for that interval
- if no candle for that interval has closed yet, the value is `na`
- lower-than-base interval references are rejected when the runtime binds feeds

This is valid:

```tradelang
interval 1m
plot(close[1])
```

This is invalid:

```tradelang
close()
```

Multi-interval example:

```tradelang
interval 1d
use 1w

let weekly_basis = ema(1w.close, 8)

if close > weekly_basis {
    plot(1)
} else {
    plot(0)
}
```

## Variables and Scope

### `let` Bindings

Variables are introduced with `let`:

```tradelang
let x = close
plot(x)
```

### Scope Rules

- top-level `let` bindings live for the whole program
- top-level `export` and `trigger` bindings live for the whole program and are
  available to later top-level statements
- top-level `fn` declarations live in the callable namespace
- each `if` branch introduces an inner block scope
- inner scopes may shadow outer bindings
- duplicate bindings in the same scope are rejected
- function parameters shadow predefined series of the same name
- host-provided external inputs are available in root scope as predefined
  series identifiers

### Function Scope Rules

User-defined functions are intentionally restricted in v1:

- functions are top-level only
- functions are expression-bodied
- functions may reference parameters, predefined series, external inputs,
  builtins, and other user-defined functions
- functions may reference interval-qualified market series
- functions may not capture `let` bindings from the program or from blocks
- duplicate function names are rejected
- duplicate parameter names are rejected
- function names may not collide with builtin names

Valid shadowing example:

```tradelang
let x = close
if close > close[1] {
    let x = close[1]
    plot(x)
} else {
    plot(x)
}
```

### Assignment

Only `let` bindings exist today.

Reassignment is not supported.

## Expressions

### Literals

```tradelang
1
3.14
true
false
na
```

### Unary Operators

Supported unary operators:

- `-expr`
- `!expr`

Type rules:

- unary `-` requires numeric input
- unary `!` requires boolean input
- `na` propagates through both operators

### Binary Operators

Supported binary operators:

- logical: `and`, `or`
- arithmetic: `+`, `-`, `*`
- equality: `==`, `!=`
- comparisons: `<`, `<=`, `>`, `>=`

Type rules:

- logical operators require boolean, series boolean, or `na` operands
- arithmetic requires numeric operands
- comparisons require numeric operands
- equality works on currently materialized runtime values

`na` propagation:

- logical operators use explicit three-valued truth tables
- if either operand is `na`, arithmetic and comparisons yield `na`

Logical truth tables:

### `and`

| lhs \\ rhs | true | false | na |
| --- | --- | --- | --- |
| true | true | false | na |
| false | false | false | false |
| na | na | false | na |

### `or`

| lhs \\ rhs | true | false | na |
| --- | --- | --- | --- |
| true | true | true | true |
| false | true | false | na |
| na | true | na | na |

### Division

The parser and VM have bytecode support for division, but the lexer does not
currently tokenize `/` as an operator.

That means:

- `// comment` works
- `a / b` currently produces a lex error

Treat division as not yet available in the source language.

## Conditionals

The only conditional form currently supported is:

```tradelang
if condition {
    ...
} else if other_condition {
    ...
} else {
    ...
}
```

Important rules:

- `else if` chains are supported
- `else` is mandatory
- the condition must type-check as `bool`, `series<bool>`, or `na`
- at runtime, `false` and `na` are both treated as falsey

`else if` is syntax sugar for a nested `if` in the false branch.

## Function Calls and Builtins

Only identifiers may be called.

Callable identifiers resolve in this order:

1. builtins
2. user-defined functions

Current callable builtins:

- `plot`
- `sma`
- `ema`
- `rsi`

Calling any other identifier is an error.

## User-Defined Functions

TradeLang supports top-level, pure, expression-bodied helper functions.

Syntax:

```tradelang
fn name(param1, param2, ...) = expr
```

Examples:

```tradelang
fn bullish_bar() = close > open
fn rising(series) = series > series[1]

if bullish_bar() and rising(close) {
    plot(1)
} else {
    plot(0)
}
```

```tradelang
fn crossover(a, b) = a > b and a[1] <= b[1]
fn crossunder(a, b) = a < b and a[1] >= b[1]

let fast = ema(close, 9)
let slow = ema(close, 21)

if crossover(fast, slow) {
    plot(1)
} else if crossunder(fast, slow) {
    plot(-1)
} else {
    plot(0)
}
```

Rules:

- functions are analyzed per call signature and inlined at compile time
- call signatures distinguish both argument types and source update clocks
- arguments are evaluated left-to-right exactly once
- valid return categories are the existing non-`void` value kinds plus `na`
- `plot(...)` is not allowed inside function bodies
- recursion and mutually recursive function graphs are rejected
- forward references between top-level functions are allowed

## Builtin Reference

### `plot(value)`

Plots the current numeric value for the current bar.

Signature:

```text
plot(number_or_series_number) -> void
```

Rules:

- requires exactly one argument
- argument must be numeric, series numeric, or `na`
- `plot(na)` produces a plot point with `null` value in serialized output

Current limitation:

- the current implementation materializes a single output plot series

Example:

```tradelang
plot(close)
```

### `sma(series, length)`

Simple moving average.

Signature:

```text
sma(series<f64>, positive_integer_literal) -> series<f64>
```

Rules:

- requires exactly two arguments
- first argument must be a numeric series
- second argument must be a non-negative integer literal greater than zero

Semantics:

- returns `na` until at least `length` bars are available
- returns `na` if any value in the active window is `na`
- otherwise returns the arithmetic mean of the most recent `length` values

Example:

```tradelang
plot(sma(close, 14))
```

### `ema(series, length)`

Exponential moving average.

Signature:

```text
ema(series<f64>, positive_integer_literal) -> series<f64>
```

Rules:

- requires exactly two arguments
- first argument must be a numeric series
- second argument must be a non-negative integer literal greater than zero

Semantics:

- returns `na` until enough history exists to seed the EMA
- seeds from the simple moving average of the initial window
- then updates incrementally with `alpha = 2 / (length + 1)`
- returns `na` if the current source value is `na`

Example:

```tradelang
plot(ema(close, 9))
```

### `rsi(series, length)`

Relative strength index.

Signature:

```text
rsi(series<f64>, positive_integer_literal) -> series<f64>
```

Rules:

- requires exactly two arguments
- first argument must be a numeric series
- second argument must be a non-negative integer literal greater than zero

Semantics:

- returns `na` until enough deltas have been accumulated
- uses rolling average gain and rolling average loss
- returns `100.0` when average loss is zero
- returns `na` if the current source value is `na`

Example:

```tradelang
plot(rsi(close, 14))
```

## Series Semantics

Series are time-indexed values.

Access rules:

- `x[0]` means the current bar
- `x[1]` means the previous bar
- `x[n]` means `n` bars ago

Indexing rules:

- only series values may be indexed
- the index must be a non-negative integer literal
- if there is insufficient history, the result is `na`

For interval-qualified market series, indexing composes on that interval's own
closed-candle clock. For example:

- `1w.close[0]` is the latest fully closed weekly close
- `1w.close[1]` is the previous fully closed weekly close
- `ema(1w.close, 5)[1]` is the previous committed weekly EMA value

Examples:

```tradelang
plot(close[0])
plot(close[1])
plot(close[10])
```

Invalid examples:

```tradelang
plot(close[-1])

let n = 5
plot(close[n])
```

## Type and Runtime Semantics

### `na` Behavior

Current runtime behavior:

- unary `-na` yields `na`
- unary `!na` yields `na`
- `true and na` yields `na`
- `false and na` yields `false`
- `true or na` yields `true`
- `false or na` yields `na`
- `na` in arithmetic yields `na`
- `na` in comparisons yields `na`
- `if na { ... } else { ... }` takes the `else` branch
- plotting `na` yields a point with `null`

Function calls do not introduce any special `na` semantics. A user-defined
function behaves as though its body expression were inserted at the call site.
This means `na` propagation is exactly the propagation already defined for the
operators, indexing, and builtins used inside the function body.

Multi-interval feeds also introduce these runtime `na` cases:

- before the first fully closed candle exists for a referenced interval
- when a referenced feed has a missing interior candle and the runtime
  synthesizes an `na` step for that closed interval boundary

### Equality

Equality compares the current runtime values being operated on.

Current runtime cases implemented directly:

- numeric equality
- boolean equality

### Series Values at Runtime

At the source level, indicators and market data behave as series values.

At runtime, the VM operates on:

- the current value for expression evaluation
- bounded history buffers for indexing and indicator windows

This is why expressions such as the following work as expected:

```tradelang
if close > sma(close, 14) {
    plot(1)
} else {
    plot(0)
}
```

For interval-qualified series, history advances only when that interval closes.
Derived series advance when one of their source interval clocks advances.

## Runtime Limits

The runtime enforces limits through `VmLimits`.

Default limits:

- max instructions per bar: `10_000`
- max history capacity: `1_024`

The compiler computes required history from:

- series indexing offsets
- indicator window lengths

History is tracked per series slot. If any slot requires more than
`max_history_capacity`, engine construction fails.

## Outputs

Running a script produces:

- `plots`
- `exports`
- `triggers`
- `trigger_events`
- `alerts`

Current state:

- plots are implemented
- exports record one typed sample per bar
- triggers record one typed sample per bar and emit a discrete event when the
  sample is `true`
- alerts are present in output types but no alert builtin exists yet

Each plot point contains:

- `bar_index`
- `time`
- `value`

Each export/trigger sample contains:

- `name`
- `bar_index`
- `time`
- `value`

Examples of generated output are available under `examples/`.

The official CLI can also execute scripts directly through CSV mode. This is
the only `run` mode today. In CSV mode, one raw `--bars` file is loaded and
rolled up automatically to the strategy's declared base and supplemental
intervals when possible. Manual per-interval `--feed` wiring is no longer part
of the CLI:

```bash
tradelang check strategy.trl
tradelang run csv strategy.trl --bars bars.csv
tradelang dump-bytecode strategy.trl
```

For editor authoring, the repository now also ships `tradelang-lsp` plus a VS
Code extension under `editors/vscode/`. The editor stack reuses the same parser
and compiler diagnostics as the CLI, so syntax and type errors are reported
before a strategy is run.

For composed strategies, editor-only compile environments are configured with a
workspace `.tradelang.json` file:

```json
{
  "version": 1,
  "documents": {
    "strategies/consumer.trl": {
      "compile_environment": {
        "external_inputs": [
          {
            "name": "trend",
            "ty": "SeriesBool",
            "kind": "ExportSeries"
          }
        ]
      }
    }
  }
}
```

## Diagnostics

The compiler reports diagnostics for:

- lexical errors
- parse errors
- type errors
- compile-time lowering errors

Examples of current compile errors:

- missing `else`
- non-literal series indexes
- invalid builtin arity
- calling unknown functions
- treating market data identifiers as callable functions
- illegal function-body captures
- recursive function definitions
- `plot(...)` inside a function body
- invalid interval literals such as `1W`
- invalid interval-qualified market fields such as `1w.foo`
- missing `interval <...>` directives
- referenced intervals that are not declared with `use <...>`

The runtime reports errors for:

- stack underflow
- type mismatch
- invalid jump targets
- invalid local or series slots
- instruction budget exhaustion
- missing or unexpected interval feeds
- missing or wrongly typed external inputs
- lower-than-base interval references
- misaligned or unsorted interval feeds
- history requirements that exceed `VmLimits.max_history_capacity`
- invalid pipeline wiring, cycles, and interval mismatches

## Unsupported or Not Yet Implemented

The following are not part of the language yet:

- strings
- arrays
- structs
- loops
- reassignment
- alert-producing builtins
- imports or modules
- source-level division with `/`
- block-bodied functions
- recursion
- closures or captured local bindings
- lower-than-base interval references
- cross-interval strategy composition between different base intervals

## Examples

See the runnable examples in `examples/`:

- `cargo run --example sma`
- `cargo run --example pipeline`
- `cargo run --example rsi`
- `cargo run --example step_engine`

Function examples:

```tradelang
fn pullback_to(avg, price) = price > avg and price[1] <= avg[1]

let basis = ema(close, 10)

if pullback_to(basis, close) {
    plot(1)
} else {
    plot(na)
}
```

Multi-interval examples:

```tradelang
let weekly_basis = ema(1w.close, 8)

if close > weekly_basis and 1d.volume > 1d.volume[1] {
    plot(1)
} else {
    plot(0)
}
```

## Maintenance Rule

This file is a reference for implemented behavior.

If the parser, compiler, builtins, or VM semantics change, update this document
in the same change so it stays aligned with the code.
