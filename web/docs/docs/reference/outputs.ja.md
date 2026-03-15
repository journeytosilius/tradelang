# 出力

このページは、PalmScript におけるユーザー可視の出力形式を定義します。

## 出力形式

PalmScript は次の出力生成構文を公開します。

- `plot(value)`
- `export name = expr`
- `regime name = expr`
- `trigger name = expr`
- `module name = entry long|short|entry2|entry3 long|short`
- `entry long = expr`, `entry1 long = expr`, `entry2 long = expr`, `entry3 long = expr`
- `entry short = expr`, `entry1 short = expr`, `entry2 short = expr`, `entry3 short = expr`
- `exit long = expr`, `exit short = expr`
- `protect long = order_spec`, `protect short = order_spec`
- `protect_after_target1 long = order_spec`, `protect_after_target2 long = order_spec`, `protect_after_target3 long = order_spec`
- `protect_after_target1 short = order_spec`, `protect_after_target2 short = order_spec`, `protect_after_target3 short = order_spec`
- `target long = order_spec`, `target1 long = order_spec`, `target2 long = order_spec`, `target3 long = order_spec`
- `target short = order_spec`, `target1 short = order_spec`, `target2 short = order_spec`, `target3 short = order_spec`
- `size entry long = expr`, `size entry1 long = expr`, `size entry2 long = expr`, `size entry3 long = expr`
- `size entry short = expr`, `size entry1 short = expr`, `size entry2 short = expr`, `size entry3 short = expr`
- `size target long = expr`, `size target1 long = expr`, `size target2 long = expr`, `size target3 long = expr`
- `size target short = expr`, `size target1 short = expr`, `size target2 short = expr`, `size target3 short = expr`
- `size module name = expr`

`plot` は builtin 呼び出しです。`export`、`regime`、`trigger` は宣言です。

## `plot`

`plot` は現在のステップに対する plot point を出力します。

ルール:

- 引数はちょうど一つ
- 引数は数値、`series<float>`、または `na`
- `plot` は再利用可能な言語束縛を作らない
- `plot` はユーザー定義関数本体の中では使えない

## `export`

`export` は名前付き出力シリーズを公開します。

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
```

ルール:

- トップレベル専用
- 名前は現在のスコープ内で一意でなければならない
- 式は数値、bool、series numeric、series bool、または `na` に評価されてもよい
- `void` は拒否される

型の正規化:

- 数値、数値 series、`na` の export は `series<float>` になる
- bool と bool series の export は `series<bool>` になる

## `regime`

`regime` は名前付きの持続的な市場状態 boolean series を公開します。

```palmscript
regime trend_long = state(
    ema(spot.close, 20) > ema(spot.close, 50),
    ema(spot.close, 20) < ema(spot.close, 50)
)
```

ルール:

- トップレベル専用
- 式は `bool`、`series<bool>`、または `na` に評価されなければならない
- 出力型は常に `series<bool>`
- `regime` 名は宣言以降で再利用可能な束縛になる
- `regime` は `state(...)`、`activated(...)`、`deactivated(...)` と組み合わせる前提で設計されている
- ランタイム診断では通常の export series と同じく記録される

## `trigger`

`trigger` は名前付き boolean 出力シリーズを公開します。

```palmscript
trigger breakout = spot.close > spot.high[1]
```

ルール:

- トップレベル専用
- 式は `bool`、`series<bool>`、または `na` に評価されなければならない
- 出力型は常に `series<bool>`

ランタイムイベント規則:

- 現在の trigger サンプルが `true` のときだけ、そのステップで trigger event が出力される
- `false` と `na` は trigger event を出力しない

## 第一級戦略シグナル

PalmScript は、戦略向け実行のために第一級戦略シグナル宣言を公開します。

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
entry short = spot.close < spot.low[1]
exit short = spot.close > ema(spot.close, 20)
```

ルール:

- 四つの宣言はすべてトップレベル専用
- 各式は `bool`、`series<bool>`、または `na` に評価されなければならない
- これらは明示的な signal-role metadata を持つ trigger 出力へコンパイルされる
- ランタイムイベント出力は通常の trigger と同じ `true` / `false` / `na` 規則に従う
- `entry long` と `entry short` は `entry1 long` と `entry1 short` の互換エイリアス
- `entry2` と `entry3` は、現在のポジションサイクルで前段階が fill された後にのみ有効になる、同方向の追加シグナル

## エントリーモジュールラベル

PalmScript では研究用診断のためにエントリーロールへラベルを付けられます。

```palmscript
module breakout = entry long
module pullback = entry2 long
size module breakout = 0.4
```

ルール:

- トップレベル専用です
- 現在は `entry`、`entry2`、`entry3` だけを受け付けます
- 各エントリーロールには 1 つだけラベルを付けられます
- module 名も一意でなければならない
- `size module <name> = expr` は、その module に結び付いた staged entry ロールに既存の entry sizing セマンティクスを再利用する
- `size module <name> = expr` は他の size 宣言と同じ数値式を受け付けるので、regime 束縛や三項式で order request を捕捉する時点の module サイズを決められる
- バックテスト診断ではこのラベルが trade の `entry_module` と cohort 集計に出ます

## `order` 宣言

PalmScript は、シグナルロールの実行方法を指定するトップレベル `order` 宣言も公開します。

```palmscript
execution exec = binance.spot("BTCUSDT")
order_template maker_entry = limit(price = spot.close[1], tif = tif.gtc, post_only = false, venue = exec)
order_template stop_exit = stop_market(trigger_price = lowest(spot.low, 5)[1], trigger_ref = trigger_ref.last, venue = exec)
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)

order entry long = maker_entry
order exit long = stop_exit
```

ルール:

- `order` 宣言はトップレベル専用
- `order_template` もトップレベル専用で、再利用可能な注文定義を作る
- シグナルロールごとに `order` 宣言は最大一つ
- 実行系 CLI モードでは、宣言した各 `entry` / `exit` シグナルロールごとに明示的な `order ...` 宣言が必要
- `order ... = <template_name>` は事前に宣言した `order_template` を再利用する
- template は別の template を参照できるが、循環参照は拒否される
- `price`, `trigger_price`, `expire_time_ms` などの数値 order フィールドは、ランタイムで隠れた内部 series として評価される
- `tif.<variant>` と `trigger_ref.<variant>` は、コンパイル時に型検査される typed enum literal
- venue 固有の互換性検査は、実行 `source` に基づいてバックテスト開始時に実行される

## Attached Exits

PalmScript は、裁量的な `exit` シグナルを自由に保つための第一級 attached exit も公開します。

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
protect long = stop_market(trigger_price = position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref = trigger_ref.last, venue = exec)
target long = take_profit_market(
    trigger_price = highest_since(position_event.long_entry_fill, spot.high) + 4,
    trigger_ref = trigger_ref.last,
    venue = exec
)
size target long = 0.5
```

ルール:

- attached exit はトップレベル専用
- `protect` はそのサイドの基本保護ステージ
- `protect_after_target1`, `protect_after_target2`, `protect_after_target3` は、各 staged target fill 後に active protect order を ratchet するための任意宣言
- `target`, `target1`, `target2`, `target3` は段階的な利確ステージ。`target` は `target1` の互換エイリアス
- `size entry1..3` と `size target1..3` は任意で、対応する staged entry または target にのみ適用される
- `size module <name>` は、その `module` 宣言が束縛した staged entry ロールをサイズ指定するための短縮形
- staged entry sizing は次をサポートする:
  - `0.5` のような旧来の裸の数値比率
- `capital_fraction(x)`
- `risk_pct(pct, stop_price)`
- regime に応じた module sizing では、`size module breakout = strong ? 0.4 : 0.15` や `size module breakout = risk_pct(strong ? 0.01 : 0.005, stop_price)` のような通常の数値式を使える
- 他の order field と同様に、そのサイズは order request を作った時点で固定され、後続バーから自動で再計算されることはない
- `capital_fraction(...)` の値は有限な `(0, 1]` の比率に評価されなければならない
- `1` 未満の entry size fraction は、後続の同方向 scale-in のために cash を残す
- `risk_pct(...)` は v1 では entry 専用で、fill 時点の実際の fill 価格と stop 距離からサイズが決まる
- `risk_pct(...)` が現在の cash や free collateral を超える場合、backtester は fill を clamp し、`capital_limited = true` を記録する
- これらは対応する entry fill が存在した後でのみ有効化される
- ポジションが開いている間、execution bar ごとに一回再評価される
- 同時に active なのは current staged protect と next staged target だけ
- `target1` が fill されると、`protect_after_target1` が宣言されていればそれに切り替わり、なければ直近の利用可能な protect stage を継承する
- staged target size fraction は有限な `(0, 1]` の比率に評価されなければならない
- `size targetN ...` は、その比率が `1` 未満なら対応 target stage を partial take-profit にする
- staged target は一つの position cycle で一回だけ実行され、順に有効化される
- 同一 execution bar で両方が fill 可能になった場合、`protect` が決定的に優先される
- `position.*` は `protect` と `target` 宣言内でのみ利用できる
- `position_event.*` は、`position_event.long_entry_fill` のような実 fill event を公開するバックテスト駆動の series namespace
- `position_event.*` は、`position_event.long_target_fill`, `position_event.long_protect_fill`, `position_event.long_liquidation_fill` のような exit-kind 固有の fill event も公開する
- staged fill event も利用できる。`position_event.long_entry1_fill`, `position_event.long_entry2_fill`, `position_event.long_entry3_fill`, `position_event.long_target1_fill`, `position_event.long_target2_fill`, `position_event.long_target3_fill` と、それに対応する short side が含まれる
- `last_exit.*`, `last_long_exit.*`, `last_short_exit.*` は、最新の closed-trade snapshot をグローバルまたはサイド別に公開する
- `last_*_exit.kind` は `exit_kind.target` や `exit_kind.liquidation` のような typed enum literal と比較される
- `last_*_exit.stage` は、該当する場合に staged target / protect の stage 番号を公開する
- バックテスト外では、`position_event.*` は定義されるが各ステップで `false` に評価される
- バックテスト外では、`last_*_exit.*` は定義されるが `na` に評価される

## Reserved Trading Trigger Names

- `trigger long_entry = ...`, `trigger long_exit = ...`, `trigger short_entry = ...`, and `trigger short_exit = ...` are no longer executable aliases
- use first-class `entry` / `exit` declarations plus matching `order ...` templates instead
- ordinary `trigger` declarations with other names remain valid

## ランタイム出力コレクション

フルランでは、ランタイムは次を蓄積します。

- `plots`
- `exports`
- `triggers`
- `order_fields`
- `trigger_events`
- `alerts`

`alerts` は現在ランタイム出力構造には存在しますが、第一級 PalmScript 言語構文からはまだ生成されません。

## 出力時刻と bar index

各出力サンプルには次が付与されます。

- 現在の `bar_index`
- 現在ステップの `time`

ソース対応ランでは、ステップ時刻は現在のベースクロックステップの始値時刻です。

## Latest Diagnostics Additions

PalmScript now exposes richer machine-readable backtest diagnostics in every public locale build:

- `run backtest`, `run walk-forward`, and `run optimize` accept `--diagnostics summary|full-trace`
- summary mode keeps cohort, fixed 4-hour UTC time-bucket cohort, drawdown-path, baseline-comparison, source-alignment, holdout-drift, robustness, overfitting-risk, and hint data, and top-level backtests also add bounded date-perturbation reruns
- walk-forward and optimize now also emit typed validation-constraint summaries plus optimize holdout pass rate data
- full-trace mode adds one typed per-bar decision trace per execution bar
- optimize output now includes top-candidate holdout checks plus validation-constraint, validated/feasible/infeasible survivor counts, constraint-failure breakdowns, optional direct-validation survivor replays, holdout-pass-rate, parameter stability, baseline-comparison, overfitting-risk, Sharpe, and time-bucket cohort summaries

## Latest Execution Additions

- `execution` declarations now separate execution routing from market-data `source` bindings.
- Order constructors accept named arguments in addition to the legacy positional form.
- `venue = <execution_alias>` binds an `order`, `protect`, or `target` role to a declared execution alias.
- Named order arguments cannot be mixed with positional arguments in the same constructor call.
- Trading scripts now require at least one declared `execution` target.
