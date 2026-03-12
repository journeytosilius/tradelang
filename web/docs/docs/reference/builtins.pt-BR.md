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
