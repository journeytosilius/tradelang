# リファレンス概要

このセクションは、公開ドキュメントとしての PalmScript の規範的定義です。

ガイドページとリファレンスページが異なる場合は、リファレンスが優先されます。

## このセクションが定義するもの

- 字句構造
- 文法
- 宣言とスコープの規則
- 型と値
- シリーズとインデックスのセマンティクス
- 評価セマンティクス
- インターバルとソースの規則
- builtins とインジケーターの契約
- 出力セマンティクス
- 診断クラス

## 現在実装されているもの

現在の PalmScript 表面には次が含まれます。

- スクリプトごとにちょうど一つのトップレベル `interval <...>` ディレクティブ
- 実行可能スクリプトごとに一つ以上の名前付き `source` エイリアス
- `spot.close` や `hl.1h.close` のようなソース修飾シリーズ
- `use <alias> <interval>` による補助インターバル
- 式本体を持つトップレベル `fn` 宣言
- `let`、`const`、`input`、タプル分解、`export`、`trigger`、第一級 `entry` / `exit`、`order`
- リテラル専用のシリーズインデックス、型付き enum リテラル `ma_type.<variant>`、`tif.<variant>`、`trigger_ref.<variant>`、`position_side.<variant>`、`exit_kind.<variant>`、および決定的な三値ブールロジック
- 一部が実行可能で、さらに予約済み名が診断として公開される TA-Lib 風 builtin サーフェス

## 現在の境界

- `interval`、`source`、`use`、`fn`、`const`、`input`、`export`、`trigger`、`entry`、`exit`、`order` はトップレベル専用です
- `close` のような裸の市場識別子は実行可能スクリプトでは有効ではありません
- 上位ソースインターバルには `use <alias> <interval>` が必要です
- 呼び出せるのは識別子だけです
- 文字列リテラルは `source` 宣言の中でのみ有効です
- シリーズインデックスには非負整数リテラルが必要です
- タプル値を返す builtin 結果は、さらに使う前に `let (...) = ...` で直ちに分解しなければなりません

## 読み方

- 受理される構文は [字句構造](lexical-structure.md) と [文法](grammar.md) から始めてください
- 束縛と可視性の規則は [宣言とスコープ](declarations-and-scope.md) を使ってください
- 言語の意味は [評価セマンティクス](evaluation-semantics.md) と [インターバルとソース](intervals-and-sources.md) を使ってください
- 呼び出しと出力の挙動は [Builtins](builtins.md)、[インジケーター](indicators.md)、[出力](outputs.md) を使ってください
