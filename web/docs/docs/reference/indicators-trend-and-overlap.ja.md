# Trend and Overlap Indicators

このページは、PalmScript の実行可能な trend と overlap 系インジケーターを定義します。

## `sma(series, length)`

ルール:

- 引数はちょうど二つ
- 第一引数は `series<float>`
- 第二引数は正の整数リテラル
- 結果型は `series<float>`
- 十分な履歴がなければ現在サンプルは `na`
- 必要な window に `na` が含まれると現在サンプルは `na`

## `ema(series, length)`

ルール:

- 引数はちょうど二つ
- 第一引数は `series<float>`
- 第二引数は正の整数リテラル
- 結果型は `series<float>`
- seed window が利用可能になるまで series は `na` を返す

## `ma(series, length, ma_type)`

ルール:

- 引数はちょうど三つ
- 第一引数は `series<float>`
- 第二引数は正の整数リテラル
- 第三引数は型付き `ma_type.<variant>` 値
- 結果型は `series<float>`
- すべての `ma_type` variant は実装済み
- `ma_type.mama` は上流 TA-Lib の挙動に一致し、明示的な `length` を無視して MAMA 既定値 `fast_limit=0.5` と `slow_limit=0.05` を使う

## `apo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])` と `ppo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`

ルール:

- 第一引数は `series<float>`
- `fast_length` と `slow_length` の既定値は `12` と `26`
- 指定する場合、`fast_length` と `slow_length` は `2` 以上の整数リテラル
- 指定する場合、第四引数は型付き `ma_type.<variant>` 値
- `ma_type` を省略した場合は `ma_type.sma`
- `apo` は `fast_ma - slow_ma` を返す
- `ppo` は `((fast_ma - slow_ma) / slow_ma) * 100` を返す
- slow moving average が `0` のとき、`ppo` は `0` を返す
- `ma(...)` と同じ実行可能 `ma_type` variant をサポートする
- 結果型は `series<float>`

## `macd(series, fast_length, slow_length, signal_length)`

ルール:

- 引数はちょうど四つ
- 第一引数は `series<float>`
- 残りの引数は正の整数リテラル
- 結果型は TA-Lib 順の 3 要素タプル `(macd_line, signal, histogram)`
- 結果は `plot`, `export`, 条件式、他の式で使う前に分解しなければならない

## `macdfix(series[, signal_length=9])`

ルール:

- 第一引数は `series<float>`
- 任意の `signal_length` の既定値は `9`
- 指定する場合、`signal_length` は正の整数リテラル
- 結果型は TA-Lib 順の 3 要素タプル `(macd_line, signal, histogram)`
- 結果は `plot`, `export`, 条件式、他の式で使う前に分解しなければならない

## `macdext(series[, fast_length=12[, fast_ma=ma_type.sma[, slow_length=26[, slow_ma=ma_type.sma[, signal_length=9[, signal_ma=ma_type.sma]]]]]])`

ルール:

- 第一引数は `series<float>`
- length を省略した場合は TA-Lib 既定値 `12`, `26`, `9` を使う
- `fast_length` と `slow_length` は `2` 以上の整数リテラル
- `signal_length` は `1` 以上の整数リテラル
- 各 MA 引数は型付き `ma_type.<variant>` 値
- `ma(...)` と同じ実行可能 `ma_type` variant をサポートする
- 結果型は TA-Lib 順の 3 要素タプル `(macd_line, signal, histogram)`
- 結果はさらに使う前に分解しなければならない

## `bbands(series[, length=5[, deviations_up=2.0[, deviations_down=2.0[, ma_type=ma_type.sma]]]])`

ルール:

- 第一引数は `series<float>`
- 任意の `length` の既定値は `5`
- 指定する場合、`length` は正の整数リテラル
- 指定する場合、`deviations_up` と `deviations_down` は数値スカラー
- 指定する場合、第五引数は型付き `ma_type.<variant>` 値
- 結果型は TA-Lib 順の 3 要素タプル `(upper, middle, lower)`
- 結果は `plot`, `export`, 条件式、他の式で使う前に分解しなければならない

## `accbands(high, low, close[, length=20])`

ルール:

- 最初の三引数は `series<float>`
- `length` を省略した場合は TA-Lib 既定値 `20`
- 指定する場合、`length` は `2` 以上の整数リテラル
- 結果型は TA-Lib 順の 3 要素タプル `(upper, middle, lower)`
- 結果はさらに使う前に分解しなければならない

## `dema(series[, length=30])`, `tema(series[, length=30])`, `trima(series[, length=30])`, `kama(series[, length=30])`, `t3(series[, length=5[, volume_factor=0.7]])`, `trix(series[, length=30])`

ルール:

- 第一引数は `series<float>`
- `dema`, `tema`, `trima`, `kama`, `trix` の任意 `length` の既定値は `30`
- `t3` の既定値は `length=5`, `volume_factor=0.7`
- 指定する場合、`length` は正の整数リテラル
- 指定する場合、`volume_factor` は数値スカラー
- 結果型は `series<float>`

## `mavp(series, periods, minimum_period, maximum_period, ma_type)`

ルール:

- 最初の二引数は `series<float>`
- `minimum_period` と `maximum_period` は `2` 以上の整数リテラル
- 第五引数は型付き `ma_type.<variant>` 値
- moving-average family は `ma(...)` と同じ実行可能 `ma_type` 部分集合
- `periods` は bar ごとに `[minimum_period, maximum_period]` へ clamp される
- 結果型は `series<float>`

## `mama(series[, fast_limit=0.5[, slow_limit=0.05]])`

ルール:

- 第一引数は `series<float>`
- `fast_limit` と `slow_limit` の既定値は `0.5` と `0.05`
- 指定する場合、両方の任意引数は数値スカラー
- 結果型は TA-Lib 順の 2 要素タプル `(mama, fama)`
- 結果はさらに使う前に分解しなければならない

## `ht_dcperiod(series)`, `ht_dcphase(series)`, `ht_phasor(series)`, `ht_sine(series)`, `ht_trendline(series)`, `ht_trendmode(series)`

ルール:

- 各関数はちょうど一つの `series<float>` 引数を取る
- `ht_dcperiod`, `ht_dcphase`, `ht_trendline` は `series<float>` を返す
- `ht_trendmode` は TA-Lib の `0` / `1` trend-mode 値を持つ `series<float>` を返す
- `ht_phasor` は 2 要素タプル `(inphase, quadrature)` を返す
- `ht_sine` は 2 要素タプル `(sine, lead_sine)` を返す
- タプル結果はさらに使う前に分解しなければならない
- これらのインジケーターは TA-Lib の Hilbert-transform warmup 挙動に従い、上流の lookback が満たされるまで `na` を返す

## `sar(high, low[, acceleration=0.02[, maximum=0.2]])` と `sarext(high, low[, ...])`

ルール:

- `high` と `low` は `series<float>`
- すべての任意 SAR パラメータは数値スカラー
- `sar` は標準 Parabolic SAR を返す
- `sarext` は拡張 TA-Lib SAR 制御を公開し、short 中は負の値を返す。これは上流 TA-Lib の挙動に一致する
- 結果型は `series<float>`

## `wma(series[, length=30])`

ルール:

- 第一引数は `series<float>`
- 任意の `length` の既定値は `30`
- 指定する場合、`length` は `2` 以上の整数リテラル
- 結果型は `series<float>`
- 十分な履歴がなければ現在サンプルは `na`
- 必要な window に `na` が含まれると現在サンプルは `na`

## `midpoint(series[, length=14])` と `midprice(high, low[, length=14])`

ルール:

- `midpoint` の第一引数は `series<float>`
- `midprice` は `high` と `low` の両方に `series<float>` を必要とする
- 任意の trailing window の既定値は `14`
- 指定する場合、window は `2` 以上の整数リテラル
- window は現在サンプルを含む
- 十分な履歴がなければ結果は `na`
- window 内の必要サンプルのいずれかが `na` なら結果は `na`
- 結果型は `series<float>`

## `linearreg(series[, length=14])`, `linearreg_angle(series[, length=14])`, `linearreg_intercept(series[, length=14])`, `linearreg_slope(series[, length=14])`, `tsf(series[, length=14])`

ルール:

- 第一引数は `series<float>`
- 任意の `length` の既定値は `14`
- 指定する場合、`length` は `2` 以上の整数リテラル
- 十分な履歴がなければ現在サンプルは `na`
- 必要な window に `na` が含まれると現在サンプルは `na`
- `linearreg` は現在 bar における fitted value を返す
- `linearreg_angle` は fitted slope angle を返す
- `linearreg_intercept` は fitted intercept を返す
- `linearreg_slope` は fitted slope を返す
- `tsf` は一歩先予測を返す
- 結果型は `series<float>`

## `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])`

Rules:

- the first three arguments must be `series<float>`
- omitted `atr_length` defaults to `10`
- omitted `multiplier` defaults to `3.0`
- if provided, `atr_length` must be an integer literal greater than or equal to `1`
- if provided, `multiplier` must be a numeric scalar
- `supertrend` returns a 2-tuple `(line, bullish)`
- `line` is the active carried band and `bullish` is the persistent regime direction
- the ATR component uses Wilder smoothing and requires prior-close history, so the result is `na` until the lookback is satisfied
- tuple-valued outputs must be destructured before further use

## `donchian(high, low[, length=20])`

Rules:

- the first two arguments must be `series<float>`
- omitted `length` defaults to `20`
- if provided, `length` must be an integer literal greater than or equal to `1`
- `donchian` returns a 3-tuple `(upper, middle, lower)`
- `upper` is the trailing highest high, `lower` is the trailing lowest low, and `middle` is `(upper + lower) / 2`
- if insufficient history exists, or any required sample is `na`, the current tuple is `(na, na, na)`
- tuple-valued outputs must be destructured before further use
