# Declarations and Scope

This page defines the binding forms that PalmScript accepts and the visibility rules attached to them.

## Top-Level-Only Forms

The following forms must appear only at the top level of a script:

- `interval`
- `source`
- `use`
- `fn`
- `const`
- `input`
- `export`
- `trigger`
- `entry`
- `exit`

Top-level `let`, `if`, and expression statements are allowed.

## Base Interval

Every script must declare exactly one base interval:

```palmscript
interval 1m
```

The compiler rejects a script with no base `interval` or with more than one base `interval`.

## Source Declarations

A source declaration has this form:

```palmscript
source hl = hyperliquid.perps("BTC")
```

Rules:

- the alias must be an identifier
- the alias must be unique across all declared sources
- the template must resolve to one of the supported source templates
- the symbol argument must be a string literal

## `use` Declarations

Supplemental intervals are declared per source:

```palmscript
use hl 1h
```

Rules:

- the alias must name a declared source
- the interval must not be lower than the base interval
- duplicate `use <alias> <interval>` declarations are rejected
- an interval equal to the base interval is accepted but redundant

## Functions

User-defined functions are top-level, expression-bodied declarations:

```palmscript
fn cross_signal(a, b) = a > b and a[1] <= b[1]
```

Rules:

- function names must be unique
- a function name must not collide with a builtin name
- parameter names within one function must be unique
- recursive and cyclic function graphs are rejected
- function bodies may reference their parameters, declared source series, and top-level immutable `const` / `input` bindings
- function bodies must not call `plot`
- function bodies must not capture `let` bindings from surrounding statement scopes

Functions are specialized by argument type and update clock.

## `let` Bindings

`let` creates a binding in the current block scope:

```palmscript
let basis = ema(spot.close, 20)
```

Rules:

- a duplicate `let` in the same scope is rejected
- inner scopes may shadow outer bindings
- the bound value may be scalar or series
- `na` is permitted and is treated as a numeric-like placeholder during compilation

PalmScript also supports tuple destructuring for immediate tuple-valued builtin results:

```palmscript
let (line, signal, hist) = macd(spot.close, 12, 26, 9)
```

Additional rules:

- tuple destructuring is a first-class `let` form
- the right-hand side must currently be an immediate tuple-valued builtin result
- tuple arity must match exactly
- tuple-valued expressions must be destructured before further use

## `const` And `input`

PalmScript supports top-level immutable bindings for strategy configuration:

```palmscript
input fast_len = 21
const neutral_rsi = 50
```

Rules:

- both forms are top-level only
- duplicate names in the same scope are rejected
- both forms are scalar-only in v1: `float`, `bool`, `ma_type`, or `na`
- `input` is compile-time only in v1 and does not yet accept CLI overrides
- `input` values must be scalar literals or enum literals
- `const` values may reference previously declared `const` / `input` bindings and pure scalar builtins
- windowed builtins and series indexing accept immutable numeric bindings anywhere an integer literal is required

## Outputs

`export`, `trigger`, and first-class strategy signals create top-level outputs:

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
trigger long_entry = spot.close > spot.high[1]
entry long = spot.close > spot.high[1]
```

Rules:

- all forms are top-level only
- duplicate names in the same scope are rejected
- `trigger` names become bindings after the declaration point
- `entry long`, `exit long`, `entry short`, and `exit short` are first-class backtest signal declarations
- legacy `trigger long_entry = ...` style scripts remain supported as a compatibility bridge when no first-class signal declarations are present

## Conditional Scope

`if` introduces two child scopes:

```palmscript
if spot.close > spot.open {
    let x = 1
} else {
    let x = 0
}
```

Rules:

- the condition must evaluate to `bool`, `series<bool>`, or `na`
- both branches are scoped independently
- bindings created inside one branch are not visible outside the `if`
