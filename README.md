# PalmScript

<p align="center">
  <img src="editors/vscode/images/palmscript.png" alt="PalmScript logo" width="220">
</p>

PalmScript is a deterministic DSL and bytecode VM for financial time-series strategies.

The language now includes indicator, signal-helper, event-memory, and early TA-Lib-style builtins such as `crossover`, `highest`, `barssince`, `valuewhen`, `ma`, `apo`, `ppo`, `macd`, `wma`, `avgdev`, `stddev`, `linearreg`, `beta`, `correl`, `aroon`, `aroonosc`, `bop`, `cci`, `cmo`, `mom`, `roc`, `willr`, and `minmax` in addition to the core OHLCV series model.

The repository currently ships:

- the Rust library crate
- a deterministic backtester on top of runtime trigger outputs
- the `palmscript` CLI
- the `palmscript-lsp` language server
- the first-party VS Code extension
- the MkDocs documentation site

## Current Language Surface

PalmScript currently implements:

- exactly one top-level base `interval <...>` directive per script
- one or more named exchange-backed `source` declarations per executable script
- source-qualified series such as `bn.close` or `hl.1h.close`
- source-scoped `use <alias> <interval>` declarations for supplemental intervals
- top-level expression-bodied `fn` declarations, `let`, tuple destructuring, `export`, and `trigger`
- deterministic three-valued boolean logic, bounded-history indexing, and typed `ma_type.<variant>` enum literals
- a partially executable TA-Lib-style builtin surface, with additional reserved catalog names exposed through diagnostics and IDE metadata
- exchange-backed execution through `palmscript run market`
- signal-to-portfolio backtesting through `palmscript run backtest` and `run_backtest_with_sources`

Checked-in strategy examples live under [`examples/strategies/`](examples/strategies/).

## Documentation

The canonical documentation source is the MkDocs site under `docs/`.

- local source: [docs/index.md](docs/index.md)
- published site: <https://palmscript.dev/docs/>
- GitHub Pages mirror: <https://journeytosilius.github.io/palmscript/>

Start here:

- [Learn](docs/learn/overview.md)
- [Language Reference](docs/reference/overview.md)
- [Indicators Reference](docs/reference/indicators.md)
- [CLI](docs/tooling/cli.md)
- [Backtesting](docs/tooling/backtesting.md)
- [VS Code Extension](docs/tooling/vscode.md)

## Common Commands

```bash
cargo build --bin palmscript --bin palmscript-lsp
target/debug/palmscript check examples/strategies/sma_cross.palm
target/debug/palmscript run market examples/strategies/sma_cross.palm --from 1704067200000 --to 1704153600000
target/debug/palmscript run market examples/strategies/macd_tuple.palm --from 1704067200000 --to 1704153600000
target/debug/palmscript run market examples/strategies/cross_source_spread.palm --from 1704067200000 --to 1704153600000
target/debug/palmscript run backtest examples/strategies/multi_strategy_backtest.palm --from 1741348800000 --to 1772884800000 --fee-bps 10 --slippage-bps 2
mkdocs build --strict
docker build -f Dockerfile.docs -t palmscript-docs .
```
