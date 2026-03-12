# Visao Geral Da Linguagem

Os scripts de PalmScript sao arquivos-fonte de nivel superior compostos por
declaracoes e statements.

Blocos comuns:

- `interval <...>` para o relogio base de execucao
- declaracoes `source` para series respaldadas por mercado
- declaracoes opcionais `use <alias> <interval>` para intervalos suplementares
- funcoes de nivel superior
- `let`, `const`, `input`, desestruturacao de tuplas, `export`, `regime`, `trigger`, `entry` / `exit` e `order`
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

## Para Onde Ir Pelas Regras Exatas

- sintaxe e tokens: [Estrutura Lexica](../reference/lexical-structure.md) e [Gramatica](../reference/grammar.md)
- declaracoes e visibilidade: [Declaracoes e Escopo](../reference/declarations-and-scope.md)
- expressoes e semantica: [Semantica De Avaliacao](../reference/evaluation-semantics.md)
- regras de series de mercado: [Intervalos e Fontes](../reference/intervals-and-sources.md)
- indicadores e builtins auxiliares: [Indicadores](../reference/indicators.md) e [Builtins](../reference/builtins.md)
- saidas: [Saidas](../reference/outputs.md)
