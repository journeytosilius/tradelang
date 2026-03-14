# 最初の戦略

この戦略は 1 分足で実行され、2 本の移動平均を計算し、そのクロスを単純な
ロング専用のエントリー/イグジットフローに変換します。

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")
execution spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow
entry long = crossover(fast, slow)
exit long = crossunder(fast, slow)

order_template market_order = market()
order entry long = market_order
order exit long = market_order
```

## ここで導入されるもの

- `interval 1m` はベース実行クロックを設定します
- `source spot = ...` は一つの取引所バックド市場を束縛します
- `execution spot = ...` は backtest、walk-forward、optimize、paper コマンドで使う実行 venue を束縛します
- `spot.close` はソース修飾されたベースシリーズです
- `let` は再利用可能な式を束縛します
- `export` は名前付き出力シリーズを公開します
- `entry long = ...` はロングエントリーシグナルを出します
- `exit long = ...` はロングイグジットシグナルを出します
- `order_template market_order = market()` は再利用可能な注文定義を宣言します
- `order entry long = market_order` と `order exit long = market_order` はその明示的な設定を再利用します

## ブラウザ IDE で試す

[https://palmscript.dev/](https://palmscript.dev/) を開き、
スクリプトをエディタに貼り付け、ヘッダーの日付コントロールで利用可能な
BTCUSDT 履歴に対して実行してください。診断パネルがクリーンなままで、その
後クロスシグナルからバックテスト要約、trades、orders が埋まるはずです。

## 上位時間足の文脈を加える

```palmscript
interval 1d
source spot = binance.spot("BTCUSDT")
execution spot = binance.spot("BTCUSDT")
use spot 1w

let weekly_basis = ema(spot.1w.close, 8)
export bullish = spot.close > weekly_basis
entry long = bullish and crossover(spot.close, weekly_basis)
exit long = crossunder(spot.close, weekly_basis)
order_template market_order = market()
order entry long = market_order
order exit long = market_order
```

`spot.1w.close`、第一級の `entry` / `exit` シグナル、インデックス、
先読みなしの挙動に関する正確なルールは次を参照してください。

- [シリーズとインデックス](../reference/series-and-indexing.md)
- [インターバルとソース](../reference/intervals-and-sources.md)
- [出力](../reference/outputs.md)
- [評価セマンティクス](../reference/evaluation-semantics.md)
