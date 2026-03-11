# Resumen Del Lenguaje

Los scripts de PalmScript son archivos fuente de nivel superior formados por
declaraciones y sentencias.

Bloques comunes:

- `interval <...>` para el reloj base de ejecucion
- declaraciones `source` para series respaldadas por mercado
- declaraciones opcionales `use <alias> <interval>` para intervalos suplementarios
- funciones de nivel superior
- `let`, `const`, `input`, destructuracion de tuplas, `export`, `trigger`, `entry` / `exit` y `order`
- `if / else if / else`
- expresiones construidas con operadores, llamadas e indexacion
- builtins auxiliares como `crossover`, `activated`, `barssince` y `valuewhen`
- literales enum tipados `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>` y `exit_kind.<variant>`

## Forma Del Script

Los scripts ejecutables de PalmScript nombran explicitamente sus fuentes de datos:

```palmscript
interval 1m
source bn = binance.spot("BTCUSDT")
source hl = hyperliquid.perps("BTC")

plot(bn.close - hl.close)
```

## Modelo Mental

- cada script tiene un intervalo base
- los scripts ejecutables declaran uno o mas bindings `source`
- las series de mercado siempre estan calificadas por fuente
- los valores de las series evolucionan a traves del tiempo
- los intervalos superiores se actualizan solo cuando esas velas cierran por completo
- la falta de historial o de datos alineados aparece como `na`
- `plot`, `export`, `trigger` y las declaraciones de estrategia emiten resultados despues de cada paso de ejecucion

## Donde Ir Para Las Reglas Exactas

- sintaxis y tokens: [Estructura Lexica](../reference/lexical-structure.md) y [Gramatica](../reference/grammar.md)
- declaraciones y visibilidad: [Declaraciones y Alcance](../reference/declarations-and-scope.md)
- expresiones y semantica: [Semantica De Evaluacion](../reference/evaluation-semantics.md)
- reglas de series de mercado: [Intervalos y Fuentes](../reference/intervals-and-sources.md)
- indicadores y builtins auxiliares: [Indicadores](../reference/indicators.md) y [Builtins](../reference/builtins.md)
- salidas: [Salidas](../reference/outputs.md)
