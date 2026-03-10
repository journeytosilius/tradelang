# PalmScript Documentation

PalmScript is a language for financial time-series strategies. This site focuses on the language itself: syntax, semantics, builtins, and code examples.

## Documentation Map

- `Learn` teaches the language through short examples and runnable workflows.
- `Reference` defines the accepted syntax and language semantics.

## Start Here

- New to PalmScript: [Learn Overview](learn/overview.md)
- Want a first runnable script: [Quickstart](learn/quickstart.md)
- Need the formal language definition: [Reference Overview](reference/overview.md)
- Looking for indicator contracts: [Indicators Overview](reference/indicators.md)

The hosted browser IDE demo keeps a minimal shell: one editor buffer, a
calendar date-range picker over the curated BTCUSDT dataset, live diagnostics,
and backtest output panels.

## Language Highlights

PalmScript supports:

- a required base `interval <...>` declaration
- named `source` declarations for market data
- source-qualified series such as `spot.close` and `perp.1h.close`
- optional `use <alias> <interval>` declarations for supplemental intervals
- literals, arithmetic, comparisons, unary operators, `and`, and `or`
- `let`, `const`, `input`, tuple destructuring, `export`, and `trigger`
- `if / else if / else`
- literal-offset series indexing
- indicators, signal helpers, event-memory helpers, and TA-Lib-style builtins
- first-class strategy declarations such as `entry`, `exit`, `order`, `protect`, and `target`

## How To Read The Docs

Start with `Learn` if you are writing PalmScript for the first time.

Use `Reference` when you need exact rules for syntax, semantics, builtins, intervals, or outputs.
