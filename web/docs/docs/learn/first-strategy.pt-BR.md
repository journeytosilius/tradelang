# Primeira Estrategia

Esta estrategia roda sobre barras de um minuto, calcula duas medias moveis e
transforma esse cruzamento em um fluxo simples de entrada e saida apenas em
long.

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")
execution spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow
entry long = crossover(fast, slow)
exit long = crossunder(fast, slow)

order_template market_order = market()
order entry long = market_order
order exit long = market_order
```

## O Que Isto Introduz

- `interval 1m` define o relogio base de execucao
- `source spot = ...` conecta um mercado respaldado por exchange
- `execution spot = ...` conecta o alvo de venue usado pelos comandos de backtest, walk-forward, optimize e paper
- `spot.close` e uma serie base qualificada por fonte
- `let` liga expressoes reutilizaveis
- `export` publica uma serie de saida com nome
- `entry long = ...` emite um sinal de entrada long
- `exit long = ...` emite um sinal de saida long
- `order_template market_order = market()` declara uma ordem reutilizavel
- `order entry long = market_order` e `order exit long = market_order` reutilizam essa configuracao explicita

## Experimente No IDE Do Navegador

Abra [https://palmscript.dev/](https://palmscript.dev/), cole o script
no editor e execute-o sobre o historico disponivel de BTCUSDT com os controles
de data do cabecalho. Voce deve ver o painel de diagnosticos limpo e depois o
resumo do backtest, trades e orders preenchidos a partir dos sinais de
cruzamento.

## Estenda Com Contexto De Timeframe Superior

```palmscript
interval 1d
source spot = binance.spot("BTCUSDT")
execution spot = binance.spot("BTCUSDT")
use spot 1w

let weekly_basis = ema(spot.1w.close, 8)
export bullish = spot.close > weekly_basis
entry long = bullish and crossover(spot.close, weekly_basis)
exit long = crossunder(spot.close, weekly_basis)
order_template market_order = market()
order entry long = market_order
order exit long = market_order
```

Para as regras exatas por tras de `spot.1w.close`, sinais de primeira classe
`entry` / `exit`, indexacao e comportamento sem lookahead, veja:

- [Series e Indexacao](../reference/series-and-indexing.md)
- [Intervalos e Fontes](../reference/intervals-and-sources.md)
- [Saidas](../reference/outputs.md)
- [Semantica De Avaliacao](../reference/evaluation-semantics.md)
