# Learn PalmScript

PalmScript public documentation is organized around:

- the language for writing strategies
- examples that show how scripts are written and used

## What You Do With PalmScript

Typical workflow:

1. write a `.ps` script
2. declare a base `interval`
3. declare one or more `source` bindings
4. validate it in the browser IDE
5. run it over historical data in the app or CLI
6. tune it with backtest, walk-forward, or optimize
7. queue a local paper session when you want live-data validation with fake money

## Long Optimize Runs

For long CLI tuning jobs:

- use `palmscript run optimize ...` for direct tuning from the CLI
- save survivors with `--preset-out best.json` so they can be rerun with `run backtest` or `run walk-forward`
- keep the default untouched holdout enabled unless you are intentionally disabling that protection
- add explicit constraints such as `--min-sharpe`, `--min-holdout-pass-rate`, and `--max-overfitting-risk` when you want the optimizer to search only the feasible region
- add `--direct-validate-top <N>` when you want the optimizer to replay the best feasible survivors on the full window automatically
- switch `--diagnostics` to `full-trace` when you want per-bar decision traces instead of only summary diagnostics

## Agent-Oriented Runtime Diagnostics

PalmScript’s runtime is designed to return rich deterministic diagnostics, not only final profit numbers.

Current backtest-oriented outputs include:

- typed order and trade diagnostics
- bounded opportunity events
- cohort summaries and drawdown-path summaries
- source-alignment summaries for missing or synthetic feed updates
- deterministic overfitting-risk summaries, validation-constraint summaries, baseline comparisons, bounded date-perturbation reruns for top-level backtests, feasible vs infeasible optimize survivor counts, constraint-failure breakdowns, optional direct-validation survivor replays, optimize holdout pass-rate data, and improvement hints that stay conservative when no out-of-sample evidence exists
- optional per-bar decision traces with `--diagnostics full-trace`

PalmScript also now includes a local execution daemon for paper sessions. The daemon reuses the same compiled VM and order simulator as backtest mode, drives them from live exchange-backed closed bars under a persistent local paper ledger, and surfaces shared top-of-book / last / mark quote snapshots so agents can inspect live paper-session valuation and feed health directly from the CLI.

That makes the CLI and JSON output suitable for automated strategy iteration by agents as well as manual inspection by humans.

## What To Read Next

- First runnable flow: [Quickstart](quickstart.md)
- First complete strategy walkthrough: [First Strategy](first-strategy.md)
- Big-picture language tour: [Language Overview](language-overview.md)
- Exact rules and semantics: [Reference Overview](../reference/overview.md)

## Documentation Roles

- `Learn` explains how to use PalmScript effectively.
- `Reference` defines what PalmScript means.
