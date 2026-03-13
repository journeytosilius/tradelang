# Visao Geral Da Linguagem

Os scripts de PalmScript sao arquivos-fonte de nivel superior compostos por
declaracoes e statements.

Blocos comuns:

- `interval <...>` para o relogio base de execucao
- declaracoes `source` para series respaldadas por mercado
- declaracoes opcionais `use <alias> <interval>` para intervalos suplementares
- funcoes de nivel superior
- `let`, `const`, `input`, desestruturacao de tuplas, `export`, `regime`, `trigger`, `entry` / `exit` e `order`
- controles declarativos de backtest como `cooldown long = 12` e `max_bars_in_trade short = 48`
- `if / else if / else`
- expressoes construidas com operadores, chamadas e indexacao
- helpers builtin como `crossover`, `state`, `activated`, `barssince` e `valuewhen`
- literais enum tipados `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>` e `exit_kind.<variant>`

## Forma Do Script

Scripts executaveis de PalmScript nomeiam explicitamente suas fontes de dados:

```palmscript
interval 1m
source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")

plot(bn.close - bb.close)
```

## Modelo Mental

- todo script tem um intervalo base
- scripts executaveis declaram um ou mais bindings `source`
- series de mercado sempre sao qualificadas por fonte
- valores de series evoluem ao longo do tempo
- timeframes superiores so atualizam quando esses candles fecham por completo
- falta de historico ou de dados alinhados aparece como `na`
- `plot`, `export`, `regime`, `trigger` e declaracoes de estrategia emitem resultados apos cada passo de execucao
- `cooldown` e `max_bars_in_trade` sao declaracoes de contagem de barras em tempo de compilacao para tornar explicitas a reentrada e as saidas por tempo

## Para Onde Ir Pelas Regras Exatas

- sintaxe e tokens: [Estrutura Lexica](../reference/lexical-structure.md) e [Gramatica](../reference/grammar.md)
- declaracoes e visibilidade: [Declaracoes e Escopo](../reference/declarations-and-scope.md)
- expressoes e semantica: [Semantica De Avaliacao](../reference/evaluation-semantics.md)
- regras de series de mercado: [Intervalos e Fontes](../reference/intervals-and-sources.md)
- indicadores e builtins auxiliares: [Indicadores](../reference/indicators.md) e [Builtins](../reference/builtins.md)
- saidas: [Saidas](../reference/outputs.md)

## Metadados De Otimizacao

`input`s numericos agora podem declarar metadados de busca para o otimizador diretamente no script:

```palmscript
input fast_len = 21 optimize(int, 8, 34, 1)
```

Isso permite que `run optimize` infira o espaco de busca do proprio script quando `--param` nao for informado.

## Latest Portfolio Additions

- PalmScript now reserves `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group`.
- These declarations are top-level only and compile-time only.
- Portfolio mode activates when backtest-oriented CLI commands receive repeated `--execution-source` flags.
- Portfolio mode shares one equity ledger across the selected aliases and blocks only the new entries that would exceed the configured caps.
