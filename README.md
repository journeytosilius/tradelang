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
- hosted IDE: <https://palmscript.dev/app/>
- local source: [web/docs/docs/index.md](web/docs/docs/index.md)

Start here:

- [Learn](web/docs/docs/learn/overview.md)
- [Language Reference](web/docs/docs/reference/overview.md)
- [Indicators Reference](web/docs/docs/reference/indicators.md)

Repo-local tooling docs:

- [Browser IDE](web/docs/docs-private/tooling/browser-ide.md)

## Common Commands

```bash
cargo build --bin palmscript
cargo build --bin palmscript-ide-server
npm --prefix web/ide run build
target/debug/palmscript check crates/palmscript/examples/strategies/sma_cross.ps
target/debug/palmscript run market crates/palmscript/examples/strategies/sma_cross.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript run market crates/palmscript/examples/strategies/cross_source_spread.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript dump-bytecode crates/palmscript/examples/strategies/sma_cross.ps
mkdocs build --strict -f web/docs/mkdocs.yml
bash infra/scripts/build_docs_site.sh
```

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
`/docs/`, Spanish is served at `/es/docs/`, and future locales follow the same
`/{lang}/docs/` pattern.
