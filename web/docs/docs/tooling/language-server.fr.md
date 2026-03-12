# Serveur De Langage

Cette page est de nouveau publique parce que PalmScript est maintenant open source. Une localisation complete sera publiee dans une mise a jour ulterieure. En attendant, le contenu canonique en anglais est inclus ci-dessous afin que cette version du site expose la meme surface publique CLI et tooling.

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
