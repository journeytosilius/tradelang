# Math, Price, and Statistics Indicators

このページは、PalmScript の実行可能な math transform、price transform、statistics 系インジケーターを定義します。

## TA-Lib Math Transforms

現在実行可能な builtin:

- `acos(real)`
- `asin(real)`
- `atan(real)`
- `ceil(real)`
- `cos(real)`
- `cosh(real)`
- `exp(real)`
- `floor(real)`
- `ln(real)`
- `log10(real)`
- `sin(real)`
- `sinh(real)`
- `sqrt(real)`
- `tan(real)`
- `tanh(real)`

ルール:

- それぞれちょうど一つの数値または `series<float>` 引数を取る
- 入力がシリーズなら結果型は `series<float>`
- 入力がスカラーなら結果型は `float`
- 入力が `na` なら結果は `na`

## TA-Lib Arithmetic and Price Transforms

現在実行可能な builtin:

- `add(a, b)`
- `div(a, b)`
- `mult(a, b)`
- `sub(a, b)`
- `avgprice(open, high, low, close)`
- `bop(open, high, low, close)`
- `medprice(high, low)`
- `typprice(high, low, close)`
- `wclprice(high, low, close)`

ルール:

- すべての引数は数値、`series<float>`、または `na`
- いずれかの引数がシリーズなら結果型は `series<float>`
- そうでなければ結果型は `float`
- 必要な入力のいずれかが `na` なら結果は `na`

追加の OHLC ルール:

- `bop` は `(close - open) / (high - low)` を返し、`high - low <= 0` のときは `0` を返す

## `max(series[, length=30])`, `min(series[, length=30])`, `sum(series[, length=30])`

ルール:

- 第一引数は `series<float>`
- 任意の trailing window の既定値は `30`
- 指定する場合、window は `2` 以上の整数リテラル
- window は現在サンプルを含む
- 十分な履歴がなければ結果は `na`
- 必要な window に `na` が含まれると結果は `na`
- 結果型は `series<float>`

## `avgdev(series[, length=14])`

ルール:

- 第一引数は `series<float>`
- 任意の `length` の既定値は `14`
- 指定する場合、`length` は `2` 以上の整数リテラル
- 結果型は `series<float>`
- 十分な履歴がなければ現在サンプルは `na`
- 必要な window に `na` が含まれると現在サンプルは `na`

## `maxindex(series[, length=30])` と `minindex(series[, length=30])`

ルール:

- 第一引数は `series<float>`
- 任意の `length` の既定値は `30`
- 指定する場合、`length` は `2` 以上の整数リテラル
- `maxindex` と `minindex` は絶対 bar index を `f64` として含む `series<float>` を返す
- 十分な履歴がなければ現在サンプルは `na`
- 必要な window に `na` が含まれると現在サンプルは `na`

## `minmax(series[, length=30])` と `minmaxindex(series[, length=30])`

ルール:

- 第一引数は `series<float>`
- 任意の `length` の既定値は `30`
- 指定する場合、`length` は `2` 以上の整数リテラル
- `minmax` は TA-Lib 出力順の 2 要素タプル `(min_value, max_value)` を返す
- `minmaxindex` は TA-Lib 出力順の 2 要素タプル `(min_index, max_index)` を返す
- タプル値出力はさらに使う前に分解しなければならない
- 十分な履歴がなければ現在サンプルは `na`
- 必要な window に `na` が含まれると現在サンプルは `na`

## `stddev(series[, length=5[, deviations=1.0]])` と `var(series[, length=5[, deviations=1.0]])`

ルール:

- 第一引数は `series<float>`
- 任意の `length` の既定値は `5`
- 指定する場合、`length` は整数リテラル
- `stddev` では `length >= 2` が必要
- `var` では `length >= 1` を許可
- `deviations` の既定値は `1.0`
- `stddev` は rolling variance の平方根に `deviations` を掛ける
- `var` は TA-Lib に合わせて `deviations` 引数を無視する
- 結果型は `series<float>`
- 十分な履歴がなければ現在サンプルは `na`
- 必要な window に `na` が含まれると現在サンプルは `na`

## `beta(series0, series1[, length=5])` と `correl(series0, series1[, length=30])`

ルール:

- 両入力は `series<float>`
- `beta` の既定値は `length=5`
- `correl` の既定値は `length=30`
- 指定する場合、`length` はその builtin の TA-Lib 最小条件を満たす整数リテラル
- `beta` は TA-Lib の return-ratio 方式に従うため、最初の出力は `length + 1` 個のソースサンプル後に現れる
- `correl` は対応する生入力 series の Pearson correlation を返す
- 結果型は `series<float>`
- 十分な履歴がなければ現在サンプルは `na`
- 必要な window に `na` が含まれると現在サンプルは `na`
