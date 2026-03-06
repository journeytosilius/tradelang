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

For direct script execution outside Rust examples, use the CLI CSV mode. This
is the only `run` mode today. The single `--bars` file is treated as raw market
data and rolled up automatically to the strategy's declared `interval` and
`use` intervals when possible. Manual per-interval `--feed` wiring is no
longer part of the CLI:

```bash
tradelang check strategy.trl
tradelang run csv strategy.trl --bars bars.csv
tradelang dump-bytecode strategy.trl
```

For editor authoring, use the VS Code extension in `editors/vscode/`. It
launches the Rust `tradelang-lsp` binary and surfaces compiler-backed
diagnostics, hovers, completions, definitions, symbols, and formatting while
editing `.trl` files.

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
./tradelang run csv examples/strategies/sma_cross.trl \
  --bars examples/data/minute_bars.csv

./tradelang run csv examples/strategies/volume_breakout.trl \
  --bars examples/data/minute_bars.csv \
  --format text

./tradelang run csv examples/strategies/weekly_bias.trl \
  --bars examples/data/daily_bars.csv
```

Current CLI-ready strategies:

- `strategies/sma_cross.trl`: `interval 1m` EMA/SMA trend state with `export`
  and `trigger`
- `strategies/volume_breakout.trl`: `interval 1m` breakout plus rising-volume
  trigger example
- `strategies/weekly_bias.trl`: daily execution using a weekly higher-timeframe
  basis with `interval 1d` and `use 1w`
