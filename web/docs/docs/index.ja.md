# PalmScript ドキュメント

PalmScript は金融時系列戦略のための言語です。このサイトでは、言語そのもの
に焦点を当てています。構文、セマンティクス、組み込み関数、コード例を扱い
ます。

## ドキュメントの構成

- `学ぶ` では短い例と実行可能な流れで言語を説明します。
- `リファレンス` では受理される構文と言語セマンティクスを定義します。

## ここから始める

- PalmScript が初めてなら: [学ぶの概要](learn/overview.md)
- 最初の実行可能なスクリプトを書きたいなら: [クイックスタート](learn/quickstart.md)
- 言語の正式な定義が必要なら: [リファレンス概要](reference/overview.md)
- インジケーターの契約を探しているなら: [インジケーター概要](reference/indicators.md)

ホストされているブラウザ IDE デモは最小限の構成です。エディタは 1 つだけ
で、React と TypeScript のシェルに Monaco を組み合わせ、利用可能な
BTCUSDT 履歴に対する日付範囲選択、ライブ診断、呼び出し可能項目の補完
スニペット、バックテスト出力パネル、そして生の JSON 列を持たない
trades/orders テーブルを備えています。ツールバーには PalmScript ロゴと
ライト/ダーク切り替えがあります。ダークモードは VS Code 風のシェルと
Dracula 風のエディタテーマを使います。
ホスト入口は `/app/` です。
[https://palmscript.dev/app](https://palmscript.dev/app) はそこへ
リダイレクトされます。

## 言語の特徴

PalmScript は次をサポートします。

- 必須のベース宣言 `interval <...>`
- 市場データのための名前付き `source` 宣言
- `spot.close` や `perp.1h.close` のようなソース修飾シリーズ
- 補助インターバルのための任意の `use <alias> <interval>` 宣言
- リテラル、算術、比較、単項演算子、`and`、`or`
- `let`、`const`、`input`、タプル分解、`export`、`trigger`
- `if / else if / else`
- リテラルオフセットによるシリーズインデックス
- インジケーター、シグナル補助、イベント記憶補助、TA-Lib 風 builtins
- `entry`、`exit`、`order`、`protect`、`target` などの第一級戦略宣言

## ドキュメントの読み方

PalmScript を初めて書くなら `学ぶ` から始めてください。

構文、セマンティクス、builtins、インターバル、出力について正確なルールが
必要なときは `リファレンス` を使ってください。

ヘッダーのタイトルはスクロールしても `PalmScript` のままで、メインサイト
のホームに戻ります。
