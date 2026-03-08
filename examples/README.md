# Examples

The canonical examples documentation now lives in the MkDocs site:

- [Multi-Interval Strategy](../docs/learn/cookbook/multi-interval.md)
- [Exchange-Backed Sources](../docs/learn/cookbook/exchange-backed-sources.md)
- [Cross-Source Spread](../docs/learn/cookbook/cross-source-spread.md)
- [Rust Examples](../docs/internals/rust-examples.md)

This file remains a short inventory for repository browsing. The canonical explanation of language behavior lives in `docs/`.

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

Checked-in `.palm` strategies live under `examples/strategies/`.

Representative files:

- `examples/strategies/adaptive_trend_backtest.palm`: adaptive multi-timeframe long-only backtest strategy with staged `entry1` / `entry2` market entries, staged `target1` / `target2` profit-taking, and `protect_after_target1 long` stop ratchets
- `examples/strategies/usdm_long_short_backtest.palm`: Binance USD-M BTCUSDT long-biased perp strategy with staged long entries, staged mark-triggered targets, and a post-target mark-triggered stop ratchet
- `examples/strategies/sma_cross.palm`: single-source market-mode strategy
- `examples/strategies/weekly_bias.palm`: single-source supplemental-interval strategy
- `examples/strategies/macd_tuple.palm`: tuple destructuring and `ma_type`
- `examples/strategies/cross_source_spread.palm`: cross-source market-mode strategy
- `examples/strategies/exchange_backed_sources.palm`: source-aware strategy with `use <alias> <interval>`
- `examples/strategies/multi_strategy_backtest.palm`: composite trend, momentum, and breakout backtest strategy using `input`, `const`, and first-class `entry` / `exit` signals
- `examples/strategies/venue_orders_backtest.palm`: backtest strategy using explicit `order` declarations with `limit(...)` and `stop_market(...)`

The backtester also supports staged entry sizing through `size entry1..3 long|short = ...`, plus staged partial attached profit-taking through `size target1..3 long|short = ...`.

For runnable commands and workflow guidance, use the linked docs pages above.

Common commands:

```bash
./palmscript check examples/strategies/sma_cross.palm
./palmscript run market examples/strategies/sma_cross.palm --from 1704067200000 --to 1704153600000
./palmscript run market examples/strategies/volume_breakout.palm --from 1704067200000 --to 1704153600000 --format text
./palmscript run market examples/strategies/weekly_bias.palm --from 1704067200000 --to 1705276800000
./palmscript run market examples/strategies/signal_helpers.palm --from 1704067200000 --to 1704153600000
./palmscript run market examples/strategies/event_memory.palm --from 1704067200000 --to 1704153600000
./palmscript run market examples/strategies/macd_tuple.palm --from 1704067200000 --to 1704153600000
./palmscript run market examples/strategies/cross_source_spread.palm --from 1704067200000 --to 1704153600000
./palmscript run market examples/strategies/exchange_backed_sources.palm --from 1704067200000 --to 1704153600000
./palmscript run backtest examples/strategies/adaptive_trend_backtest.palm --from 1646611200000 --to 1772841600000
./palmscript run walk-forward examples/strategies/adaptive_trend_backtest.palm --from 1646611200000 --to 1772841600000 --train-bars 252 --test-bars 63 --step-bars 63
./palmscript run backtest examples/strategies/multi_strategy_backtest.palm --from 1741348800000 --to 1772884800000 --fee-bps 10 --slippage-bps 2
./palmscript run backtest examples/strategies/venue_orders_backtest.palm --from 1704067200000 --to 1704931200000 --format text
```
