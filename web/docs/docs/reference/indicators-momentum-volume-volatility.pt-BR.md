# Momentum, Volume, And Volatility Indicators

Esta pagina define os indicadores executaveis de momentum, osciladores, volume
e volatilidade do PalmScript.

## `rsi(series, length)`

Regras:

- exige exatamente dois argumentos
- o primeiro argumento deve ser `series<float>`
- o segundo argumento deve ser um literal inteiro positivo
- o tipo de resultado e `series<float>`
- a serie retorna `na` ate que exista historico suficiente para inicializar o
  estado do indicador

## `roc(series[, length=10])`, `mom(series[, length=10])`, `rocp(series[, length=10])`, `rocr(series[, length=10])` e `rocr100(series[, length=10])`

Regras:

- o primeiro argumento deve ser `series<float>`
- o argumento opcional `length` deve ser um literal inteiro positivo
- `length` omitido usa o default do TA-Lib `10`
- `roc` se avalia como `((series - series[length]) / series[length]) * 100`
- `mom` se avalia como `series - series[length]`
- `rocp` se avalia como `(series - series[length]) / series[length]`
- `rocr` se avalia como `series / series[length]`
- `rocr100` se avalia como `(series / series[length]) * 100`
- se a amostra atual ou referenciada for `na`, o resultado e `na`
- se `series[length]` for `0`, `roc`, `rocp`, `rocr` e `rocr100` retornam `na`

## `cmo(series[, length=14])`

Regras:

- o primeiro argumento deve ser `series<float>`
- `length` omitido usa o default do TA-Lib `14`
- se fornecido, `length` deve ser um literal inteiro maior ou igual a `2`
- `cmo` usa o estado de ganhos e perdas suavizado no estilo Wilder do TA-Lib
- o tipo de resultado e `series<float>`
- se a soma de ganho e perda suavizados for `0`, `cmo` retorna `0`

## `cci(high, low, close[, length=14])`

Regras:

- os tres primeiros argumentos devem ser `series<float>`
- `length` omitido usa o default do TA-Lib `14`
- se fornecido, `length` deve ser um literal inteiro maior ou igual a `2`
- `cci` usa a media do preco tipico e o desvio medio na janela solicitada
- se o delta atual do preco tipico ou o desvio medio for `0`, `cci` retorna `0`
- o tipo de resultado e `series<float>`

## `aroon(high, low[, length=14])` e `aroonosc(high, low[, length=14])`

Regras:

- os dois primeiros argumentos devem ser `series<float>`
- `length` omitido usa o default do TA-Lib `14`
- se fornecido, `length` deve ser um literal inteiro maior ou igual a `2`
- `aroon` usa uma janela trailing de high/low com `length + 1` para corresponder
  ao lookback do TA-Lib
- `aroon` retorna uma tupla `(aroon_down, aroon_up)` na ordem de saida do
  TA-Lib
- `aroonosc` retorna `aroon_up - aroon_down`
- saidas tuple-valued devem ser desestruturadas antes de qualquer outro uso

## `plus_dm(high, low[, length=14])`, `minus_dm(high, low[, length=14])`, `plus_di(high, low, close[, length=14])`, `minus_di(high, low, close[, length=14])`, `dx(high, low, close[, length=14])`, `adx(high, low, close[, length=14])` e `adxr(high, low, close[, length=14])`

Regras:

- todos os argumentos de preco devem ser `series<float>`
- `length` omitido usa o default do TA-Lib `14`
- se fornecido, `length` deve ser um literal inteiro positivo
- `plus_dm` e `minus_dm` retornam movimento direcional suavizado por Wilder
- `plus_di` e `minus_di` retornam indicadores direcionais de Wilder
- `dx` retorna o spread direcional absoluto multiplicado por 100
- `adx` retorna a media Wilder de `dx`
- `adxr` retorna a media entre o `adx` atual e o `adx` defasado
- se qualquer preco exigido na barra ativa for `na`, o resultado dessa barra sera `na`
- o tipo de resultado e `series<float>`

## `atr(high, low, close[, length=14])` e `natr(high, low, close[, length=14])`

Regras:

- todos os argumentos devem ser `series<float>`
- `length` omitido usa o default do TA-Lib `14`
- se fornecido, `length` deve ser um literal inteiro positivo
- `atr` e inicializado a partir do average true range inicial e depois aplica
  a suavizacao de Wilder
- `natr` retorna `(atr / close) * 100`
- se qualquer preco exigido na barra ativa for `na`, o resultado dessa barra sera `na`
- o tipo de resultado e `series<float>`

## `willr(high, low, close[, length=14])`

Regras:

- os tres primeiros argumentos devem ser `series<float>`
- `length` omitido usa o default do TA-Lib `14`
- se fornecido, `length` deve ser um literal inteiro maior ou igual a `2`
- `willr` usa o highest high e lowest low trailing da janela solicitada
- o tipo de resultado e `series<float>`
- se o range trailing entre high e low for `0`, `willr` retorna `0`

## `mfi(high, low, close, volume[, length=14])` e `imi(open, close[, length=14])`

Regras:

- todos os argumentos devem ser `series<float>`
- `length` omitido usa o default do TA-Lib `14`
- se fornecido, `length` deve ser um literal inteiro positivo
- `mfi` usa preco tipico e money flow em uma janela trailing
- `imi` usa o movimento intraday open-close na janela solicitada
- o tipo de resultado e `series<float>`

## `stoch(high, low, close[, fast_k=5[, slow_k=3[, slow_k_ma=ma_type.sma[, slow_d=3[, slow_d_ma=ma_type.sma]]]]])`, `stochf(high, low, close[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]])` e `stochrsi(series[, time_period=14[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]]])`

Regras:

- todos os argumentos de preco ou source devem ser `series<float>`
- periodos omitidos usam defaults do TA-Lib
- comprimentos `fast_k`, `slow_k`, `fast_d` e `slow_d` devem ser literais
  inteiros positivos
- `time_period` para `stochrsi` deve ser um literal inteiro maior ou igual a
  `2`
- todos os argumentos de media movel devem ser valores tipados
  `ma_type.<variant>`
- `stoch` retorna `(slowk, slowd)` na ordem do TA-Lib
- `stochf` retorna `(fastk, fastd)` na ordem do TA-Lib
- `stochrsi` retorna `(fastk, fastd)` na ordem do TA-Lib
- saidas tuple-valued devem ser desestruturadas antes de qualquer outro uso

## `ad(high, low, close, volume)`, `adosc(high, low, close, volume[, fast_length=3[, slow_length=10]])` e `obv(series, volume)`

Regras:

- todos os argumentos devem ser `series<float>`
- `ad` retorna a linha cumulativa de acumulacao / distribuicao
- `adosc` retorna a diferenca entre as EMAs rapida e lenta da linha de
  acumulacao / distribuicao
- `fast_length` e `slow_length` omitidos usam os defaults do TA-Lib `3` e `10`
- `obv` e iniciado a partir do `volume` atual e depois soma ou subtrai volume
  de acordo com a direcao do preco
- se a amostra de preco ou volume necessaria for `na`, o resultado e `na`
- o tipo de resultado e `series<float>`

## `trange(high, low, close)`

Regras:

- todos os argumentos devem ser `series<float>`
- a primeira amostra de saida e `na`
- amostras posteriores usam a semantica de true range do TA-Lib baseada em
  `high` atual, `low` atual e `close` anterior
- se qualquer amostra necessaria for `na`, o resultado e `na`
- o tipo de resultado e `series<float>`

## `anchored_vwap(anchor, price, volume)`

Rules:

- `anchor` must be `series<bool>`
- `price` and `volume` must be `series<float>`
- when the current `anchor` sample is `true`, the running VWAP resets on that same bar
- the anchor bar is included in the new anchored accumulation window
- if the current anchor, price, or volume sample is `na`, the current output sample is `na`
- if cumulative anchored volume is `0`, the current output sample is `na`
- the result type is `series<float>`
