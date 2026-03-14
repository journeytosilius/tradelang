# Aprenda PalmScript

A documentacao publica do PalmScript esta organizada em torno de:

- a linguagem para escrever estrategias
- exemplos que mostram como os scripts sao escritos e usados

## O Que Voce Faz Com PalmScript

Fluxo tipico:

1. escrever um script `.ps`
2. declarar um `interval` base
3. declarar um ou mais bindings `source`
4. valida-lo no IDE do navegador
5. executa-lo sobre dados historicos na app

## Otimizacoes Longas

Para jobs longos de tuning pela CLI:

- use `palmscript run optimize ...` para otimizar diretamente pela CLI
- salve os candidatos uteis com `--preset-out best.json` para reroda-los com `run backtest` ou `run walk-forward`
- mantenha o holdout final intacto ativado por padrao, a menos que queira desativar essa protecao de forma intencional

## O Que Ler Depois

- Primeiro fluxo executavel: [Inicio Rapido](quickstart.md)
- Primeiro walkthrough completo de estrategia: [Primeira Estrategia](first-strategy.md)
- Visao geral da linguagem: [Visao Geral Da Linguagem](language-overview.md)
- Regras e semantica exatas: [Visao Geral Da Referencia](../reference/overview.md)

## Papeis Da Documentacao

- `Aprenda` explica como usar PalmScript de forma eficaz.
- `Referencia` define o que PalmScript significa.

## Latest Diagnostics Additions

PalmScript now exposes richer machine-readable backtest diagnostics in every public locale build:

- `run backtest`, `run walk-forward`, and `run optimize` accept `--diagnostics summary|full-trace`
- summary mode keeps cohort, drawdown-path, source-alignment, holdout-drift, robustness, overfitting-risk, and hint data
- full-trace mode adds one typed per-bar decision trace per execution bar
- optimize output now includes top-candidate holdout checks plus parameter stability and overfitting-risk summaries

## Execucao Paper Local

PalmScript agora tambem inclui um daemon local de execucao paper:

- `palmscript run paper ...` cria uma sessao paper persistente
- `palmscript execution serve` processa essas sessoes com dados reais de exchange em candles fechados
- a sessao reutiliza a mesma VM compilada, a mesma simulacao de ordens e as mesmas regras de portfolio do backtest
- os snapshots paper agora tambem mostram bid/ask top-of-book, preco medio derivado e precos last/mark quando existirem
- a v1 usa apenas dinheiro falso e nunca envia ordens reais
