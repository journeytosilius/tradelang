# インターバルとソース

このページは、PalmScript の規範的なインターバル規則とソース規則を定義します。

## サポートされるインターバル

PalmScript は [インターバル一覧](intervals.md) に記載されたインターバルリテラルを受け付けます。インターバルは大文字小文字を区別します。

## ベースインターバル

各スクリプトはちょうど一つのベースインターバルを宣言します。

```palmscript
interval 1m
```

ベースインターバルは実行クロックを定義します。

## 名前付きソース

実行可能なスクリプトは、一つ以上の名前付き取引所ソースを宣言します。

```palmscript
interval 1m
source bb = bybit.usdt_perps("BTCUSDT")
source bn = binance.spot("BTCUSDT")
use bb 1h

plot(bn.close - bb.1h.close)
```

ルール:

- 少なくとも一つの `source` 宣言が必要
- 市場シリーズはソース修飾されていなければならない
- 各宣言済みソースは、スクリプトのベースインターバル上でベースフィードを提供する
- `use <alias> <interval>` は、そのソースの追加インターバルを宣言する
- `<alias>.<field>` は、そのソースのベースインターバルを参照する
- `<alias>.<interval>.<field>` は、そのソースの指定インターバルを参照する
- ベースより低いインターバル参照は拒否される

## サポートされるソーステンプレート

PalmScript は現在、次の第一級テンプレートをサポートします。

- `binance.spot("<symbol>")`
- `binance.usdm("<symbol>")`
- `bybit.spot("<symbol>")`
- `bybit.usdt_perps("<symbol>")`
- `gate.spot("<symbol>")`
- `gate.usdt_perps("<symbol>")`

インターバル対応はテンプレートごとに異なります。

- `binance.spot` は PalmScript の全サポートインターバルを受け付ける
- `binance.usdm` は PalmScript の全サポートインターバルを受け付ける
- `bybit.spot` は `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w`, `1M` を受け付ける
- `bybit.usdt_perps` は `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w`, `1M` を受け付ける
- `gate.spot` は `1s`, `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h`, `1d`, `1M` を受け付ける
- `gate.usdt_perps` は `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h`, `1d` を受け付ける

運用上の取得制約もテンプレートごとに異なります。

- Bybit は `BTCUSDT` のような venue ネイティブシンボルを使う
- Gate は `BTC_USDT` のような venue ネイティブシンボルを使う
- Bybit REST の kline は降順で返るため、PalmScript はランタイム整列検証の前に並べ替える
- Bybit の spot / perp kline タイムスタンプは JSON 整数または整数風文字列で返る場合があり、PalmScript はその両方を直接受け付ける
- Gate のローソク足 API は Unix 秒を使い、PalmScript はそれを UTC の Unix ミリ秒に正規化する
- Gate spot / futures のページングは、公開 API が `limit` と `from` / `to` を同時に許可しないため時間窓単位で行う
- Gate spot / futures への HTTP リクエストは 1 回あたり 1000 本のローソク足に制限され、広すぎる範囲による `400 Bad Request` を避ける
- Binance / Bybit / Gate フィードは内部でページ分割される
- venue 取得が失敗した場合、PalmScript はリクエスト URL とレスポンス本文の切り詰めた断片を表示する。これは non-200 の HTTP 失敗と JSON ペイロード破損の両方に適用される
- ベース URL は `PALMSCRIPT_BINANCE_SPOT_BASE_URL`,
  `PALMSCRIPT_BINANCE_USDM_BASE_URL`, `PALMSCRIPT_BYBIT_BASE_URL`,
  `PALMSCRIPT_GATE_BASE_URL` で上書きできる。Gate では
  `https://api.gateio.ws` のようなホストルートでも、完全な `/api/v4`
  ベース URL でも利用できる

## ソースフィールド集合

すべてのソーステンプレートは、同じ正規化された市場フィールドにそろえられます。

- `time`
- `open`
- `high`
- `low`
- `close`
- `volume`

ルール:

- `time` は UTC の Unix ミリ秒で表したローソク足の始値時刻
- 価格と出来高フィールドは数値
- `binance.usdm("<symbol>")` は履歴専用の補助フィールド `funding_rate`,
  `mark_price`, `index_price`, `premium_index`, `basis`
  も公開する
- これらの補助フィールドは `binance.usdm` エイリアスでのみ有効
- 履歴モードは、スクリプトが参照したときにそれらを自動取得する
- Binance USD-M の補助エンドポイントが要求ウィンドウで行を返さない場合、そのフィールドはスクリプトや paper session を中断せず、そのウィンドウでは `na` のままになる
- `run paper` はこれらの補助データセットも共有 paper feed cache に初期化し、armed paper session で使える状態を維持する

## 等しい / 上位 / 下位インターバル

PalmScript は、参照インターバルをベースインターバルとの比較で三つに分けます。

- 等しいインターバル: 有効
- 上位インターバル: `use <alias> <interval>` で宣言されていれば有効
- 下位インターバル: 拒否される

## ランタイムセマンティクス

market mode では:

- PalmScript は必要な `(source, interval)` フィードを venue から直接取得する
- ベース実行タイムラインは、宣言された全ソースのベースインターバルのバー始値時刻の和集合
- タイムライン上のあるステップで一方のソースにベースバーがなければ、そのソースはそのステップで `na` を返す
- より遅いソースインターバルは、次のクローズ境界まで最後に完全確定した値を保持する

## ノールックアヘッド保証

PalmScript は、上位インターバルのローソク足が完全に確定する前にその値を公開してはなりません。

これは `bb.1h.close` のようなソース対応の修飾インターバルにも適用されます。

## ランタイム整列ルール

準備されたフィードは、宣言されたインターバル境界に整列していなければなりません。

ランタイムは次のフィードを拒否します。

- インターバル境界に整列していない
- 未ソート
- 同一インターバル始値時刻に重複がある
