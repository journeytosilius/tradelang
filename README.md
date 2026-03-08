# PalmScript

<p align="center">
  <img src="editors/vscode/images/palmscript.png" alt="PalmScript logo" width="220">
</p>

PalmScript is a deterministic DSL and bytecode VM for financial time-series strategies.

The language now includes indicator, signal-helper, null-handling, event-memory, anchored-event, and TA-Lib-style builtins such as `crossover`, `highest`, `highestbars`, `activated`, `deactivated`, `barssince`, `valuewhen`, `highest_since`, `lowest_since`, `highestbars_since`, `lowestbars_since`, `valuewhen_since`, `count_since`, `na(...)`, `nz`, `coalesce`, `cum`, `ma`, `apo`, `ppo`, `macd`, `mama`, `wma`, `avgdev`, `stddev`, `linearreg`, `beta`, `correl`, `aroon`, `aroonosc`, `bop`, `cci`, `cmo`, `mom`, `roc`, `willr`, `minmax`, `ht_dcperiod`, and `ht_sine` in addition to the core OHLCV series model.

The repository currently ships:

- the Rust library crate
- a deterministic venue-aware backtester on top of runtime trigger outputs and order metadata
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
- top-level expression-bodied `fn` declarations, `let`, `const`, `input`, tuple destructuring, `export`, `trigger`, first-class `entry` / `exit` signals, attached `protect` / `target` exits, `size entry ...` scale-in and `risk_pct(...)` declarations, `size target ...` partial-take-profit declarations, and `order` declarations
- deterministic three-valued boolean logic, bounded-history indexing, and typed `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>`, and `exit_kind.<variant>` enum literals
- an expanding executable TA-Lib-style builtin surface, with remaining reserved catalog names exposed through diagnostics and IDE metadata
- exchange-backed execution through `palmscript run market`
- venue-aware signal-to-portfolio backtesting through `palmscript run backtest` and `run_backtest_with_sources`, including machine-readable order, trade, regime, and opportunity diagnostics
- isolated-margin perp backtesting for `binance.usdm` and `hyperliquid.perps`, with live venue risk snapshots, leverage, and deterministic liquidation exits
- rolling out-of-sample walk-forward evaluation through `palmscript run walk-forward` and `run_walk_forward_with_sources`
- bounded explicit `input` grid search through `palmscript run walk-forward-sweep` and `run_walk_forward_sweep_with_source`
- seeded hyper-parameter optimization through `palmscript run optimize` and `run_optimize_with_source`

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
target/debug/palmscript run backtest strategy.palm --from 1741348800000 --to 1772884800000 --execution-source perp --leverage 3 --margin-mode isolated
target/debug/palmscript run walk-forward examples/strategies/multi_strategy_backtest.palm --from 1741348800000 --to 1772884800000 --train-bars 252 --test-bars 63 --step-bars 63
target/debug/palmscript run walk-forward-sweep strategy.palm --from 1741348800000 --to 1772884800000 --train-bars 252 --test-bars 63 --step-bars 63 --set fast_len=13,21,34 --set target_atr_mult=2.0,2.5,3.0 --objective total-return --top 5
target/debug/palmscript run optimize strategy.palm --from 1741348800000 --to 1772884800000 --train-bars 252 --test-bars 63 --step-bars 63 --param int:fast_len=8:34 --param float:target_atr_mult=1.5:4.0 --objective robust-return --trials 50 --top 5 --preset-out /tmp/adaptive-best.json
target/debug/palmscript run backtest examples/strategies/venue_orders_backtest.palm --from 1704067200000 --to 1704931200000 --format text
mkdocs build --strict
docker build -f Dockerfile.docs -t palmscript-docs .
```
