# Language Overview

PalmScript scripts are top-level source files made of declarations and statements.

Common building blocks:

- `interval <...>` for the base execution clock
- `source` declarations for market-backed series
- optional supplemental `use <alias> <interval>` declarations
- top-level functions
- `let`, `const`, `input`, tuple destructuring, `export`, `regime`, `trigger`, `entry` / `exit`, and `order`
- optional `input ... optimize(...)` metadata for optimizer search-space inference
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

plot(bn.close - bb.close)
```

## Mental Model

- every script has one base interval
- executable scripts declare one or more `source` bindings
- market series are always source-qualified
- series values evolve over time
- higher intervals update only when those candles fully close
- missing history or missing aligned source data appears as `na`
- `plot`, `export`, `regime`, `trigger`, and strategy declarations emit results after each execution step
- optimizer-aware `input` declarations can carry bounded integer, float, or choice search metadata without changing runtime semantics

## Where To Go For Exact Rules

- syntax and tokens: [Lexical Structure](../reference/lexical-structure.md) and [Grammar](../reference/grammar.md)
- declarations and visibility: [Declarations and Scope](../reference/declarations-and-scope.md)
- expressions and semantics: [Evaluation Semantics](../reference/evaluation-semantics.md)
- market series rules: [Intervals and Sources](../reference/intervals-and-sources.md)
- indicators and helper builtins: [Indicators](../reference/indicators.md) and [Builtins](../reference/builtins.md)
- outputs: [Outputs](../reference/outputs.md)
