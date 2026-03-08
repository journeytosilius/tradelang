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
- `protect`
- `target`

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
- both forms are scalar-only in v1: `float`, `bool`, `ma_type`, `tif`, `trigger_ref`, `position_side`, `exit_kind`, or `na`
- `input` is compile-time only in v1 and does not yet accept CLI overrides
- `input` values must be scalar literals or enum literals
- `const` values may reference previously declared `const` / `input` bindings and pure scalar builtins
- windowed builtins and series indexing accept immutable numeric bindings anywhere an integer literal is required

## Outputs

`export`, `trigger`, first-class strategy signals, and order-facing backtest declarations are top-level only:

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
trigger long_entry = spot.close > spot.high[1]
entry1 long = spot.close > spot.high[1]
entry2 long = crossover(spot.close, ema(spot.close, 20))
order entry1 long = limit(spot.close[1], tif.gtc, false)
protect long = stop_market(position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref.last)
protect_after_target1 long = stop_market(position.entry_price, trigger_ref.last)
target1 long = take_profit_market(position.entry_price + 4, trigger_ref.last)
target2 long = take_profit_market(position.entry_price + 8, trigger_ref.last)
size entry1 long = 0.5
size entry2 long = 0.5
size entry3 long = risk_pct(0.01, stop_price)
size target1 long = 0.5
```

Rules:

- all forms are top-level only
- duplicate names in the same scope are rejected
- `trigger` names become bindings after the declaration point
- `entry long` and `entry short` are compatibility aliases for `entry1 long` and `entry1 short`
- `entry1`, `entry2`, and `entry3` are staged backtest entry signal declarations
- `exit long` and `exit short` remain single discretionary full-position exits
- `order entry ...` and `order exit ...` attach an execution template to a matching signal role
- `protect`, `protect_after_target1..3`, and `target1..3` declare staged attached exits that arm only while the matching position is open
- `size entry1..3 long|short` optionally size a staged entry fill with either `capital_fraction(x)` / legacy bare numeric fraction semantics, or `risk_pct(pct, stop_price)` for risk-based entry sizing
- `size target1..3 long|short` optionally size a staged `target` fill as a fraction of the open position
- at most one `order` declaration is allowed per signal role
- at most one declaration is allowed per staged role
- if a signal role has no explicit `order` declaration, the backtester uses an implicit `market()` order
- `size entry ...` and `size target ...` each require a matching staged `order ...` or staged attached `target ...` declaration for the same role
- `risk_pct(...)` is only valid on staged entry size declarations in v1
- staged attached exits are sequential: only the next target stage and the current protect stage are active at once
- `position.*` is only available inside `protect` and `target` declarations
- `position_event.*` is available anywhere a `series<bool>` is valid and is intended to anchor logic to actual backtest fills
- current `position_event` fields are:
  `long_entry_fill`, `short_entry_fill`, `long_exit_fill`, `short_exit_fill`,
  `long_protect_fill`, `short_protect_fill`, `long_target_fill`, `short_target_fill`,
  `long_signal_exit_fill`, `short_signal_exit_fill`, `long_reversal_exit_fill`,
  `short_reversal_exit_fill`, `long_liquidation_fill`, and `short_liquidation_fill`
- staged entry and target fill fields are also available:
  `long_entry1_fill` .. `long_entry3_fill`, `short_entry1_fill` .. `short_entry3_fill`,
  `long_target1_fill` .. `long_target3_fill`, and `short_target1_fill` .. `short_target3_fill`
- `last_exit.*`, `last_long_exit.*`, and `last_short_exit.*` are available anywhere ordinary expressions are valid
- current `last_*_exit` fields are `kind`, `stage`, `side`, `price`, `time`, `bar_index`, `realized_pnl`, `realized_return`, and `bars_held`
- `last_*_exit.kind` includes `exit_kind.liquidation` in addition to the existing exit kinds
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
