# 言語概要

PalmScript スクリプトはトップレベルのソースファイルであり、宣言と文で構成
されます。

よく使う構成要素:

- ベース実行クロックのための `interval <...>`
- 市場由来シリーズのための `source` 宣言
- 任意の補助 `use <alias> <interval>` 宣言
- トップレベル関数
- `let`、`const`、`input`、タプル分解、`export`、`regime`、`trigger`、`entry` / `exit`、`order`
- `if / else if / else`
- 演算子、呼び出し、インデックスで構成される式
- `crossover`、`state`、`activated`、`barssince`、`valuewhen` などの helper builtins
- 型付き enum リテラル `ma_type.<variant>`、`tif.<variant>`、`trigger_ref.<variant>`、`position_side.<variant>`、`exit_kind.<variant>`

## スクリプトの形

実行可能な PalmScript スクリプトはデータソースを明示的に名前付けします。

```palmscript
interval 1m
source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")

plot(bn.close - bb.close)
```

## メンタルモデル

- すべてのスクリプトには一つのベースインターバルがあります
- 実行可能スクリプトは一つ以上の `source` バインディングを宣言します
- 市場シリーズは常にソース修飾されます
- シリーズ値は時間とともに変化します
- 上位インターバルはその足が完全に確定したときだけ更新されます
- 履歴不足やソース整列不足は `na` として現れます
- `plot`、`export`、`regime`、`trigger`、戦略宣言は各実行ステップの後に結果を出します

## 正確なルールを知るには

- 構文とトークン: [字句構造](../reference/lexical-structure.md) と [文法](../reference/grammar.md)
- 宣言と可視性: [宣言とスコープ](../reference/declarations-and-scope.md)
- 式とセマンティクス: [評価セマンティクス](../reference/evaluation-semantics.md)
- 市場シリーズのルール: [インターバルとソース](../reference/intervals-and-sources.md)
- インジケーターと helper builtins: [インジケーター](../reference/indicators.md) と [Builtins](../reference/builtins.md)
- 出力: [出力](../reference/outputs.md)

## 最適化メタデータ

数値 `input` は、最適化用の探索空間メタデータをスクリプト内に直接持てるようになりました。

```palmscript
input fast_len = 21 optimize(int, 8, 34, 1)
```

これにより `--param` を渡さない場合でも、`run optimize` と `runs submit optimize` はスクリプトから探索空間を推論できます。
