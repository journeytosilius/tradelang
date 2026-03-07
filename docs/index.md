# PalmScript Documentation

PalmScript is a deterministic DSL for financial time-series programs. Scripts compile to bytecode and execute inside a bounded-history VM with no filesystem, network, wall-clock, or randomness access during strategy execution.

This site is the canonical documentation source for the repository.

## Documentation Map

- `Learn` teaches how to write and run PalmScript strategies.
- `Reference` is the normative definition of PalmScript syntax and semantics.
- `Tooling` explains the CLI, execution modes, and editor integrations.
- `Tooling` also covers the library backtesting surface built on top of runtime outputs.
- `Internals` documents compiler, bytecode, VM, and runtime architecture.
- `Contributing` covers repository workflow, testing, releases, and docs maintenance.

## Start Here

- New to PalmScript: [Learn Overview](learn/overview.md)
- Want a first runnable strategy: [Quickstart](learn/quickstart.md)
- Need the formal language definition: [Reference Overview](reference/overview.md)
- Looking for indicator contracts: [Indicators Overview](reference/indicators.md)
- Running scripts from the CLI: [CLI](tooling/cli.md)
- Integrating the library backtester: [Backtesting](tooling/backtesting.md)
- Understanding the editor workflow: [VS Code Extension](tooling/vscode.md)

## Current Capabilities

PalmScript currently implements:

- a mandatory base `interval <...>` declaration
- one or more named exchange-backed `source` declarations per executable script
- source-qualified market series such as `spot.close` and `hl.1h.close`
- source-scoped `use <alias> <interval>` declarations for supplemental intervals
- numeric, boolean, string-in-source-declaration, and `na` literals
- top-level expression-bodied `fn` declarations
- `let`, tuple destructuring, `export`, and `trigger`
- `if / else if / else`
- arithmetic, comparisons, unary operators, `and`, and `or`
- series indexing with literal offsets
- builtins: indicators, signal helpers, event-memory helpers, `plot`, and a partially executable TA-Lib-style catalog
- exchange-backed execution through market mode
- library backtesting through `run_backtest_with_sources`
- a CLI, language server, and first-party VS Code extension

## Reading Strategy

If you want to learn PalmScript, start in `Learn`.

If you need exact rules for what the compiler accepts or what the runtime does, use `Reference`.

If you are changing the implementation, use `Internals` and `Contributing`.
