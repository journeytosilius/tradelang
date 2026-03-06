# CLI

The official CLI binary is `tradelang`.

## Commands

- `tradelang run csv <script.trl> --bars <bars.csv>`
- `tradelang check <script.trl>`
- `tradelang dump-bytecode <script.trl>`

## `run csv`

Executes a strategy in CSV mode:

```bash
tradelang run csv examples/strategies/sma_cross.trl \
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
tradelang check strategy.trl
tradelang check strategy.trl --env env.json
```

## `dump-bytecode`

Compiles and renders the compiled program:

```bash
tradelang dump-bytecode strategy.trl
tradelang dump-bytecode strategy.trl --format json
```

The optional `--env` flag lets you load a compile environment for external inputs.
