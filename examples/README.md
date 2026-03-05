# Examples

Run the examples from the repository root with Cargo:

```bash
cargo run --example sma
cargo run --example rsi
cargo run --example step_engine
cargo run --example multi_interval
cargo run --example monthly_trend
cargo run --example pipeline
```

Each example compiles a TradeLang script, runs it against a small OHLCV fixture,
and prints the resulting outputs.

For direct script execution outside Rust examples, use the CLI:

```bash
tradelang check strategy.trl
tradelang run strategy.trl --bars bars.csv --base-interval 1m
tradelang dump-bytecode strategy.trl
```

Current examples:

- `sma`: single-interval SMA over the base `close` series
- `rsi`: single-interval RSI over the base `close` series
- `step_engine`: per-bar stepping with the single-interval `Engine` API
- `multi_interval`: daily execution using a weekly EMA signal from `1w.close`
- `monthly_trend`: weekly execution combining `1M.close` and `1d.volume`
- `pipeline`: host-managed composition where one strategy exports signals and a
  downstream strategy consumes them as external inputs

CLI-ready `.trl` strategies and fixtures live under `examples/strategies/` and
`examples/data/`.

Suggested commands:

```bash
./tradelang check examples/strategies/sma_cross.trl
./tradelang run examples/strategies/sma_cross.trl \
  --bars examples/data/minute_bars.csv \
  --base-interval 1m

./tradelang run examples/strategies/volume_breakout.trl \
  --bars examples/data/minute_bars.csv \
  --base-interval 1m \
  --format text

./tradelang run examples/strategies/weekly_bias.trl \
  --bars examples/data/daily_bars.csv \
  --base-interval 1d \
  --feed 1w=examples/data/weekly_bars.csv
```

Current CLI-ready strategies:

- `strategies/sma_cross.trl`: EMA/SMA trend state with `export` and `trigger`
- `strategies/volume_breakout.trl`: breakout plus rising-volume trigger example
- `strategies/weekly_bias.trl`: daily execution using a weekly higher-timeframe
  basis
