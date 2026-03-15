# Declaraciones y Alcance

Esta pagina define las formas de binding que PalmScript acepta y las reglas de
visibilidad asociadas a ellas.

## Formas Solo De Nivel Superior

Las siguientes formas deben aparecer solo en el nivel superior de un script:

- `interval`
- `source`
- `use`
- `fn`
- `const`
- `input`
- `export`
- `regime`
- `trigger`
- `cooldown`
- `max_bars_in_trade`
- `entry`
- `exit`
- `protect`
- `target`

Se permiten `let`, `if` y sentencias de expresion en el nivel superior.

## Intervalo Base

Todo script debe declarar exactamente un intervalo base:

```palmscript
interval 1m
```

El compilador rechaza un script sin `interval` base o con mas de un `interval`
base.

## Declaraciones De Fuente

Una declaracion de fuente tiene esta forma:

```palmscript
source bb = bybit.usdt_perps("BTCUSDT")
```

Reglas:

- el alias debe ser un identificador
- el alias debe ser unico entre todas las fuentes declaradas
- el template debe resolver a uno de los templates de fuente soportados
- el argumento de simbolo debe ser un literal string

## Declaraciones `use`

Los intervalos suplementarios se declaran por fuente:

```palmscript
use bb 1h
```

Reglas:

- el alias debe nombrar una fuente declarada
- el intervalo no debe ser menor que el intervalo base
- las declaraciones duplicadas `use <alias> <interval>` se rechazan
- un intervalo igual al intervalo base se acepta pero es redundante

## Funciones

Las funciones definidas por el usuario son declaraciones de nivel superior con
cuerpo de expresion:

```palmscript
fn cross_signal(a, b) = a > b and a[1] <= b[1]
```

Reglas:

- los nombres de funciones deben ser unicos
- un nombre de funcion no debe colisionar con un nombre builtin
- los nombres de parametros dentro de una funcion deben ser unicos
- los grafos de funciones recursivos y ciclicos se rechazan
- los cuerpos de funcion pueden referenciar sus parametros, series de fuentes
  declaradas y bindings inmutables `const` / `input` de nivel superior
- los cuerpos de funcion no deben llamar a `plot`
- los cuerpos de funcion no deben capturar bindings `let` del scope de
  sentencias circundante

Las funciones se especializan por tipo de argumento y reloj de actualizacion.

## Bindings `let`

`let` crea un binding en el scope del bloque actual:

```palmscript
let basis = ema(spot.close, 20)
```

Reglas:

- un `let` duplicado en el mismo scope se rechaza
- los scopes internos pueden sombrear bindings externos
- el valor enlazado puede ser escalar o serie
- `na` esta permitido y se trata como marcador numerico durante la compilacion

PalmScript tambien soporta destructuracion de tuplas para resultados inmediatos
de builtins que devuelven tuplas:

```palmscript
let (line, signal, hist) = macd(spot.close, 12, 26, 9)
```

Reglas adicionales:

- la destructuracion de tuplas es una forma de `let` de primera clase
- el lado derecho actualmente debe ser un resultado inmediato de un builtin que
  devuelva tuplas
- la aridad de la tupla debe coincidir exactamente
- las expresiones que devuelven tuplas deben destructurarse antes de cualquier
  otro uso

## `const` E `input`

PalmScript soporta bindings inmutables de nivel superior para configuracion de
estrategias:

```palmscript
input fast_len = 21
const neutral_rsi = 50
```

Reglas:

- ambas formas existen solo en el nivel superior
- los nombres duplicados en el mismo scope se rechazan
- ambas formas son solo escalares en v1: `float`, `bool`, `ma_type`, `tif`,
  `trigger_ref`, `position_side`, `exit_kind` o `na`
- `input` es solo de tiempo de compilacion en v1
- los valores `input` deben ser literales escalares o literales enum
- los valores `const` pueden referenciar bindings `const` / `input`
  previamente declarados y builtins escalares puros
- los builtins con ventana y la indexacion de series aceptan bindings numericos
  inmutables donde se requiere un literal entero

## Salidas

`export`, `regime`, `trigger`, las senales de estrategia de primera clase y las
declaraciones de backtest orientadas a ordenes existen solo en el nivel
superior:

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
regime trend_long = state(ema(spot.close, 20) > ema(spot.close, 50), ema(spot.close, 20) < ema(spot.close, 50))
trigger breakout = spot.close > spot.high[1]
entry1 long = spot.close > spot.high[1]
entry2 long = crossover(spot.close, ema(spot.close, 20))
order entry1 long = limit(price = spot.close[1], tif = tif.gtc, post_only = false, venue = exec)
protect long = stop_market(trigger_price = position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref = trigger_ref.last, venue = exec)
protect_after_target1 long = stop_market(trigger_price = position.entry_price, trigger_ref = trigger_ref.last, venue = exec)
target1 long = take_profit_market(trigger_price = position.entry_price + 4, trigger_ref = trigger_ref.last, venue = exec)
target2 long = take_profit_market(trigger_price = position.entry_price + 8, trigger_ref = trigger_ref.last, venue = exec)
size entry1 long = 0.5
size entry2 long = 0.5
size entry3 long = risk_pct(0.01, stop_price)
size module breakout = 0.5
size target1 long = 0.5
```

Reglas:

- todas las formas son solo de nivel superior
- los nombres duplicados en el mismo scope se rechazan
- `regime` requiere `bool`, `series<bool>` o `na` y esta pensado para series persistentes de estado de mercado
- los nombres `regime` pasan a ser bindings despues del punto de declaracion y se registran con diagnosticos exportados ordinarios
- los nombres `trigger` pasan a ser bindings despues del punto de declaracion
- `entry long` y `entry short` son aliases de compatibilidad para
  `entry1 long` y `entry1 short`
- `entry1`, `entry2` y `entry3` son declaraciones escalonadas de senales de
  entrada para backtesting
- `exit long` y `exit short` siguen siendo salidas discretas unicas de salida
  total de posicion
- `cooldown long|short = <bars>` bloquea nuevas entradas del mismo lado durante
  las siguientes `<bars>` barras de ejecucion despues de un cierre completo en
  ese lado
- `max_bars_in_trade long|short = <bars>` fuerza una salida market del mismo
  lado en la siguiente apertura de ejecucion cuando la operacion alcanza
  `<bars>` barras de ejecucion abiertas
- ambos controles declarativos requieren en v1 una expresion escalar de numero
  entero no negativo resuelta en compilacion
- `order entry ...` y `order exit ...` adjuntan un template de ejecucion a un
  rol de senal correspondiente
- `protect`, `protect_after_target1..3` y `target1..3` declaran salidas
  adjuntas escalonadas que solo se arman mientras la posicion correspondiente
  permanezca abierta
- `size entry1..3 long|short` opcionalmente dimensiona un fill de entrada
  escalonada usando `capital_fraction(x)` / la semantica legacy de fraccion
  numerica desnuda, o `risk_pct(pct, stop_price)` para dimensionamiento basado
  en riesgo
- `size module <name>` opcionalmente dimensiona el fill de entrada escalonada ligado a una declaracion `module <name> = entry...` compatible usando la misma semantica de tamanio de entrada
- `size target1..3 long|short` opcionalmente dimensiona un fill `target`
  escalonado como fraccion de la posicion abierta
- se permite como maximo una declaracion `order` por rol de senal
- se permite como maximo una declaracion por rol escalonado
- si un rol de senal no tiene una declaracion `order` explicita, el backtester
  requiere una declaracion explicita `order ...`
- `size entry ...` y `size target ...` requieren una declaracion escalonada
  `order ...` o `target ...` adjunta correspondiente para el mismo rol
- `size module ...` requiere una declaracion `module` correspondiente que se resuelva a un rol de entrada escalonada
- `risk_pct(...)` solo es valido en declaraciones de tamano de entrada
  escalonada en v1
- las salidas adjuntas escalonadas son secuenciales: solo la siguiente etapa de
  target y la etapa protect actual estan activas al mismo tiempo
- `position.*` solo esta disponible dentro de declaraciones `protect` y
  `target`
- `position_event.*` esta disponible en cualquier lugar donde un `series<bool>`
  sea valido y esta pensado para anclar logica a fills reales del backtest
- los campos actuales de `position_event` son:
  `long_entry_fill`, `short_entry_fill`, `long_exit_fill`, `short_exit_fill`,
  `long_protect_fill`, `short_protect_fill`, `long_target_fill`,
  `short_target_fill`, `long_signal_exit_fill`, `short_signal_exit_fill`,
  `long_reversal_exit_fill`, `short_reversal_exit_fill`,
  `long_liquidation_fill` y `short_liquidation_fill`
- tambien estan disponibles campos de fill escalonado:
  `long_entry1_fill` .. `long_entry3_fill`,
  `short_entry1_fill` .. `short_entry3_fill`,
  `long_target1_fill` .. `long_target3_fill` y
  `short_target1_fill` .. `short_target3_fill`
- `last_exit.*`, `last_long_exit.*` y `last_short_exit.*` estan disponibles en
  cualquier lugar donde una expresion ordinaria sea valida
- los campos actuales de `last_*_exit` son `kind`, `stage`, `side`, `price`,
  `time`, `bar_index`, `realized_pnl`, `realized_return` y `bars_held`
- `last_*_exit.kind` incluye `exit_kind.liquidation` ademas de los tipos de
  salida existentes
- los nombres reservados de trigger como `trigger long_entry = ...` ya no son
  aliases ejecutables; use declaraciones `entry` / `exit` de primera clase mas
  templates `order ...` correspondientes

## Scope Condicional

`if` introduce dos scopes hijos:

```palmscript
if spot.close > spot.open {
    let x = 1
} else {
    let x = 0
}
```

Reglas:

- la condicion debe evaluar a `bool`, `series<bool>` o `na`
- ambas ramas tienen scopes independientes
- los bindings creados dentro de una rama no son visibles fuera del `if`

## Metadata De Optimizacion En `input`

Los `input` numericos pueden declarar metadata de busqueda directamente:

```palmscript
input fast_len = 21 optimize(int, 8, 34, 1)
input atr_mult = 2.5 optimize(float, 1.5, 4.0, 0.25)
input weekly_bias = 21 optimize(choice, 13, 21, 34)
```

Reglas:

- `optimize(int, low, high[, step])` exige un default entero dentro del rango inclusivo y alineado al paso
- `optimize(float, low, high[, step])` exige un default finito dentro del rango inclusivo
- `optimize(choice, v1, v2, ...)` exige que el default sea una de las opciones numericas listadas
- esta metadata solo describe el espacio de busqueda del optimizador; no cambia el valor compilado del `input`

## Latest Portfolio Additions

- PalmScript now reserves `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group`.
- These declarations are top-level only and compile-time only.
- Portfolio mode activates when backtest-oriented CLI commands receive repeated `--execution-source` flags.
- Portfolio mode shares one equity ledger across the selected aliases and blocks only the new entries that would exceed the configured caps.

## Latest Execution Additions

- PalmScript now reserves `execution` as a top-level declaration separate from `source`.
- `execution exec = bybit.usdt_perps("BTCUSDT")` declares an execution target without creating new market series.
- Matching `source` and `execution` aliases may mirror each other when the template and symbol are the same.
- Order constructors now accept named arguments, and `venue = exec` binds that order role to a declared execution alias.
- Positional and named order arguments cannot be mixed in the same order constructor call.
- Trading scripts now require at least one declared `execution` target.
