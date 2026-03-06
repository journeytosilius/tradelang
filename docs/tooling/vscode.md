# VS Code Extension

The first-party VS Code extension provides editor support for `.trl` files.

Marketplace identity:

- display name: `PalmScript`
- publisher: `palmscript`
- extension id: `palmscript.palmscript-vscode`

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

The extension resolves `palmscript-lsp` in this order:

1. `palmscript.server.path`
2. bundled platform binary inside the extension
3. local development fallback in the repository `target/` directory

## Settings

- `palmscript.server.path`
- `palmscript.projectConfigPath`
- `palmscript.trace.server`

## Packaging

Release builds bundle platform-specific `palmscript-lsp` binaries under:

```text
server/<platform>-<arch>/palmscript-lsp
server/<platform>-<arch>/palmscript-lsp.exe
```

See [Release Workflows](../contributing/releases.md) for the publishing pipeline.
