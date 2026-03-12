# PalmScript VS Code Extension

This file is a short repository-local development note for the VS Code extension.

The public PalmScript documentation site is language-focused and does not publish editor-stack internals. Private repo-only notes now live under:

- `../../web/docs/docs-private/tooling/vscode.md`
- `../../web/docs/docs-private/tooling/language-server.md`
- `../../web/docs/docs-private/contributing/releases.md`

The extension now gets callable completion snippets such as `sar(...)` and
`crossover(...)` from the shared PalmScript language server metadata, so VS
Code and the hosted Monaco editor accept the same function-call completions,
including while the active line is still syntactically incomplete.

## Development

From the repository root:

```bash
cargo build --bin palmscript-lsp
cd editors/vscode
npm install
npm run compile
```

The extension resolves the language server in this order:

1. `palmscript.server.path`
2. bundled binary in `server/<platform>-<arch>/`
3. local repo fallback in `../../target/debug/` or `../../target/release/`

For compatibility with older local bundles, the extension also accepts the
legacy non-Windows binary name `tradelang-lsp` when resolving a bundled server.

## Packaging

```bash
npm run verify:server
npm run package:vsix
```
