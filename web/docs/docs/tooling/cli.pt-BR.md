# CLI

Esta pagina voltou a ficar disponivel publicamente porque o PalmScript agora e open source. A localizacao completa sera publicada em uma atualizacao posterior. Enquanto isso, o conteudo canonico em ingles aparece abaixo para que esta versao do site exponha a mesma superficie publica de CLI e ferramentas.

## English Canonical Content

# CLI

The public command-line entrypoint is `palmscript`.

Use this page for the normal user workflow. Use [CLI Command Reference](../reference/cli.md) for the compact command and flag listing.

## Common Workflow

Typical flow:

1. validate a script with `palmscript check`
2. run it with `palmscript run market`
3. inspect the compiled form with `palmscript dump-bytecode` when you want to understand how the script is compiled
4. tune strategies with `palmscript run optimize` and save the best preset with `--preset-out`
5. rerun the winner with `run backtest` or `run walk-forward` before you trust it
6. switch `--diagnostics` between `summary` and `full-trace` depending on whether you need compact output or per-bar decision traces
7. repeat `--execution-source` when you want one shared-equity portfolio backtest across multiple execution aliases

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

## Optimize Strategies

Use `run optimize` directly when tuning a strategy from the CLI:

```bash
palmscript run optimize strategy.ps \
  --from 1741348800000 \
  --to 1772884800000 \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --trials 50 \
  --preset-out best.json
```

The backtest, walk-forward, and optimize commands all accept:

- `--diagnostics summary`
- `--diagnostics full-trace`

Use `summary` for normal iterative tuning. Use `full-trace` when you want one typed per-bar decision trace per execution bar so an agent can inspect why signals were queued, blocked, ignored, expired, or forced out.

The same backtest-oriented commands also accept repeated `--execution-source` flags. Passing one alias keeps the legacy single-position mode. Passing multiple aliases activates portfolio mode, which evaluates the compiled strategy logic for each selected execution alias under one shared equity ledger.

Portfolio scripts can declare compile-time caps directly in the source:

```palmscript
portfolio_group "majors" = [left, right]
max_positions = 2
max_long_positions = 2
max_gross_exposure_pct = 0.8
max_net_exposure_pct = 0.8
```

Those declarations block only the new entry that would exceed the configured cap. They do not auto-resize orders or force exits after exposure drifts.

Walk-forward optimize now reserves a final untouched holdout window by default. If you pass `--test-bars 63`, PalmScript also reserves the last `63` execution bars as an unseen holdout unless you override that with `--holdout-bars <N>` or disable it with `--no-holdout`.

The optimize result now also reports:

- holdout drift versus the stitched optimization summary
- holdout checks for the top ranked candidates, not only the winner
- parameter stability ranges across the ranked and holdout-passing candidates
- deterministic machine-readable improvement hints

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
