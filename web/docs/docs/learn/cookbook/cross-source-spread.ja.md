# Cookbook: クロスソース・スプレッド

このパターンは、同じベースクロック上で二つの名前付き市場を比較します。

```palmscript
interval 1m

source spot = binance.spot("BTCUSDT")
source perp = binance.usdm("BTCUSDT")

let spread = spot.close - perp.close
plot(spread)
```

## なぜ重要か

ソース対応実行では、ベースクロックは宣言された各ソースのベースタイムスタンプの和集合から構築されます。

つまり次の意味になります。

- 戦略はベースインターバルごとに一度だけ実行される
- あるステップで一方のソースが欠けている場合、そのソースは `na` を返す
- その欠落入力に依存する式も通常のセマンティクスに従って `na` を伝播する

参照:

- [評価セマンティクス](../../reference/evaluation-semantics.md)
- [インターバルとソース](../../reference/intervals-and-sources.md)
