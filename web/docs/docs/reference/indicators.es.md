# Resumen De Indicadores

Esta seccion define la superficie ejecutable de indicadores de PalmScript.

Usa [Builtins](builtins.md) para las reglas compartidas de llamadas, los
builtins helper, `plot` y las reglas de destructuracion de tuplas que aplican
en todo el lenguaje.

## Familias De Indicadores

PalmScript documenta actualmente los indicadores en estas familias:

- [Tendencia y Superposicion](indicators-trend-and-overlap.md)
- [Momentum, Volumen y Volatilidad](indicators-momentum-volume-volatility.md)
- [Matematicas, Precio y Estadistica](indicators-math-price-statistics.md)

## Reglas Compartidas De Indicadores

Reglas:

- los nombres de indicadores son identificadores builtin, por lo que se llaman
  directamente, por ejemplo `ema(spot.close, 20)`
- las entradas de indicadores deben seguir las reglas de series calificadas por
  fuente definidas en [Intervalos y Fuentes](intervals-and-sources.md)
- los argumentos de longitud opcionales usan los defaults de TA-Lib
  documentados en las paginas de familia
- los argumentos parecidos a longitud descritos como literales deben ser
  literales enteros en el codigo fuente
- los indicadores que devuelven tuplas deben destructurarse con `let (...) = ...`
  antes de seguir usandose
- las salidas de los indicadores siguen el reloj de actualizacion implicado por
  sus entradas de series
- los indicadores propagan `na` salvo que el contrato especifico diga lo
  contrario

## Indicadores Que Devuelven Tuplas

Los indicadores que hoy devuelven tuplas son:

- `macd(series, fast_length, slow_length, signal_length)`
- `minmax(series[, length=30])`
- `minmaxindex(series[, length=30])`
- `aroon(high, low[, length=14])`
- `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])`
- `donchian(high, low[, length=20])`

Deben destructurarse de inmediato:

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let (line, signal, hist) = macd(spot.close, 12, 26, 9)
plot(line)
```

## Nombres TA-Lib Ejecutables Frente A Reservados

PalmScript reserva un catalogo TA-Lib mas amplio del que ejecuta hoy.

- estas paginas de indicadores definen el subconjunto ejecutable
- [Superficie TA-Lib](ta-lib.md) define la superficie mas amplia de nombres
  reservados y metadatos
- un nombre TA-Lib reservado pero aun no ejecutable produce un diagnostico de
  compilacion determinista
