# Indicadores De Momentum, Volumen Y Volatilidad

Esta pagina define los indicadores ejecutables de momentum, osciladores,
volumen y volatilidad de PalmScript.

## `rsi(series, length)`

Reglas:

- requiere exactamente dos argumentos
- el primer argumento debe ser `series<float>`
- el segundo argumento debe ser un literal entero positivo
- el tipo de resultado es `series<float>`
- la serie devuelve `na` hasta que existe suficiente historial para sembrar el
  estado del indicador

## `roc(series[, length=10])`, `mom(series[, length=10])`, `rocp(series[, length=10])`, `rocr(series[, length=10])` y `rocr100(series[, length=10])`

Reglas:

- el primer argumento debe ser `series<float>`
- la opcion `length` debe ser un literal entero positivo
- `length` omitido usa el default TA-Lib `10`
- `roc` evalua como `((series - series[length]) / series[length]) * 100`
- `mom` evalua como `series - series[length]`
- `rocp` evalua como `(series - series[length]) / series[length]`
- `rocr` evalua como `series / series[length]`
- `rocr100` evalua como `(series / series[length]) * 100`
- si la muestra actual o la referenciada es `na`, el resultado es `na`
- si `series[length]` es `0`, `roc`, `rocp`, `rocr` y `rocr100` devuelven `na`

## `cmo(series[, length=14])`

Reglas:

- el primer argumento debe ser `series<float>`
- `length` omitido usa el default TA-Lib `14`
- si se provee, `length` debe ser un literal entero mayor o igual a `2`
- `cmo` usa el estado suavizado estilo Wilder de ganancias y perdidas
- el tipo de resultado es `series<float>`
- si la suma de ganancias y perdidas suavizadas es `0`, `cmo` devuelve `0`

## `cci(high, low, close[, length=14])`

Reglas:

- los tres primeros argumentos deben ser `series<float>`
- `length` omitido usa el default TA-Lib `14`
- si se provee, `length` debe ser un literal entero mayor o igual a `2`
- `cci` usa el promedio de precio tipico y la desviacion media sobre la ventana
  solicitada
- si el delta actual de precio tipico o la desviacion media es `0`, `cci`
  devuelve `0`
- el tipo de resultado es `series<float>`

## `aroon(high, low[, length=14])` y `aroonosc(high, low[, length=14])`

Reglas:

- los dos primeros argumentos deben ser `series<float>`
- `length` omitido usa el default TA-Lib `14`
- si se provee, `length` debe ser un literal entero mayor o igual a `2`
- `aroon` usa una ventana de maximos/minimos de `length + 1` para coincidir con
  el lookback de TA-Lib
- `aroon` devuelve una tupla de 2 valores `(aroon_down, aroon_up)` en el orden
  de salida de TA-Lib
- `aroonosc` devuelve `aroon_up - aroon_down`
- las salidas tuple deben destructurarse antes de cualquier otro uso

## `plus_dm(high, low[, length=14])`, `minus_dm(high, low[, length=14])`, `plus_di(high, low, close[, length=14])`, `minus_di(high, low, close[, length=14])`, `dx(high, low, close[, length=14])`, `adx(high, low, close[, length=14])` y `adxr(high, low, close[, length=14])`

Reglas:

- todos los argumentos de precio deben ser `series<float>`
- `length` omitido usa el default TA-Lib `14`
- si se provee, `length` debe ser un literal entero positivo
- `plus_dm` y `minus_dm` devuelven directional movement suavizado estilo Wilder
- `plus_di` y `minus_di` devuelven directional indicators estilo Wilder
- `dx` devuelve el spread direccional absoluto escalado por 100
- `adx` devuelve el promedio Wilder de `dx`
- `adxr` devuelve el promedio del `adx` actual y el `adx` retrasado
- el tipo de resultado es `series<float>`

## `atr(high, low, close[, length=14])` y `natr(high, low, close[, length=14])`

Reglas:

- todos los argumentos deben ser `series<float>`
- `length` omitido usa el default TA-Lib `14`
- si se provee, `length` debe ser un literal entero positivo
- `atr` se siembra a partir del promedio inicial del true range y luego aplica
  suavizado Wilder
- `natr` devuelve `(atr / close) * 100`
- el tipo de resultado es `series<float>`

## `willr(high, low, close[, length=14])`

Reglas:

- los tres primeros argumentos deben ser `series<float>`
- `length` omitido usa el default TA-Lib `14`
- si se provee, `length` debe ser un literal entero mayor o igual a `2`
- `willr` usa el maximo mas alto y el minimo mas bajo sobre la ventana
  solicitada
- el tipo de resultado es `series<float>`
- si el rango maximo-minimo de la ventana es `0`, `willr` devuelve `0`

## `mfi(high, low, close, volume[, length=14])` y `imi(open, close[, length=14])`

Reglas:

- todos los argumentos deben ser `series<float>`
- `length` omitido usa el default TA-Lib `14`
- si se provee, `length` debe ser un literal entero positivo
- `mfi` usa precio tipico y money flow sobre una ventana arrastrada
- `imi` usa el movimiento intradiario open-close sobre la ventana solicitada
- el tipo de resultado es `series<float>`

## `stoch(high, low, close[, fast_k=5[, slow_k=3[, slow_k_ma=ma_type.sma[, slow_d=3[, slow_d_ma=ma_type.sma]]]]])`, `stochf(high, low, close[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]])` y `stochrsi(series[, time_period=14[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]]])`

Reglas:

- todos los argumentos de precio o serie deben ser `series<float>`
- los periodos omitidos usan los defaults de TA-Lib
- `fast_k`, `slow_k` y las longitudes `fast_d`/`slow_d` deben ser literales
  enteros positivos
- `time_period` para `stochrsi` debe ser un literal entero mayor o igual a `2`
- todos los argumentos MA deben ser valores tipados `ma_type.<variant>`
- `stoch` devuelve `(slowk, slowd)` en el orden de TA-Lib
- `stochf` devuelve `(fastk, fastd)` en el orden de TA-Lib
- `stochrsi` devuelve `(fastk, fastd)` en el orden de TA-Lib
- las salidas tuple deben destructurarse antes de cualquier otro uso

## `ad(high, low, close, volume)`, `adosc(high, low, close, volume[, fast_length=3[, slow_length=10]])` y `obv(series, volume)`

Reglas:

- todos los argumentos deben ser `series<float>`
- `ad` devuelve la linea acumulativa de accumulation/distribution
- `adosc` devuelve la diferencia entre las EMAs rapida y lenta de esa linea
- `fast_length` y `slow_length` omitidos usan los defaults TA-Lib `3` y `10`
- `obv` se siembra con el `volume` actual y luego suma o resta volumen segun la
  direccion del precio
- si la muestra de precio o volumen requerida es `na`, el resultado es `na`
- el tipo de resultado es `series<float>`

## `trange(high, low, close)`

Reglas:

- todos los argumentos deben ser `series<float>`
- la primera muestra de salida es `na`
- las muestras posteriores usan la semantica de true range de TA-Lib basada en
  `high` actual, `low` actual y `close` previo
- si cualquier muestra requerida es `na`, el resultado es `na`
- el tipo de resultado es `series<float>`

## `anchored_vwap(anchor, price, volume)`

Rules:

- `anchor` must be `series<bool>`
- `price` and `volume` must be `series<float>`
- when the current `anchor` sample is `true`, the running VWAP resets on that same bar
- the anchor bar is included in the new anchored accumulation window
- if the current anchor, price, or volume sample is `na`, the current output sample is `na`
- if cumulative anchored volume is `0`, the current output sample is `na`
- the result type is `series<float>`
