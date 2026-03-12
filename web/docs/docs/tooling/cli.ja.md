# CLI

PalmScript がオープンソース化されたため、このページは再び公開されました。完全な翻訳は後続の更新で追加します。それまでは、この言語版のサイトでも同じ公開 CLI / ツール内容を参照できるよう、下に英語の正規版を掲載します。

## English Canonical Content


The public command-line entrypoint is `palmscript`.

Use this page for the normal user workflow. Use [CLI Command Reference](../reference/cli.md) for the compact command and flag listing.

## Common Workflow

Typical flow:

1. validate a script with `palmscript check`
2. run it with `palmscript run market`
3. inspect the compiled form with `palmscript dump-bytecode` when you want to understand how the script is compiled
4. submit long optimize jobs with `palmscript runs submit optimize`
5. inspect or resume them later with `palmscript runs status`, `show`, `tail`, `best`, and `resume`

## Validate Without Running

```bash
palmscript check strategy.ps
```

This compiles the script and reports source diagnostics without executing it.

## Run A Script

```bash
palmscript run market strategy.ps \
  --from 1704067200000 \
  --to 1704153600000
```

Use `run market` when:

- the script declares one or more `source` directives
- you want PalmScript to fetch the required historical candles and execute the script over that window

When a script uses multiple sources or supplemental intervals, PalmScript fetches the required feeds automatically from the declarations in the script.

## Inspect Compiled Output

```bash
palmscript dump-bytecode strategy.ps
palmscript dump-bytecode strategy.ps --format json
```

This prints the compiled form rather than executing the script.

## Read Embedded Docs In The CLI

The CLI embeds the public English docs snapshot at build time so agents and offline workflows can read the canonical docs without opening the site.

```bash
palmscript docs --list
palmscript docs tooling/cli
palmscript docs --all
```

Use:

- `palmscript docs --list` to discover exact topic paths
- `palmscript docs <topic>` to read one embedded page
- `palmscript docs --all` to stream the full embedded English docs set in one terminal-friendly output

The embedded docs are generated from `web/docs/docs/` during the CLI build and stay aligned with the public documentation tree.

## Durable Optimize Runs

Use the `runs` command family when `run optimize` would be too long to babysit in one terminal:

```bash
palmscript runs submit optimize strategy.ps \
  --from 1741348800000 \
  --to 1772884800000 \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --trials 50

palmscript runs serve
palmscript runs status <run-id>
palmscript runs show <run-id>
palmscript runs best <run-id> --preset-out best.json
```

These commands keep local durable state under the platform state directory, persist artifacts for each run, and let you resume interrupted optimize work without changing strategy syntax.

Walk-forward optimize now reserves a final untouched holdout window by default. If you pass `--test-bars 63`, PalmScript also reserves the last `63` execution bars as an unseen holdout unless you override that with `--holdout-bars <N>` or disable it with `--no-holdout`.

Optimizer parameter-space precedence is:

1. explicit repeated `--param ...`
2. preset parameter space from `--preset`
3. inferred script metadata from `input ... optimize(...)`

Explicit `--param` declarations still accept:

- `int:name=low:high[:step]`
- `float:name=low:high[:step]`
- `choice:name=v1,v2,v3`

So a script can either keep the search space in the CLI, or declare it directly on the inputs:

```palmscript
input fast_len = 21 optimize(int, 8, 34, 1)
input target_atr_mult = 2.5 optimize(float, 1.5, 4.0, 0.25)
input weekly_bias = 21 optimize(choice, 13, 21, 34)
```

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
