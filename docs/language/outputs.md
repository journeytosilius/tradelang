# Outputs

PalmScript currently supports three main output forms inside strategies:

- `plot(...)`
- `export name = expr`
- `trigger name = expr`

The runtime output model also includes alerts as a structured output channel.

## `plot`

`plot` emits chart-oriented numeric output points.

## `export`

`export` publishes a named output series that other strategies or host systems can consume later.

Valid value categories:

- numeric
- boolean
- `na`
- derived series values of those types

Example:

```palmscript
export trend = ema(close, 20) > ema(close, 50)
```

## `trigger`

`trigger` publishes a named boolean-like output series and also emits a discrete trigger event when the current sample is `true`.

`false` and `na` do not emit trigger events.

Example:

```palmscript
trigger long_entry = close > high[1]
```

## Runtime Output Shapes

The runtime accumulates:

- `plots`
- `exports`
- `triggers`
- `trigger_events`
- `alerts`

See [CLI Command Reference](../reference/cli.md) and [Diagnostics and Error Classes](../reference/diagnostics.md) for serialization and failure behavior.
