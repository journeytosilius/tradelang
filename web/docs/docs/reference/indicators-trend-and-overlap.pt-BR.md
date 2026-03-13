# Trend And Overlap Indicators

Esta pagina define os indicadores executaveis de tendencia e overlap do
PalmScript.

## `sma(series, length)`

Regras:

- exige exatamente dois argumentos
- o primeiro argumento deve ser `series<float>`
- o segundo argumento deve ser um literal inteiro positivo
- o tipo de resultado e `series<float>`
- se nao houver historico suficiente, a amostra atual e `na`
- se a janela necessaria contiver `na`, a amostra atual e `na`

## `ema(series, length)`

Regras:

- exige exatamente dois argumentos
- o primeiro argumento deve ser `series<float>`
- o segundo argumento deve ser um literal inteiro positivo
- o tipo de resultado e `series<float>`
- a serie retorna `na` ate que a janela de seed esteja disponivel

## `ma(series, length, ma_type)`

Regras:

- exige exatamente tres argumentos
- o primeiro argumento deve ser `series<float>`
- o segundo argumento deve ser um literal inteiro positivo
- o terceiro argumento deve ser um valor tipado `ma_type.<variant>`
- o tipo de resultado e `series<float>`
- todas as variantes `ma_type` estao implementadas
- `ma_type.mama` corresponde ao comportamento upstream do TA-Lib e ignora o
  parametro explicito `length`, usando os defaults `fast_limit=0.5` e
  `slow_limit=0.05`

## `apo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])` e `ppo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`

Regras:

- o primeiro argumento deve ser `series<float>`
- `fast_length` e `slow_length` usam `12` e `26` por padrao
- se fornecidos, `fast_length` e `slow_length` devem ser literais inteiros
  maiores ou iguais a `2`
- se fornecido, o quarto argumento deve ser um valor tipado
  `ma_type.<variant>`
- se omitido, `ma_type` usa `ma_type.sma`
- `apo` retorna `fast_ma - slow_ma`
- `ppo` retorna `((fast_ma - slow_ma) / slow_ma) * 100`
- se a media movel lenta for `0`, `ppo` retorna `0`
- o mesmo conjunto executavel de variantes `ma_type` de `ma(...)` e suportado
- o tipo de resultado e `series<float>`

## `macd(series, fast_length, slow_length, signal_length)`

Regras:

- exige exatamente quatro argumentos
- o primeiro argumento deve ser `series<float>`
- os argumentos restantes devem ser literais inteiros positivos
- o tipo de resultado e uma tupla de 3 series na ordem do TA-Lib:
  `(macd_line, signal, histogram)`
- o resultado deve ser desestruturado antes de ser usado em `plot`, `export`,
  condicoes ou outras expressoes

## `macdfix(series[, signal_length=9])`

Regras:

- o primeiro argumento deve ser `series<float>`
- `signal_length` usa `9` por padrao
- se fornecido, `signal_length` deve ser um literal inteiro positivo
- o tipo de resultado e uma tupla de 3 series na ordem do TA-Lib:
  `(macd_line, signal, histogram)`
- o resultado deve ser desestruturado antes de ser usado em `plot`, `export`,
  condicoes ou outras expressoes

## `macdext(series[, fast_length=12[, fast_ma=ma_type.sma[, slow_length=26[, slow_ma=ma_type.sma[, signal_length=9[, signal_ma=ma_type.sma]]]]]])`

Regras:

- o primeiro argumento deve ser `series<float>`
- comprimentos omitidos usam os defaults do TA-Lib `12`, `26` e `9`
- `fast_length` e `slow_length` devem ser literais inteiros maiores ou iguais
  a `2`
- `signal_length` deve ser um literal inteiro maior ou igual a `1`
- cada argumento de media movel deve ser um valor tipado `ma_type.<variant>`
- o mesmo conjunto executavel de variantes `ma_type` de `ma(...)` e suportado
- o tipo de resultado e uma tupla de 3 series na ordem do TA-Lib:
  `(macd_line, signal, histogram)`
- o resultado deve ser desestruturado antes de qualquer outro uso

## `bbands(series[, length=5[, deviations_up=2.0[, deviations_down=2.0[, ma_type=ma_type.sma]]]])`

Regras:

- o primeiro argumento deve ser `series<float>`
- `length` usa `5` por padrao
- se fornecido, `length` deve ser um literal inteiro positivo
- se fornecidos, `deviations_up` e `deviations_down` devem ser escalares
  numericos
- se fornecido, o quinto argumento deve ser um valor tipado
  `ma_type.<variant>`
- o tipo de resultado e uma tupla de 3 series na ordem do TA-Lib:
  `(upper, middle, lower)`
- o resultado deve ser desestruturado antes de ser usado em `plot`, `export`,
  condicoes ou outras expressoes

## `accbands(high, low, close[, length=20])`

Regras:

- os tres primeiros argumentos devem ser `series<float>`
- `length` omitido usa o default do TA-Lib `20`
- se fornecido, `length` deve ser um literal inteiro maior ou igual a `2`
- o tipo de resultado e uma tupla de 3 series na ordem do TA-Lib:
  `(upper, middle, lower)`
- o resultado deve ser desestruturado antes de qualquer outro uso

## `dema(series[, length=30])`, `tema(series[, length=30])`, `trima(series[, length=30])`, `kama(series[, length=30])`, `t3(series[, length=5[, volume_factor=0.7]])` e `trix(series[, length=30])`

Regras:

- o primeiro argumento deve ser `series<float>`
- `length` opcional usa `30` por padrao em `dema`, `tema`, `trima`, `kama` e
  `trix`
- `t3` usa `length=5` e `volume_factor=0.7` por padrao
- se fornecido, `length` deve ser um literal inteiro positivo
- se fornecido, `volume_factor` deve ser um escalar numerico
- o tipo de resultado e `series<float>`

## `mavp(series, periods, minimum_period, maximum_period, ma_type)`

Regras:

- os dois primeiros argumentos devem ser `series<float>`
- `minimum_period` e `maximum_period` devem ser literais inteiros maiores ou
  iguais a `2`
- o quinto argumento deve ser um valor tipado `ma_type.<variant>`
- a familia de medias moveis e o mesmo subconjunto executavel de `ma_type`
  usado em `ma(...)`
- `periods` e limitado barra a barra ao intervalo `[minimum_period, maximum_period]`
- o tipo de resultado e `series<float>`

## `mama(series[, fast_limit=0.5[, slow_limit=0.05]])`

Regras:

- o primeiro argumento deve ser `series<float>`
- `fast_limit` e `slow_limit` usam `0.5` e `0.05` por padrao
- se fornecidos, ambos os argumentos opcionais devem ser escalares numericos
- o tipo de resultado e uma tupla de 2 series na ordem do TA-Lib:
  `(mama, fama)`
- o resultado deve ser desestruturado antes de qualquer outro uso

## `ht_dcperiod(series)`, `ht_dcphase(series)`, `ht_phasor(series)`, `ht_sine(series)`, `ht_trendline(series)` e `ht_trendmode(series)`

Regras:

- cada funcao exige exatamente um argumento `series<float>`
- `ht_dcperiod`, `ht_dcphase` e `ht_trendline` retornam `series<float>`
- `ht_trendmode` retorna `series<float>` com os valores `0` / `1` de trend-mode
  do TA-Lib
- `ht_phasor` retorna uma tupla de 2 valores `(inphase, quadrature)`
- `ht_sine` retorna uma tupla de 2 valores `(sine, lead_sine)`
- resultados em tupla devem ser desestruturados antes de qualquer outro uso
- esses indicadores seguem o comportamento de warmup de transformada de Hilbert
  do TA-Lib e produzem `na` ate que o lookback upstream seja satisfeito

## `sar(high, low[, acceleration=0.02[, maximum=0.2]])` e `sarext(high, low[, ...])`

Regras:

- `high` e `low` devem ser `series<float>`
- todos os parametros opcionais de SAR sao escalares numericos
- `sar` retorna o Parabolic SAR padrao
- `sarext` expoe os controles estendidos de SAR do TA-Lib e retorna valores
  negativos durante posicoes short, correspondendo ao comportamento upstream
  do TA-Lib
- o tipo de resultado e `series<float>`

## `wma(series[, length=30])`

Regras:

- o primeiro argumento deve ser `series<float>`
- `length` opcional usa `30` por padrao
- se fornecido, `length` deve ser um literal inteiro maior ou igual a `2`
- o tipo de resultado e `series<float>`
- se nao houver historico suficiente, a amostra atual e `na`
- se a janela necessaria contiver `na`, a amostra atual e `na`

## `midpoint(series[, length=14])` e `midprice(high, low[, length=14])`

Regras:

- `midpoint` exige `series<float>` como primeiro argumento
- `midprice` exige `series<float>` para `high` e `low`
- a janela opcional usa `14` por padrao
- se fornecida, a janela deve ser um literal inteiro maior ou igual a `2`
- a janela inclui a amostra atual
- se nao houver historico suficiente, o resultado e `na`
- se qualquer amostra necessaria na janela for `na`, o resultado e `na`
- o tipo de resultado e `series<float>`

## `linearreg(series[, length=14])`, `linearreg_angle(series[, length=14])`, `linearreg_intercept(series[, length=14])`, `linearreg_slope(series[, length=14])` e `tsf(series[, length=14])`

Regras:

- o primeiro argumento deve ser `series<float>`
- `length` opcional usa `14` por padrao
- se fornecido, `length` deve ser um literal inteiro maior ou igual a `2`
- se nao houver historico suficiente, a amostra atual e `na`
- se a janela necessaria contiver `na`, a amostra atual e `na`
- `linearreg` retorna o valor ajustado na barra atual
- `linearreg_angle` retorna o angulo da inclinacao ajustada
- `linearreg_intercept` retorna o intercepto ajustado
- `linearreg_slope` retorna a inclinacao ajustada
- `tsf` retorna a previsao de um passo a frente
- o tipo de resultado e `series<float>`

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
