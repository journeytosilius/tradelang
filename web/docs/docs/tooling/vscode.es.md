# Extension De VS Code

Esta pagina vuelve a estar disponible publicamente porque PalmScript ahora es de codigo abierto. La localizacion completa se publicara en una actualizacion posterior. Mientras tanto, el contenido canonico en ingles se incluye abajo para que esta version del sitio exponga la misma superficie publica de CLI y herramientas.

## English Canonical Content


The first-party VS Code extension provides editor support for PalmScript source files.

- source extension: `.ps`

Marketplace identity:

- display name: `PalmScript`
- publisher: `journeytosilius`
- extension id: `journeytosilius.palmscript-vscode`
- marketplace icon: `editors/vscode/images/palmscript.png`

## Capabilities

- syntax highlighting
- snippets
- diagnostics
- hover
- completions
- callable completion snippets
- definitions
- document symbols
- formatting

The extension is intentionally thin. Language semantics, diagnostics,
completion data, callable completion snippets, and formatting come from
`palmscript-lsp` rather than a second parser or analyzer inside the extension.
Builtin completions remain available even while the current line is incomplete,
so typing through partial assignments still keeps the language suggestions open.

## Language Server Resolution

The extension resolves `palmscript-lsp` in this order:

1. `palmscript.server.path`
2. bundled platform binary inside the extension
3. local development fallback in the repository `target/` directory

## Settings

- `palmscript.server.path`
- `palmscript.trace.server`

## Packaging

Release builds bundle platform-specific `palmscript-lsp` binaries under:

```text
server/<platform>-<arch>/palmscript-lsp
server/<platform>-<arch>/palmscript-lsp.exe
```

The extension also tolerates the legacy non-Windows bundled filename
`tradelang-lsp` so locally checked-out older bundles still activate the
language server instead of falling back to syntax-only editing.

Contributor-only release workflow notes remain in the repository and are not
part of the public documentation site.
