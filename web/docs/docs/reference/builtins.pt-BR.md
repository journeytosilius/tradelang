# Builtins

Esta pagina define as regras builtin compartilhadas do PalmScript e os helpers
builtin que nao sao indicadores.

Contratos especificos de indicadores vivem na secao dedicada
[Indicadores](indicators.md).

## Builtins Executaveis Versus Nomes Reservados

PalmScript expoe tres superficies relacionadas:

- helpers builtin executaveis e saidas documentados nesta pagina
- indicadores executaveis documentados na secao [Indicadores](indicators.md)
- um catalogo TA-Lib reservado mais amplo descrito em [TA-Lib Surface](ta-lib.md)

Nem todo nome reservado do TA-Lib e executavel hoje. Nomes reservados mas ainda
nao executaveis produzem diagnosticos de compilacao deterministas em vez de
serem tratados como identificadores desconhecidos.

## Categorias De Builtin

PalmScript atualmente expoe estas categorias de builtin:

- indicadores: [Trend and Overlap](indicators-trend-and-overlap.md),
  [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md)
  e [Math, Price, and Statistics](indicators-math-price-statistics.md)
- helpers relacionais: `above`, `below`, `between`, `outside`
- helpers de cruzamento: `cross`, `crossover`, `crossunder`
- helpers de tempo/sessao: `hour_utc`, `weekday_utc`, `session_utc`
- helpers de preco de saida: `trail_stop_long`, `trail_stop_short`, `break_even_long`, `break_even_short`
- helpers de selecao de venue: `cheapest`, `richest`, `spread_bps`, `rank_asc`, `rank_desc`, `current_execution`, `select_asc`, `select_desc`, `in_top_n`, `in_bottom_n`
- helpers de null: `na(value)`, `nz(value[, fallback])`,
  `coalesce(value, fallback)`
- helpers de serie e janela: `change`, `highest`, `lowest`, `highestbars`,
  `lowestbars`, `rising`, `falling`, `cum`
- helpers de memoria de evento: `state`, `activated`, `deactivated`, `barssince`,
  `valuewhen`, `highest_since`, `lowest_since`, `highestbars_since`,
  `lowestbars_since`, `valuewhen_since`, `count_since`
- saidas: `plot`

Campos de mercado sao selecionados por series qualificadas por fonte, como
`spot.open`, `spot.close` ou `bb.1h.volume`. Apenas identificadores sao
invocaveis, portanto `spot.close()` e rejeitado.

## Helpers De Selecao De Venue

### `cheapest(exec_a, exec_b, ...)` e `richest(exec_a, exec_b, ...)`

Regras:

- exigem pelo menos dois aliases `execution` declarados
- cada argumento deve ser um `execution_alias` ou `na`
- comparam o fechamento de execucao atual de cada alias na barra ativa
- `cheapest(...)` retorna o alias com o menor fechamento atual
- `richest(...)` retorna o alias com o maior fechamento atual
- aliases sem fechamento de execucao atual na barra ativa sao ignorados
- se todos os aliases referenciados estiverem indisponiveis na barra ativa, o resultado sera `na`
- o tipo de resultado e `execution_alias`

Os resultados de selecao servem para logica posterior com aliases de execucao,
como comparacoes de igualdade ou helpers de spread. Eles nao sao exportados
diretamente.

### `spread_bps(buy_exec, sell_exec)`

Regras:

- exige exatamente dois aliases `execution` declarados
- ambos os argumentos devem ser `execution_alias` ou `na`
- e avaliado como `((sell_close - buy_close) / buy_close) * 10000`
- se qualquer alias referenciado nao tiver fechamento de execucao atual na barra ativa, o resultado sera `na`
- o tipo de resultado e `float` ou `series<float>` conforme o clock de atualizacao ativo

### `rank_asc(target_exec, exec_a, exec_b, ...)` e `rank_desc(target_exec, exec_a, exec_b, ...)`

Regras:

- exigem pelo menos tres aliases `execution` declarados no total: um alias alvo e pelo menos dois aliases comparados
- o primeiro argumento e o alias alvo; os argumentos restantes formam o conjunto comparado
- cada argumento deve ser um `execution_alias` ou `na`
- classificam os fechamentos de execucao atuais dentro do conjunto comparado fornecido
- `rank_asc(...)` atribui o rank `1` ao menor fechamento atual
- `rank_desc(...)` atribui o rank `1` ao maior fechamento atual
- empates sao resolvidos de forma deterministica pela ordem dos argumentos comparados
- aliases sem fechamento de execucao atual na barra ativa sao ignorados
- se o alias alvo estiver indisponivel na barra ativa ou ausente do conjunto ranqueado, o resultado sera `na`
- o tipo de resultado e `float` ou `series<float>` conforme o clock de atualizacao ativo

### `current_execution()`

Regras:

- nao aceita argumentos
- dentro de backtests orientados a `execution` e no modo portfolio, retorna o alias de execucao que esta sendo avaliado naquela barra
- fora desse contexto de runtime, o resultado sera `na`
- o tipo de resultado e `execution_alias`
- ele foi feito para logica de sinais, exports e helpers; ordens single-leg ainda exigem `venue = <execution_alias_identifier>`

### `select_asc(rank, exec_a, exec_b, ...)` e `select_desc(rank, exec_a, exec_b, ...)`

Regras:

- exigem um rank inteiro positivo e pelo menos dois aliases `execution` candidatos
- o primeiro argumento e o rank solicitado, em que `1` significa o melhor candidato na ordenacao escolhida
- cada argumento restante deve ser um `execution_alias` ou `na`
- classificam o fechamento de execucao atual entre os candidatos fornecidos
- `select_asc(...)` retorna rank `1` para o menor fechamento atual
- `select_desc(...)` retorna rank `1` para o maior fechamento atual
- empates sao resolvidos de forma deterministica pela ordem dos argumentos comparados
- aliases sem fechamento de execucao atual na barra ativa sao ignorados
- se o rank solicitado for invalido ou exceder o conjunto disponivel de candidatos, o resultado sera `na`
- o tipo de resultado e `execution_alias`

### `in_top_n(target_exec, count, exec_a, exec_b, ...)` e `in_bottom_n(target_exec, count, exec_a, exec_b, ...)`

Regras:

- exigem um alias alvo, um tamanho inteiro positivo de coorte e pelo menos dois aliases `execution` candidatos
- o primeiro argumento e o alias cuja participacao sera verificada
- o segundo argumento e o tamanho da coorte
- cada argumento restante deve ser um `execution_alias` ou `na`
- classificam o fechamento de execucao atual entre os candidatos fornecidos usando a mesma ordenacao deterministica de `select_asc(...)` e `select_desc(...)`
- `in_top_n(...)` verifica participacao na coorte mais alta
- `in_bottom_n(...)` verifica participacao na coorte mais baixa
- aliases sem fechamento de execucao atual na barra ativa sao ignorados
- se o alias alvo estiver indisponivel na barra ativa, ausente do conjunto candidato ou se o tamanho da coorte for invalido, o resultado sera `na`
- se o tamanho da coorte exceder o conjunto disponivel de candidatos, todos os candidatos disponiveis sao considerados dentro da coorte
- o tipo de resultado e `bool` ou `series<bool>` conforme o clock de atualizacao ativo

Exemplo:

```palmscript
execution bn = binance.spot("BTCUSDT")
execution gt = gate.spot("BTC_USDT")
execution bb = bybit.spot("BTCUSDT")

export buy_gate = cheapest(bn, gt) == gt
export venue_spread_bps = spread_bps(cheapest(bn, gt), richest(bn, gt))
export bn_rank_desc = rank_desc(bn, bn, gt)
export best_exec = current_execution() == select_desc(1, bn, gt, bb)
export gt_in_top_two = in_top_n(gt, 2, bn, gt, bb)
```

## Helpers De Tempo E Sessao

### `hour_utc(time_value)` e `weekday_utc(time_value)`

Regras:

- ambos os helpers aceitam um timestamp numerico ou um timestamp `series<float>` como `spot.time`
- `hour_utc(...)` retorna a hora UTC no intervalo `0..23`
- `weekday_utc(...)` retorna o dia da semana UTC com `Segunda=0` ate `Domingo=6`
- se a entrada for `na`, o resultado sera `na`
- se a entrada for uma serie, o tipo de resultado sera `series<float>`
- caso contrario, o tipo de resultado sera `float`

### `session_utc(time_value, start_hour, end_hour)`

Regras:

- o primeiro argumento e um timestamp numerico ou um timestamp `series<float>` como `spot.time`
- o segundo e o terceiro argumento sao horas UTC numericas literais ou inputs numericos imutaveis no intervalo `0..24`
- a janela de sessao e semiaberta: `[start_hour, end_hour)`
- se `start_hour < end_hour`, o helper corresponde diretamente a essa janela intraday
- se `start_hour > end_hour`, o helper faz wrap overnight, por exemplo `22 -> 2`
- se `start_hour == end_hour`, o helper cobre todo o dia UTC
- se o timestamp for `na`, o resultado sera `na`
- se o timestamp for uma serie, o tipo de resultado sera `series<bool>`
- caso contrario, o tipo de resultado sera `bool`

Exemplo:

```palmscript
source spot = binance.spot("BTCUSDT")

export hour = hour_utc(spot.time)
export weekday = weekday_utc(spot.time)
export london_morning = session_utc(spot.time, 8, 12)
export asia_wrap = session_utc(spot.time, 22, 2)
```

## Helpers De Preco De Saida

### `trail_stop_long(anchor_price, stop_offset)` e `trail_stop_short(anchor_price, stop_offset)`

Regras:

- ambos os helpers aceitam entradas numericas ou `series<float>`
- `trail_stop_long(...)` avalia como `anchor_price - stop_offset`
- `trail_stop_short(...)` avalia como `anchor_price + stop_offset`
- se qualquer entrada for `na`, o resultado sera `na`
- se `stop_offset` for negativo ou alguma entrada numerica nao for finita, o resultado sera `na`
- se alguma entrada for serie, o tipo de resultado sera `series<float>`
- caso contrario, o tipo de resultado sera `float`

### `break_even_long(entry_price, stop_offset)` e `break_even_short(entry_price, stop_offset)`

Regras:

- ambos os helpers aceitam entradas numericas ou `series<float>`
- `break_even_long(...)` avalia como `entry_price + stop_offset`
- `break_even_short(...)` avalia como `entry_price - stop_offset`
- se qualquer entrada for `na`, o resultado sera `na`
- se `stop_offset` for negativo ou alguma entrada numerica nao for finita, o resultado sera `na`
- se alguma entrada for serie, o tipo de resultado sera `series<float>`
- caso contrario, o tipo de resultado sera `float`

Exemplo:

```palmscript
protect long = stop_market(
    trigger_price = trail_stop_long(highest_since(position_event.long_entry_fill, spot.high), 3 * atr(spot.high, spot.low, spot.close, 14)),
    trigger_ref = trigger_ref.last,
    venue = exec
)
protect_after_target1 long = stop_market(
    trigger_price = break_even_long(position.entry_price, 0),
    trigger_ref = trigger_ref.last,
    venue = exec
)
```

## Builtins Tuple-Valued

Os builtins tuple-valued executaveis atuais sao:

- `macd(series, fast_length, slow_length, signal_length)` documentado em
  [Trend and Overlap](indicators-trend-and-overlap.md)
- `minmax(series[, length=30])` documentado em
  [Math, Price, and Statistics](indicators-math-price-statistics.md)
- `minmaxindex(series[, length=30])` documentado em
  [Math, Price, and Statistics](indicators-math-price-statistics.md)
- `aroon(high, low[, length=14])` documentado em
  [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md)
- `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])` documentado em
  [Trend and Overlap](indicators-trend-and-overlap.md)
- `donchian(high, low[, length=20])` documentado em
  [Trend and Overlap](indicators-trend-and-overlap.md)

Todos os resultados builtin tuple-valued precisam ser desestruturados
imediatamente com `let (...) = ...` antes de qualquer outro uso.

## Regras Comuns Dos Builtins

Regras:

- todos os builtins sao deterministicos
- builtins nao devem realizar I/O, acessar tempo ou acessar a rede
- `plot` escreve no fluxo de saida; todos os outros builtins sao puros
- helpers builtin e indicadores propagam `na` a menos que uma regra mais
  especifica substitua esse comportamento
- resultados builtin seguem os clocks de atualizacao implicados por seus
  argumentos de serie

## Helpers Relacionais

### `above(a, b)` e `below(a, b)`

Regras:

- ambos os argumentos devem ser numericos, `series<float>` ou `na`
- `above(a, b)` se avalia como `a > b`
- `below(a, b)` se avalia como `a < b`
- se qualquer entrada necessaria for `na`, o resultado e `na`
- se qualquer entrada for uma serie, o tipo de resultado e `series<bool>`
- caso contrario, o tipo de resultado e `bool`

### `between(x, low, high)` e `outside(x, low, high)`

Regras:

- todos os argumentos devem ser numericos, `series<float>` ou `na`
- `between(x, low, high)` se avalia como `low < x and x < high`
- `outside(x, low, high)` se avalia como `x < low or x > high`
- se qualquer entrada necessaria for `na`, o resultado e `na`
- se qualquer argumento for uma serie, o tipo de resultado e `series<bool>`
- caso contrario, o tipo de resultado e `bool`

## Helpers De Cruzamento

### `crossover(a, b)`

Regras:

- ambos os argumentos devem ser numericos, `series<float>` ou `na`
- pelo menos um argumento deve ser `series<float>`
- argumentos escalares sao tratados como thresholds, entao sua amostra anterior
  e seu valor atual
- se avalia como `a > b` no presente e `a[1] <= b[1]` no passado
- se qualquer amostra atual ou anterior necessaria for `na`, o resultado e `na`
- o tipo de resultado e `series<bool>`

### `crossunder(a, b)`

Regras:

- ambos os argumentos devem ser numericos, `series<float>` ou `na`
- pelo menos um argumento deve ser `series<float>`
- argumentos escalares sao tratados como thresholds, entao sua amostra anterior
  e seu valor atual
- se avalia como `a < b` no presente e `a[1] >= b[1]` no passado
- se qualquer amostra atual ou anterior necessaria for `na`, o resultado e `na`
- o tipo de resultado e `series<bool>`

### `cross(a, b)`

Regras:

- ambos os argumentos seguem o mesmo contrato de `crossover` e `crossunder`
- se avalia como `crossover(a, b) or crossunder(a, b)`
- se qualquer amostra atual ou anterior necessaria for `na`, o resultado e `na`
- o tipo de resultado e `series<bool>`

## Helpers De Serie E Janela

### `change(series, length)`

Regras:

- exige exatamente dois argumentos
- o primeiro argumento deve ser `series<float>`
- o segundo argumento deve ser um literal inteiro positivo
- se avalia como `series - series[length]`
- se a amostra atual ou referenciada for `na`, o resultado e `na`
- o tipo de resultado e `series<float>`

### `highest(series, length)` e `lowest(series, length)`

Regras:

- o primeiro argumento deve ser `series<float>`
- o segundo argumento deve ser um literal inteiro positivo
- a janela inclui a amostra atual
- se nao houver historico suficiente, o resultado e `na`
- se qualquer amostra necessaria na janela for `na`, o resultado e `na`
- o tipo de resultado e `series<float>`

O argumento `length` pode ser um literal inteiro positivo ou um binding
numerico imutavel de nivel superior declarado com `const` ou `input`.

### `highestbars(series, length)` e `lowestbars(series, length)`

Regras:

- o primeiro argumento deve ser `series<float>`
- o segundo argumento segue a mesma regra de inteiro positivo de
  `highest` / `lowest`
- a janela inclui a amostra atual
- o resultado e o numero de barras desde a maior ou menor amostra na janela
  ativa
- se nao houver historico suficiente, o resultado e `na`
- se qualquer amostra necessaria na janela for `na`, o resultado e `na`
- o tipo de resultado e `series<float>`

### `rising(series, length)` e `falling(series, length)`

Regras:

- o primeiro argumento deve ser `series<float>`
- o segundo argumento deve ser um literal inteiro positivo
- `rising(series, length)` significa que a amostra atual e estritamente maior
  do que toda amostra anterior nas `length` barras anteriores
- `falling(series, length)` significa que a amostra atual e estritamente menor
  do que toda amostra anterior nas `length` barras anteriores
- se nao houver historico suficiente, o resultado e `na`
- se qualquer amostra necessaria for `na`, o resultado e `na`
- o tipo de resultado e `series<bool>`

### `cum(value)`

Regras:

- exige exatamente um argumento numerico ou `series<float>`
- retorna a soma cumulativa no clock de atualizacao do argumento
- se a amostra atual de entrada for `na`, a amostra atual de saida e `na`
- amostras posteriores nao-`na` continuam acumulando a partir do total anterior
- o tipo de resultado e `series<float>`

## Helpers De Null

### `na(value)`

Regras:

- exige exatamente um argumento
- retorna `true` quando a amostra atual do argumento e `na`
- retorna `false` quando a amostra atual do argumento e um valor escalar
  concreto
- se o argumento for baseado em serie, o tipo de resultado e `series<bool>`
- caso contrario, o tipo de resultado e `bool`

### `nz(value[, fallback])`

Regras:

- aceita um ou dois argumentos
- com um argumento, entradas numericas usam `0` e entradas booleanas usam
  `false` como fallback
- com dois argumentos, o segundo argumento e retornado quando o primeiro e `na`
- ambos os argumentos devem ser valores numericos ou booleanos compativeis em
  tipo
- o tipo de resultado segue o tipo elevado dos operandos

### `coalesce(value, fallback)`

Regras:

- exige exatamente dois argumentos
- retorna o primeiro argumento quando ele nao e `na`
- caso contrario, retorna o segundo argumento
- ambos os argumentos devem ser valores numericos ou booleanos compativeis em
  tipo
- o tipo de resultado segue o tipo elevado dos operandos

## Helpers De Memoria De Evento

### `activated(condition)` e `deactivated(condition)`

Regras:

- ambos exigem exatamente um argumento
- o argumento deve ser `series<bool>`
- `activated` retorna `true` quando a amostra atual da condicao e `true` e a
  amostra anterior era `false` ou `na`
- `deactivated` retorna `true` quando a amostra atual da condicao e `false` e a
  amostra anterior era `true`
- se a amostra atual for `na`, ambos os helpers retornam `false`
- o tipo de resultado e `series<bool>`

### `state(enter, exit)`

Regras:

- exige exatamente dois argumentos
- ambos os argumentos devem ser `series<bool>`
- retorna um estado persistente `series<bool>` que comeca em `false`
- `enter = true` com `exit = false` liga o estado
- `exit = true` com `enter = false` desliga o estado
- se ambos os argumentos forem `true` na mesma barra, o estado anterior e preservado
- se qualquer amostra de entrada atual for `na`, essa entrada e tratada como uma transicao inativa na barra atual
- o tipo de resultado e `series<bool>`

Esta e a base pretendida para declaracoes `regime` de primeira classe:

```palmscript
regime trend_long = state(close > ema(close, 20), close < ema(close, 20))
export trend_started = activated(trend_long)
```

### `barssince(condition)`

Regras:

- exige exatamente um argumento
- o argumento deve ser `series<bool>`
- retorna `0` nas barras onde a amostra atual da condicao e `true`
- incrementa a cada atualizacao do proprio clock da condicao depois do ultimo
  evento verdadeiro
- retorna `na` ate o primeiro evento verdadeiro
- se a amostra atual da condicao for `na`, a saida atual e `na`
- o tipo de resultado e `series<float>`

### `valuewhen(condition, source, occurrence)`

Regras:

- exige exatamente tres argumentos
- o primeiro argumento deve ser `series<bool>`
- o segundo argumento deve ser `series<float>` ou `series<bool>`
- o terceiro argumento deve ser um literal inteiro nao negativo
- ocorrencia `0` significa o evento verdadeiro mais recente
- o tipo de resultado corresponde ao tipo do segundo argumento
- retorna `na` ate existirem eventos verdadeiros suficientes
- se a amostra atual da condicao for `na`, a saida atual e `na`
- quando a amostra atual da condicao e `true`, a amostra atual de `source` e
  capturada para ocorrencias futuras

### `highest_since(anchor, source)` e `lowest_since(anchor, source)`

Regras:

- ambos exigem exatamente dois argumentos
- o primeiro argumento deve ser `series<bool>`
- o segundo argumento deve ser `series<float>`
- quando a amostra atual da ancora e `true`, uma nova epoca ancorada comeca na
  barra atual
- a barra atual contribui imediatamente para a nova epoca
- antes da primeira ancora, o resultado e `na`
- ancoras verdadeiras posteriores descartam a epoca anterior e iniciam outra
- o tipo de resultado e `series<float>`

### `highestbars_since(anchor, source)` e `lowestbars_since(anchor, source)`

Regras:

- ambos exigem exatamente dois argumentos
- o primeiro argumento deve ser `series<bool>`
- o segundo argumento deve ser `series<float>`
- seguem as mesmas regras de reset de epoca ancorada que `highest_since` /
  `lowest_since`
- o resultado e o numero de barras desde a maior ou menor amostra dentro da
  epoca ancorada atual
- antes da primeira ancora, o resultado e `na`
- o tipo de resultado e `series<float>`

### `valuewhen_since(anchor, condition, source, occurrence)`

Regras:

- exige exatamente quatro argumentos
- o primeiro e o segundo argumentos devem ser `series<bool>`
- o terceiro argumento deve ser `series<float>` ou `series<bool>`
- o quarto argumento deve ser um literal inteiro nao negativo
- quando a amostra atual da ancora e `true`, correspondencias anteriores de
  `condition` sao esquecidas e uma nova epoca ancorada comeca na barra atual
- ocorrencia `0` significa o evento correspondente mais recente dentro da epoca
  ancorada atual
- antes da primeira ancora, o resultado e `na`
- o tipo de resultado corresponde ao tipo do terceiro argumento

### `count_since(anchor, condition)`

Regras:

- exige exatamente dois argumentos
- ambos os argumentos devem ser `series<bool>`
- quando a amostra atual da ancora e `true`, a contagem acumulada e
  reinicializada e uma nova epoca ancorada comeca na barra atual
- a barra atual contribui imediatamente para a nova epoca ancorada
- a contagem aumenta apenas nas barras onde a amostra atual de `condition` e
  `true`
- antes da primeira ancora, o resultado e `na`
- ancoras verdadeiras posteriores descartam a epoca anterior e iniciam outra
- o tipo de resultado e `series<float>`

## `plot(value)`

`plot` emite um ponto de plot para o passo atual.

Regras:

- exige exatamente um argumento
- o argumento deve ser numerico, `series<float>` ou `na`
- o tipo de resultado da expressao e `void`
- `plot` nao deve ser chamado dentro do corpo de uma funcao definida pelo
  usuario

Em runtime:

- valores numericos sao registrados como pontos de plot
- `na` registra um ponto de plot sem valor numerico

## Clocks De Atualizacao

Resultados builtin seguem os clocks de atualizacao de suas entradas.

Exemplos:

- `ema(spot.close, 20)` avanca no clock base
- `highest(spot.1w.close, 5)` avanca no clock semanal
- `cum(spot.1w.close - spot.1w.close[1])` avanca no clock semanal
- `crossover(bb.close, bn.close)` avanca quando qualquer serie de source
  referenciada avanca
- `activated(trend_long)` avanca no clock de `trend_long`
- `barssince(spot.close > spot.close[1])` avanca no clock daquela serie de
  condicao
- `valuewhen(trigger_series, bb.1h.close, 0)` avanca no clock de
  `trigger_series`
- `highest_since(position_event.long_entry_fill, spot.high)` avanca no clock
  compartilhado pela ancora e pela serie fonte
