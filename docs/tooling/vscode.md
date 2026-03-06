# VS Code Extension

The first-party VS Code extension provides editor support for `.trl` files.

Marketplace identity:

- display name: `TradeLang`
- publisher: `tradelang`
- extension id: `tradelang.tradelang-vscode`

## Capabilities

- syntax highlighting
- snippets
- diagnostics
- hover
- completions
- definitions
- document symbols
- formatting

## Language Server Resolution

The extension resolves `tradelang-lsp` in this order:

1. `tradelang.server.path`
2. bundled platform binary inside the extension
3. local development fallback in the repository `target/` directory

## Settings

- `tradelang.server.path`
- `tradelang.projectConfigPath`
- `tradelang.trace.server`

## Packaging

Release builds bundle platform-specific `tradelang-lsp` binaries under:

```text
server/<platform>-<arch>/tradelang-lsp
server/<platform>-<arch>/tradelang-lsp.exe
```

See [Release Workflows](../contributing/releases.md) for the publishing pipeline.
