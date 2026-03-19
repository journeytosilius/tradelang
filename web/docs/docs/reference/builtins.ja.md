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
- time/session helper: `hour_utc`, `weekday_utc`, `session_utc`
- exit-price helper: `trail_stop_long`, `trail_stop_short`, `break_even_long`, `break_even_short`
- venue-selection helper: `cheapest`, `richest`, `spread_bps`, `rank_asc`, `rank_desc`, `current_execution`, `select_asc`, `select_desc`, `in_top_n`, `in_bottom_n`
- null helper: `na(value)`, `nz(value[, fallback])`, `coalesce(value, fallback)`
- series / window helper: `change`, `highest`, `lowest`, `highestbars`, `lowestbars`, `rising`, `falling`, `cum`
- event-memory helper: `state`, `activated`, `deactivated`, `barssince`, `valuewhen`, `highest_since`, `lowest_since`, `highestbars_since`, `lowestbars_since`, `valuewhen_since`, `count_since`
- 出力: `plot`

市場フィールドは `spot.open`, `spot.close`, `bb.1h.volume` のようなソース修飾 series を通じて選択されます。呼び出せるのは識別子だけなので、`spot.close()` は拒否されます。

## Venue-Selection Helpers

### `cheapest(exec_a, exec_b, ...)` と `richest(exec_a, exec_b, ...)`

ルール:

- 少なくとも二つの宣言済み `execution` alias が必要です
- 各引数は `execution_alias` または `na` でなければなりません
- アクティブバー上で各 alias の現在の execution close を比較します
- `cheapest(...)` は現在 close が最も低い alias を返します
- `richest(...)` は現在 close が最も高い alias を返します
- アクティブバー上に現在 execution close がない alias はスキップされます
- 参照された alias がすべてアクティブバー上で利用できない場合、結果は `na` です
- 結果型は `execution_alias` です

これらの selector 結果は、alias との等価比較や spread helper のような
後続の execution-alias ロジック向けです。直接 export することはできません。

### `spread_bps(buy_exec, sell_exec)`

ルール:

- ちょうど二つの宣言済み `execution` alias が必要です
- 両引数は `execution_alias` または `na` でなければなりません
- `((sell_close - buy_close) / buy_close) * 10000` として評価されます
- 参照された alias のどちらかにアクティブバー上の現在 execution close がなければ結果は `na` です
- 結果型は、アクティブな更新クロックに従って `float` または `series<float>` です

### `rank_asc(target_exec, exec_a, exec_b, ...)` と `rank_desc(target_exec, exec_a, exec_b, ...)`

ルール:

- 合計で少なくとも 3 つの宣言済み `execution` alias が必要です。内訳は target alias 1 つと比較対象 alias 2 つ以上です
- 第 1 引数が target alias で、残りの引数が比較集合になります
- 各引数は `execution_alias` または `na` でなければなりません
- 指定した比較集合の現在 execution close を順位付けします
- `rank_asc(...)` は最も低い現在 close に rank `1` を割り当てます
- `rank_desc(...)` は最も高い現在 close に rank `1` を割り当てます
- 同値は比較引数の順序で決定論的に解決されます
- アクティブバー上に現在 execution close がない alias はスキップされます
- target alias がアクティブバー上で利用できない、または順位集合に含まれていない場合、結果は `na` です
- 結果型は、アクティブな更新クロックに従って `float` または `series<float>` です

### `current_execution()`

ルール:

- 引数は取りません
- execution-aware backtest と portfolio mode の内部では、そのバーで現在評価中の execution alias を返します
- それ以外の runtime context では結果は `na` です
- 結果型は `execution_alias` です
- signal / export / helper logic 向けであり、single-leg order constructor でも `venue = <execution_alias_expr>` を通して利用できます

### `select_asc(rank, exec_a, exec_b, ...)` と `select_desc(rank, exec_a, exec_b, ...)`

ルール:

- 正の整数 rank と、少なくとも二つの候補 `execution` alias が必要です
- 第 1 引数は要求する rank で、`1` はその並び順で最良の候補を意味します
- 残りの各引数は `execution_alias` または `na` でなければなりません
- 指定した候補群の現在 execution close を順位付けします
- `select_asc(...)` は最も低い現在 close を rank `1` として返します
- `select_desc(...)` は最も高い現在 close を rank `1` として返します
- 同値は比較引数の順序で決定論的に解決されます
- アクティブバー上に現在 execution close がない alias はスキップされます
- 要求した rank が不正、または利用可能な候補数を超える場合、結果は `na` です
- 結果型は `execution_alias` です

### `in_top_n(target_exec, count, exec_a, exec_b, ...)` と `in_bottom_n(target_exec, count, exec_a, exec_b, ...)`

ルール:

- target alias、一つの正の整数 cohort size、そして少なくとも二つの候補 `execution` alias が必要です
- 第 1 引数は membership を判定する alias です
- 第 2 引数は cohort size です
- 残りの各引数は `execution_alias` または `na` でなければなりません
- 指定した候補群の現在 execution close を `select_asc(...)` / `select_desc(...)` と同じ決定論的順序で順位付けします
- `in_top_n(...)` は上位 cohort への membership を判定します
- `in_bottom_n(...)` は下位 cohort への membership を判定します
- アクティブバー上に現在 execution close がない alias はスキップされます
- target alias がアクティブバー上で利用できない、候補集合に存在しない、または cohort size が不正な場合、結果は `na` です
- cohort size が利用可能な候補数を超える場合、利用可能な候補はすべて cohort に含まれるとみなされます
- 結果型は、アクティブな更新クロックに従って `bool` または `series<bool>` です

例:

```palmscript
execution bn = binance.spot("BTCUSDT")
execution gt = gate.spot("BTC_USDT")
execution bb = bybit.spot("BTCUSDT")

export buy_gate = cheapest(bn, gt) == gt
export venue_spread_bps = spread_bps(cheapest(bn, gt), richest(bn, gt))
export bn_rank_desc = rank_desc(bn, bn, gt)
export best_exec = current_execution() == select_desc(1, bn, gt, bb)
export gt_in_top_two = in_top_n(gt, 2, bn, gt, bb)
```

## Time / Session Helpers

### `hour_utc(time_value)` と `weekday_utc(time_value)`

ルール:

- どちらも `spot.time` のような数値 timestamp または `series<float>` timestamp を受け取ります
- `hour_utc(...)` は UTC の hour-of-day を `0..23` で返します
- `weekday_utc(...)` は UTC weekday を `Monday=0` から `Sunday=6` で返します
- 入力が `na` の場合、結果は `na` です
- 入力が series の場合、結果型は `series<float>` です
- それ以外では、結果型は `float` です

### `session_utc(time_value, start_hour, end_hour)`

ルール:

- 第 1 引数は `spot.time` のような数値 timestamp または `series<float>` timestamp です
- 第 2 引数と第 3 引数は `0..24` の範囲にある数値 UTC hour literal、または不変な数値 input です
- session window は半開区間 `[start_hour, end_hour)` です
- `start_hour < end_hour` の場合、その intraday window をそのまま判定します
- `start_hour > end_hour` の場合、例えば `22 -> 2` のように overnight wrap を行います
- `start_hour == end_hour` の場合、UTC の一日全体に一致します
- timestamp 入力が `na` の場合、結果は `na` です
- timestamp 入力が series の場合、結果型は `series<bool>` です
- それ以外では、結果型は `bool` です

例:

```palmscript
source spot = binance.spot("BTCUSDT")

export hour = hour_utc(spot.time)
export weekday = weekday_utc(spot.time)
export london_morning = session_utc(spot.time, 8, 12)
export asia_wrap = session_utc(spot.time, 22, 2)
```

## Exit-Price Helpers

### `trail_stop_long(anchor_price, stop_offset)` と `trail_stop_short(anchor_price, stop_offset)`

ルール:

- どちらも数値または `series<float>` 入力を受け取ります
- `trail_stop_long(...)` は `anchor_price - stop_offset` として評価されます
- `trail_stop_short(...)` は `anchor_price + stop_offset` として評価されます
- いずれかの入力が `na` の場合、結果は `na` です
- `stop_offset` が負、またはいずれかの数値入力が非有限なら結果は `na` です
- いずれかの入力が series の場合、結果型は `series<float>` です
- それ以外では、結果型は `float` です

### `break_even_long(entry_price, stop_offset)` と `break_even_short(entry_price, stop_offset)`

ルール:

- どちらも数値または `series<float>` 入力を受け取ります
- `break_even_long(...)` は `entry_price + stop_offset` として評価されます
- `break_even_short(...)` は `entry_price - stop_offset` として評価されます
- いずれかの入力が `na` の場合、結果は `na` です
- `stop_offset` が負、またはいずれかの数値入力が非有限なら結果は `na` です
- いずれかの入力が series の場合、結果型は `series<float>` です
- それ以外では、結果型は `float` です

例:

```palmscript
protect long = stop_market(
    trigger_price = trail_stop_long(highest_since(position_event.long_entry_fill, spot.high), 3 * atr(spot.high, spot.low, spot.close, 14)),
    trigger_ref = trigger_ref.last,
    venue = exec
)
protect_after_target1 long = stop_market(
    trigger_price = break_even_long(position.entry_price, 0),
    trigger_ref = trigger_ref.last,
    venue = exec
)
```

## タプル値 Builtins

現在実行可能なタプル値 builtin は次のとおりです。

- [Trend and Overlap](indicators-trend-and-overlap.md) に記載された `macd(series, fast_length, slow_length, signal_length)`
- [Math, Price, and Statistics](indicators-math-price-statistics.md) に記載された `minmax(series[, length=30])`
- [Math, Price, and Statistics](indicators-math-price-statistics.md) に記載された `minmaxindex(series[, length=30])`
- [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md) に記載された `aroon(high, low[, length=14])`
- [Trend and Overlap](indicators-trend-and-overlap.md) に記載された `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])`
- [Trend and Overlap](indicators-trend-and-overlap.md) に記載された `donchian(high, low[, length=20])`

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

### `state(enter, exit)`

ルール:

- 引数はちょうど二つ
- 両引数は `series<bool>`
- `false` から始まる持続的な `series<bool>` 状態を返す
- `enter = true` かつ `exit = false` なら状態はオンになる
- `exit = true` かつ `enter = false` なら状態はオフになる
- 同じバーで両引数が `true` の場合、直前の状態を保持する
- 現在入力サンプルのどちらかが `na` なら、その入力は現在バーで非アクティブな遷移として扱われる
- 結果型は `series<bool>`

これは第一級 `regime` 宣言のための基盤です。

```palmscript
regime trend_long = state(close > ema(close, 20), close < ema(close, 20))
export trend_started = activated(trend_long)
```

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
- `crossover(bb.close, bn.close)` は、参照されたどちらかの source series が進んだときに進む
- `activated(trend_long)` は `trend_long` のクロックで進む
- `barssince(spot.close > spot.close[1])` はその condition series のクロックで進む
- `valuewhen(trigger_series, bb.1h.close, 0)` は `trigger_series` のクロックで進む
- `highest_since(position_event.long_entry_fill, spot.high)` は anchor と source series に共有されたクロックで進む
