# Examples

The canonical examples documentation now lives in the MkDocs site:

- [Multi-Interval Strategy](../../web/docs/docs/learn/cookbook/multi-interval.md)
- [Exchange-Backed Sources](../../web/docs/docs/learn/cookbook/exchange-backed-sources.md)
- [Cross-Source Spread](../../web/docs/docs/learn/cookbook/cross-source-spread.md)
- `Rust Examples` are documented privately in `../../web/docs/docs-private/internals/rust-examples.md`

This file remains a short inventory for repository browsing. The canonical public explanation of language behavior lives in `web/docs/docs/`.

## Rust Examples

Run from the repository root:

```bash
cargo run --example sma
cargo run --example rsi
cargo run --example step_engine
cargo run --example multi_interval
cargo run --example monthly_trend
```

## CLI Strategies

Checked-in `.ps` strategies live under `crates/palmscript/examples/strategies/`.
Experimental variants belong under
`crates/palmscript/examples/strategies/experimental/`, and generated artifacts
belong under `crates/palmscript/examples/strategies/artifacts/`.

Start here when you want to see the language in action:

- `crates/palmscript/examples/strategies/cross_source_spread.ps`: best first stop for source-aware research, spread math, and explicit alias-qualified market fields
- `crates/palmscript/examples/strategies/indicator_showcase.ps`: compact tour of the richer indicator surface, including `supertrend`, `anchored_vwap`, `donchian`, `percentile`, `zscore`, and `ulcer_index`
- `crates/palmscript/examples/strategies/weekly_bias.ps`: simple supplemental-interval example showing `use <alias> <interval>` without execution logic
- `crates/palmscript/examples/strategies/venue_orders_backtest.ps`: cleanest backtest example for explicit `execution`, named-argument order constructors, and attached exit flow
- `crates/palmscript/examples/strategies/portfolio_caps_backtest.ps`: best portfolio-mode example for repeated `--execution-source`, `portfolio_group`, and exposure/position caps
- `crates/palmscript/examples/strategies/adaptive_trend_backtest.ps`: strongest optimization example with inline `input ... optimize(...)` metadata, staged entries, staged targets, and stop ratchets
- `crates/palmscript/examples/strategies/strategy.ps`: advanced perp/spot multi-source strategy with higher-interval filters, staged entries, and mark-triggered protective exits
- `crates/palmscript/examples/strategies/triiger_happy.ps`: intentionally aggressive paper-trading smoke test for fill churn, order lifecycle transitions, and paper-daemon validation

Additional representative files:

- `crates/palmscript/examples/strategies/experimental/xrp_usdm_mean_reversion.ps`: experimental 5m Binance USD-M XRPUSDT mean-reversion strategy with both long and short legs, 1h regime gating, and smaller short sizing
- `crates/palmscript/examples/strategies/experimental/ada_usdm_regime_scalper.ps`: experimental 3m Binance USD-M ADAUSDT regime-switching long/short scalper with explicit range, trend, and risk-off states, a 04:00-08:00 UTC session filter, and higher-expectancy defaults that heavily favor trend pullback continuations
- `crates/palmscript/examples/strategies/risk_controls_backtest.ps`: staged spot backtest example using declarative `cooldown` and `max_bars_in_trade` controls to gate same-side re-entry and time-box open trades
- `crates/palmscript/examples/strategies/risk_sized_entry_backtest.ps`: staged spot backtest example using `size entry long = risk_pct(...)` to size from stop distance instead of capital fraction
- `crates/palmscript/examples/strategies/usdm_long_short_backtest.ps`: Binance USD-M BTCUSDT long-biased perp strategy with staged long entries, staged mark-triggered targets, and a post-target mark-triggered stop ratchet
- `crates/palmscript/examples/strategies/bybit_spot.ps`: Bybit spot market-mode example with a supplemental `1h` feed
- `crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps`: Bybit USDT perpetual backtest example with a higher-interval trend filter
- `crates/palmscript/examples/strategies/gate_spot.ps`: Gate spot market-mode example with a supplemental `4h` feed
- `crates/palmscript/examples/strategies/gate_usdt_perps_backtest.ps`: Gate USDT perpetual backtest example with a higher-interval trend filter
- `crates/palmscript/examples/strategies/sma_cross.ps`: minimal single-source market-mode strategy
- `crates/palmscript/examples/strategies/macd_tuple.ps`: tuple destructuring and `ma_type`
- `crates/palmscript/examples/strategies/cross_exchange_bybit_gate_spread.ps`: cross-exchange market-mode spread example mixing Bybit and Gate spot feeds
- `crates/palmscript/examples/strategies/exchange_backed_sources.ps`: source-aware strategy with `use <alias> <interval>`
- `crates/palmscript/examples/strategies/multi_strategy_backtest.ps`: composite trend, momentum, and breakout backtest strategy using `input`, `const`, and first-class `entry` / `exit` signals

For runnable public examples and workflow guidance, use the linked docs pages above.

When you inspect these strategies from the CLI, `run backtest`, `run walk-forward`, and `run optimize` now support `--diagnostics summary|full-trace`. Use `summary` for the normal compact diagnostics payload and `full-trace` when you want one typed per-bar decision trace record per execution bar.

Execution-oriented commands now require at least one declared `execution`
target in the script. The checked-in backtest and paper examples already
declare those execution aliases explicitly. If a script declares exactly one
`execution` alias, the CLI uses it automatically. If it declares multiple
execution aliases, pass `--execution-source <alias>` or repeat that flag for
portfolio mode.

Those execution-oriented commands also require explicit `order ...`
declarations for each declared `entry` / `exit` signal role plus explicit
`--maker-fee-bps` and `--taker-fee-bps` inputs on the CLI. The checked-in
backtest and paper examples now declare those orders explicitly instead of
depending on synthesized default orders, and `palmscript check` rejects
execution scripts that omit them.

Use `--max-volume-fill-pct 0.10` when you want to reject simulated fills above
10% of an execution bar volume instead of assuming full liquidity. PalmScript
keeps that behavior deterministic and cancels the oversized fill rather than
modeling partial fills.

The same checked-in strategies can also be queued into the local paper daemon with `run paper`:

```bash
./palmscript run paper crates/palmscript/examples/strategies/strategy.ps --maker-fee-bps 2 --taker-fee-bps 5
./palmscript run paper crates/palmscript/examples/strategies/triiger_happy.ps --maker-fee-bps 2 --taker-fee-bps 5
./palmscript run paper crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps --execution-source perp --maker-fee-bps 2 --taker-fee-bps 5
./palmscript execution serve --once
./palmscript run paper-export <session-id> --format json
```

Paper snapshots now include the latest top-of-book bid/ask, derived mid price, and any available last/mark price snapshots for each execution alias. That makes it easier for agents to inspect live paper-session valuation and quote health without leaving the CLI.

Paper mode is local-only and fake-money-only in v1, but it reuses the same compiled VM, backtest order semantics, portfolio caps, cooldowns, and `max_bars_in_trade` controls as ordinary backtests.

Common commands:

```bash
./palmscript check crates/palmscript/examples/strategies/sma_cross.ps
./palmscript run market crates/palmscript/examples/strategies/sma_cross.ps --from 1704067200000 --to 1704153600000
./palmscript run market crates/palmscript/examples/strategies/volume_breakout.ps --from 1704067200000 --to 1704153600000 --format text
./palmscript run market crates/palmscript/examples/strategies/weekly_bias.ps --from 1704067200000 --to 1705276800000
./palmscript run market crates/palmscript/examples/strategies/signal_helpers.ps --from 1704067200000 --to 1704153600000
./palmscript run market crates/palmscript/examples/strategies/event_memory.ps --from 1704067200000 --to 1704153600000
./palmscript run market crates/palmscript/examples/strategies/macd_tuple.ps --from 1704067200000 --to 1704153600000
./palmscript run market crates/palmscript/examples/strategies/cross_source_spread.ps --from 1704067200000 --to 1704153600000
./palmscript run market crates/palmscript/examples/strategies/bybit_spot.ps --from 1704067200000 --to 1704153600000
./palmscript run market crates/palmscript/examples/strategies/gate_spot.ps --from 1704067200000 --to 1704153600000
./palmscript run market crates/palmscript/examples/strategies/cross_exchange_bybit_gate_spread.ps --from 1704067200000 --to 1704153600000
./palmscript run market crates/palmscript/examples/strategies/exchange_backed_sources.ps --from 1704067200000 --to 1704153600000
./palmscript run backtest crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps --from 1704067200000 --to 1704153600000 --maker-fee-bps 2 --taker-fee-bps 5 --leverage 2
./palmscript run backtest crates/palmscript/examples/strategies/gate_usdt_perps_backtest.ps --from 1704067200000 --to 1704153600000 --maker-fee-bps 2 --taker-fee-bps 5 --leverage 2
./palmscript run backtest crates/palmscript/examples/strategies/portfolio_caps_backtest.ps --from 1704067200000 --to 1704153600000 --execution-source left --execution-source right --maker-fee-bps 2 --taker-fee-bps 5
./palmscript run backtest crates/palmscript/examples/strategies/risk_controls_backtest.ps --from 1704067200000 --to 1706745600000 --maker-fee-bps 2 --taker-fee-bps 5
./palmscript run backtest crates/palmscript/examples/strategies/experimental/xrp_usdm_mean_reversion.ps --from 1771286400000 --to 1773878400000 --maker-fee-bps 2 --taker-fee-bps 5 --slippage-bps 1 --leverage 2
./palmscript run backtest crates/palmscript/examples/strategies/experimental/ada_usdm_regime_scalper.ps --from 1771286400000 --to 1773878400000 --maker-fee-bps 2 --taker-fee-bps 5 --slippage-bps 1 --leverage 2
./palmscript run backtest crates/palmscript/examples/strategies/adaptive_trend_backtest.ps --from 1646611200000 --to 1772841600000 --maker-fee-bps 2 --taker-fee-bps 5
./palmscript run walk-forward crates/palmscript/examples/strategies/adaptive_trend_backtest.ps --from 1646611200000 --to 1772841600000 --maker-fee-bps 2 --taker-fee-bps 5 --train-bars 252 --test-bars 63 --step-bars 63
./palmscript run optimize crates/palmscript/examples/strategies/adaptive_trend_backtest.ps --from 1646611200000 --to 1772841600000 --maker-fee-bps 2 --taker-fee-bps 5 --train-bars 252 --test-bars 63 --step-bars 63 --trials 50 --preset-out best.json
./palmscript run backtest crates/palmscript/examples/strategies/multi_strategy_backtest.ps --from 1741348800000 --to 1772884800000 --maker-fee-bps 2 --taker-fee-bps 5 --slippage-bps 2
./palmscript run backtest crates/palmscript/examples/strategies/venue_orders_backtest.ps --from 1704067200000 --to 1704931200000 --maker-fee-bps 2 --taker-fee-bps 5 --format text
```

Backtest and paper commands require `--maker-fee-bps` and `--taker-fee-bps`, and they also accept repeated `--fee-schedule <alias:maker:taker>` overrides when you need per-venue fee simulation for selected execution aliases.
