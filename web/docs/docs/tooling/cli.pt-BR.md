# CLI

Esta pagina voltou a ficar disponivel publicamente porque o PalmScript agora e open source. A localizacao completa sera publicada em uma atualizacao posterior. Enquanto isso, o conteudo canonico em ingles aparece abaixo para que esta versao do site exponha a mesma superficie publica de CLI e ferramentas.

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

## Durable Optimize Runs

Use the `runs` command family when `run optimize` would be too long to babysit in one terminal:

```bash
palmscript runs submit optimize strategy.ps \
  --from 1741348800000 \
  --to 1772884800000 \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --param int:fast_len=8:34 \
  --param float:target_atr_mult=1.5:4.0 \
  --trials 50

palmscript runs serve
palmscript runs status <run-id>
palmscript runs show <run-id>
palmscript runs best <run-id> --preset-out best.json
```

These commands keep local durable state under the platform state directory, persist artifacts for each run, and let you resume interrupted optimize work without changing strategy syntax.

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
