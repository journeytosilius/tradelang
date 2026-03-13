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

Representative files:

- `crates/palmscript/examples/strategies/adaptive_trend_backtest.ps`: adaptive multi-timeframe long-only backtest strategy with optimizer-tuned EMA, RSI, MACD, entry sizing, ATR target, and post-target stop-ratchet inputs around staged `entry1` / `entry2` and `target1` / `target2` order flow, including inline `input ... optimize(...)` metadata for durable CLI optimization
- `crates/palmscript/examples/strategies/portfolio_caps_backtest.ps`: multi-alias portfolio backtest example using `portfolio_group`, position-count caps, and gross/net exposure caps under repeated `--execution-source` CLI selection
- `crates/palmscript/examples/strategies/risk_controls_backtest.ps`: staged spot backtest example using declarative `cooldown` and `max_bars_in_trade` controls to gate same-side re-entry and time-box open trades
- `crates/palmscript/examples/strategies/risk_sized_entry_backtest.ps`: staged spot backtest example using `size entry long = risk_pct(...)` to size from stop distance instead of capital fraction
- `crates/palmscript/examples/strategies/usdm_long_short_backtest.ps`: Binance USD-M BTCUSDT long-biased perp strategy with staged long entries, staged mark-triggered targets, and a post-target mark-triggered stop ratchet
- `crates/palmscript/examples/strategies/bybit_spot.ps`: Bybit spot market-mode example with a supplemental `1h` feed
- `crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps`: Bybit USDT perpetual backtest example with a higher-interval trend filter
- `crates/palmscript/examples/strategies/gate_spot.ps`: Gate spot market-mode example with a supplemental `4h` feed
- `crates/palmscript/examples/strategies/gate_usdt_perps_backtest.ps`: Gate USDT perpetual backtest example with a higher-interval trend filter
- `crates/palmscript/examples/strategies/sma_cross.ps`: single-source market-mode strategy
- `crates/palmscript/examples/strategies/weekly_bias.ps`: single-source supplemental-interval strategy
- `crates/palmscript/examples/strategies/macd_tuple.ps`: tuple destructuring and `ma_type`
- `crates/palmscript/examples/strategies/indicator_showcase.ps`: single-source indicator showcase covering `supertrend`, `anchored_vwap`, `donchian`, `percentile`, `zscore`, and `ulcer_index`
- `crates/palmscript/examples/strategies/cross_source_spread.ps`: cross-source market-mode strategy
- `crates/palmscript/examples/strategies/cross_exchange_bybit_gate_spread.ps`: cross-exchange market-mode spread example mixing Bybit and Gate spot feeds
- `crates/palmscript/examples/strategies/exchange_backed_sources.ps`: source-aware strategy with `use <alias> <interval>`
- `crates/palmscript/examples/strategies/multi_strategy_backtest.ps`: composite trend, momentum, and breakout backtest strategy using `input`, `const`, and first-class `entry` / `exit` signals
- `crates/palmscript/examples/strategies/venue_orders_backtest.ps`: backtest strategy using explicit `order` declarations with `limit(...)` and `stop_market(...)`

For runnable public examples and workflow guidance, use the linked docs pages above.

When you inspect these strategies from the CLI, `run backtest`, `run walk-forward`, and `run optimize` now support `--diagnostics summary|full-trace`. Use `summary` for the normal compact diagnostics payload and `full-trace` when you want one typed per-bar decision trace record per execution bar.

The same checked-in strategies can also be queued into the local paper daemon with `run paper`:

```bash
./palmscript run paper crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps --execution-source perp
./palmscript execution serve --once
./palmscript run paper-export <session-id> --format json
```

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
./palmscript run backtest crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps --from 1704067200000 --to 1704153600000 --leverage 2
./palmscript run backtest crates/palmscript/examples/strategies/gate_usdt_perps_backtest.ps --from 1704067200000 --to 1704153600000 --leverage 2
./palmscript run backtest crates/palmscript/examples/strategies/portfolio_caps_backtest.ps --from 1704067200000 --to 1704153600000 --execution-source left --execution-source right
./palmscript run backtest crates/palmscript/examples/strategies/risk_controls_backtest.ps --from 1704067200000 --to 1706745600000
./palmscript run backtest crates/palmscript/examples/strategies/adaptive_trend_backtest.ps --from 1646611200000 --to 1772841600000
./palmscript run walk-forward crates/palmscript/examples/strategies/adaptive_trend_backtest.ps --from 1646611200000 --to 1772841600000 --train-bars 252 --test-bars 63 --step-bars 63
./palmscript run optimize crates/palmscript/examples/strategies/adaptive_trend_backtest.ps --from 1646611200000 --to 1772841600000 --train-bars 252 --test-bars 63 --step-bars 63 --trials 50 --preset-out best.json
./palmscript run backtest crates/palmscript/examples/strategies/multi_strategy_backtest.ps --from 1741348800000 --to 1772884800000 --fee-bps 10 --slippage-bps 2
./palmscript run backtest crates/palmscript/examples/strategies/venue_orders_backtest.ps --from 1704067200000 --to 1704931200000 --format text
```
