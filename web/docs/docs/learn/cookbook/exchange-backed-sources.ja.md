# Cookbook: 取引所バックのソース

戦略が対応取引所から履歴ローソク足を直接取得する必要がある場合は、名前付きソースを使います。

```palmscript
interval 1m

source bn = binance.spot("BTCUSDT")
source hl = hyperliquid.perps("BTC")
use hl 1h

plot(bn.close)
plot(hl.1h.close)
```

## ブラウザ IDE で試す

[https://palmscript.dev/app/](https://palmscript.dev/app/) を開き、この例をエディタに貼り付け、アプリで利用可能な BTCUSDT 履歴に対して実行してください。

## 確認するポイント

- ソース対応スクリプトでは、ソース修飾された市場シリーズを使う必要がある
- `hl.1h.close` を使う前に `use hl 1h` が必要
- スクリプトには依然として一つのグローバルなベース `interval` がある
- 実行前に、ランタイムは必要な各 `(source, interval)` フィードを解決する

参照:

- [インターバルとソース](../../reference/intervals-and-sources.md)
