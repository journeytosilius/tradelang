# CLI

The official CLI binary is `palmscript`.

## Commands

- `palmscript run csv <script.palm> --bars <bars.csv>`
- `palmscript check <script.palm>`
- `palmscript dump-bytecode <script.palm>`

## `run csv`

Executes a strategy in CSV mode:

```bash
palmscript run csv examples/strategies/sma_cross.palm \
  --bars examples/data/minute_bars.csv
```

Options:

- `--format json|text`
- `--max-instructions-per-bar <N>`
- `--max-history-capacity <N>`

The command:

1. loads source
2. compiles it
3. loads the raw CSV bars
4. infers the raw interval
5. rolls bars into the declared `interval` and `use` intervals
6. runs the existing runtime
7. prints structured outputs

## `check`

Validates source without running it:

```bash
palmscript check strategy.palm
```

## `dump-bytecode`

Compiles and renders the compiled program:

```bash
palmscript dump-bytecode strategy.palm
palmscript dump-bytecode strategy.palm --format json
```
