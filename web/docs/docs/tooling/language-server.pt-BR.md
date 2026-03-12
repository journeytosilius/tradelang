# Servidor De Linguagem

Esta pagina voltou a ficar disponivel publicamente porque o PalmScript agora e open source. A localizacao completa sera publicada em uma atualizacao posterior. Enquanto isso, o conteudo canonico em ingles aparece abaixo para que esta versao do site exponha a mesma superficie publica de CLI e ferramentas.

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
