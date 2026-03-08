# CLI

The first-party command-line entrypoint is `palmscript`.

Use this page for workflows and examples. Use [CLI Command Reference](../reference/cli.md) for the exact command and flag surface.

## Common Workflow

Typical development flow:

1. validate a strategy with `palmscript check`
2. run it in `market` mode
3. backtest it with `palmscript run backtest` when the script emits trading triggers
4. run `palmscript run walk-forward` when you want rolling out-of-sample evaluation
5. run `palmscript run walk-forward-sweep` when you want to rank explicit `input` grids by stitched out-of-sample results
6. run `palmscript run optimize` when you want seeded bounded search over selected numeric `input`s
7. inspect outputs in `json` or `text`
8. inspect the compiled form with `palmscript dump-bytecode` when debugging semantics

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
- the script may also declare attached exits through `protect` / `target`
- the script optionally declares explicit order templates with `order entry ... = ...` or `order exit ... = ...`
- the script may optionally size staged `order entry ...` fills with `size entry1..3 long|short ...`
- the script may optionally size staged attached `target` exits with `size target1..3 long|short ...`
- you want PalmScript to fetch exchange-backed candles and run the built-in deterministic portfolio simulator in one command

Backtest mode compiles the script, fetches all required source feeds, runs the VM, collects trigger events, resolves venue-aware order templates, and simulates fills on the selected execution source.

Backtest mode also accepts:

- `--preset <path>`

When supplied, PalmScript loads the optimizer preset JSON, verifies the script hash, and applies the preset's `best_input_overrides` before compiling the strategy.

Perp execution sources also accept:

- `--leverage <N>`
- `--margin-mode isolated`

Current V1 notes:

- spot sources reject `--leverage` and `--margin-mode`
- perp sources default to isolated `1.0x` when those flags are omitted
- Binance USD-M uses live signed leverage brackets when `PALMSCRIPT_BINANCE_USDM_API_KEY` and `PALMSCRIPT_BINANCE_USDM_API_SECRET` are available; otherwise it falls back to an approximate single-tier public `exchangeInfo` snapshot
- Hyperliquid perps fetch live margin tables publicly and currently use execution candles as the liquidation-mark fallback

When the script declares exactly one `source`, backtest mode uses it as the execution source automatically. When multiple sources are declared, pass `--execution-source <alias>`.

## Run Walk-Forward Evaluation

```bash
palmscript run walk-forward strategy.palm \
  --from 1741348800000 \
  --to 1772884800000 \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63
```

Use walk-forward mode when:

- the script already backtests normally and you want rolling out-of-sample evaluation
- you want repeated train/test windows without changing fill semantics
- you want a stitched summary of the out-of-sample segments

V1 semantics:

- PalmScript fetches the full requested source window once
- it runs rolling windows using `train_bars`, `test_bars`, and `step_bars`
- each segment reuses the training slice as in-sample context and reports the trailing test slice as out-of-sample
- this mode does not auto-optimize parameters yet; it evaluates the fixed script/inputs you supplied

Walk-forward mode also accepts:

- `--preset <path>`

When the script declares exactly one `source`, walk-forward mode uses it as the execution source automatically. When multiple sources are declared, pass `--execution-source <alias>`.

## Run Walk-Forward Parameter Sweeps

```bash
palmscript run walk-forward-sweep strategy.palm \
  --from 1741348800000 \
  --to 1772884800000 \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --set fast_len=13,21,34 \
  --set target_atr_mult=2.0,2.5,3.0 \
  --objective total-return \
  --top 5
```

Use walk-forward sweep mode when:

- the strategy already has `input` declarations you want to tune systematically
- you want stitched out-of-sample ranking instead of hand-running one variant at a time
- you want a bounded explicit grid search rather than unconstrained optimization

V1 semantics:

- PalmScript fetches the requested source window once
- each candidate recompiles the same script with numeric `input` overrides from `--set`
- every candidate reuses the same deterministic walk-forward engine and venue rules
- the explicit candidate grid is bounded to `10000` combinations
- the output ranks candidates by stitched OOS `total-return`, `ending-equity`, or `return-over-drawdown`

Walk-forward sweep also accepts:

- `--preset <path>`

Preset `best_input_overrides` are applied first and explicit `--set` values override only the inputs you are sweeping.

## Run Hyper-Parameter Optimization

```bash
palmscript run optimize strategy.palm \
  --from 1741348800000 \
  --to 1772884800000 \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --param int:fast_len=8:34 \
  --param float:target_atr_mult=1.5:4.0 \
  --objective robust-return \
  --trials 50 \
  --top 5 \
  --preset-out /tmp/adaptive-best.json
```

Use optimize mode when:

- the strategy already has a decent base design
- the parameters you want to tune already exist as numeric `input`s
- you want stitched walk-forward ranking without hand-running one variant at a time

V1 semantics:

- optimize defaults to the walk-forward runner
- `--runner backtest` is available for single-window ranking
- only declared numeric `input`s are tunable
- the search is seeded and deterministic for the same seed, search space, and runner config
- candidate suggestion uses a bounded TPE-style search rather than an unbounded evolutionary search
- `--workers` only controls bounded evaluation concurrency; it does not change the ranked result for the same seed
- `--preset-out` writes a reusable JSON artifact with the best overrides and top candidate summaries

## Output Formats

Market mode supports:

- `--format json`
- `--format text`

`json` is the default.

Backtest mode supports the same output formats.

- JSON output includes order lifecycle records in `orders`
- JSON output also includes backtest diagnostics in `diagnostics`, including order/trade context, capture summaries, export summaries, and opportunity events
- JSON output includes optional perp metadata in `perp` when the execution source is a perp venue
- text output includes compact diagnostics, order, and trade summaries plus short export/opportunity sections when available

Walk-forward mode also supports `json` and `text`.

- JSON output includes per-segment in-sample and out-of-sample summaries, per-segment out-of-sample diagnostics, plus a stitched out-of-sample summary
- text output includes a compact stitched summary, config, recent segment rows, and a short weakest-segment section with out-of-sample protect/target counts

Walk-forward sweep mode also supports `json` and `text`.

- JSON output includes the sweep config, candidate count, and ranked stitched OOS candidates
- text output includes a compact best-candidate summary plus the ranked top candidates

Optimize mode also supports `json` and `text`.

- JSON output includes the optimize config, trial counts, best candidate, and ranked top candidates
- text output includes a compact optimization summary plus the best and top candidates

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
