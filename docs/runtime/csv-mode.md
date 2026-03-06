# CSV Mode and Roll-Up Rules

`palmscript run csv ...` is the only run mode today.

## Input Contract

CSV mode accepts one raw market-data file:

```bash
palmscript run csv strategy.trl --bars bars.csv
```

Schema:

```text
time,open,high,low,close,volume
```

- `time` is Unix milliseconds UTC and represents candle open time
- rows must be strictly increasing in time

## Interval Inference

The data-preparation layer infers the raw input interval from timestamps before the runtime starts.

Inference requires:

- strictly increasing timestamps
- alignment to a supported Binance interval boundary
- consecutive gaps that are whole multiples of the inferred interval
- at least one exact one-candle gap

## Roll-Up Behavior

CSV mode uses the single raw file to build:

- the strategy's base feed from `interval <...>`
- each declared supplemental feed from `use <...>`

Roll-up is strict:

- buckets must be complete
- missing raw bars inside a bucket are fatal
- no partial rolled candle is emitted
- if a declared interval cannot produce even one full candle, execution fails

Aggregation rules:

- `open`: first raw bar open
- `high`: max high
- `low`: min low
- `close`: last raw bar close
- `volume`: sum of raw volume
- `time`: bucket open time

## Common Failure Example

If a script declares `interval 1d` and the raw file is only 8 one-minute bars long, CSV mode rejects it because one full daily candle needs 1440 complete one-minute bars.

That failure happens in data preparation, not in compilation.
