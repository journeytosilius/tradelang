# PalmScript

<p align="center">
  <img src="editors/vscode/images/palmscript.png" alt="PalmScript logo" width="220">
</p>

PalmScript is a language for financial time-series strategies.

The public documentation is focused on:

- language syntax and semantics
- builtins and indicators
- examples and learning guides
- the basic CLI flow for checking and running scripts

Documentation:

- published site: <https://palmscript.dev/docs/>
- Spanish docs: <https://palmscript.dev/es/docs/>
- Portuguese (Brazil) docs: <https://palmscript.dev/pt-BR/docs/>
- German docs: <https://palmscript.dev/de/docs/>
- Japanese docs: <https://palmscript.dev/ja/docs/>
- French docs: <https://palmscript.dev/fr/docs/>
- hosted IDE: <https://palmscript.dev/app/>
- local source: [web/docs/docs/index.md](web/docs/docs/index.md)

Start here:

- [Learn](web/docs/docs/learn/overview.md)
- [Language Reference](web/docs/docs/reference/overview.md)
- [Indicators Reference](web/docs/docs/reference/indicators.md)

Repo-local tooling docs:

- [Browser IDE](web/docs/docs/tooling/browser-ide.md)

## Common Commands

```bash
cargo build --bin palmscript
cargo build --bin palmscript-ide-server
npm --prefix web/ide run build
target/debug/palmscript check crates/palmscript/examples/strategies/sma_cross.ps
target/debug/palmscript docs --list
target/debug/palmscript docs --all
target/debug/palmscript run market crates/palmscript/examples/strategies/sma_cross.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript run market crates/palmscript/examples/strategies/cross_source_spread.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript run market crates/palmscript/examples/strategies/bybit_spot.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript run market crates/palmscript/examples/strategies/gate_spot.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript run backtest crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps --from 1704067200000 --to 1704153600000 --leverage 2
target/debug/palmscript run backtest crates/palmscript/examples/strategies/gate_usdt_perps_backtest.ps --from 1704067200000 --to 1704153600000 --leverage 2
target/debug/palmscript runs submit optimize crates/palmscript/examples/strategies/adaptive_trend_backtest.ps --from 1646611200000 --to 1772841600000 --train-bars 252 --test-bars 63 --step-bars 63 --param int:fast_len=8:34 --param float:target_atr_mult=1.5:4.0 --trials 50
target/debug/palmscript runs serve
target/debug/palmscript dump-bytecode crates/palmscript/examples/strategies/sma_cross.ps
mkdocs build --strict -f web/docs/mkdocs.yml
sh infra/scripts/build_docs_site.sh
```

`run optimize` and `runs submit optimize` now default to walk-forward tuning with a final untouched holdout window reserved from the tail of the selected execution range. By default that holdout size matches `--test-bars`. PalmScript also supports first-class `regime` declarations backed by the `state(enter, exit)` builtin for persistent market-state logic.

Exchange-backed source endpoints can be overridden with environment variables for mock servers and venue-specific routing:

- `PALMSCRIPT_BINANCE_SPOT_BASE_URL`
- `PALMSCRIPT_BINANCE_USDM_BASE_URL`
- `PALMSCRIPT_BYBIT_BASE_URL`
- `PALMSCRIPT_GATE_BASE_URL`

## Browser IDE Container

```bash
bash infra/scripts/build_ide_web.sh
docker build -f infra/docker/Dockerfile.ide -t palmscript-ide .
docker run --rm -p 8080:8080 palmscript-ide
```

The browser IDE shell now ships as a Vite-built React and TypeScript frontend
using Monaco Editor, embedded directly by the `palmscript-ide-server` binary.
Refresh the checked-in browser bundle with `bash infra/scripts/build_ide_web.sh`
when you change the frontend under `web/ide/`.

The web shell keeps the same blue-grey and accent-blue visual language as the
published docs at <https://palmscript.dev/docs/>.

The public demo keeps the chrome intentionally minimal: one editor buffer, a
calendar date-range picker over the available BTCUSDT dataset history, Monaco editing,
compile diagnostics, Monaco hover and completion docs for builtins and language
constructs, callable completion snippets in Monaco and VS Code, and backtest
output panels. The toolbar keeps the PalmScript logo inside the header instead
of a text title, plus a light/dark mode switch. Dark mode uses a VS Code-like
shell with a Dracula-inspired Monaco theme, the shell typography uses Inter,
and the browser tab favicon now matches the current PalmScript logo.

The hosted reverse-proxy entrypoint is `/app/`. `https://palmscript.dev/app`
redirects there.

The documentation build is locale-aware. English is the canonical default at
`/docs/`, Spanish is served at `/es/docs/`, Portuguese (Brazil) is served at
`/pt-BR/docs/`, German is served at `/de/docs/`, Japanese is served at
`/ja/docs/`, French is served at `/fr/docs/`, and future locales follow the
same `/{lang}/docs/` pattern.
