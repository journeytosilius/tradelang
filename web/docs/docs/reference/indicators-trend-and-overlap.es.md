# Indicadores De Tendencia Y Superposicion

Esta pagina define los indicadores ejecutables de tendencia y superposicion de
PalmScript.

## `sma(series, length)`

Reglas:

- requiere exactamente dos argumentos
- el primer argumento debe ser `series<float>`
- el segundo argumento debe ser un literal entero positivo
- el tipo de resultado es `series<float>`
- si no existe suficiente historial, la muestra actual es `na`
- si la ventana requerida contiene `na`, la muestra actual es `na`

## `ema(series, length)`

Reglas:

- requiere exactamente dos argumentos
- el primer argumento debe ser `series<float>`
- el segundo argumento debe ser un literal entero positivo
- el tipo de resultado es `series<float>`
- la serie devuelve `na` hasta que la ventana semilla esta disponible

## `ma(series, length, ma_type)`

Reglas:

- requiere exactamente tres argumentos
- el primer argumento debe ser `series<float>`
- el segundo argumento debe ser un literal entero positivo
- el tercer argumento debe ser un valor tipado `ma_type.<variant>`
- el tipo de resultado es `series<float>`
- todas las variantes `ma_type` estan implementadas
- `ma_type.mama` coincide con el comportamiento upstream de TA-Lib e ignora el
  parametro explicito `length`, usando los defaults de MAMA
  `fast_limit=0.5` y `slow_limit=0.05`

## `apo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])` y `ppo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`

Reglas:

- el primer argumento debe ser `series<float>`
- `fast_length` y `slow_length` usan `12` y `26` por defecto
- si se proveen, `fast_length` y `slow_length` deben ser literales enteros
  mayores o iguales a `2`
- si se provee, el cuarto argumento debe ser un valor tipado `ma_type.<variant>`
- `ma_type` omitido usa `ma_type.sma`
- `apo` devuelve `fast_ma - slow_ma`
- `ppo` devuelve `((fast_ma - slow_ma) / slow_ma) * 100`
- si la media movil lenta es `0`, `ppo` devuelve `0`
- se soportan las mismas variantes ejecutables de `ma_type` que en `ma(...)`
- el tipo de resultado es `series<float>`

## `macd(series, fast_length, slow_length, signal_length)`

Reglas:

- requiere exactamente cuatro argumentos
- el primer argumento debe ser `series<float>`
- los argumentos restantes deben ser literales enteros positivos
- el tipo de resultado es una tupla de 3 series en el orden de TA-Lib:
  `(macd_line, signal, histogram)`
- el resultado debe destructurarse antes de poder usarse en `plot`, `export`,
  condiciones o expresiones posteriores

## `macdfix(series[, signal_length=9])`

Reglas:

- el primer argumento debe ser `series<float>`
- el `signal_length` opcional usa `9` por defecto
- si se provee, `signal_length` debe ser un literal entero positivo
- el tipo de resultado es una tupla de 3 series en el orden de TA-Lib:
  `(macd_line, signal, histogram)`
- el resultado debe destructurarse antes de poder usarse en `plot`, `export`,
  condiciones o expresiones posteriores

## `macdext(series[, fast_length=12[, fast_ma=ma_type.sma[, slow_length=26[, slow_ma=ma_type.sma[, signal_length=9[, signal_ma=ma_type.sma]]]]]])`

Reglas:

- el primer argumento debe ser `series<float>`
- las longitudes omitidas usan los defaults de TA-Lib `12`, `26` y `9`
- `fast_length` y `slow_length` deben ser literales enteros mayores o iguales a
  `2`
- `signal_length` debe ser un literal entero mayor o igual a `1`
- cada argumento MA debe ser un valor tipado `ma_type.<variant>`
- se soportan las mismas variantes ejecutables de `ma_type` que en `ma(...)`
- el tipo de resultado es una tupla de 3 series en el orden de TA-Lib:
  `(macd_line, signal, histogram)`
- el resultado debe destructurarse antes de cualquier otro uso

## `bbands(series[, length=5[, deviations_up=2.0[, deviations_down=2.0[, ma_type=ma_type.sma]]]])`

Reglas:

- el primer argumento debe ser `series<float>`
- el `length` opcional usa `5` por defecto
- si se provee, `length` debe ser un literal entero positivo
- si se proveen, `deviations_up` y `deviations_down` deben ser escalares
  numericos
- si se provee, el quinto argumento debe ser un valor tipado
  `ma_type.<variant>`
- el tipo de resultado es una tupla de 3 series en el orden de TA-Lib:
  `(upper, middle, lower)`
- el resultado debe destructurarse antes de poder usarse en `plot`, `export`,
  condiciones o expresiones posteriores

## `accbands(high, low, close[, length=20])`

Reglas:

- los tres primeros argumentos deben ser `series<float>`
- el `length` omitido usa el default TA-Lib `20`
- si se provee, `length` debe ser un literal entero mayor o igual a `2`
- el tipo de resultado es una tupla de 3 series en el orden de TA-Lib:
  `(upper, middle, lower)`
- el resultado debe destructurarse antes de cualquier otro uso

## `dema(series[, length=30])`, `tema(series[, length=30])`, `trima(series[, length=30])`, `kama(series[, length=30])`, `t3(series[, length=5[, volume_factor=0.7]])` y `trix(series[, length=30])`

Reglas:

- el primer argumento debe ser `series<float>`
- la opcion `length` usa `30` por defecto para `dema`, `tema`, `trima`,
  `kama` y `trix`
- `t3` usa `length=5` y `volume_factor=0.7` por defecto
- si se provee, `length` debe ser un literal entero positivo
- si se provee, `volume_factor` debe ser un escalar numerico
- el tipo de resultado es `series<float>`

## `mavp(series, periods, minimum_period, maximum_period, ma_type)`

Reglas:

- los dos primeros argumentos deben ser `series<float>`
- `minimum_period` y `maximum_period` deben ser literales enteros mayores o
  iguales a `2`
- el quinto argumento debe ser un valor tipado `ma_type.<variant>`
- la familia de medias moviles es el mismo subconjunto ejecutable de
  `ma_type` que en `ma(...)`
- `periods` se recorta por barra dentro de `[minimum_period, maximum_period]`
- el tipo de resultado es `series<float>`

## `mama(series[, fast_limit=0.5[, slow_limit=0.05]])`

Reglas:

- el primer argumento debe ser `series<float>`
- `fast_limit` y `slow_limit` usan `0.5` y `0.05` por defecto
- si se proveen, ambos argumentos opcionales deben ser escalares numericos
- el tipo de resultado es una tupla de 2 series en el orden de TA-Lib:
  `(mama, fama)`
- el resultado debe destructurarse antes de cualquier otro uso

## `ht_dcperiod(series)`, `ht_dcphase(series)`, `ht_phasor(series)`, `ht_sine(series)`, `ht_trendline(series)` y `ht_trendmode(series)`

Reglas:

- cada funcion requiere exactamente un argumento `series<float>`
- `ht_dcperiod`, `ht_dcphase` y `ht_trendline` devuelven `series<float>`
- `ht_trendmode` devuelve `series<float>` con los valores de modo de tendencia
  `0`/`1` de TA-Lib
- `ht_phasor` devuelve una tupla de 2 valores `(inphase, quadrature)`
- `ht_sine` devuelve una tupla de 2 valores `(sine, lead_sine)`
- los resultados tuple deben destructurarse antes de cualquier otro uso
- estos indicadores siguen el comportamiento de warmup de transformada de
  Hilbert de TA-Lib y producen `na` hasta que se satisface el lookback upstream

## `sar(high, low[, acceleration=0.02[, maximum=0.2]])` y `sarext(high, low[, ...])`

Reglas:

- `high` y `low` deben ser `series<float>`
- todos los parametros opcionales de SAR son escalares numericos
- `sar` devuelve el Parabolic SAR estandar
- `sarext` expone los controles extendidos de SAR de TA-Lib y devuelve valores
  negativos mientras la posicion es short, siguiendo el comportamiento upstream
  de TA-Lib
- el tipo de resultado es `series<float>`

## `wma(series[, length=30])`

Reglas:

- el primer argumento debe ser `series<float>`
- la opcion `length` usa `30` por defecto
- si se provee, `length` debe ser un literal entero mayor o igual a `2`
- el tipo de resultado es `series<float>`
- si no existe suficiente historial, la muestra actual es `na`
- si la ventana requerida contiene `na`, la muestra actual es `na`

## `midpoint(series[, length=14])` y `midprice(high, low[, length=14])`

Reglas:

- `midpoint` requiere `series<float>` como primer argumento
- `midprice` requiere `series<float>` tanto para `high` como para `low`
- la ventana opcional final usa `14` por defecto
- si se provee, la ventana debe ser un literal entero mayor o igual a `2`
- la ventana incluye la muestra actual
- si no existe suficiente historial, el resultado es `na`
- si cualquier muestra requerida de la ventana es `na`, el resultado es `na`
- el tipo de resultado es `series<float>`

## `linearreg(series[, length=14])`, `linearreg_angle(series[, length=14])`, `linearreg_intercept(series[, length=14])`, `linearreg_slope(series[, length=14])` y `tsf(series[, length=14])`

Reglas:

- el primer argumento debe ser `series<float>`
- la opcion `length` usa `14` por defecto
- si se provee, `length` debe ser un literal entero mayor o igual a `2`
- si no existe suficiente historial, la muestra actual es `na`
- si la ventana requerida contiene `na`, la muestra actual es `na`
- `linearreg` devuelve el valor ajustado en la barra actual
- `linearreg_angle` devuelve el angulo de la pendiente ajustada
- `linearreg_intercept` devuelve la interseccion ajustada
- `linearreg_slope` devuelve la pendiente ajustada
- `tsf` devuelve el forecast de un paso hacia adelante
- el tipo de resultado es `series<float>`

## `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])`

Rules:

- the first three arguments must be `series<float>`
- omitted `atr_length` defaults to `10`
- omitted `multiplier` defaults to `3.0`
- if provided, `atr_length` must be an integer literal greater than or equal to `1`
- if provided, `multiplier` must be a numeric scalar
- `supertrend` returns a 2-tuple `(line, bullish)`
- `line` is the active carried band and `bullish` is the persistent regime direction
- the ATR component uses Wilder smoothing and requires prior-close history, so the result is `na` until the lookback is satisfied
- tuple-valued outputs must be destructured before further use

## `donchian(high, low[, length=20])`

Rules:

- the first two arguments must be `series<float>`
- omitted `length` defaults to `20`
- if provided, `length` must be an integer literal greater than or equal to `1`
- `donchian` returns a 3-tuple `(upper, middle, lower)`
- `upper` is the trailing highest high, `lower` is the trailing lowest low, and `middle` is `(upper + lower) / 2`
- if insufficient history exists, or any required sample is `na`, the current tuple is `(na, na, na)`
- tuple-valued outputs must be destructured before further use
