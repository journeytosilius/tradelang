# Cookbook: マルチインターバル戦略

このパターンは、より速い、または同じベース戦略に対して、より遅いコンテキストを追加します。

```palmscript
interval 1d
source spot = binance.spot("BTCUSDT")
use spot 1w

let weekly_basis = ema(spot.1w.close, 8)

if spot.close > weekly_basis {
    plot(1)
} else {
    plot(0)
}
```

## ブラウザ IDE で試す

[https://palmscript.dev/app/](https://palmscript.dev/app/) を開き、この例をエディタに貼り付け、複数の週足終値を含む日付範囲で実行してください。

## 確認するポイント

- `spot.1w.close` を使う前に `use spot 1w` が必要
- 上位インターバルの値は、その上位足が完全に確定した後でのみ現れる
- 未確定の週足は公開されない
- インデックスはベースクロックではなく、遅いインターバルの更新クロックで合成される

参照:

- [インターバルとソース](../../reference/intervals-and-sources.md)
- [シリーズとインデックス](../../reference/series-and-indexing.md)
- [評価セマンティクス](../../reference/evaluation-semantics.md)
