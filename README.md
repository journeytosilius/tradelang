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
- hosted IDE: <https://palmscript.dev/app/>
- local source: [docs/index.md](docs/index.md)

Start here:

- [Learn](docs/learn/overview.md)
- [Language Reference](docs/reference/overview.md)
- [Indicators Reference](docs/reference/indicators.md)

Repo-local tooling docs:

- [Browser IDE](docs-private/tooling/browser-ide.md)

## Common Commands

```bash
cargo build --bin palmscript
cargo build --bin palmscript-ide-server
cargo test --manifest-path ide-wasm/Cargo.toml
target/debug/palmscript check examples/strategies/sma_cross.ps
target/debug/palmscript run market examples/strategies/sma_cross.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript run market examples/strategies/cross_source_spread.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript dump-bytecode examples/strategies/sma_cross.ps
mkdocs build --strict
```

## Browser IDE Container

```bash
bash scripts/build_ide_wasm.sh
docker build -f Dockerfile.ide -t palmscript-ide .
docker run --rm -p 8080:8080 palmscript-ide
```

The browser IDE shell now ships as an `iced` Rust frontend compiled to WASM,
embedded directly by the `palmscript-ide-server` binary. Refresh the checked-in
browser bundle with `bash scripts/build_ide_wasm.sh` when you change the
frontend crate under `ide-wasm/`.

The WASM shell keeps the same blue-grey and accent-blue visual language as the
published docs at <https://palmscript.dev/docs/>.

The public demo keeps the chrome intentionally minimal: one editor buffer, a
calendar date-range picker over the curated BTCUSDT dataset, diagnostics, and
backtest output panels. Day clicks apply immediately and the calendar panels
float over the toolbar instead of resizing it. The editor supports browser
copy/cut/paste shortcuts and semantic token coloring backed by the shared IDE
analysis pipeline. On a fresh hosted session, the shell also attempts a
clipboard-read preflight so paste permission is warmed before the first editor
paste in browsers that allow it. The toolbar uses a centered PalmScript logo
mark instead of a text title.

The hosted reverse-proxy entrypoint is `/app/`. `https://palmscript.dev/app`
redirects there.
