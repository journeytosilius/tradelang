# TradeLang Language Reference

This document describes the TradeLang language as implemented in this
repository today.

It is intended to be the working reference for both humans and agents. If the
implementation changes, this file should be updated to match the actual parser,
compiler, and runtime behavior.

## Status

TradeLang is currently a small deterministic DSL for financial time-series
programs compiled to bytecode and executed by a VM.

The implemented language supports:

- numeric, boolean, and `na` literals
- `let` bindings
- `if`, `else if`, and `else` statements
- logical `and` and `or` expressions
- arithmetic and comparison expressions
- unary negation and boolean negation
- series indexing with literal lookback offsets
- builtins: `sma`, `ema`, `rsi`, `plot`
- predefined market data series: `open`, `high`, `low`, `close`, `volume`,
  `time`

## Execution Model

TradeLang scripts are compiled once and then executed once per bar.

At each bar:

1. market data values are loaded into predefined series
2. the compiled bytecode runs
3. current values may be stored into bounded series buffers
4. `plot(...)` outputs are emitted for the current bar

TradeLang is deterministic:

- no IO
- no filesystem access
- no network access
- no wall clock access
- no randomness

## Source File Structure

A program is a sequence of statements.

Supported statement forms:

- `let name = expr`
- `if condition { ... } else if other_condition { ... } else { ... }`
- expression statements such as `plot(close)`

Example:

```tradelang
let fast = ema(close, 5)
let slow = sma(close, 10)

if fast > slow {
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

- `let`
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

## Grammar Sketch

This is a practical sketch of the implemented grammar, not a formal grammar.

```text
program      := stmt*
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
             | "-" expr
             | "!" expr
             | "(" expr ")"

postfix      := "(" args? ")"
             | "[" expr "]"

args         := expr ("," expr)*
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

## Predefined Series

The following identifiers are predefined and available in every script:

- `open`
- `high`
- `low`
- `close`
- `volume`
- `time`

These are series values, not ordinary functions.

This is valid:

```tradelang
plot(close[1])
```

This is invalid:

```tradelang
close()
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
- each `if` branch introduces an inner block scope
- inner scopes may shadow outer bindings
- duplicate bindings in the same scope are rejected

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

Current callable builtins:

- `plot`
- `sma`
- `ema`
- `rsi`

Calling any other identifier is an error.

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

## Runtime Limits

The runtime enforces limits through `VmLimits`.

Default limits:

- max instructions per bar: `10_000`
- max history capacity: `1_024`

The compiler computes required history from:

- series indexing offsets
- indicator window lengths

At runtime, requested history is capped by `max_history_capacity`.

## Outputs

Running a script produces:

- `plots`
- `alerts`

Current state:

- plots are implemented
- alerts are present in output types but no alert builtin exists yet

Each plot point contains:

- `bar_index`
- `time`
- `value`

Examples of generated output are available under `examples/`.

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

The runtime reports errors for:

- stack underflow
- type mismatch
- invalid jump targets
- invalid local or series slots
- instruction budget exhaustion

## Unsupported or Not Yet Implemented

The following are not part of the language yet:

- strings
- arrays
- structs
- user-defined functions
- loops
- reassignment
- alert-producing builtins
- imports or modules
- multi-plot output materialization
- source-level division with `/`

## Examples

See the runnable examples in `examples/`:

- `cargo run --example sma`
- `cargo run --example rsi`
- `cargo run --example step_engine`

## Maintenance Rule

This file is a reference for implemented behavior.

If the parser, compiler, builtins, or VM semantics change, update this document
in the same change so it stays aligned with the code.
