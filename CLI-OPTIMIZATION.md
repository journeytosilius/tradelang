# Agentic CLI Optimization Flow For `adaptive_trend_backtest.ps`

This file defines the operating procedure for optimizing
`crates/palmscript/examples/strategies/adaptive_trend_backtest.ps` with the
PalmScript CLI.

The strategy is a deterministic multi-timeframe long-only backtest over:

- base interval `4h`
- execution source `spot = binance.spot("BTCUSDT")`
- supplemental `1d` and `1w` feeds

The goal is not one single metric. The goal is to find parameter sets that:

- increase profit
- improve win rate
- reduce max drawdown
- keep trade count high enough to matter
- reduce losing behavior as much as possible

PalmScript CLI reality:

- `run optimize` and `runs submit optimize` optimize only declared numeric `input`s
- optimize objectives are limited to:
  - `robust-return`
  - `total-return`
  - `ending-equity`
  - `return-over-drawdown`
- `trade_count`, `winning_trade_count`, `losing_trade_count`, and `win_rate_pct`
  are visible in backtest and walk-forward summaries, so they must be used in
  post-run ranking rather than as direct optimizer objectives

## Strategy Inputs That Can Be Tuned Now

The current script exposes these tunable `input`s:

- `fast_len`
- `slow_len`
- `daily_fast_len`
- `daily_slow_len`
- `weekly_fast_len`
- `weekly_slow_len`
- `rsi_len`
- `breakout_len`
- `kama_len`
- `atr_len`
- `protect_atr_mult`
- `protect_cooldown_bars`
- `target_atr_mult`
- `entry2_size`
- `target1_size`

The current script does not expose these as `input`s:

- `target_return`
- `long_rsi_threshold`
- `macd_fast_len`
- `macd_slow_len`
- `macd_signal_len`
- the `2.5 * atr_4h` ratchet multiplier
- the `4 * atr_4h` second target multiplier

If those constants need to be optimized, first refactor them into `input`
declarations in the strategy. Do not expect the current PalmScript CLI to tune
`const` values directly.

## Mandatory Docs Read Before Every Run

Before any optimization or evaluation run, read the current English docs:

```bash
sed -n '1,200p' web/docs/docs/learn/overview.md
sed -n '1,260p' web/docs/docs-private/tooling/backtesting.md
sed -n '1,220p' web/docs/docs-private/reference/cli.md
```

Reason:

- the CLI surface is evolving
- durable optimize runs now exist
- the exact supported workflow must be checked before each batch

## Run Root And Artifact Policy

Use a dedicated local run root for this strategy so every iteration is durable
and inspectable later.

```bash
export STRATEGY=crates/palmscript/examples/strategies/adaptive_trend_backtest.ps
export FROM=1646611200000
export TO=1772841600000
export RUN_ROOT="$HOME/.local/state/palmscript/adaptive-trend"
mkdir -p "$RUN_ROOT"/{presets,reports,notes}
export PALMSCRIPT_RUNS_STATE_DIR="$RUN_ROOT/runs"
```

Use one note file per campaign:

```bash
printf "campaign started %s\n" "$(date -u +%FT%TZ)" >> "$RUN_ROOT/notes/campaign.log"
```

## Phase 0: Compile And Baseline

Always start from a clean baseline.

Compile:

```bash
palmscript check "$STRATEGY"
```

Baseline backtest in text mode:

```bash
palmscript run backtest "$STRATEGY" \
  --from "$FROM" \
  --to "$TO" \
  --format text \
  > "$RUN_ROOT/reports/baseline-backtest.txt"
```

Baseline walk-forward in text mode:

```bash
palmscript run walk-forward "$STRATEGY" \
  --from "$FROM" \
  --to "$TO" \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --format text \
  > "$RUN_ROOT/reports/baseline-walk-forward.txt"
```

Extract and record these baseline metrics:

- `ending_equity`
- `total_return_pct`
- `trade_count`
- `winning_trade_count`
- `losing_trade_count`
- `win_rate_pct`
- `max_drawdown`
- `average_bars_held`
- `protect_exit_count`
- `target_exit_count`

The baseline is the minimum bar for acceptance. A tuned preset should not be
called "better" if it raises profit by crushing trade count or by increasing
drawdown into an unacceptable regime.

## Phase 1: Broad Search Space

Use a wide but still defensible search space first. Keep constraints coherent:

- fast averages should stay faster than slow averages
- daily fast should stay faster than daily slow
- weekly fast should stay faster than weekly slow
- sizing fractions must stay in `(0, 1]`

Recommended first-pass search space:

```text
int:fast_len=8:34
int:slow_len=55:144
int:daily_fast_len=10:34
int:daily_slow_len=35:89
int:weekly_fast_len=5:13
int:weekly_slow_len=14:34
int:rsi_len=7:21
int:breakout_len=10:55
int:kama_len=10:55
int:atr_len=7:28
float:protect_atr_mult=1.8:4.5
int:protect_cooldown_bars=4:28
float:target_atr_mult=1.4:4.5
float:entry2_size=0.10:0.75
float:target1_size=0.10:0.75
```

## Phase 2: Objective Sweep

Do not trust one optimize objective. Run all four and keep artifacts from each.

For each objective in:

- `robust-return`
- `total-return`
- `ending-equity`
- `return-over-drawdown`

submit one durable optimize run:

```bash
palmscript runs submit optimize "$STRATEGY" \
  --from "$FROM" \
  --to "$TO" \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --objective robust-return \
  --param int:fast_len=8:34 \
  --param int:slow_len=55:144 \
  --param int:daily_fast_len=10:34 \
  --param int:daily_slow_len=35:89 \
  --param int:weekly_fast_len=5:13 \
  --param int:weekly_slow_len=14:34 \
  --param int:rsi_len=7:21 \
  --param int:breakout_len=10:55 \
  --param int:kama_len=10:55 \
  --param int:atr_len=7:28 \
  --param float:protect_atr_mult=1.8:4.5 \
  --param int:protect_cooldown_bars=4:28 \
  --param float:target_atr_mult=1.4:4.5 \
  --param float:entry2_size=0.10:0.75 \
  --param float:target1_size=0.10:0.75 \
  --trials 200 \
  --startup-trials 40 \
  --seed 11 \
  --workers 4 \
  --top 20
```

Repeat the same command for the other three objectives, changing:

- `--objective`
- `--seed`

Recommended seeds:

- `robust-return`: `11`
- `total-return`: `12`
- `ending-equity`: `13`
- `return-over-drawdown`: `14`

Then run the daemon:

```bash
palmscript runs serve
```

While it runs:

```bash
palmscript runs list
palmscript runs status <run-id>
palmscript runs tail <run-id>
```

If interrupted:

```bash
palmscript runs resume <run-id>
palmscript runs serve
```

## Phase 3: Export Best Presets Per Objective

For every finished run:

```bash
palmscript runs best <run-id> --preset-out "$RUN_ROOT/presets/<objective>-best.json"
palmscript runs show <run-id> > "$RUN_ROOT/reports/<objective>-run.txt"
```

Keep all of them. Do not discard a preset just because it loses one objective.

## Phase 4: Evaluate Presets On Real Metrics

The direct optimizer objective is not enough. Every exported preset must be
scored with both backtest and walk-forward summaries.

For each preset:

```bash
palmscript run backtest "$STRATEGY" \
  --preset "$RUN_ROOT/presets/<objective>-best.json" \
  --from "$FROM" \
  --to "$TO" \
  --format text \
  > "$RUN_ROOT/reports/<objective>-best-backtest.txt"
```

```bash
palmscript run walk-forward "$STRATEGY" \
  --preset "$RUN_ROOT/presets/<objective>-best.json" \
  --from "$FROM" \
  --to "$TO" \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --format text \
  > "$RUN_ROOT/reports/<objective>-best-walk-forward.txt"
```

Judge every preset on this composite checklist:

- `ending_equity` higher than baseline
- `total_return_pct` higher than baseline
- `max_drawdown` lower than baseline, or at least not meaningfully worse
- `trade_count` not collapsed
- `win_rate_pct` improved or at minimum not degraded badly
- `losing_trade_count` not exploding

Use this decision order:

1. reject presets with unacceptable drawdown
2. reject presets with too few trades to trust
3. prefer presets with stronger walk-forward behavior over raw in-sample profit
4. break ties with higher win rate and lower losing trade count

## Phase 5: Narrow Around Survivors With Walk-Forward Sweep

Once 2-4 promising presets survive, use `run walk-forward-sweep` as the
deterministic grid check around them.

Example around a promising preset:

```bash
palmscript run walk-forward-sweep "$STRATEGY" \
  --preset "$RUN_ROOT/presets/robust-return-best.json" \
  --from "$FROM" \
  --to "$TO" \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --set fast_len=11,13,15 \
  --set slow_len=80,89,98 \
  --set breakout_len=24,30,36 \
  --set protect_atr_mult=2.7,3.1,3.5 \
  --set target_atr_mult=2.4,2.8,3.2 \
  --set entry2_size=0.30,0.425,0.55 \
  --set target1_size=0.25,0.325,0.40 \
  --objective return-over-drawdown \
  --top 20 \
  --format text \
  > "$RUN_ROOT/reports/focused-sweep.txt"
```

Use the sweep to answer:

- does the candidate sit in a stable neighborhood?
- do tiny parameter shifts keep similar trade count and drawdown?
- is the "best" point a spike or a plateau?

Prefer plateaus over spikes.

## Phase 6: Multi-Campaign Refinement

Run multiple campaigns, not one giant search:

### Campaign A: Regime Sensitivity

Focus on:

- `fast_len`
- `slow_len`
- `daily_fast_len`
- `daily_slow_len`
- `weekly_fast_len`
- `weekly_slow_len`
- `breakout_len`
- `kama_len`

Purpose:

- trend definition quality
- trade frequency
- false breakout control

### Campaign B: Risk And Exit Shape

Focus on:

- `atr_len`
- `protect_atr_mult`
- `protect_cooldown_bars`
- `target_atr_mult`
- `target1_size`

Purpose:

- drawdown control
- loss containment
- runner behavior

### Campaign C: Position Scaling

Focus on:

- `entry2_size`
- `target1_size`
- `protect_atr_mult`
- `target_atr_mult`

Purpose:

- add-on aggressiveness
- early de-risking
- win-rate vs payoff balance

For each campaign:

- narrow the parameter ranges
- keep one frozen preset as the base
- change only the variables relevant to that campaign

## Phase 7: Final Selection Rule

The final preset should not be selected by raw profit alone.

Use this ranking order:

1. acceptable walk-forward max drawdown
2. positive walk-forward total return
3. sufficient trade count
4. improved or stable win rate
5. lower losing-trade burden
6. stronger ending equity

If one preset has the highest profit but clearly worse drawdown and a lower
win rate, it is not the winner.

## Phase 8: Final Export And Audit Trail

Once a preset wins:

```bash
cp "$RUN_ROOT/presets/<winner>.json" "$RUN_ROOT/presets/adaptive-trend-final.json"
```

Then save the final evaluation:

```bash
palmscript run backtest "$STRATEGY" \
  --preset "$RUN_ROOT/presets/adaptive-trend-final.json" \
  --from "$FROM" \
  --to "$TO" \
  --format text \
  > "$RUN_ROOT/reports/final-backtest.txt"

palmscript run walk-forward "$STRATEGY" \
  --preset "$RUN_ROOT/presets/adaptive-trend-final.json" \
  --from "$FROM" \
  --to "$TO" \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --format text \
  > "$RUN_ROOT/reports/final-walk-forward.txt"
```

Append a note:

```bash
printf "%s final_preset=%s\n" \
  "$(date -u +%FT%TZ)" \
  "$RUN_ROOT/presets/adaptive-trend-final.json" \
  >> "$RUN_ROOT/notes/campaign.log"
```

## Hard Rules For The Agent

- read the English docs before every new run batch
- never optimize only one objective and declare victory
- always keep durable run IDs and exported presets
- always compare optimized presets against baseline text reports
- prefer walk-forward robustness over backtest-only profit
- reject presets that improve profit by collapsing trade count too far
- reject presets that improve profit while materially worsening drawdown
- if a constant needs tuning, refactor the strategy first instead of pretending
  the CLI can optimize it

## Minimal Command Loop

When in doubt, use this loop:

1. read docs
2. `palmscript check "$STRATEGY"`
3. baseline backtest and walk-forward
4. submit four durable optimize runs, one per objective
5. `palmscript runs serve`
6. export best preset from each run
7. evaluate each preset with backtest and walk-forward text reports
8. run focused walk-forward sweeps around the survivors
9. choose the most robust preset
10. save final preset and final reports
