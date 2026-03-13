# 宣言とスコープ

このページは、PalmScript が受け入れる束縛形式と、それらに付随する可視性ルールを定義します。

## トップレベル専用の形式

次の形式はスクリプトのトップレベルにのみ現れなければなりません。

- `interval`
- `source`
- `use`
- `fn`
- `const`
- `input`
- `export`
- `regime`
- `trigger`
- `cooldown`
- `max_bars_in_trade`
- `entry`
- `exit`
- `protect`
- `target`

トップレベルの `let`、`if`、式文は許可されます。

## ベースインターバル

各スクリプトはちょうど一つのベースインターバルを宣言しなければなりません。

```palmscript
interval 1m
```

コンパイラは、ベース `interval` がないスクリプト、またはベース `interval` が複数あるスクリプトを拒否します。

## `source` 宣言

`source` 宣言の形式:

```palmscript
source bb = bybit.usdt_perps("BTCUSDT")
```

ルール:

- エイリアスは識別子でなければならない
- エイリアスは宣言済み全ソースの中で一意でなければならない
- テンプレートはサポートされるソーステンプレートの一つに解決されなければならない
- シンボル引数は文字列リテラルでなければならない

## `use` 宣言

補助インターバルはソースごとに宣言されます。

```palmscript
use bb 1h
```

ルール:

- エイリアスは宣言済み `source` を指していなければならない
- インターバルはベースインターバルより低くてはならない
- 重複した `use <alias> <interval>` 宣言は拒否される
- ベースインターバルと等しいインターバルは受理されるが冗長である

## 関数

ユーザー定義関数はトップレベルの式本体宣言です。

```palmscript
fn cross_signal(a, b) = a > b and a[1] <= b[1]
```

ルール:

- 関数名は一意でなければならない
- 関数名は builtin 名と衝突してはならない
- 一つの関数内でパラメータ名は一意でなければならない
- 再帰および循環する関数グラフは拒否される
- 関数本体は、そのパラメータ、宣言済みソースシリーズ、トップレベルの不変 `const` / `input` 束縛を参照できる
- 関数本体は `plot` を呼んではならない
- 関数本体は周囲の文スコープにある `let` 束縛をキャプチャしてはならない

関数は引数型と更新クロックによって特殊化されます。

## `let` 束縛

`let` は現在のブロックスコープに束縛を作ります。

```palmscript
let basis = ema(spot.close, 20)
```

ルール:

- 同一スコープの重複 `let` は拒否される
- 内側のスコープは外側の束縛をシャドーイングできる
- 束縛される値はスカラーでもシリーズでもよい
- `na` は許可され、コンパイル中は数値風プレースホルダーとして扱われる

PalmScript は、即時のタプル値 builtin 結果に対するタプル分解もサポートします。

```palmscript
let (line, signal, hist) = macd(spot.close, 12, 26, 9)
```

追加ルール:

- タプル分解は第一級の `let` 形式
- 右辺は現在、即時のタプル値 builtin 結果でなければならない
- タプル arity は完全一致しなければならない
- タプル値式は、さらに使う前に分解しなければならない

## `const` と `input`

PalmScript は、戦略設定のためのトップレベル不変束縛をサポートします。

```palmscript
input fast_len = 21
const neutral_rsi = 50
```

ルール:

- どちらの形式もトップレベル専用
- 同一スコープでの重複名は拒否される
- v1 ではどちらもスカラー専用: `float`、`bool`、`ma_type`、`tif`、`trigger_ref`、`position_side`、`exit_kind`、または `na`
- v1 では `input` はコンパイル時専用
- `input` 値はスカラーリテラルまたは enum リテラルでなければならない
- `const` 値は、以前に宣言された `const` / `input` 束縛と純粋なスカラー builtin を参照できる
- window 系 builtin と series indexing は、整数リテラルが必要な場所で不変数値束縛を受け付ける

## 出力

`export`、`regime`、`trigger`、第一級戦略シグナル、order 向けバックテスト宣言はトップレベル専用です。

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
regime trend_long = state(ema(spot.close, 20) > ema(spot.close, 50), ema(spot.close, 20) < ema(spot.close, 50))
trigger long_entry = spot.close > spot.high[1]
entry1 long = spot.close > spot.high[1]
entry2 long = crossover(spot.close, ema(spot.close, 20))
order entry1 long = limit(spot.close[1], tif.gtc, false)
protect long = stop_market(position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref.last)
protect_after_target1 long = stop_market(position.entry_price, trigger_ref.last)
target1 long = take_profit_market(position.entry_price + 4, trigger_ref.last)
target2 long = take_profit_market(position.entry_price + 8, trigger_ref.last)
size entry1 long = 0.5
size entry2 long = 0.5
size entry3 long = risk_pct(0.01, stop_price)
size target1 long = 0.5
```

ルール:

- すべての形式はトップレベル専用
- 同一スコープでの重複名は拒否される
- `regime` は `bool`、`series<bool>`、または `na` を要求し、持続的な市場状態シリーズ向けである
- `regime` 名は宣言以降の束縛になり、通常の export 診断とともに記録される
- `trigger` 名は宣言以降の束縛になる
- `entry long` と `entry short` は `entry1 long` と `entry1 short` の互換エイリアス
- `entry1`、`entry2`、`entry3` は段階的なバックテスト entry シグナル宣言
- `exit long` と `exit short` は単一の裁量的フルポジション exit のまま
- `cooldown long|short = <bars>` は、そのサイドで完全決済した後の次の
  `<bars>` 本の実行バーについて同方向の新規エントリーをブロックします
- `max_bars_in_trade long|short = <bars>` は、ポジション保有が `<bars>`
  本の実行バーに達した時点で次の実行始値で同方向の market exit を強制
  します
- どちらの宣言的制御も v1 ではコンパイル時に解決される非負整数スカラー
  式が必要です
- `order entry ...` と `order exit ...` は、対応するシグナルロールに実行テンプレートを付ける
- `protect`、`protect_after_target1..3`、`target1..3` は、対応ポジションが開いている間だけ有効になる段階付き attached exit を宣言する
- `size entry1..3 long|short` は、`capital_fraction(x)` / 旧来の裸の数値比率セマンティクス、またはリスクベース entry sizing 用の `risk_pct(pct, stop_price)` によって段階付き entry fill のサイズを任意指定できる
- `size target1..3 long|short` は、段階付き `target` fill を open position の比率として任意指定できる
- 各シグナルロールにつき `order` 宣言は最大一つ
- 各段階ロールにつき宣言は最大一つ
- あるシグナルロールに明示的な `order` 宣言がなければ、バックテスタは暗黙の `market()` order を使う
- `size entry ...` と `size target ...` は、それぞれ同じロールに対応する段階付き `order ...` または段階付き attached `target ...` 宣言を必要とする
- v1 では `risk_pct(...)` は段階付き entry size 宣言でのみ有効
- 段階付き attached exit は順次的であり、一度に有効なのは次の target 段階と現在の protect 段階だけ
- `position.*` は `protect` と `target` 宣言内でのみ利用できる
- `position_event.*` は `series<bool>` が有効な場所ならどこでも利用でき、実際のバックテスト fill にロジックを固定するために使われる
- 現在の `position_event` フィールドは:
  `long_entry_fill`, `short_entry_fill`, `long_exit_fill`, `short_exit_fill`,
  `long_protect_fill`, `short_protect_fill`, `long_target_fill`, `short_target_fill`,
  `long_signal_exit_fill`, `short_signal_exit_fill`, `long_reversal_exit_fill`,
  `short_reversal_exit_fill`, `long_liquidation_fill`, `short_liquidation_fill`
- 段階付き entry と target の fill フィールドも利用できる:
  `long_entry1_fill` .. `long_entry3_fill`, `short_entry1_fill` .. `short_entry3_fill`,
  `long_target1_fill` .. `long_target3_fill`, `short_target1_fill` .. `short_target3_fill`
- `last_exit.*`、`last_long_exit.*`、`last_short_exit.*` は通常の式が有効な場所ならどこでも使える
- 現在の `last_*_exit` フィールドは `kind`, `stage`, `side`, `price`, `time`, `bar_index`, `realized_pnl`, `realized_return`, `bars_held`
- `last_*_exit.kind` には既存の exit 種別に加えて `exit_kind.liquidation` が含まれる
- 第一級シグナル宣言が存在しない場合に限り、`trigger long_entry = ...` 形式の旧来スクリプトは互換ブリッジとして引き続きサポートされる

## 条件スコープ

`if` は二つの子スコープを導入します。

```palmscript
if spot.close > spot.open {
    let x = 1
} else {
    let x = 0
}
```

ルール:

- 条件は `bool`、`series<bool>`、または `na` に評価されなければならない
- 両方の分岐は独立したスコープを持つ
- 一方の分岐で作られた束縛は `if` の外では見えない

## `input` の最適化メタデータ

数値 `input` は探索空間メタデータを直接宣言できます。

```palmscript
input fast_len = 21 optimize(int, 8, 34, 1)
input atr_mult = 2.5 optimize(float, 1.5, 4.0, 0.25)
input weekly_bias = 21 optimize(choice, 13, 21, 34)
```

ルール:

- `optimize(int, low, high[, step])` は、包括範囲内にあり step に整列した整数デフォルト値を要求します
- `optimize(float, low, high[, step])` は、包括範囲内の有限なデフォルト値を要求します
- `optimize(choice, v1, v2, ...)` は、デフォルト値が列挙された数値候補のどれかであることを要求します
- このメタデータは最適化探索空間を記述するだけで、コンパイル後の `input` 値自体は変えません

## Latest Portfolio Additions

- PalmScript now reserves `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group`.
- These declarations are top-level only and compile-time only.
- Portfolio mode activates when backtest-oriented CLI commands receive repeated `--execution-source` flags.
- Portfolio mode shares one equity ledger across the selected aliases and blocks only the new entries that would exceed the configured caps.
