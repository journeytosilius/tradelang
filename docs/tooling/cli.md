# CLI

The first-party command-line entrypoint is `palmscript`.

Use this page for workflows and examples. Use [CLI Command Reference](../reference/cli.md) for the exact command and flag surface.

## Common Workflow

Typical development flow:

1. validate a strategy with `palmscript check`
2. run it in `market` mode
3. backtest it with `palmscript run backtest` when the script emits trading triggers
4. inspect outputs in `json` or `text`
5. inspect the compiled form with `palmscript dump-bytecode` when debugging semantics

## Validate Without Running

```bash
palmscript check strategy.palm
```

This compiles the script and reports source diagnostics without executing it.

## Run In Market Mode

```bash
palmscript run market strategy.palm \
  --from 1704067200000 \
  --to 1704153600000
```

Use market mode when:

- the script declares one or more `source` directives
- you want PalmScript to fetch each required base or supplemental feed directly from supported exchanges

Market mode compiles the script, resolves the required source-qualified feeds, validates venue-specific guardrails, fetches candles for each required `(source, interval)`, constructs the source-aware runtime inputs, runs the VM on the union of base timestamps, and prints outputs.

See [Market Mode](market-mode.md) for supported templates and fetch behavior.

## Run A Backtest

```bash
palmscript run backtest strategy.palm \
  --from 1741348800000 \
  --to 1772884800000 \
  --fee-bps 10 \
  --slippage-bps 2
```

Use backtest mode when:

- the script emits backtest signals through `entry` / `exit` declarations or legacy trigger names
- the script optionally declares explicit order templates with `order entry ... = ...` or `order exit ... = ...`
- you want PalmScript to fetch exchange-backed candles and run the built-in deterministic portfolio simulator in one command

Backtest mode compiles the script, fetches all required source feeds, runs the VM, collects trigger events, resolves venue-aware order templates, and simulates fills on the selected execution source.

When the script declares exactly one `source`, backtest mode uses it as the execution source automatically. When multiple sources are declared, pass `--execution-source <alias>`.

## Output Formats

Market mode supports:

- `--format json`
- `--format text`

`json` is the default.

Backtest mode supports the same output formats.

- JSON output includes order lifecycle records in `orders`
- JSON output also includes event diagnostics in `diagnostics`
- text output includes diagnostics, order, and trade summaries

## Execution Limits

Market mode supports:

- `--max-instructions-per-bar`
- `--max-history-capacity`

Use these when testing pathological scripts or when tightening deterministic operational bounds.

## Inspect Bytecode

```bash
palmscript dump-bytecode strategy.palm
palmscript dump-bytecode strategy.palm --format json
```

This prints the compiled program rather than executing it.
