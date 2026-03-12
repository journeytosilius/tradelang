# Builtins

このページは、PalmScript の共通 builtin ルールと、非インジケーター系 builtin helper を定義します。

インジケーター固有の契約は専用の [Indicators](indicators.md) セクションにあります。

## 実行可能 Builtins と予約済み名

PalmScript は三つの関連する surface を公開します。

- このページに記載された実行可能な builtin helper と出力
- [Indicators](indicators.md) セクションに記載された実行可能インジケーター
- [TA-Lib Surface](ta-lib.md) で説明される、より広い予約済み TA-Lib カタログ

現在、予約済み TA-Lib 名のすべてが実行可能というわけではありません。予約済みだが未実装の名前は、不明な識別子として扱われる代わりに決定的な compile diagnostic を返します。

## Builtin カテゴリ

PalmScript は現在、次の builtin カテゴリを公開します。

- インジケーター: [Trend and Overlap](indicators-trend-and-overlap.md), [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md), [Math, Price, and Statistics](indicators-math-price-statistics.md)
- relational helper: `above`, `below`, `between`, `outside`
- crossing helper: `cross`, `crossover`, `crossunder`
- null helper: `na(value)`, `nz(value[, fallback])`, `coalesce(value, fallback)`
- series / window helper: `change`, `highest`, `lowest`, `highestbars`, `lowestbars`, `rising`, `falling`, `cum`
- event-memory helper: `activated`, `deactivated`, `barssince`, `valuewhen`, `highest_since`, `lowest_since`, `highestbars_since`, `lowestbars_since`, `valuewhen_since`, `count_since`
- 出力: `plot`

市場フィールドは `spot.open`, `spot.close`, `hl.1h.volume` のようなソース修飾 series を通じて選択されます。呼び出せるのは識別子だけなので、`spot.close()` は拒否されます。

## タプル値 Builtins

現在実行可能なタプル値 builtin は次のとおりです。

- [Trend and Overlap](indicators-trend-and-overlap.md) に記載された `macd(series, fast_length, slow_length, signal_length)`
- [Math, Price, and Statistics](indicators-math-price-statistics.md) に記載された `minmax(series[, length=30])`
- [Math, Price, and Statistics](indicators-math-price-statistics.md) に記載された `minmaxindex(series[, length=30])`
- [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md) に記載された `aroon(high, low[, length=14])`

タプル値 builtin の結果は、さらに使う前に必ず `let (...) = ...` で即座に分解しなければなりません。

## 共通 Builtin ルール

ルール:

- すべての builtin は決定的
- builtin は I/O、時刻アクセス、ネットワークアクセスを行ってはならない
- `plot` は出力ストリームへ書き込む。それ以外の builtin は純粋
- builtin helper と indicator は、より具体的な規則がない限り `na` を伝播する
- builtin 結果は、その series 引数が示す更新クロックに従う

## Relational Helpers

### `above(a, b)` と `below(a, b)`

ルール:

- 両引数は数値、`series<float>`、または `na`
- `above(a, b)` は `a > b`
- `below(a, b)` は `a < b`
- 必要な入力のいずれかが `na` なら結果は `na`
- いずれかの入力が series なら結果型は `series<bool>`
- そうでなければ結果型は `bool`

### `between(x, low, high)` と `outside(x, low, high)`

ルール:

- 全引数は数値、`series<float>`、または `na`
- `between(x, low, high)` は `low < x and x < high`
- `outside(x, low, high)` は `x < low or x > high`
- 必要な入力のいずれかが `na` なら結果は `na`
- いずれかの引数が series なら結果型は `series<bool>`
- そうでなければ結果型は `bool`

## Crossing Helpers

### `crossover(a, b)`

ルール:

- 両引数は数値、`series<float>`、または `na`
- 少なくとも一方の引数は `series<float>`
- スカラー引数は threshold として扱われるため、その prior sample は current value と同じ
- 現在 `a > b` かつ前回 `a[1] <= b[1]` として評価される
- 必要な現在サンプルまたは前回サンプルのいずれかが `na` なら結果は `na`
- 結果型は `series<bool>`

### `crossunder(a, b)`

ルール:

- 両引数は数値、`series<float>`、または `na`
- 少なくとも一方の引数は `series<float>`
- スカラー引数は threshold として扱われるため、その prior sample は current value と同じ
- 現在 `a < b` かつ前回 `a[1] >= b[1]` として評価される
- 必要な現在サンプルまたは前回サンプルのいずれかが `na` なら結果は `na`
- 結果型は `series<bool>`

### `cross(a, b)`

ルール:

- 両引数は `crossover` と `crossunder` と同じ契約に従う
- `crossover(a, b) or crossunder(a, b)` として評価される
- 必要な現在サンプルまたは前回サンプルのいずれかが `na` なら結果は `na`
- 結果型は `series<bool>`

## Series and Window Helpers

### `change(series, length)`

ルール:

- 引数はちょうど二つ
- 第一引数は `series<float>`
- 第二引数は正の整数リテラル
- `series - series[length]` として評価される
- 現在サンプルまたは参照サンプルが `na` なら結果は `na`
- 結果型は `series<float>`

### `highest(series, length)` と `lowest(series, length)`

ルール:

- 第一引数は `series<float>`
- 第二引数は正の整数リテラル
- window は現在サンプルを含む
- 十分な履歴がなければ結果は `na`
- 必要な window のいずれかのサンプルが `na` なら結果は `na`
- 結果型は `series<float>`

`length` 引数には、正の整数リテラルまたはトップレベル不変数値束縛 `const` / `input` を使えます。

### `highestbars(series, length)` と `lowestbars(series, length)`

ルール:

- 第一引数は `series<float>`
- 第二引数は `highest` / `lowest` と同じ正の整数規則に従う
- window は現在サンプルを含む
- 結果は、現在の active window 内で highest / lowest sample から何本 bar が経過したか
- 十分な履歴がなければ結果は `na`
- 必要な window のいずれかのサンプルが `na` なら結果は `na`
- 結果型は `series<float>`

### `rising(series, length)` と `falling(series, length)`

ルール:

- 第一引数は `series<float>`
- 第二引数は正の整数リテラル
- `rising(series, length)` は、現在サンプルが trailing `length` bars のすべての prior sample より厳密に大きいことを意味する
- `falling(series, length)` は、現在サンプルが trailing `length` bars のすべての prior sample より厳密に小さいことを意味する
- 十分な履歴がなければ結果は `na`
- 必要なサンプルのいずれかが `na` なら結果は `na`
- 結果型は `series<bool>`

### `cum(value)`

ルール:

- ちょうど一つの数値または `series<float>` 引数を取る
- 引数の更新クロック上で累積 running sum を返す
- 現在入力サンプルが `na` なら現在出力サンプルは `na`
- その後の非 `na` サンプルは prior running total から累積を続ける
- 結果型は `series<float>`

## Null Helpers

### `na(value)`

ルール:

- 引数はちょうど一つ
- 現在の引数サンプルが `na` なら `true`
- 現在の引数サンプルが具体的なスカラー値なら `false`
- 引数が series-backed なら結果型は `series<bool>`
- そうでなければ結果型は `bool`

### `nz(value[, fallback])`

ルール:

- 一引数または二引数を取る
- 一引数の場合、数値入力は `0`、真偽値入力は `false` を fallback とする
- 二引数の場合、第一引数が `na` のとき第二引数を返す
- 両引数は型整合する数値または bool 値でなければならない
- 結果型はオペランドのリフト後の型に従う

### `coalesce(value, fallback)`

ルール:

- 引数はちょうど二つ
- 第一引数が `na` でなければそれを返す
- そうでなければ第二引数を返す
- 両引数は型整合する数値または bool 値でなければならない
- 結果型はオペランドのリフト後の型に従う

## Event Memory Helpers

### `activated(condition)` と `deactivated(condition)`

ルール:

- どちらもちょうど一引数
- 引数は `series<bool>`
- `activated` は現在サンプルが `true` かつ prior sample が `false` または `na` のとき `true`
- `deactivated` は現在サンプルが `false` かつ prior sample が `true` のとき `true`
- 現在サンプルが `na` のとき、どちらも `false`
- 結果型は `series<bool>`

### `barssince(condition)`

ルール:

- 引数はちょうど一つ
- 引数は `series<bool>`
- 現在の condition sample が `true` の bar では `0` を返す
- 最後の true event 以降、condition 自身のクロックが進むごとに加算される
- 最初の true event までは `na`
- 現在の condition sample が `na` の場合、現在出力は `na`
- 結果型は `series<float>`

### `valuewhen(condition, source, occurrence)`

ルール:

- 引数はちょうど三つ
- 第一引数は `series<bool>`
- 第二引数は `series<float>` または `series<bool>`
- 第三引数は非負整数リテラル
- occurrence `0` は直近の true event を意味する
- 結果型は第二引数の型に一致する
- 十分な matching true event が存在するまでは `na`
- 現在の condition sample が `na` なら現在出力は `na`
- 現在の condition sample が `true` のとき、現在の `source` sample が将来の occurrence 用にキャプチャされる

### `highest_since(anchor, source)` と `lowest_since(anchor, source)`

ルール:

- どちらもちょうど二引数
- 第一引数は `series<bool>`
- 第二引数は `series<float>`
- 現在の anchor sample が `true` のとき、新しい anchored epoch が現在 bar から始まる
- 現在 bar は直ちに新しい epoch に寄与する
- 最初の anchor 前は結果は `na`
- 後続の true anchor は prior anchored epoch を破棄し、新しいものを開始する
- 結果型は `series<float>`

### `highestbars_since(anchor, source)` と `lowestbars_since(anchor, source)`

ルール:

- どちらもちょうど二引数
- 第一引数は `series<bool>`
- 第二引数は `series<float>`
- `highest_since` / `lowest_since` と同じ anchored-epoch reset ルールに従う
- 結果は、現在の anchored epoch 内の highest / lowest sample から何 bar 経過したか
- 最初の anchor 前は結果は `na`
- 結果型は `series<float>`

### `valuewhen_since(anchor, condition, source, occurrence)`

ルール:

- 引数はちょうど四つ
- 第一引数と第二引数は `series<bool>`
- 第三引数は `series<float>` または `series<bool>`
- 第四引数は非負整数リテラル
- 現在の anchor sample が `true` のとき、prior `condition` match は忘れられ、現在 bar から新しい anchored epoch が始まる
- occurrence `0` は現在の anchored epoch 内で最も新しい matching event を意味する
- 最初の anchor 前は結果は `na`
- 結果型は第三引数の型に一致する

### `count_since(anchor, condition)`

ルール:

- 引数はちょうど二つ
- 両引数は `series<bool>`
- 現在の anchor sample が `true` のとき、running count は reset され、現在 bar から新しい anchored epoch が始まる
- 現在 bar は直ちに新しい anchored epoch に寄与する
- count が増えるのは現在の `condition` sample が `true` の bar のみ
- 最初の anchor 前は結果は `na`
- 後続の true anchor は prior anchored epoch を破棄し、新しいものを開始する
- 結果型は `series<float>`

## `plot(value)`

`plot` は現在ステップに対する plot point を出力します。

ルール:

- 引数はちょうど一つ
- 引数は数値、`series<float>`、または `na`
- 式結果型は `void`
- `plot` はユーザー定義関数本体の中で呼び出してはならない

ランタイムでは:

- 数値は plot point として記録される
- `na` は数値値を持たない plot point を記録する

## 更新クロック

builtin 結果は、その入力の更新クロックに従います。

例:

- `ema(spot.close, 20)` はベースクロックで進む
- `highest(spot.1w.close, 5)` は週次クロックで進む
- `cum(spot.1w.close - spot.1w.close[1])` は週次クロックで進む
- `crossover(hl.close, bn.close)` は、参照されたどちらかの source series が進んだときに進む
- `activated(trend_long)` は `trend_long` のクロックで進む
- `barssince(spot.close > spot.close[1])` はその condition series のクロックで進む
- `valuewhen(trigger_series, hl.1h.close, 0)` は `trigger_series` のクロックで進む
- `highest_since(position_event.long_entry_fill, spot.high)` は anchor と source series に共有されたクロックで進む
