# 言語サーバー

PalmScript がオープンソース化されたため、このページは再び公開されました。完全な翻訳は後続の更新で追加します。それまでは、この言語版のサイトでも同じ公開 CLI / ツール内容を参照できるよう、下に英語の正規版を掲載します。

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
