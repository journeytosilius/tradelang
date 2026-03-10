# CLI

The public command-line entrypoint is `palmscript`.

Use this page for the normal user workflow. Use [CLI Command Reference](../reference/cli.md) for the compact command and flag listing.

## Common Workflow

Typical flow:

1. validate a script with `palmscript check`
2. run it with `palmscript run market`
3. inspect the compiled form with `palmscript dump-bytecode` when you want to understand how the script is compiled

## Validate Without Running

```bash
palmscript check strategy.palm
```

This compiles the script and reports source diagnostics without executing it.

## Run A Script

```bash
palmscript run market strategy.palm \
  --from 1704067200000 \
  --to 1704153600000
```

Use `run market` when:

- the script declares one or more `source` directives
- you want PalmScript to fetch the required historical candles and execute the script over that window

When a script uses multiple sources or supplemental intervals, PalmScript fetches the required feeds automatically from the declarations in the script.

## Inspect Compiled Output

```bash
palmscript dump-bytecode strategy.palm
palmscript dump-bytecode strategy.palm --format json
```

This prints the compiled form rather than executing the script.

## Output Formats

Market mode supports:

- `--format json`
- `--format text`

`json` is the default.

## Execution Limits

Market mode supports:

- `--max-instructions-per-bar`
- `--max-history-capacity`

Use these when testing large or pathological scripts and you want tighter deterministic execution bounds.
