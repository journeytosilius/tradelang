# Primera Estrategia

Esta estrategia corre sobre barras de un minuto, calcula dos medias moviles y
convierte ese cruce en un flujo simple de entrada y salida solo en largo.

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow
entry long = crossover(fast, slow)
exit long = crossunder(fast, slow)

order entry long = market()
```

## Que Introduce

- `interval 1m` fija el reloj base de ejecucion
- `source spot = ...` enlaza un mercado respaldado por un exchange
- `spot.close` es una serie base calificada por fuente
- `let` enlaza expresiones reutilizables
- `export` publica una serie de salida con nombre
- `entry long = ...` emite una senal de entrada larga
- `exit long = ...` emite una senal de salida larga
- `order entry long = market()` le dice al backtester como llenar la senal de entrada

## Pruebala En El IDE Del Navegador

Abre [https://palmscript.dev/app/](https://palmscript.dev/app/), pega el script
en el editor y ejecutalo sobre el historial disponible de BTCUSDT con los
controles de fecha del encabezado. Deberias ver el panel de diagnosticos limpio
y despues el resumen del backtest, trades y orders poblados a partir de las
senales del cruce.

## Extiendela Con Contexto De Un Marco Temporal Superior

```palmscript
interval 1d
source spot = binance.spot("BTCUSDT")
use spot 1w

let weekly_basis = ema(spot.1w.close, 8)
export bullish = spot.close > weekly_basis
entry long = bullish and crossover(spot.close, weekly_basis)
exit long = crossunder(spot.close, weekly_basis)
order entry long = market()
```

Para las reglas exactas detras de `spot.1w.close`, las senales de primera clase
`entry` / `exit`, la indexacion y el comportamiento sin lookahead, consulta:

- [Series E Indexacion](../reference/series-and-indexing.md)
- [Intervalos y Fuentes](../reference/intervals-and-sources.md)
- [Salidas](../reference/outputs.md)
- [Semantica De Evaluacion](../reference/evaluation-semantics.md)
