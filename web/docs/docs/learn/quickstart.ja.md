# クイックスタート

## 1. ブラウザ IDE を開く

ホストされている IDE を使います:
[https://palmscript.dev/app/](https://palmscript.dev/app/)

## 2. スクリプトを貼り付ける

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow
plot(spot.close)
```

## 3. 診断を確認する

エディタは入力中にスクリプトを検査し、コンパイル診断があれば右側パネルに
表示します。

## 4. バックテストを実行する

日付範囲を選び、`Run Backtest` を押して、アプリ内の利用可能な BTCUSDT
履歴に対してスクリプトを実行します。

次へ: [最初の戦略](first-strategy.md)
