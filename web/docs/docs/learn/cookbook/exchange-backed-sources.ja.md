# Cookbook: 取引所バックのソース

戦略が対応取引所から履歴ローソク足を直接取得する必要がある場合は、名前付きソースを使います。

```palmscript
interval 1m

source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")
use bb 1h

plot(bn.close)
plot(bb.1h.close)
```

PalmScript は Bybit と Gate のソーステンプレートもサポートします。

- `bybit.spot("BTCUSDT")`
- `bybit.usdt_perps("BTCUSDT")`
- `gate.spot("BTC_USDT")`
- `gate.usdt_perps("BTC_USDT")`

リポジトリ内の代表的なサンプル:

- `crates/palmscript/examples/strategies/binance_spot_btcusdt_weekly_trend.ps`
- `crates/palmscript/examples/strategies/binance_usdm_auxiliary_fields.ps`
- `crates/palmscript/examples/strategies/bybit_spot.ps`
- `crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps`
- `crates/palmscript/examples/strategies/gate_spot.ps`
- `crates/palmscript/examples/strategies/gate_usdt_perps_backtest.ps`
- `crates/palmscript/examples/strategies/cross_exchange_bybit_gate_spread.ps`

## ブラウザ IDE で試す

[https://palmscript.dev/](https://palmscript.dev/) を開き、この例をエディタに貼り付け、アプリで利用可能な BTCUSDT 履歴に対して実行してください。

## 確認するポイント

- ソース対応スクリプトでは、ソース修飾された市場シリーズを使う必要がある
- `bb.1h.close` を使う前に `use bb 1h` が必要
- スクリプトには依然として一つのグローバルなベース `interval` がある
- 実行前に、ランタイムは必要な各 `(source, interval)` フィードを解決する
- `binance.usdm` は `funding_rate`, `mark_price`,
  `index_price`, `premium_index`, `basis` の履歴フィールドもサポートする
- Bybit は `BTCUSDT` のような venue ネイティブシンボルを期待する
- Gate は `BTC_USDT` のような venue ネイティブシンボルを期待する
- `run paper` はその Binance USD-M 補助フィールドも同じ履歴フィード経路から初期化し、armed paper session に持ち込む
- `run market`, `run backtest`, `run walk-forward`, `run walk-forward-sweep`,
  `run optimize` は同じ exchange-backed source 宣言を解決する

参照:

- [インターバルとソース](../../reference/intervals-and-sources.md)
