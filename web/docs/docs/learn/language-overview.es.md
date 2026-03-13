# Resumen Del Lenguaje

Los scripts de PalmScript son archivos fuente de nivel superior formados por
declaraciones y sentencias.

Bloques comunes:

- `interval <...>` para el reloj base de ejecucion
- declaraciones `source` para series respaldadas por mercado
- declaraciones opcionales `use <alias> <interval>` para intervalos suplementarios
- funciones de nivel superior
- `let`, `const`, `input`, destructuracion de tuplas, `export`, `regime`, `trigger`, `entry` / `exit` y `order`
- controles declarativos de backtest como `cooldown long = 12` y `max_bars_in_trade short = 48`
- `if / else if / else`
- expresiones construidas con operadores, llamadas e indexacion
- builtins auxiliares como `crossover`, `state`, `activated`, `barssince` y `valuewhen`
- literales enum tipados `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>` y `exit_kind.<variant>`

## Forma Del Script

Los scripts ejecutables de PalmScript nombran explicitamente sus fuentes de datos:

```palmscript
interval 1m
source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")

plot(bn.close - bb.close)
```

## Modelo Mental

- cada script tiene un intervalo base
- los scripts ejecutables declaran uno o mas bindings `source`
- las series de mercado siempre estan calificadas por fuente
- los valores de las series evolucionan a traves del tiempo
- los intervalos superiores se actualizan solo cuando esas velas cierran por completo
- la falta de historial o de datos alineados aparece como `na`
- `plot`, `export`, `regime`, `trigger` y las declaraciones de estrategia emiten resultados despues de cada paso de ejecucion
- `cooldown` y `max_bars_in_trade` son declaraciones de conteo de barras de tiempo de compilacion para explicitar la reentrada y las salidas por tiempo

## Donde Ir Para Las Reglas Exactas

- sintaxis y tokens: [Estructura Lexica](../reference/lexical-structure.md) y [Gramatica](../reference/grammar.md)
- declaraciones y visibilidad: [Declaraciones y Alcance](../reference/declarations-and-scope.md)
- expresiones y semantica: [Semantica De Evaluacion](../reference/evaluation-semantics.md)
- reglas de series de mercado: [Intervalos y Fuentes](../reference/intervals-and-sources.md)
- indicadores y builtins auxiliares: [Indicadores](../reference/indicators.md) y [Builtins](../reference/builtins.md)
- salidas: [Salidas](../reference/outputs.md)

## Metadata De Optimizacion

Los `input` numericos ahora pueden declarar metadata de busqueda para el optimizador directamente en el script:

```palmscript
input fast_len = 21 optimize(int, 8, 34, 1)
```

Esto hace que `run optimize` puedan inferir el espacio de busqueda desde el propio script cuando no se pasa `--param`.

## Latest Portfolio Additions

- PalmScript now reserves `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group`.
- These declarations are top-level only and compile-time only.
- Portfolio mode activates when backtest-oriented CLI commands receive repeated `--execution-source` flags.
- Portfolio mode shares one equity ledger across the selected aliases and blocks only the new entries that would exceed the configured caps.
