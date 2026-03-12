# 型と値

PalmScript は、スカラー数値、スカラー真偽値、型付き enum リテラル、それらのシリーズ、`na`、`void` を扱います。

## 具体的な型

実装は次の具体型を区別します。

- `float`
- `bool`
- `ma_type`
- `tif`
- `trigger_ref`
- `position_side`
- `exit_kind`
- `series<float>`
- `series<bool>`
- `void`

`void` は `plot(...)` のような、再利用可能な値を返さない式の結果型です。

## 基本値

PalmScript の値はランタイムで次の形を取ります。

- 数値は `f64`
- 真偽値は `true` または `false`
- `ma_type.<variant>` 値は型付き enum リテラル
- `tif.<variant>` 値は型付き enum リテラル
- `trigger_ref.<variant>` 値は型付き enum リテラル
- `position_side.<variant>` 値は型付き enum リテラル
- `exit_kind.<variant>` 値は型付き enum リテラル
- `na` は欠損値センチネル
- `void` はユーザーが書けるリテラルではない

現在の型付き enum サーフェス:

- `ma_type.sma`
- `ma_type.ema`
- `ma_type.wma`
- `ma_type.dema`
- `ma_type.tema`
- `ma_type.trima`
- `ma_type.kama`
- `ma_type.mama`
- `ma_type.t3`
- `tif.gtc`
- `tif.ioc`
- `tif.fok`
- `tif.gtd`
- `trigger_ref.last`
- `trigger_ref.mark`
- `trigger_ref.index`
- `position_side.long`
- `position_side.short`
- `exit_kind.protect`
- `exit_kind.target`
- `exit_kind.signal`
- `exit_kind.reversal`

現在の `ma_type` バリアントはすべて TA-Lib 風移動平均 builtin を通じて実行可能です。詳しくは [TA-Lib Surface](ta-lib.md) を参照してください。`tif`、`trigger_ref`、`position_side`、`exit_kind` の値は、現在はバックテスト用の order 宣言と、バックテスト駆動の position / exit 状態をパラメータ化するために存在します。

## シリーズ型

シリーズ値は時間インデックス付きストリームです。

シリーズ型は:

- 更新クロックに従って進む
- 有界な履歴を保持する
- 式で使われると現在サンプルを公開する
- あるサンプルで `na` を返すことがある

市場フィールドはシリーズ値です。インジケーター、シグナルヘルパー、イベントメモリ builtin もシリーズ値を返すことがあります。

一部の builtin は固定長タプルのシリーズ値も返します。現在の実装では、タプル結果は即時の builtin 結果としてのみサポートされ、`let (...) = ...` で分解しなければなりません。

例:

```palm
let (line, signal, hist) = macd(spot.close, 12, 26, 9)
plot(hist)
```

現在のタプル対応制限:

- タプル値を生成できるのは特定の builtin のみ
- タプル値は通常の再利用可能値として保存できない
- タプル値式を `plot`、`export`、`trigger`、条件式、他の式へ直接渡すことはできない
- タプル分解だけがタプル結果を消費するサポートされた方法

## `na`

`na` は通常の言語セマンティクスの一部です。ランタイム例外ではありません。

`na` は次から生じることがあります。

- インデックスに十分な履歴がない
- インジケーターのウォームアップ
- ソース対応ベースクロックステップでデータが欠落している
- すでにオペランドが `na` である算術や比較
- `na` リテラルの明示的使用

PalmScript は、裸の `na` リテラルとは別に `na(value)` という builtin 述語も公開します。

- 単独の `na` は欠損値リテラル
- `na(expr)` は引数に応じて `bool` または `series<bool>` を返す
- `nz(value[, fallback])` と `coalesce(value, fallback)` が主要な null 処理ヘルパー

## シリーズとスカラーの組み合わせ

PalmScript は、基礎となる演算子がオペランドカテゴリを受け入れる場合、式の中でスカラーとシリーズの混在を許します。

ルール:

- 受理されるどちらかのオペランドが `series<float>` なら、算術は `series<float>` を返す
- 受理されるどちらかのオペランドが `series<bool>` なら、論理演算は `series<bool>` を返す
- 受理されるどちらかのオペランドが `series<float>` なら、数値比較は `series<bool>` を返す
- いずれかのシリーズオペランドに対する等価比較は `series<bool>` を返す

これは値のリフティングであり、無限シリーズの暗黙的な具現化ではありません。評価は引き続き [評価セマンティクス](evaluation-semantics.md) で説明される更新クロックに従います。

## 型検査における `na`

`na` は、周囲の構文に従う限り、後で数値または真偽値式が必要になる場所ならどこでも受け入れられます。

例:

- `plot(na)` は有効
- `export x = na` は有効
- `trigger t = na` は有効
- `if na { ... } else { ... }` は有効
- `ma(spot.close, 20, ma_type.ema)` は有効

## 真偽値ロジック

`and` と `or` は PalmScript の三値論理を使います。

`na` を `false` へ強制変換しません。ランタイムの真理値表は [評価セマンティクス](evaluation-semantics.md) で定義されます。

## 出力の正規化

出力宣言は値型を次のように正規化します。

- 数値、数値 series、または `na` に対する `export` は `series<float>` を返す
- 真偽値または真偽値 series に対する `export` は `series<bool>` を返す
- `trigger`、`entry`、`exit` 出力は常に `series<bool>` を返す

正確な出力動作は [出力](outputs.md) を参照してください。
