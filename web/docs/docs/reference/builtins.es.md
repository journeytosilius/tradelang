# Builtins

Esta pagina define las reglas compartidas de builtins en PalmScript y los
helpers builtin que no son indicadores.

Los contratos especificos de indicadores viven en la seccion dedicada
[Indicadores](indicators.md).

## Builtins Ejecutables Frente A Nombres Reservados

PalmScript expone tres superficies relacionadas:

- helpers builtin ejecutables y salidas documentados en esta pagina
- indicadores ejecutables documentados en la seccion [Indicadores](indicators.md)
- un catalogo TA-Lib reservado mas amplio descrito en
  [Superficie TA-Lib](ta-lib.md)

No todos los nombres TA-Lib reservados son ejecutables hoy. Los nombres
reservados pero aun no ejecutables producen diagnosticos de compilacion
deterministas en vez de tratarse como identificadores desconocidos.

## Categorias De Builtins

PalmScript expone actualmente estas categorias builtin:

- indicadores: [Tendencia y Superposicion](indicators-trend-and-overlap.md),
  [Momentum, Volumen y Volatilidad](indicators-momentum-volume-volatility.md) y
  [Matematicas, Precio y Estadistica](indicators-math-price-statistics.md)
- helpers relacionales: `above`, `below`, `between`, `outside`
- helpers de cruce: `cross`, `crossover`, `crossunder`
- helpers de seleccion de venue: `cheapest`, `richest`, `spread_bps`
- helpers de nulos: `na(value)`, `nz(value[, fallback])`,
  `coalesce(value, fallback)`
- helpers de series y ventanas: `change`, `highest`, `lowest`, `highestbars`,
  `lowestbars`, `rising`, `falling`, `cum`
- helpers de memoria de eventos: `state`, `activated`, `deactivated`, `barssince`,
  `valuewhen`, `highest_since`, `lowest_since`, `highestbars_since`,
  `lowestbars_since`, `valuewhen_since`, `count_since`
- salidas: `plot`

Los campos de mercado se seleccionan mediante series calificadas por fuente
como `spot.open`, `spot.close` o `bb.1h.volume`. Solo los identificadores son
invocables, por lo que `spot.close()` se rechaza.

## Helpers De Seleccion De Venue

### `cheapest(exec_a, exec_b, ...)` y `richest(exec_a, exec_b, ...)`

Reglas:

- requieren al menos dos alias `execution` declarados
- cada argumento debe ser un `execution_alias` o `na`
- comparan el cierre de ejecucion actual de cada alias en la barra activa
- `cheapest(...)` devuelve el alias con el cierre actual mas bajo
- `richest(...)` devuelve el alias con el cierre actual mas alto
- si algun alias referenciado no tiene cierre de ejecucion actual en la barra activa, el resultado es `na`
- el tipo de resultado es `execution_alias`

Los resultados de seleccion se usan para logica posterior con aliases de
ejecucion, como comparaciones de igualdad o helpers de spread. No se exportan
directamente.

### `spread_bps(buy_exec, sell_exec)`

Reglas:

- requiere exactamente dos alias `execution` declarados
- ambos argumentos deben ser `execution_alias` o `na`
- se evalua como `((sell_close - buy_close) / buy_close) * 10000`
- si alguno de los aliases referenciados no tiene cierre de ejecucion actual en la barra activa, el resultado es `na`
- el tipo de resultado es `float` o `series<float>` segun el reloj de actualizacion activo

Ejemplo:

```palmscript
execution bn = binance.spot("BTCUSDT")
execution gt = gate.spot("BTC_USDT")

export buy_gate = cheapest(bn, gt) == gt
export venue_spread_bps = spread_bps(cheapest(bn, gt), richest(bn, gt))
```

## Builtins Que Devuelven Tuplas

Los builtins ejecutables que hoy devuelven tuplas son:

- `macd(series, fast_length, slow_length, signal_length)` documentado en
  [Tendencia y Superposicion](indicators-trend-and-overlap.md)
- `minmax(series[, length=30])` documentado en
  [Matematicas, Precio y Estadistica](indicators-math-price-statistics.md)
- `minmaxindex(series[, length=30])` documentado en
  [Matematicas, Precio y Estadistica](indicators-math-price-statistics.md)
- `aroon(high, low[, length=14])` documentado en
  [Momentum, Volumen y Volatilidad](indicators-momentum-volume-volatility.md)
- `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])` documentado en
  [Tendencia y Superposicion](indicators-trend-and-overlap.md)
- `donchian(high, low[, length=20])` documentado en
  [Tendencia y Superposicion](indicators-trend-and-overlap.md)

Todos los resultados builtin que devuelven tuplas deben destructurarse de
inmediato con `let (...) = ...` antes de cualquier otro uso.

## Reglas Comunes De Builtins

Reglas:

- todos los builtins son deterministas
- los builtins no deben realizar I/O, acceder al tiempo ni acceder a la red
- `plot` escribe en el flujo de salida; todos los demas builtins son puros
- los helpers builtin y los indicadores propagan `na` salvo que una regla mas
  especifica sobrescriba ese comportamiento
- los resultados builtin siguen los relojes de actualizacion implicados por sus
  argumentos de serie

## Helpers Relacionales

### `above(a, b)` y `below(a, b)`

Reglas:

- ambos argumentos deben ser numericos, `series<float>` o `na`
- `above(a, b)` evalua como `a > b`
- `below(a, b)` evalua como `a < b`
- si cualquier entrada requerida es `na`, el resultado es `na`
- si cualquiera de las entradas es una serie, el tipo de resultado es
  `series<bool>`
- en caso contrario el tipo de resultado es `bool`

### `between(x, low, high)` y `outside(x, low, high)`

Reglas:

- todos los argumentos deben ser numericos, `series<float>` o `na`
- `between(x, low, high)` evalua como `low < x and x < high`
- `outside(x, low, high)` evalua como `x < low or x > high`
- si cualquier entrada requerida es `na`, el resultado es `na`
- si cualquier argumento es una serie, el tipo de resultado es `series<bool>`
- en caso contrario el tipo de resultado es `bool`

## Helpers De Cruce

### `crossover(a, b)`

Reglas:

- ambos argumentos deben ser numericos, `series<float>` o `na`
- al menos un argumento debe ser `series<float>`
- los argumentos escalares se tratan como umbrales, por lo que su muestra
  previa es su valor actual
- evalua como `a > b` en el presente y `a[1] <= b[1]` en la muestra previa
- si falta cualquier muestra actual o previa requerida, el resultado es `na`
- el tipo de resultado es `series<bool>`

### `crossunder(a, b)`

Reglas:

- ambos argumentos deben ser numericos, `series<float>` o `na`
- al menos un argumento debe ser `series<float>`
- los argumentos escalares se tratan como umbrales, por lo que su muestra
  previa es su valor actual
- evalua como `a < b` en el presente y `a[1] >= b[1]` en la muestra previa
- si falta cualquier muestra actual o previa requerida, el resultado es `na`
- el tipo de resultado es `series<bool>`

### `cross(a, b)`

Reglas:

- ambos argumentos siguen el mismo contrato que `crossover` y `crossunder`
- evalua como `crossover(a, b) or crossunder(a, b)`
- si falta cualquier muestra actual o previa requerida, el resultado es `na`
- el tipo de resultado es `series<bool>`

## Helpers De Series Y Ventanas

### `change(series, length)`

Reglas:

- requiere exactamente dos argumentos
- el primer argumento debe ser `series<float>`
- el segundo argumento debe ser un literal entero positivo
- evalua como `series - series[length]`
- si la muestra actual o la muestra referenciada es `na`, el resultado es `na`
- el tipo de resultado es `series<float>`

### `highest(series, length)` y `lowest(series, length)`

Reglas:

- el primer argumento debe ser `series<float>`
- el segundo argumento debe ser un literal entero positivo
- la ventana incluye la muestra actual
- si no existe suficiente historial, el resultado es `na`
- si cualquier muestra de la ventana requerida es `na`, el resultado es `na`
- el tipo de resultado es `series<float>`

El argumento `length` puede ser un literal entero positivo o un binding
numerico inmutable de nivel superior declarado con `const` o `input`.

### `highestbars(series, length)` y `lowestbars(series, length)`

Reglas:

- el primer argumento debe ser `series<float>`
- el segundo argumento sigue la misma regla de enteros positivos que
  `highest` / `lowest`
- la ventana incluye la muestra actual
- el resultado es la cantidad de barras desde la muestra mas alta o mas baja en
  la ventana activa
- si no existe suficiente historial, el resultado es `na`
- si cualquier muestra de la ventana requerida es `na`, el resultado es `na`
- el tipo de resultado es `series<float>`

### `rising(series, length)` y `falling(series, length)`

Reglas:

- el primer argumento debe ser `series<float>`
- el segundo argumento debe ser un literal entero positivo
- `rising(series, length)` significa que la muestra actual es estrictamente
  mayor que toda muestra previa dentro de las ultimas `length` barras
- `falling(series, length)` significa que la muestra actual es estrictamente
  menor que toda muestra previa dentro de las ultimas `length` barras
- si no existe suficiente historial, el resultado es `na`
- si cualquier muestra requerida es `na`, el resultado es `na`
- el tipo de resultado es `series<bool>`

### `cum(value)`

Reglas:

- requiere exactamente un argumento numerico o `series<float>`
- devuelve la suma acumulada sobre el reloj de actualizacion del argumento
- si la muestra de entrada actual es `na`, la muestra actual de salida es `na`
- las muestras no `na` posteriores siguen acumulando desde el total anterior
- el tipo de resultado es `series<float>`

## Helpers De Nulos

### `na(value)`

Reglas:

- requiere exactamente un argumento
- devuelve `true` cuando la muestra actual del argumento es `na`
- devuelve `false` cuando la muestra actual del argumento es un valor escalar
  concreto
- si el argumento esta respaldado por una serie, el tipo de resultado es
  `series<bool>`
- en caso contrario el tipo de resultado es `bool`

### `nz(value[, fallback])`

Reglas:

- acepta uno o dos argumentos
- con un argumento, las entradas numericas usan `0` y las booleanas usan
  `false` como fallback
- con dos argumentos, el segundo se devuelve cuando el primero es `na`
- ambos argumentos deben ser valores numericos o booleanos compatibles
- el tipo de resultado sigue al tipo elevado de los operandos

### `coalesce(value, fallback)`

Reglas:

- requiere exactamente dos argumentos
- devuelve el primer argumento cuando no es `na`
- en caso contrario devuelve el segundo argumento
- ambos argumentos deben ser valores numericos o booleanos compatibles
- el tipo de resultado sigue al tipo elevado de los operandos

## Helpers De Memoria De Eventos

### `activated(condition)` y `deactivated(condition)`

Reglas:

- ambos requieren exactamente un argumento
- el argumento debe ser `series<bool>`
- `activated` devuelve `true` cuando la muestra actual de la condicion es
  `true` y la muestra previa era `false` o `na`
- `deactivated` devuelve `true` cuando la muestra actual de la condicion es
  `false` y la muestra previa era `true`
- si la muestra actual es `na`, ambos helpers devuelven `false`
- el tipo de resultado es `series<bool>`

### `state(enter, exit)`

Reglas:

- requiere exactamente dos argumentos
- ambos argumentos deben ser `series<bool>`
- devuelve un estado persistente `series<bool>` que comienza en `false`
- `enter = true` con `exit = false` enciende el estado
- `exit = true` con `enter = false` apaga el estado
- si ambos argumentos son `true` en la misma barra, se preserva el estado previo
- si cualquier muestra de entrada actual es `na`, esa entrada se trata como una transicion inactiva en la barra actual
- el tipo de resultado es `series<bool>`

Esta es la base prevista para las declaraciones `regime` de primera clase:

```palmscript
regime trend_long = state(close > ema(close, 20), close < ema(close, 20))
export trend_started = activated(trend_long)
```

### `barssince(condition)`

Reglas:

- requiere exactamente un argumento
- el argumento debe ser `series<bool>`
- devuelve `0` en las barras donde la muestra actual de la condicion es `true`
- se incrementa en cada actualizacion del propio reloj de la condicion despues
  del ultimo evento verdadero
- devuelve `na` hasta que existe el primer evento verdadero
- si la muestra actual de la condicion es `na`, la salida actual es `na`
- el tipo de resultado es `series<float>`

### `valuewhen(condition, source, occurrence)`

Reglas:

- requiere exactamente tres argumentos
- el primer argumento debe ser `series<bool>`
- el segundo argumento debe ser `series<float>` o `series<bool>`
- el tercer argumento debe ser un literal entero no negativo
- la ocurrencia `0` significa el evento verdadero mas reciente
- el tipo de resultado coincide con el tipo del segundo argumento
- devuelve `na` hasta que existan suficientes eventos verdaderos coincidentes
- si la muestra actual de la condicion es `na`, la salida actual es `na`
- cuando la muestra actual de la condicion es `true`, la muestra actual de
  `source` se captura para ocurrencias futuras

### `highest_since(anchor, source)` y `lowest_since(anchor, source)`

Reglas:

- ambos requieren exactamente dos argumentos
- el primer argumento debe ser `series<bool>`
- el segundo argumento debe ser `series<float>`
- cuando la muestra actual de `anchor` es `true`, una nueva epoca anclada
  comienza en la barra actual
- la barra actual contribuye de inmediato a la nueva epoca
- antes del primer anchor, el resultado es `na`
- anchors verdaderos posteriores descartan la epoca anclada previa y comienzan
  una nueva
- el tipo de resultado es `series<float>`

### `highestbars_since(anchor, source)` y `lowestbars_since(anchor, source)`

Reglas:

- ambos requieren exactamente dos argumentos
- el primer argumento debe ser `series<bool>`
- el segundo argumento debe ser `series<float>`
- siguen las mismas reglas de reinicio de epoca anclada que
  `highest_since` / `lowest_since`
- el resultado es la cantidad de barras desde la muestra mas alta o mas baja
  dentro de la epoca anclada actual
- antes del primer anchor, el resultado es `na`
- el tipo de resultado es `series<float>`

### `valuewhen_since(anchor, condition, source, occurrence)`

Reglas:

- requiere exactamente cuatro argumentos
- el primer y segundo argumento deben ser `series<bool>`
- el tercer argumento debe ser `series<float>` o `series<bool>`
- el cuarto argumento debe ser un literal entero no negativo
- cuando la muestra actual de `anchor` es `true`, se olvidan las coincidencias
  previas de `condition` y comienza una nueva epoca anclada en la barra actual
- la ocurrencia `0` significa el evento coincidente mas reciente dentro de la
  epoca anclada actual
- antes del primer anchor, el resultado es `na`
- el tipo de resultado coincide con el tipo del tercer argumento

### `count_since(anchor, condition)`

Reglas:

- requiere exactamente dos argumentos
- ambos argumentos deben ser `series<bool>`
- cuando la muestra actual de `anchor` es `true`, el conteo acumulado se
  reinicia y comienza una nueva epoca anclada en la barra actual
- la barra actual contribuye de inmediato a la nueva epoca anclada
- el conteo se incrementa solo en las barras donde la muestra actual de
  `condition` es `true`
- antes del primer anchor, el resultado es `na`
- anchors verdaderos posteriores descartan la epoca anclada previa y comienzan
  una nueva
- el tipo de resultado es `series<float>`

## `plot(value)`

`plot` emite un punto de grafico para el paso actual.

Reglas:

- requiere exactamente un argumento
- el argumento debe ser numerico, `series<float>` o `na`
- el tipo de resultado de la expresion es `void`
- `plot` no debe llamarse dentro del cuerpo de una funcion definida por el
  usuario

En runtime:

- los valores numericos se registran como puntos de grafico
- `na` registra un punto de grafico sin valor numerico

## Relojes De Actualizacion

Los resultados builtin siguen los relojes de actualizacion de sus entradas.

Ejemplos:

- `ema(spot.close, 20)` avanza sobre el reloj base
- `highest(spot.1w.close, 5)` avanza sobre el reloj semanal
- `cum(spot.1w.close - spot.1w.close[1])` avanza sobre el reloj semanal
- `crossover(bb.close, bn.close)` avanza cuando cualquiera de las series fuente
  referenciadas avanza
- `activated(trend_long)` avanza sobre el reloj de `trend_long`
- `barssince(spot.close > spot.close[1])` avanza sobre el reloj de esa serie de
  condicion
- `valuewhen(trigger_series, bb.1h.close, 0)` avanza sobre el reloj de
  `trigger_series`
- `highest_since(position_event.long_entry_fill, spot.high)` avanza sobre el
  reloj compartido por la serie ancla y la serie fuente
