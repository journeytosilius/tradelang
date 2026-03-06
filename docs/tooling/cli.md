# CLI

The first-party command-line entrypoint is `palmscript`.

Use this page for workflows and examples. Use [CLI Command Reference](../reference/cli.md) for the exact command and flag surface.

## Common Workflow

Typical development flow:

1. validate a strategy with `palmscript check`
2. run it in `csv` mode or `market` mode
3. inspect outputs in `json` or `text`
4. inspect the compiled form with `palmscript dump-bytecode` when debugging semantics

## Validate Without Running

```bash
palmscript check examples/strategies/sma_cross.palm
```

This compiles the script and reports source diagnostics without executing it.

## Run In CSV Mode

```bash
palmscript run csv examples/strategies/sma_cross.palm \
  --bars examples/data/minute_bars.csv
```

Use CSV mode when:

- the strategy is source-less
- you already have canonical OHLCV bars in a file
- you want strict roll-up behavior from one raw feed

CSV mode compiles the script, loads the raw file, infers the raw interval, prepares required feeds, runs the VM, and prints outputs.

See [CSV Mode](csv-mode.md) for the file contract and roll-up rules.

## Run In Market Mode

```bash
palmscript run market strategy.palm \
  --from 1704067200000 \
  --to 1704153600000
```

Use market mode when:

- the script declares one or more `source` directives
- you want PalmScript to fetch exchange candles directly

Market mode compiles the script, resolves the required source feeds, validates venue-specific guardrails, fetches candles for each required `(source, interval)`, constructs the source-aware runtime inputs, runs the VM, and prints outputs.

See [Market Mode](market-mode.md) for supported templates and fetch behavior.

## Output Formats

Both run modes support:

- `--format json`
- `--format text`

`json` is the default.

## Execution Limits

Both run modes support:

- `--max-instructions-per-bar`
- `--max-history-capacity`

Use these when testing pathological scripts or when tightening deterministic operational bounds.

## Inspect Bytecode

```bash
palmscript dump-bytecode examples/strategies/sma_cross.palm
palmscript dump-bytecode examples/strategies/sma_cross.palm --format json
```

This prints the compiled program rather than executing it.
