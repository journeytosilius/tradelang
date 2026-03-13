# Saidas

Esta pagina define as formas de saida visiveis ao usuario no PalmScript.

## Formas De Saida

PalmScript expoe tres construtos produtores de saida:

- `plot(value)`
- `export name = expr`
- `regime name = expr`
- `trigger name = expr`
- `entry long = expr`, `entry1 long = expr`, `entry2 long = expr`,
  `entry3 long = expr`
- `entry short = expr`, `entry1 short = expr`, `entry2 short = expr`,
  `entry3 short = expr`
- `exit long = expr`, `exit short = expr`
- `protect long = order_spec`, `protect short = order_spec`
- `protect_after_target1 long = order_spec`,
  `protect_after_target2 long = order_spec`,
  `protect_after_target3 long = order_spec`
- `protect_after_target1 short = order_spec`,
  `protect_after_target2 short = order_spec`,
  `protect_after_target3 short = order_spec`
- `target long = order_spec`, `target1 long = order_spec`,
  `target2 long = order_spec`, `target3 long = order_spec`
- `target short = order_spec`, `target1 short = order_spec`,
  `target2 short = order_spec`, `target3 short = order_spec`
- `size entry long = expr`, `size entry1 long = expr`,
  `size entry2 long = expr`, `size entry3 long = expr`
- `size entry short = expr`, `size entry1 short = expr`,
  `size entry2 short = expr`, `size entry3 short = expr`
- `size target long = expr`, `size target1 long = expr`,
  `size target2 long = expr`, `size target3 long = expr`
- `size target short = expr`, `size target1 short = expr`,
  `size target2 short = expr`, `size target3 short = expr`

`plot` e uma chamada builtin. `export`, `regime` e `trigger` sao declaracoes.

## `plot`

`plot` emite um ponto de plot para o passo atual.

Regras:

- o argumento deve ser numerico, `series<float>` ou `na`
- o passo atual contribui com um ponto de plot por chamada `plot` executada
- `plot` nao cria um binding reutilizavel na linguagem
- `plot` nao e permitido dentro de corpos de funcao definidos pelo usuario

## `export`

`export` publica uma serie de saida nomeada:

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
```

Regras:

- apenas de nivel superior
- o nome deve ser unico dentro do escopo atual
- a expressao pode se avaliar como numerico, bool, serie numerica, serie
  booleana ou `na`
- `void` e rejeitado

Normalizacao de tipo:

- `export` numerico, serie numerica e `na` viram `series<float>`
- `export` bool e serie booleana viram `series<bool>`

## `regime`

`regime` publica uma serie booleana persistente de estado de mercado com nome:

```palmscript
regime trend_long = state(
    ema(spot.close, 20) > ema(spot.close, 50),
    ema(spot.close, 20) < ema(spot.close, 50)
)
```

Regras:

- apenas de nivel superior
- a expressao deve se avaliar como `bool`, `series<bool>` ou `na`
- o tipo de saida e sempre `series<bool>`
- nomes `regime` viram bindings reutilizaveis depois do ponto de declaracao
- `regime` foi pensado para combinar com `state(...)`, `activated(...)` e `deactivated(...)`
- diagnosticos de runtime o registram junto das series exportadas comuns

## `trigger`

`trigger` publica uma serie de saida booleana nomeada:

```palmscript
trigger long_entry = spot.close > spot.high[1]
```

Regras:

- apenas de nivel superior
- a expressao deve se avaliar como `bool`, `series<bool>` ou `na`
- o tipo de saida e sempre `series<bool>`

Regra de evento em runtime:

- um evento de trigger e emitido para um passo apenas quando a amostra atual do
  trigger e `true`
- `false` e `na` nao emitem eventos de trigger

## Sinais De Estrategia De Primeira Classe

PalmScript expoe declaracoes de sinais de estrategia de primeira classe para
execucao orientada a estrategia:

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
entry short = spot.close < spot.low[1]
exit short = spot.close > ema(spot.close, 20)
```

Regras:

- as quatro declaracoes sao apenas de nivel superior
- cada expressao deve se avaliar como `bool`, `series<bool>` ou `na`
- elas sao compiladas em saidas de trigger com metadata explicita de role de
  sinal
- a emissao de eventos em runtime segue as mesmas regras `true` / `false` /
  `na` dos triggers comuns
- `entry long` e `entry short` sao aliases de compatibilidade para
  `entry1 long` e `entry1 short`
- `entry2` e `entry3` sao sinais sequenciais de adicao no mesmo lado, que so
  ficam elegiveis depois que o estagio anterior foi preenchido no ciclo atual
  da posicao

## Declaracoes `order`

PalmScript tambem expoe declaracoes `order` de nivel superior que parametrizam
como um role de sinal e executado:

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)

order entry long = limit(spot.close[1], tif.gtc, false)
order exit long = stop_market(lowest(spot.low, 5)[1], trigger_ref.last)
```

Regras:

- declaracoes `order` sao apenas de nivel superior
- pode existir no maximo uma declaracao `order` por role de sinal
- modos CLI orientados a execucao exigem uma declaracao explicita `order ...` para cada role de sinal `entry` / `exit`
- campos numericos de ordem como `price`, `trigger_price` e `expire_time_ms`
  sao avaliados pelo runtime como series internas ocultas
- `tif.<variant>` e `trigger_ref.<variant>` sao literais enum tipados validados
  em tempo de compilacao
- validacoes de compatibilidade especificas do venue rodam quando o backtest
  comeca, com base na `source` de execucao

## Saidas Anexadas

PalmScript tambem expoe saidas anexadas de primeira classe para manter livre o
sinal discricionario `exit`:

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
protect long = stop_market(position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref.last)
target long = take_profit_market(
    highest_since(position_event.long_entry_fill, spot.high) + 4,
    trigger_ref.last
)
size target long = 0.5
```

Regras:

- saidas anexadas sao apenas de nivel superior
- `protect` e o estagio base de protecao de um lado
- `protect_after_target1`, `protect_after_target2` e
  `protect_after_target3` opcionalmente fazem ratchet da ordem protect ativa
  depois de cada fill de target em estagio
- `target`, `target1`, `target2` e `target3` sao estagios sequenciais de
  realizacao de lucro; `target` e um alias de compatibilidade para `target1`
- `size entry1..3` e `size target1..3` sao opcionais e se aplicam apenas ao
  entry ou target em estagio correspondente
- o dimensionamento de entradas em estagio suporta:
  - uma fracao numerica nua legada como `0.5`
  - `capital_fraction(x)`
  - `risk_pct(pct, stop_price)`
- valores `capital_fraction(...)` devem se avaliar como uma fracao finita em
  `(0, 1]`
- uma fracao de entrada abaixo de `1` deixa caixa para scale-ins posteriores no
  mesmo lado
- `risk_pct(...)` e apenas para entradas na v1 e dimensiona a partir do preco
  real de fill e da distancia ate o stop no momento do fill
- se um tamanho `risk_pct(...)` pedir mais do que o caixa atual ou a garantia
  livre suporta, o backtester limita o fill e registra
  `capital_limited = true`
- elas so armam depois que existe um fill de entrada correspondente
- sao reavaliadas uma vez por barra de execucao enquanto a posicao permanecer
  aberta
- apenas o protect em estagio atual e o proximo target em estagio ficam ativos
  ao mesmo tempo
- quando `target1` e preenchido, o motor troca de `protect` para
  `protect_after_target1` se declarado; caso contrario, herda o protect mais
  recente disponivel
- fracoes de tamanho de target em estagio devem se avaliar como uma fracao
  finita em `(0, 1]`
- uma declaracao `size targetN ...` transforma o estagio `target` correspondente
  em uma realizacao parcial quando a fracao e menor que `1`
- targets em estagio sao one-shot dentro de um ciclo de posicao e ativam em
  sequencia
- se ambos puderem preencher na mesma barra de execucao, `protect` vence de
  forma deterministica
- `position.*` esta disponivel apenas dentro de declaracoes `protect` e
  `target`
- `position_event.*` e um namespace de serie dirigido por backtest que expoe
  eventos de fill reais como `position_event.long_entry_fill`
- `position_event.*` tambem expoe eventos de fill especificos por tipo de
  saida, como `position_event.long_target_fill`,
  `position_event.long_protect_fill` e
  `position_event.long_liquidation_fill`
- eventos de fill em estagio tambem estao disponiveis, incluindo
  `position_event.long_entry1_fill`, `position_event.long_entry2_fill`,
  `position_event.long_entry3_fill`, `position_event.long_target1_fill`,
  `position_event.long_target2_fill` e `position_event.long_target3_fill`,
  com equivalentes para short
- `last_exit.*`, `last_long_exit.*` e `last_short_exit.*` expoem o snapshot do
  trade fechado mais recente globalmente ou por lado
- `last_*_exit.kind` e comparado com literais enum tipados como
  `exit_kind.target` e `exit_kind.liquidation`
- `last_*_exit.stage` expoe o numero do estagio de target / protect quando
  aplicavel
- fora de backtests, `position_event.*` e definido, mas se avalia como `false`
  em todos os passos
- fora de backtests, `last_*_exit.*` e definido, mas se avalia como `na`

## Compatibilidade Com Triggers Legados

Scripts de estrategia legados que usam nomes de trigger ainda sao suportados
temporariamente:

- `trigger long_entry = ...`
- `trigger long_exit = ...`
- `trigger short_entry = ...`
- `trigger short_exit = ...`

Regras de compatibilidade:

- se um script declarar quaisquer sinais `entry` / `exit` de primeira classe, o
  backtester usa esses roles diretamente
- se um script nao declarar sinais de primeira classe, o backtester faz
  fallback para os nomes de trigger legados acima
- declaracoes `trigger` comuns continuam validas para alertas ou consumidores
  nao estrategicos

## Colecoes De Saida Em Runtime

Ao longo de uma execucao completa, o runtime acumula:

- `plots`
- `exports`
- `triggers`
- `order_fields`
- `trigger_events`
- `alerts`

`alerts` atualmente existem nas estruturas de saida do runtime, mas nao sao
produzidos por um construto de linguagem PalmScript de primeira classe.

## Tempo De Saida E Indice De Barra

Cada amostra de saida e marcada com:

- o `bar_index` atual
- o `time` atual do passo

Em execucoes source-aware, o tempo do passo e o horario de abertura do passo
atual do clock base.

## Latest Diagnostics Additions

PalmScript now exposes richer machine-readable backtest diagnostics in every public locale build:

- `run backtest`, `run walk-forward`, and `run optimize` accept `--diagnostics summary|full-trace`
- summary mode keeps cohort, drawdown-path, source-alignment, holdout-drift, robustness, and hint data
- full-trace mode adds one typed per-bar decision trace per execution bar
- optimize output now includes top-candidate holdout checks plus parameter stability summaries

## Latest Execution Additions

- `execution` declarations now separate execution routing from market-data `source` bindings.
- Order constructors accept named arguments in addition to the legacy positional form.
- `venue = <execution_alias>` binds an `order`, `protect`, or `target` role to a declared execution alias.
- Named order arguments cannot be mixed with positional arguments in the same constructor call.
- Execution-oriented CLI modes now require at least one declared `execution` target.
