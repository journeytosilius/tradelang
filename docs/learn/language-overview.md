# Language Overview

PalmScript strategies are top-level source files made of declarations and statements.

Common building blocks:

- `interval <...>` for the base execution clock
- `source` declarations for exchange-backed markets
- optional supplemental `use <alias> <interval>` declarations for higher or equal intervals
- top-level functions
- `let`, `const`, `input`, tuple destructuring, `export`, `trigger`, `entry` / `exit`, and `order`
- `if / else if / else`
- expressions built from operators, calls, and indexing
- helper builtins such as `crossover`, `activated`, `barssince`, and `valuewhen`
- typed `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>`, and `exit_kind.<variant>` enum literals

## Script Shape

Executable PalmScript scripts name exchange-backed markets explicitly:

```palmscript
interval 1m
source bn = binance.spot("BTCUSDT")
source hl = hyperliquid.perps("BTC")

plot(bn.close - hl.close)
```

## Mental Model

- the script always has exactly one base interval
- every executable script declares at least one `source`
- market series are always source-qualified
- series values evolve over time
- higher intervals update only when those candles fully close
- missing history or missing aligned source data appears as `na`
- `plot`, `export`, `trigger`, and first-class strategy signals emit results after each execution step

## Where To Go For Exact Rules

- syntax and tokens: [Lexical Structure](../reference/lexical-structure.md) and [Grammar](../reference/grammar.md)
- declarations and visibility: [Declarations and Scope](../reference/declarations-and-scope.md)
- expressions and runtime meaning: [Evaluation Semantics](../reference/evaluation-semantics.md)
- market series rules: [Intervals and Sources](../reference/intervals-and-sources.md)
- indicators and helper builtins: [Indicators](../reference/indicators.md) and [Builtins](../reference/builtins.md)
- outputs: [Outputs](../reference/outputs.md)
