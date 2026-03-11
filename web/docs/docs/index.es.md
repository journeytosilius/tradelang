# Documentacion De PalmScript

PalmScript es un lenguaje para estrategias financieras de series temporales.
Este sitio se enfoca en el lenguaje: sintaxis, semantica, builtins y ejemplos
de codigo.

## Mapa De La Documentacion

- `Aprende` ensena el lenguaje mediante ejemplos cortos y flujos ejecutables.
- `Referencia` define la sintaxis aceptada y la semantica del lenguaje.

## Empieza Aqui

- Si eres nuevo en PalmScript: [Resumen De Aprende](learn/overview.md)
- Si quieres tu primer script ejecutable: [Inicio Rapido](learn/quickstart.md)
- Si necesitas la definicion formal del lenguaje: [Resumen De Referencia](reference/overview.md)
- Si buscas los contratos de indicadores: [Resumen De Indicadores](reference/indicators.md)

La demo del IDE alojado mantiene una interfaz minima: un editor, una shell en
React y TypeScript con Monaco, selectores de fecha sobre el historial disponible
de BTCUSDT, diagnosticos en vivo, snippets de autocompletado para callables,
paneles de backtest y tablas de trades y orders sin una columna de JSON crudo.
La barra superior mantiene el logo de PalmScript dentro del encabezado, junto
con un interruptor claro/oscuro. El modo oscuro usa una shell inspirada en
VS Code con un tema tipo Dracula en el editor.
La entrada alojada es `/app/`. [https://palmscript.dev/app](https://palmscript.dev/app) redirige ahi.

## Puntos Destacados Del Lenguaje

PalmScript soporta:

- una declaracion base obligatoria `interval <...>`
- declaraciones `source` con nombre para datos de mercado
- series calificadas por fuente como `spot.close` y `perp.1h.close`
- declaraciones opcionales `use <alias> <interval>` para intervalos suplementarios
- literales, aritmetica, comparaciones, operadores unarios, `and` y `or`
- `let`, `const`, `input`, destructuracion de tuplas, `export` y `trigger`
- `if / else if / else`
- indexacion de series con desplazamientos literales
- indicadores, helpers de senales, helpers de memoria de eventos y builtins estilo TA-Lib
- declaraciones de estrategia de primera clase como `entry`, `exit`, `order`, `protect` y `target`

## Como Leer La Documentacion

Empieza con `Aprende` si vas a escribir PalmScript por primera vez.

Usa `Referencia` cuando necesites reglas exactas de sintaxis, semantica,
builtins, intervalos o salidas.

El titulo del encabezado se mantiene como `PalmScript` mientras haces scroll y
enlaza de vuelta a la pagina principal del sitio.
