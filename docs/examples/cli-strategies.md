# CLI Strategies

Checked-in `.trl` strategies live under `examples/strategies/`.

## `sma_cross.trl`

- base interval: `1m`
- demonstrates `let`, `export`, `trigger`, and indicator comparison

```bash
tradelang run csv examples/strategies/sma_cross.trl \
  --bars examples/data/minute_bars.csv
```

## `volume_breakout.trl`

- base interval: `1m`
- demonstrates breakout logic plus trigger output

```bash
tradelang run csv examples/strategies/volume_breakout.trl \
  --bars examples/data/minute_bars.csv \
  --format text
```

## `weekly_bias.trl`

- base interval: `1d`
- declared supplemental interval: `1w`
- demonstrates higher-timeframe basis logic

```bash
tradelang run csv examples/strategies/weekly_bias.trl \
  --bars /path/to/daily_bars.csv
```

That example requires enough daily data to roll into full weekly candles.
