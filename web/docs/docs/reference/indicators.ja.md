# インジケーター概要

このセクションは、PalmScript の実行可能なインジケーターサーフェスを定義します。

言語全体で共有される callable ルール、ヘルパー builtins、`plot`、タプル分解ルールについては [Builtins](builtins.md) を使ってください。

## インジケーターファミリー

PalmScript は現在、次のファミリーでインジケーターを文書化しています。

- [Trend and Overlap](indicators-trend-and-overlap.md)
- [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md)
- [Math, Price, and Statistics](indicators-math-price-statistics.md)

## 共通インジケータールール

ルール:

- インジケーター名は builtin 識別子なので、`ema(spot.close, 20)` のように直接呼び出す
- インジケーター入力も [インターバルとソース](intervals-and-sources.md) のソース修飾シリーズ規則に従う必要がある
- 任意の length 引数は、各ファミリーページに記載された TA-Lib 既定値を使う
- リテラルと説明されている length 系引数は、ソースコード中で整数リテラルでなければならない
- タプル値を返すインジケーターは、さらに使う前に `let (...) = ...` で分解しなければならない
- インジケーター出力は、シリーズ入力が示す更新クロックに従う
- 個別の契約が別の規則を定義しない限り、インジケーターは `na` を伝播する

## タプル値インジケーター

現在のタプル値インジケーターは次のとおりです。

- `macd(series, fast_length, slow_length, signal_length)`
- `minmax(series[, length=30])`
- `minmaxindex(series[, length=30])`
- `aroon(high, low[, length=14])`

これらは直ちに分解する必要があります。

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let (line, signal, hist) = macd(spot.close, 12, 26, 9)
plot(line)
```

## 実行可能名と予約済み TA-Lib 名

PalmScript は、現在実行できるよりも広い TA-Lib カタログ名を予約しています。

- これらのインジケーターページは実行可能な部分集合を定義する
- [TA-Lib Surface](ta-lib.md) は、より広い予約名とメタデータのサーフェスを定義する
- 予約済みだが未実装の TA-Lib 名を呼ぶと、決定的なコンパイル診断が返る
