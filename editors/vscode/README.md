# TradeLang VS Code Extension

The canonical documentation for the editor stack now lives in the MkDocs site:

- [VS Code Extension](../../docs/tooling/vscode.md)
- [Language Server](../../docs/tooling/language-server.md)
- [.tradelang.json](../../docs/tooling/project-config.md)
- [Release Workflows](../../docs/contributing/releases.md)

This file remains the short repository-local development note.

## Development

From the repository root:

```bash
cargo build --bin tradelang-lsp
cd editors/vscode
npm install
npm run compile
```

The extension resolves the language server in this order:

1. `tradelang.server.path`
2. bundled binary in `server/<platform>-<arch>/`
3. local repo fallback in `../../target/debug/` or `../../target/release/`

## Packaging

```bash
npm run verify:server
npm run package:vsix
```
