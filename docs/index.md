# PalmScript Documentation

PalmScript is a deterministic DSL for financial time-series programs. Scripts compile to bytecode and run inside a bounded-history VM with no filesystem, network, clock, or randomness access.

This site is the canonical documentation source for the repository. It covers:

- the language itself
- runtime semantics and data ingestion
- the `palmscript` CLI
- the Rust language server and VS Code extension
- examples, testing expectations, and release workflows

## Start Here

- New to the project: [Getting Started](getting-started/overview.md)
- Writing strategies: [Language](language/syntax.md)
- Running scripts: [CLI](tooling/cli.md)
- Understanding CSV mode and roll-up behavior: [CSV Mode and Roll-Up Rules](runtime/csv-mode.md)
- Editor integration: [VS Code Extension](tooling/vscode.md)
- Repository internals: [Compiler Pipeline](runtime/compiler-pipeline.md)

## Current Capabilities

PalmScript currently implements:

- numeric, boolean, and `na` literals
- mandatory source-level `interval <...>` declarations
- explicit `use <...>` declarations for additional intervals
- `let`, `export`, and `trigger` bindings
- `if / else if / else`
- arithmetic, comparisons, unary operators, `and`, and `or`
- series indexing with literal offsets
- builtins: `sma`, `ema`, `rsi`, `plot`
- predefined market data series: `open`, `high`, `low`, `close`, `volume`, `time`
- interval-qualified market series such as `1w.close` and `4h.volume`
- compile-time-inlined user-defined functions
- the `palmscript` CLI, `palmscript-lsp`, and a first-party VS Code extension

## Design Principles

- Determinism: the same program and data produce the same outputs.
- Bounded execution: history, instruction budgets, and runtime memory are capped.
- No lookahead: higher-interval values appear only after those candles fully close.
- Thin tooling wrappers: the CLI, language server, and editor integrations reuse the library instead of reimplementing semantics.
