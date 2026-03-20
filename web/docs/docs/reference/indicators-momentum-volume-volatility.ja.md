# Momentum, Volume, and Volatility Indicators

このページは、PalmScript の実行可能な momentum、oscillator、volume、volatility 系インジケーターを定義します。

## `rsi(series, length)`

ルール:

- 引数はちょうど二つ
- 第一引数は `series<float>`
- 第二引数は正の整数リテラル
- 結果型は `series<float>`
- インジケーター状態を seed するのに十分な履歴があるまで series は `na` を返す

## `roc(series[, length=10])`, `mom(series[, length=10])`, `rocp(series[, length=10])`, `rocr(series[, length=10])`, `rocr100(series[, length=10])`

ルール:

- 第一引数は `series<float>`
- 任意の `length` は正の整数リテラル
- `length` を省略した場合は TA-Lib 既定値 `10`
- `roc` は `((series - series[length]) / series[length]) * 100` として評価される
- `mom` は `series - series[length]` として評価される
- `rocp` は `(series - series[length]) / series[length]` として評価される
- `rocr` は `series / series[length]` として評価される
- `rocr100` は `(series / series[length]) * 100` として評価される
- 現在サンプルまたは参照サンプルが `na` の場合、結果は `na`
- `series[length]` が `0` の場合、`roc`, `rocp`, `rocr`, `rocr100` は `na` を返す

## `cmo(series[, length=14])`

ルール:

- 第一引数は `series<float>`
- `length` を省略した場合は TA-Lib 既定値 `14`
- 指定する場合、`length` は `2` 以上の整数リテラル
- `cmo` は TA-Lib の Wilder 風に平滑化された gain / loss 状態を使う
- 結果型は `series<float>`
- 平滑化 gain と loss の合計が `0` の場合、`cmo` は `0` を返す

## `cci(high, low, close[, length=14])`

ルール:

- 最初の三引数は `series<float>`
- `length` を省略した場合は TA-Lib 既定値 `14`
- 指定する場合、`length` は `2` 以上の整数リテラル
- `cci` は要求 window 上の trailing typical-price average と mean deviation を使う
- 現在の typical-price delta または mean deviation が `0` の場合、`cci` は `0` を返す
- 結果型は `series<float>`

## `aroon(high, low[, length=14])` と `aroonosc(high, low[, length=14])`

ルール:

- 最初の二引数は `series<float>`
- `length` を省略した場合は TA-Lib 既定値 `14`
- 指定する場合、`length` は `2` 以上の整数リテラル
- `aroon` は TA-Lib lookback に合わせるため、`length + 1` の trailing high/low window を使う
- `aroon` は TA-Lib 出力順の 2 要素タプル `(aroon_down, aroon_up)` を返す
- `aroonosc` は `aroon_up - aroon_down` を返す
- タプル値出力はさらに使う前に分解しなければならない

## `plus_dm(high, low[, length=14])`, `minus_dm(high, low[, length=14])`, `plus_di(high, low, close[, length=14])`, `minus_di(high, low, close[, length=14])`, `dx(high, low, close[, length=14])`, `adx(high, low, close[, length=14])`, `adxr(high, low, close[, length=14])`

ルール:

- すべての価格引数は `series<float>`
- `length` を省略した場合は TA-Lib 既定値 `14`
- 指定する場合、`length` は正の整数リテラル
- `plus_dm` と `minus_dm` は Wilder 平滑化された directional movement を返す
- `plus_di` と `minus_di` は Wilder directional indicator を返す
- `dx` は絶対 directional spread を 100 倍した値を返す
- `adx` は `dx` の Wilder average を返す
- `adxr` は現在 `adx` と遅延 `adx` の平均を返す
- アクティブバーで必要な価格入力のいずれかが `na` の場合、そのバーの結果は `na`
- 結果型は `series<float>`

## `atr(high, low, close[, length=14])` と `natr(high, low, close[, length=14])`

ルール:

- すべての引数は `series<float>`
- `length` を省略した場合は TA-Lib 既定値 `14`
- 指定する場合、`length` は正の整数リテラル
- `atr` は最初の average true range から seed され、その後 Wilder smoothing を適用する
- `natr` は `(atr / close) * 100` を返す
- アクティブバーで必要な価格入力のいずれかが `na` の場合、そのバーの結果は `na`
- 結果型は `series<float>`

## `willr(high, low, close[, length=14])`

ルール:

- 最初の三引数は `series<float>`
- `length` を省略した場合は TA-Lib 既定値 `14`
- 指定する場合、`length` は `2` 以上の整数リテラル
- `willr` は要求 window 上の trailing highest high と lowest low を使う
- 結果型は `series<float>`
- trailing high-low range が `0` の場合、`willr` は `0` を返す

## `mfi(high, low, close, volume[, length=14])` と `imi(open, close[, length=14])`

ルール:

- すべての引数は `series<float>`
- `length` を省略した場合は TA-Lib 既定値 `14`
- 指定する場合、`length` は正の整数リテラル
- `mfi` は trailing window 上の typical price と money flow を使う
- `imi` は要求 window 上の intraday open-close movement を使う
- 結果型は `series<float>`

## `stoch(high, low, close[, fast_k=5[, slow_k=3[, slow_k_ma=ma_type.sma[, slow_d=3[, slow_d_ma=ma_type.sma]]]]])`, `stochf(high, low, close[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]])`, `stochrsi(series[, time_period=14[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]]])`

ルール:

- すべての価格または source 引数は `series<float>`
- period を省略した場合は TA-Lib 既定値を使う
- `fast_k`, `slow_k`, `fast_d`, `slow_d` の length は正の整数リテラル
- `stochrsi` の `time_period` は `2` 以上の整数リテラル
- すべての MA 引数は型付き `ma_type.<variant>` 値
- `stoch` は TA-Lib 順の `(slowk, slowd)` を返す
- `stochf` は TA-Lib 順の `(fastk, fastd)` を返す
- `stochrsi` は TA-Lib 順の `(fastk, fastd)` を返す
- タプル値出力はさらに使う前に分解しなければならない

## `ad(high, low, close, volume)`, `adosc(high, low, close, volume[, fast_length=3[, slow_length=10]])`, `obv(series, volume)`

ルール:

- すべての引数は `series<float>`
- `ad` は cumulative accumulation/distribution line を返す
- `adosc` は accumulation/distribution line の fast EMA と slow EMA の差を返す
- `fast_length` と `slow_length` を省略した場合は TA-Lib 既定値 `3` と `10`
- `obv` は現在の `volume` から seed され、その後は価格方向に応じて volume を加算または減算する
- 必要な価格または volume サンプルが `na` の場合、結果は `na`
- 結果型は `series<float>`

## `trange(high, low, close)`

ルール:

- すべての引数は `series<float>`
- 最初の出力サンプルは `na`
- 以降のサンプルは、現在 `high`、現在 `low`、前回 `close` に基づく TA-Lib true range セマンティクスを使う
- 必要サンプルのいずれかが `na` の場合、結果は `na`
- 結果型は `series<float>`

## `anchored_vwap(anchor, price, volume)`

Rules:

- `anchor` must be `series<bool>`
- `price` and `volume` must be `series<float>`
- when the current `anchor` sample is `true`, the running VWAP resets on that same bar
- the anchor bar is included in the new anchored accumulation window
- if the current anchor, price, or volume sample is `na`, the current output sample is `na`
- if cumulative anchored volume is `0`, the current output sample is `na`
- the result type is `series<float>`
