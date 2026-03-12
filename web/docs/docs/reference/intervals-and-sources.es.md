# Intervalos y Fuentes

Esta pagina define las reglas normativas de intervalos y fuentes en PalmScript.

## Intervalos Soportados

PalmScript acepta los literales de intervalo listados en la
[Tabla De Intervalos](intervals.md). Los intervalos distinguen mayusculas y
minusculas.

## Intervalo Base

Todo script declara exactamente un intervalo base:

```palmscript
interval 1m
```

El intervalo base define el reloj de ejecucion.

## Fuentes Con Nombre

Los scripts ejecutables declaran una o mas fuentes con nombre respaldadas por
exchanges:

```palmscript
interval 1m
source bb = bybit.usdt_perps("BTCUSDT")
source bn = binance.spot("BTCUSDT")
use bb 1h

plot(bn.close - bb.1h.close)
```

Reglas:

- al menos una declaracion `source` es obligatoria
- las series de mercado deben estar calificadas por fuente
- cada fuente declarada aporta un feed base en el intervalo base del script
- `use <alias> <interval>` declara un intervalo adicional para esa fuente
- `<alias>.<field>` se refiere a esa fuente en el intervalo base
- `<alias>.<interval>.<field>` se refiere a esa fuente en el intervalo
  nombrado
- las referencias a intervalos inferiores al base se rechazan

## Templates De Fuente Soportados

PalmScript soporta actualmente estos templates de primera clase:

- `binance.spot("<symbol>")`
- `binance.usdm("<symbol>")`
- `bybit.spot("<symbol>")`
- `bybit.usdt_perps("<symbol>")`
- `gate.spot("<symbol>")`
- `gate.usdt_perps("<symbol>")`

El soporte de intervalos depende del template:

- `binance.spot` acepta todos los intervalos PalmScript soportados
- `binance.usdm` acepta todos los intervalos PalmScript soportados
- `bybit.spot` acepta `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w` y `1M`
- `bybit.usdt_perps` acepta `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w` y `1M`
- `gate.spot` acepta `1s`, `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h`, `1d` y `1M`
- `gate.usdt_perps` acepta `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h` y `1d`

Las restricciones operativas de carga tambien dependen del template:

- Bybit usa simbolos nativos del venue como `BTCUSDT`
- Gate usa simbolos nativos del venue como `BTC_USDT`
- las klines REST de Bybit llegan en orden descendente y PalmScript las reordena antes de validar la alineacion
- las APIs de velas de Gate usan Unix seconds y PalmScript las normaliza a Unix milliseconds UTC
- la paginacion de Gate spot y futures se hace por ventanas de tiempo porque la API publica no permite `limit` junto con `from` / `to`
- las solicitudes de Gate spot y futures se limitan a 1000 velas por llamada HTTP para evitar `400 Bad Request` provocados por rangos demasiado amplios
- los feeds de Binance, Bybit y Gate se paginan internamente
- cuando un venue rechaza una carga, PalmScript muestra el estado HTTP junto con la URL de la solicitud y un fragmento truncado del cuerpo de respuesta si existe
- las URLs base se pueden sobreescribir con
  `PALMSCRIPT_BINANCE_SPOT_BASE_URL`, `PALMSCRIPT_BINANCE_USDM_BASE_URL`,
  `PALMSCRIPT_BYBIT_BASE_URL` y `PALMSCRIPT_GATE_BASE_URL`; para Gate se
  acepta tanto la raiz del host, por ejemplo `https://api.gateio.ws`, como la
  URL base completa `/api/v4`

## Conjunto De Campos De Fuente

Todos los templates de fuente se normalizan en los mismos campos canonicos de
mercado:

- `time`
- `open`
- `high`
- `low`
- `close`
- `volume`

Reglas:

- `time` es la hora de apertura de la vela en Unix milliseconds UTC
- los campos de precio y volumen son numericos
- los campos extra especificos de cada venue no se exponen en el lenguaje

## Intervalos Iguales, Superiores E Inferiores

PalmScript distingue tres casos para un intervalo referenciado respecto al
intervalo base:

- intervalo igual: valido
- intervalo superior: valido si se declara con `use <alias> <interval>`
- intervalo inferior: rechazado

## Semantica De Runtime

En modo mercado:

- PalmScript obtiene directamente desde las venues los feeds requeridos
  `(source, interval)`
- la linea temporal base de ejecucion es la union de los tiempos de apertura de
  barras del intervalo base para todas las fuentes declaradas
- si una fuente no tiene barra base en un paso de la linea temporal, esa fuente
  aporta `na` en ese paso
- los intervalos lentos de una fuente retienen su ultimo valor completamente
  cerrado hasta su siguiente frontera de cierre

## Garantia Sin Lookahead

PalmScript no debe exponer una vela de intervalo superior antes de que esa vela
haya cerrado por completo.

Esto aplica a intervalos calificados por fuente como `bb.1h.close`.

## Reglas De Alineacion De Runtime

Los feeds preparados deben alinearse con sus intervalos declarados.

El runtime rechaza feeds que esten:

- desalineados con la frontera del intervalo
- desordenados
- duplicados en un mismo tiempo de apertura de intervalo
