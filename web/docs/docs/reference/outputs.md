# Outputs

This page defines the user-visible output forms in PalmScript.

## Output Forms

PalmScript exposes three output-producing constructs:

- `plot(value)`
- `export name = expr`
- `regime name = expr`
- `trigger name = expr`
- `module name = entry long|short|entry2|entry3 long|short`
- `entry long = expr`, `entry1 long = expr`, `entry2 long = expr`, `entry3 long = expr`
- `entry short = expr`, `entry1 short = expr`, `entry2 short = expr`, `entry3 short = expr`
- `exit long = expr`, `exit short = expr`
- `protect long = order_spec`, `protect short = order_spec`
- `protect_after_target1 long = order_spec`, `protect_after_target2 long = order_spec`, `protect_after_target3 long = order_spec`
- `protect_after_target1 short = order_spec`, `protect_after_target2 short = order_spec`, `protect_after_target3 short = order_spec`
- `target long = order_spec`, `target1 long = order_spec`, `target2 long = order_spec`, `target3 long = order_spec`
- `target short = order_spec`, `target1 short = order_spec`, `target2 short = order_spec`, `target3 short = order_spec`
- `size entry long = expr`, `size entry1 long = expr`, `size entry2 long = expr`, `size entry3 long = expr`
- `size entry short = expr`, `size entry1 short = expr`, `size entry2 short = expr`, `size entry3 short = expr`
- `size target long = expr`, `size target1 long = expr`, `size target2 long = expr`, `size target3 long = expr`
- `size target short = expr`, `size target1 short = expr`, `size target2 short = expr`, `size target3 short = expr`

`plot` is a builtin call. `export`, `regime`, and `trigger` are declarations.

## `plot`

`plot` emits a plot point for the current step.

Rules:

- the argument must be numeric, `series<float>`, or `na`
- the current step contributes one plot point per executed `plot` call
- `plot` does not create a reusable language binding
- `plot` is not allowed inside user-defined function bodies

## `export`

`export` publishes a named output series:

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
```

Rules:

- it is top-level only
- the name must be unique within the current scope
- the expression may evaluate to numeric, bool, series numeric, series bool, or `na`
- `void` is rejected

Type normalization:

- numeric, series numeric, and `na` exports become `series<float>`
- bool and series bool exports become `series<bool>`

## `regime`

`regime` publishes a named persistent boolean market-state series:

```palmscript
regime trend_long = state(
    ema(spot.close, 20) > ema(spot.close, 50),
    ema(spot.close, 20) < ema(spot.close, 50)
)
```

Rules:

- it is top-level only
- the expression must evaluate to `bool`, `series<bool>`, or `na`
- the output type is always `series<bool>`
- `regime` names become reusable bindings after the declaration point
- `regime` is intended to pair with `state(...)`, `activated(...)`, and `deactivated(...)`
- runtime diagnostics record it with ordinary exported series output

## `trigger`

`trigger` publishes a named boolean output series:

```palmscript
trigger breakout = spot.close > spot.high[1]
```

Rules:

- it is top-level only
- the expression must evaluate to `bool`, `series<bool>`, or `na`
- the output type is always `series<bool>`

Runtime event rule:

- a trigger event is emitted for a step only when the current trigger sample is `true`
- `false` and `na` do not emit trigger events

## First-Class Strategy Signals

PalmScript exposes first-class strategy signal declarations for strategy-oriented execution:

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
entry short = spot.close < spot.low[1]
exit short = spot.close > ema(spot.close, 20)
```

Rules:

- the four declarations are top-level only
- each expression must evaluate to `bool`, `series<bool>`, or `na`
- they compile to trigger outputs with explicit signal-role metadata
- runtime event emission follows the same `true`/`false`/`na` rules as ordinary triggers
- `entry long` and `entry short` are compatibility aliases for `entry1 long` and `entry1 short`
- `entry2` and `entry3` are sequential same-side add-on signals that only become eligible after the previous stage has filled in the current position cycle

## Entry Module Labels

PalmScript can also label entry roles for attribution in research diagnostics:

```palmscript
module breakout = entry long
module pullback = entry2 long
```

Rules:

- module declarations are top-level only
- they currently bind only to `entry`, `entry2`, or `entry3` roles
- each entry role may have at most one module label
- backtest-oriented diagnostics expose the label as `entry_module` on trades and in cohort summaries

## Order Declarations

PalmScript also exposes top-level order declarations that parameterize how a signal role is executed:

```palmscript
execution exec = binance.spot("BTCUSDT")
order_template maker_entry = limit(price = spot.close[1], tif = tif.gtc, post_only = false, venue = exec)
order_template stop_exit = stop_market(trigger_price = lowest(spot.low, 5)[1], trigger_ref = trigger_ref.last, venue = exec)
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)

order entry long = maker_entry
order exit long = stop_exit
```

Rules:

- order declarations are top-level only
- `order_template` declarations are top-level only and define reusable named order specs
- there may be at most one `order` declaration per signal role
- any script that declares trading signal roles requires an explicit `order ...` declaration for every declared `entry` / `exit` signal role
- `order ... = <template_name>` reuses a previously declared `order_template`
- templates may reference another template, but cyclic references are rejected
- `execution` declarations are top-level venue bindings that keep execution routing separate from market-data `source` declarations
- order constructors support the legacy positional form and the named-argument form, and inline constructor calls remain valid anywhere `order_spec` is accepted
- named order arguments may not be mixed with positional arguments in the same constructor call
- `venue = <execution_alias>` binds that order role to a declared execution alias
- numeric order fields such as `price`, `trigger_price`, and `expire_time_ms` are evaluated by the runtime as hidden internal series
- `tif.<variant>` and `trigger_ref.<variant>` are typed enum literals checked at compile time
- venue-specific compatibility checks run when the backtest starts, based on the selected execution target
- any script that declares trading signal roles requires at least one declared `execution` target

## Attached Exits

PalmScript also exposes first-class attached exits that keep the discretionary `exit` signal free:

```palmscript
execution exec = binance.spot("BTCUSDT")
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
order entry long = market(venue = exec)
order exit long = market(venue = exec)
protect long = stop_market(trigger_price = position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref = trigger_ref.last, venue = exec)
target long = take_profit_market(
    trigger_price = highest_since(position_event.long_entry_fill, spot.high) + 4,
    trigger_ref = trigger_ref.last,
    venue = exec
)
size target long = 0.5
```

Rules:

- attached exits are top-level only
- `protect` is the base protection stage for a side
- `protect_after_target1`, `protect_after_target2`, and `protect_after_target3` optionally ratchet the active protect order after each staged target fill
- `target`, `target1`, `target2`, and `target3` are sequential attached profit-taking stages; `target` is a compatibility alias for `target1`
- `size entry1..3` and `size target1..3` are optional per stage and only apply to the matching staged entry or target
- staged entry sizing supports:
  - a legacy bare numeric fraction such as `0.5`
  - `capital_fraction(x)`
  - `risk_pct(pct, stop_price)`
- `capital_fraction(...)` values must evaluate to a finite fraction in `(0, 1]`
- an entry size fraction below `1` leaves cash available for later same-side scale-ins on later staged entries
- `risk_pct(...)` is entry-only in v1 and sizes from actual fill price and stop distance at fill time
- if a `risk_pct(...)` size wants more than current cash or free collateral can support, the backtester clamps the fill and records `capital_limited = true`
- they arm only after a matching entry fill exists
- they are reevaluated once per execution bar while that position remains open
- only the current staged protect and the next staged target are active at the same time
- when `target1` fills, the engine swaps from `protect` to `protect_after_target1` if declared, otherwise it inherits the most recent available protect stage
- staged target size fractions must evaluate to a finite fraction in `(0, 1]`
- a `size targetN ...` declaration turns the matching target stage into a partial take-profit when the fraction is below `1`
- staged targets are one-shot within a position cycle and activate sequentially
- if both become fillable on the same execution bar, `protect` wins deterministically
- `position.*` is available only inside `protect` and `target` declarations
- `position_event.*` is a backtest-driven series namespace that exposes actual fill events such as `position_event.long_entry_fill`
- `position_event.*` also exposes exit-kind-specific fill events such as `position_event.long_target_fill`, `position_event.long_protect_fill`, and `position_event.long_liquidation_fill`
- staged fill events are also available, including `position_event.long_entry1_fill`, `position_event.long_entry2_fill`, `position_event.long_entry3_fill`, `position_event.long_target1_fill`, `position_event.long_target2_fill`, and `position_event.long_target3_fill` with matching short-side fields
- `last_exit.*`, `last_long_exit.*`, and `last_short_exit.*` expose the most recent closed-trade snapshot globally or per side
- `last_*_exit.kind` is compared against typed enum literals such as `exit_kind.target` and `exit_kind.liquidation`
- `last_*_exit.stage` exposes the staged target/protect stage number when applicable
- outside backtests, `position_event.*` is defined but evaluates to `false` on every step
- outside backtests, `last_*_exit.*` is defined but evaluates to `na`

## Reserved Trading Trigger Names

Ordinary `trigger` declarations remain valid for alerting and non-trading
consumers, but the reserved trading trigger names are no longer executable
aliases.

Rules:

- `trigger long_entry = ...`, `trigger long_exit = ...`, `trigger short_entry = ...`, and `trigger short_exit = ...` are rejected for executable trading scripts
- use first-class `entry` / `exit` declarations plus matching `order ...` templates instead
- non-reserved trigger names such as `trigger breakout = ...` remain valid

## Runtime Output Collections

Over a full run, the runtime accumulates:

- `plots`
- `exports`
- `triggers`
- `order_fields`
- `trigger_events`
- `alerts`

`alerts` currently exist in the runtime output structures but are not produced by a first-class PalmScript language construct.

## Backtest And Optimize Diagnostics

Backtest-oriented commands also return typed diagnostics payloads alongside the raw outputs.

Always-on summary diagnostics include:

- order diagnostics
- trade diagnostics
- opportunity events
- export summaries
- cohort summaries
- drawdown duration and stagnation metrics
- annualized Sharpe ratio in backtest, walk-forward window, and stitched walk-forward summaries
- baseline comparisons against flat cash and execution-asset buy-and-hold
- source alignment diagnostics
- deterministic overfitting-risk summaries with typed risk reasons and scores
- deterministic improvement hints
- bounded date-perturbation reruns on top-level backtests

When `--diagnostics full-trace` is enabled, PalmScript also records one `per_bar_trace` record for each execution bar. Each trace includes:

- the execution `bar_index` and `time`
- the current position snapshot
- the current exported feature snapshot
- typed signal decisions
- typed order decisions

Walk-forward and optimize outputs reuse the same diagnostics model and add:

- per-segment drift flags
- typed validation-constraint summaries for stitched walk-forward results
- final holdout drift summaries
- optimizer robustness summaries across the top ranked candidates
- optimize holdout pass rate plus best-candidate and holdout constraint summaries
- parameter stability, baseline-comparison, overfitting-risk, and Sharpe summaries
- optimize validated / feasible / infeasible candidate counts
- an optional best-infeasible-candidate fallback summary when no validated candidate satisfies every enabled constraint
- typed constraint-failure breakdowns for the validated survivor set
- optional full-window direct-validation replays for the top feasible validated survivors, including stitched-vs-direct drift summaries

## Output Time And Bar Index

Each output sample is tagged with:

- the current `bar_index`
- the current step `time`

In source-aware runs, the step time is the open time of the current base-clock step.
