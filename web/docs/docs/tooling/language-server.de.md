# Language Server

Diese Seite ist wieder oeffentlich verfuegbar, weil PalmScript jetzt Open Source ist. Eine vollstaendige Lokalisierung folgt in einer spaeteren Aktualisierung. Bis dahin steht unten der kanonische englische Inhalt, damit diese Sprachversion dieselbe oeffentliche CLI- und Tooling-Oberflaeche zeigt.

## English Canonical Content


`palmscript-lsp` is the first-party language server for PalmScript.

## Role

It is a thin stdio LSP wrapper over the library's IDE analysis APIs. It does not reimplement parsing, semantic analysis, or formatting.

## Supported Features

- diagnostics
- completions
- hover
- go-to-definition
- document symbols
- formatting

## Diagnostics

Diagnostics come from the same compiler-backed analysis used by the CLI. The goal is to surface source problems before a strategy is run.

## Source Of Truth

Language-server behavior should track the same rules documented in `Reference`. If the editor experience and a `Reference` page ever disagree, the language server is expected to be brought back into line with the reference and implementation.
