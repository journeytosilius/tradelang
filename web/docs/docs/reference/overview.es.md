# Resumen De Referencia

Esta seccion es la definicion normativa de PalmScript tal como se documenta publicamente.

Si alguna vez una guia y una pagina de referencia difieren, la referencia es la fuente autoritativa.

## Que Define Esta Seccion

- estructura lexica
- gramatica
- reglas de declaraciones y alcance
- tipos y valores
- semantica de series e indexacion
- semantica de evaluacion
- reglas de intervalos y fuentes
- contratos de builtins e indicadores
- semantica de salidas
- clases de diagnosticos

## Implementado Hoy

La superficie actual de PalmScript incluye:

- exactamente una directiva base `interval <...>` de nivel superior por script
- uno o mas alias `source` con nombre por script ejecutable
- series calificadas por fuente como `spot.close` o `hl.1h.close`
- intervalos suplementarios mediante `use <alias> <interval>`
- declaraciones `fn` de nivel superior con cuerpo de expresion
- `let`, `const`, `input`, destructuracion de tuplas, `export`, `trigger`, `entry` / `exit` de primera clase y `order`
- indexacion de series solo con literales, literales enum tipados `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>` y `exit_kind.<variant>`, y logica booleana determinista de tres valores
- una superficie estilo TA-Lib donde algunos nombres son ejecutables hoy y nombres reservados adicionales se exponen mediante diagnosticos

## Limites Actuales

- `interval`, `source`, `use`, `fn`, `const`, `input`, `export`, `trigger`, `entry`, `exit` y `order` solo se permiten a nivel superior
- identificadores de mercado desnudos como `close` no son validos en scripts ejecutables
- los intervalos superiores requieren `use <alias> <interval>`
- solo los identificadores son invocables
- los literales string solo son validos dentro de declaraciones `source`
- la indexacion de series requiere un literal entero no negativo
- los resultados tuple-valued de builtins deben destructurarse con `let (...) = ...` antes de seguir usandolos

## Como Leerla

- empieza con [Estructura Lexica](lexical-structure.md) y [Gramatica](grammar.md) para la sintaxis aceptada
- usa [Declaraciones y Alcance](declarations-and-scope.md) para reglas de bindings y visibilidad
- usa [Semantica De Evaluacion](evaluation-semantics.md) e [Intervalos y Fuentes](intervals-and-sources.md) para el significado del lenguaje
- usa [Builtins](builtins.md), [Indicadores](indicators.md) y [Salidas](outputs.md) para el comportamiento de llamadas y salidas
