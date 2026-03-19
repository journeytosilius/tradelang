# Language Overview

PalmScript scripts are top-level source files made of declarations and statements.

Common building blocks:

- `interval <...>` for the base execution clock
- `source` declarations for market-backed series
- separate `execution` declarations for order-routing targets
- optional supplemental `use <alias> <interval>` declarations
- top-level functions
- `let`, `const`, `input`, tuple destructuring, `export`, `regime`, `trigger`, `entry` / `exit`, and `order`
- compile-time portfolio declarations such as `max_positions = 2` and `portfolio_group "majors" = [left, right]`
- optional `input ... optimize(...)` metadata for optimizer search-space inference
- declarative backtest controls such as `cooldown long = 12` and `max_bars_in_trade short = 48`
- `if / else if / else`
- expressions built from operators, calls, and indexing
- helper builtins such as `crossover`, `state`, `activated`, `barssince`, and `valuewhen`
- typed `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>`, and `exit_kind.<variant>` enum literals

## Script Shape

Executable PalmScript scripts name data sources explicitly:

```palmscript
interval 1m
source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")
execution exec = bybit.usdt_perps("BTCUSDT")

plot(bn.close - bb.close)
```

## Mental Model

- every script has one base interval
- executable scripts declare one or more `source` bindings
- scripts may also declare one or more `execution` bindings for order routing without exposing new market series
- market series are always source-qualified
- series values evolve over time
- higher intervals update only when those candles fully close
- missing history or missing aligned source data appears as `na`
- `plot`, `export`, `regime`, `trigger`, and strategy declarations emit results after each execution step
- optimizer-aware `input` declarations can carry bounded integer, float, or choice search metadata without changing runtime semantics
- `cooldown` and `max_bars_in_trade` are compile-time bar-count declarations that make re-entry and time-based exits explicit in the script
- portfolio declarations are compile-time only and become active when backtest-oriented CLI commands receive multiple `--execution-source` aliases
- order declarations can target a declared execution binding with named arguments such as `venue = exec` or `venue = current_execution()`
- portfolio mode shares one equity ledger across the selected execution aliases by default and blocks only the new entries that would exceed the configured position-count or exposure caps
- `--spot-virtual-rebalance` switches multi-venue spot portfolio runs to long/flat per-alias quote-wallet accounting with automatic quote transfers before long entries

## Where To Go For Exact Rules

- syntax and tokens: [Lexical Structure](../reference/lexical-structure.md) and [Grammar](../reference/grammar.md)
- declarations and visibility: [Declarations and Scope](../reference/declarations-and-scope.md)
- expressions and semantics: [Evaluation Semantics](../reference/evaluation-semantics.md)
- market series rules: [Intervals and Sources](../reference/intervals-and-sources.md)
- indicators and helper builtins: [Indicators](../reference/indicators.md) and [Builtins](../reference/builtins.md)
- outputs: [Outputs](../reference/outputs.md)
