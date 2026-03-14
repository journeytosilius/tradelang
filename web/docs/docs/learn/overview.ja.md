# PalmScript を学ぶ

PalmScript の公開ドキュメントは次の二つを中心に構成されています。

- 戦略を書くための言語
- スクリプトの書き方と使い方を示す例

## PalmScript で行うこと

典型的な流れ:

1. `.ps` スクリプトを書く
2. ベースの `interval` を宣言する
3. 1 つ以上の `source` バインディングを宣言する
4. ブラウザ IDE で検証する
5. アプリ内で履歴データに対して実行する

## 長時間の最適化

長い CLI チューニングジョブでは:

- すぐに前面で結果が欲しいなら `palmscript run optimize ...` を使う
- CLI から直接最適化したいなら `palmscript run optimize ...` を使う
- 有望な candidate は `--preset-out best.json` で保存し、`run backtest` や `run walk-forward` で再評価する
- 明示的に無効化したい場合を除き、既定の untouched holdout を有効のままにする

## 次に読むもの

- 最初の実行フロー: [クイックスタート](quickstart.md)
- 最初の完全な戦略 walkthrough: [最初の戦略](first-strategy.md)
- 言語全体の見取り図: [言語概要](language-overview.md)
- 正確なルールとセマンティクス: [リファレンス概要](../reference/overview.md)

## ドキュメントの役割

- `学ぶ` は PalmScript を効果的に使う方法を説明します。
- `リファレンス` は PalmScript の意味を定義します。

## Latest Diagnostics Additions

PalmScript now exposes richer machine-readable backtest diagnostics in every public locale build:

- `run backtest`, `run walk-forward`, and `run optimize` accept `--diagnostics summary|full-trace`
- summary mode keeps cohort, drawdown-path, baseline-comparison, source-alignment, holdout-drift, robustness, overfitting-risk, validation-constraint, and hint data, and top-level backtests also add bounded date-perturbation reruns
- full-trace mode adds one typed per-bar decision trace per execution bar
- optimize output now includes top-candidate holdout checks plus validation-constraint, holdout-pass-rate, parameter stability, baseline-comparison, and overfitting-risk summaries

## ローカル Paper 実行

PalmScript はローカル paper 実行デーモンも提供するようになりました。

- `palmscript run paper ...` は永続的な paper セッションを作成します
- `palmscript execution serve` はクローズ済みバーの実データでそのセッションを処理します
- セッションはバックテストと同じコンパイル済み VM、注文シミュレーション、ポートフォリオ制約を再利用します
- paper スナップショットには top-of-book の bid/ask、そこから計算した mid price、利用可能な last/mark price も含まれます
- v1 は仮想資金のみを使い、実際の注文は送信しません
