# Composition Examples

TradeLang supports host-managed composition through exports, triggers, external inputs, and the pipeline runtime.

## Producer Strategy

```tradelang
interval 1m

export trend = ema(close, 20) > ema(close, 50)
trigger breakout = close > high[1]
```

## Consumer Strategy

The consumer does not declare the upstream values in source. They arrive through a compile environment and runtime pipeline wiring:

```tradelang
interval 1m

if trend and breakout {
    plot(1)
} else {
    plot(0)
}
```

## Where To Configure It

- editor-time awareness: `.tradelang.json`
- runtime wiring: pipeline host / `PipelineEngine`

See [Composition and External Inputs](../language/composition.md) and [Pipeline Runtime](../runtime/pipeline-runtime.md).
