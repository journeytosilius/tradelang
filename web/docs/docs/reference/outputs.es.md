# Salidas

Esta pagina define las formas de salida visibles para el usuario en PalmScript.

## Formas De Salida

PalmScript expone tres construcciones que producen salidas:

- `plot(value)`
- `export name = expr`
- `regime name = expr`
- `trigger name = expr`
- `entry long = expr`, `entry1 long = expr`, `entry2 long = expr`,
  `entry3 long = expr`
- `entry short = expr`, `entry1 short = expr`, `entry2 short = expr`,
  `entry3 short = expr`
- `exit long = expr`, `exit short = expr`
- `protect long = order_spec`, `protect short = order_spec`
- `protect_after_target1 long = order_spec`,
  `protect_after_target2 long = order_spec`,
  `protect_after_target3 long = order_spec`
- `protect_after_target1 short = order_spec`,
  `protect_after_target2 short = order_spec`,
  `protect_after_target3 short = order_spec`
- `target long = order_spec`, `target1 long = order_spec`,
  `target2 long = order_spec`, `target3 long = order_spec`
- `target short = order_spec`, `target1 short = order_spec`,
  `target2 short = order_spec`, `target3 short = order_spec`
- `size entry long = expr`, `size entry1 long = expr`,
  `size entry2 long = expr`, `size entry3 long = expr`
- `size entry short = expr`, `size entry1 short = expr`,
  `size entry2 short = expr`, `size entry3 short = expr`
- `size target long = expr`, `size target1 long = expr`,
  `size target2 long = expr`, `size target3 long = expr`
- `size target short = expr`, `size target1 short = expr`,
  `size target2 short = expr`, `size target3 short = expr`

`plot` es una llamada builtin. `export`, `regime` y `trigger` son declaraciones.

## `plot`

`plot` emite un punto de grafico para el paso actual.

Reglas:

- el argumento debe ser numerico, `series<float>` o `na`
- el paso actual aporta un punto de grafico por cada llamada `plot` ejecutada
- `plot` no crea un binding reutilizable en el lenguaje
- `plot` no esta permitido dentro del cuerpo de funciones definidas por el
  usuario

## `export`

`export` publica una serie de salida con nombre:

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
```

Reglas:

- es solo de nivel superior
- el nombre debe ser unico dentro del scope actual
- la expresion puede evaluar a numerico, bool, serie numerica, serie bool o
  `na`
- `void` se rechaza

Normalizacion de tipos:

- los exports numericos, de series numericas y `na` se vuelven
  `series<float>`
- los exports bool y series bool se vuelven `series<bool>`

## `regime`

`regime` publica una serie booleana persistente de estado de mercado con nombre:

```palmscript
regime trend_long = state(
    ema(spot.close, 20) > ema(spot.close, 50),
    ema(spot.close, 20) < ema(spot.close, 50)
)
```

Reglas:

- es solo de nivel superior
- la expresion debe evaluar a `bool`, `series<bool>` o `na`
- el tipo de salida siempre es `series<bool>`
- los nombres `regime` pasan a ser bindings reutilizables despues del punto de declaracion
- `regime` esta pensado para combinarse con `state(...)`, `activated(...)` y `deactivated(...)`
- los diagnosticos de runtime lo registran junto con las series exportadas ordinarias

## `trigger`

`trigger` publica una serie de salida booleana con nombre:

```palmscript
trigger breakout = spot.close > spot.high[1]
```

Reglas:

- es solo de nivel superior
- la expresion debe evaluar a `bool`, `series<bool>` o `na`
- el tipo de salida siempre es `series<bool>`

Regla de evento en runtime:

- se emite un evento de trigger para un paso solo cuando la muestra actual del
  trigger es `true`
- `false` y `na` no emiten eventos de trigger

## Senales De Estrategia De Primera Clase

PalmScript expone declaraciones de senales de estrategia de primera clase para
una ejecucion orientada a estrategias:

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
entry short = spot.close < spot.low[1]
exit short = spot.close > ema(spot.close, 20)
```

Reglas:

- las cuatro declaraciones son solo de nivel superior
- cada expresion debe evaluar a `bool`, `series<bool>` o `na`
- se compilan como salidas trigger con metadatos explicitos de rol de senal
- la emision de eventos en runtime sigue las mismas reglas `true`/`false`/`na`
  que los triggers ordinarios
- `entry long` y `entry short` son aliases de compatibilidad para `entry1 long`
  y `entry1 short`
- `entry2` y `entry3` son senales secuenciales de agregado en el mismo lado que
  solo se vuelven elegibles despues de que la etapa previa se lleno dentro del
  ciclo actual de posicion

## Declaraciones De Orden

PalmScript tambien expone declaraciones `order` de nivel superior que
parametrizan como se ejecuta un rol de senal:

```palmscript
execution exec = binance.spot("BTCUSDT")
order_template maker_entry = limit(price = spot.close[1], tif = tif.gtc, post_only = false, venue = exec)
order_template stop_exit = stop_market(trigger_price = lowest(spot.low, 5)[1], trigger_ref = trigger_ref.last, venue = exec)
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)

order entry long = maker_entry
order exit long = stop_exit
```

Reglas:

- las declaraciones `order` son solo de nivel superior
- `order_template` tambien es solo de nivel superior y define especificaciones reutilizables
- puede haber como maximo una declaracion `order` por rol de senal
- los modos CLI orientados a ejecucion requieren una declaracion explicita `order ...` para cada rol de senal `entry` / `exit`
- `order ... = <template_name>` reutiliza un `order_template` declarado antes
- los templates pueden referenciar otro template, pero los ciclos se rechazan
- campos numericos de orden como `price`, `trigger_price` y `expire_time_ms` se
  evalúan en runtime como series internas ocultas
- `tif.<variant>` y `trigger_ref.<variant>` son literales enum tipados
  verificados en tiempo de compilacion
- las comprobaciones de compatibilidad especificas de la venue se ejecutan al
  iniciar el backtest, segun la `source` de ejecucion

## Salidas Adjuntas

PalmScript tambien expone salidas adjuntas de primera clase que dejan libre la
senal discrecional `exit`:

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
protect long = stop_market(trigger_price = position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref = trigger_ref.last, venue = exec)
target long = take_profit_market(
    trigger_price = highest_since(position_event.long_entry_fill, spot.high) + 4,
    trigger_ref = trigger_ref.last,
    venue = exec
)
size target long = 0.5
```

Reglas:

- las salidas adjuntas son solo de nivel superior
- `protect` es la etapa base de proteccion para un lado
- `protect_after_target1`, `protect_after_target2` y `protect_after_target3`
  opcionalmente ajustan la orden protect activa despues de cada fill de target
  escalonado
- `target`, `target1`, `target2` y `target3` son etapas secuenciales de toma de
  ganancias adjunta; `target` es un alias de compatibilidad para `target1`
- `size entry1..3` y `size target1..3` son opcionales por etapa y solo aplican
  a la entrada o target escalonado correspondiente
- el tamanio de entradas escalonadas soporta:
  - una fraccion numerica legacy como `0.5`
  - `capital_fraction(x)`
  - `risk_pct(pct, stop_price)`
- los valores `capital_fraction(...)` deben evaluar a una fraccion finita en
  `(0, 1]`
- una fraccion de tamanio de entrada menor que `1` deja efectivo disponible
  para scale-ins posteriores del mismo lado
- `risk_pct(...)` es solo para entradas en v1 y dimensiona usando el precio de
  fill real y la distancia al stop en el momento del fill
- si un tamanio `risk_pct(...)` quiere mas de lo que el efectivo o colateral
  libre puede soportar, el backtester recorta el fill y registra
  `capital_limited = true`
- se activan solo despues de que existe un fill de entrada correspondiente
- se reevalúan una vez por barra de ejecucion mientras esa posicion permanezca
  abierta
- solo el `protect` actual escalonado y el siguiente `target` escalonado estan
  activos al mismo tiempo
- cuando `target1` se llena, el motor cambia de `protect` a
  `protect_after_target1` si esta declarado; de lo contrario hereda la etapa de
  proteccion mas reciente disponible
- las fracciones de tamanio de target escalonado deben evaluar a una fraccion
  finita en `(0, 1]`
- una declaracion `size targetN ...` convierte a la etapa target
  correspondiente en una toma de ganancias parcial cuando la fraccion es menor
  que `1`
- los targets escalonados son de un solo uso dentro de un ciclo de posicion y
  se activan secuencialmente
- si ambos se vuelven ejecutables en la misma barra de ejecucion, `protect`
  gana de forma determinista
- `position.*` esta disponible solo dentro de declaraciones `protect` y
  `target`
- `position_event.*` es un namespace de series impulsado por backtests que
  expone eventos de fill reales como `position_event.long_entry_fill`
- `position_event.*` tambien expone eventos de fill especificos por tipo de
  salida, como `position_event.long_target_fill`,
  `position_event.long_protect_fill` y `position_event.long_liquidation_fill`
- tambien hay eventos de fill escalonado, incluidos
  `position_event.long_entry1_fill`, `position_event.long_entry2_fill`,
  `position_event.long_entry3_fill`, `position_event.long_target1_fill`,
  `position_event.long_target2_fill` y `position_event.long_target3_fill`, con
  campos equivalentes para el lado short
- `last_exit.*`, `last_long_exit.*` y `last_short_exit.*` exponen el snapshot
  del trade mas recientemente cerrado, globalmente o por lado
- `last_*_exit.kind` se compara con literales enum tipados como
  `exit_kind.target` y `exit_kind.liquidation`
- `last_*_exit.stage` expone el numero de etapa del target/protect cuando
  aplica
- fuera de los backtests, `position_event.*` esta definido pero evalua a
  `false` en cada paso
- fuera de los backtests, `last_*_exit.*` esta definido pero evalua a `na`

## Reserved Trading Trigger Names

- `trigger long_entry = ...`, `trigger long_exit = ...`, `trigger short_entry = ...`, and `trigger short_exit = ...` are no longer executable aliases
- use first-class `entry` / `exit` declarations plus matching `order ...` templates instead
- ordinary `trigger` declarations with other names remain valid

## Colecciones De Salida En Runtime

Durante una corrida completa, el runtime acumula:

- `plots`
- `exports`
- `triggers`
- `order_fields`
- `trigger_events`
- `alerts`

`alerts` existen actualmente en las estructuras de salida del runtime, pero no
las produce una construccion de lenguaje PalmScript de primera clase.

## Tiempo De Salida E Indice De Barra

Cada muestra de salida se etiqueta con:

- el `bar_index` actual
- el `time` del paso actual

En corridas conscientes de fuentes, el tiempo del paso es la hora de apertura
del paso actual del reloj base.

## Latest Diagnostics Additions

PalmScript now exposes richer machine-readable backtest diagnostics in every public locale build:

- `run backtest`, `run walk-forward`, and `run optimize` accept `--diagnostics summary|full-trace`
- summary mode keeps cohort, drawdown-path, source-alignment, holdout-drift, robustness, overfitting-risk, and hint data
- full-trace mode adds one typed per-bar decision trace per execution bar
- optimize output now includes top-candidate holdout checks plus parameter stability plus overfitting-risk summaries
- Trading scripts now require at least one declared `execution` target.
