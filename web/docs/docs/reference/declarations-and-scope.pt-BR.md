# Declaracoes E Escopo

Esta pagina define as formas de binding que PalmScript aceita e as regras de
visibilidade ligadas a elas.

## Formas Restritas Ao Nivel Superior

As formas a seguir devem aparecer apenas no nivel superior de um script:

- `interval`
- `source`
- `use`
- `fn`
- `const`
- `input`
- `export`
- `regime`
- `trigger`
- `cooldown`
- `max_bars_in_trade`
- `entry`
- `exit`
- `protect`
- `target`

`let`, `if` e instrucoes de expressao no nivel superior sao permitidos.

## Intervalo Base

Todo script deve declarar exatamente um intervalo base:

```palmscript
interval 1m
```

O compilador rejeita um script sem `interval` base ou com mais de um
`interval` base.

## Declaracoes `source`

Uma declaracao `source` tem esta forma:

```palmscript
source bb = bybit.usdt_perps("BTCUSDT")
```

Regras:

- o alias deve ser um identificador
- o alias deve ser unico entre todas as fontes declaradas
- o template deve ser resolvido para um dos templates de source suportados
- o argumento do simbolo deve ser um literal string

## Declaracoes `use`

Intervalos suplementares sao declarados por fonte:

```palmscript
use bb 1h
```

Regras:

- o alias deve nomear uma source declarada
- o intervalo nao deve ser inferior ao intervalo base
- declaracoes duplicadas `use <alias> <interval>` sao rejeitadas
- um intervalo igual ao intervalo base e aceito, mas redundante

## Funcoes

Funcoes definidas pelo usuario sao declaracoes de nivel superior com corpo de
expressao:

```palmscript
fn cross_signal(a, b) = a > b and a[1] <= b[1]
```

Regras:

- nomes de funcao devem ser unicos
- o nome de uma funcao nao deve colidir com um nome builtin
- nomes de parametros dentro de uma mesma funcao devem ser unicos
- grafos de funcao recursivos e ciclicos sao rejeitados
- corpos de funcao podem referenciar seus parametros, series de source
  declaradas e bindings imutaveis `const` / `input` de nivel superior
- corpos de funcao nao devem chamar `plot`
- corpos de funcao nao devem capturar bindings `let` de escopos de instrucao
  ao redor

Funcoes sao especializadas por tipo de argumento e clock de atualizacao.

## Bindings `let`

`let` cria um binding no escopo de bloco atual:

```palmscript
let basis = ema(spot.close, 20)
```

Regras:

- um `let` duplicado no mesmo escopo e rejeitado
- escopos internos podem sombrear bindings externos
- o valor associado pode ser escalar ou serie
- `na` e permitido e tratado como placeholder numerico-like durante a
  compilacao

PalmScript tambem suporta desestruturacao de tupla para resultados builtin
tuple-valued imediatos:

```palmscript
let (line, signal, hist) = macd(spot.close, 12, 26, 9)
```

Regras adicionais:

- desestruturacao de tupla e uma forma `let` de primeira classe
- o lado direito atualmente deve ser um resultado builtin tuple-valued imediato
- a aridade da tupla deve corresponder exatamente
- expressoes tuple-valued devem ser desestruturadas antes de qualquer outro uso

## `const` E `input`

PalmScript suporta bindings imutaveis de nivel superior para configuracao de
estrategia:

```palmscript
input fast_len = 21
const neutral_rsi = 50
```

Regras:

- ambas as formas sao apenas de nivel superior
- nomes duplicados no mesmo escopo sao rejeitados
- ambas as formas sao apenas escalares na v1: `float`, `bool`, `ma_type`,
  `tif`, `trigger_ref`, `position_side`, `exit_kind` ou `na`
- `input` existe apenas em tempo de compilacao na v1
- valores `input` devem ser literais escalares ou literais enum
- valores `const` podem referenciar bindings `const` / `input` declarados
  anteriormente e builtins escalares puros
- builtins com janela e indexacao de serie aceitam bindings numericos imutaveis
  em qualquer lugar onde um literal inteiro e exigido

## Saidas

`export`, `regime`, `trigger`, sinais de estrategia de primeira classe e declaracoes de
backtest orientadas a ordens sao apenas de nivel superior:

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
regime trend_long = state(ema(spot.close, 20) > ema(spot.close, 50), ema(spot.close, 20) < ema(spot.close, 50))
trigger long_entry = spot.close > spot.high[1]
entry1 long = spot.close > spot.high[1]
entry2 long = crossover(spot.close, ema(spot.close, 20))
order entry1 long = limit(spot.close[1], tif.gtc, false)
protect long = stop_market(position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref.last)
protect_after_target1 long = stop_market(position.entry_price, trigger_ref.last)
target1 long = take_profit_market(position.entry_price + 4, trigger_ref.last)
target2 long = take_profit_market(position.entry_price + 8, trigger_ref.last)
size entry1 long = 0.5
size entry2 long = 0.5
size entry3 long = risk_pct(0.01, stop_price)
size target1 long = 0.5
```

Regras:

- todas as formas sao apenas de nivel superior
- nomes duplicados no mesmo escopo sao rejeitados
- `regime` exige `bool`, `series<bool>` ou `na` e se destina a series persistentes de estado de mercado
- nomes `regime` viram bindings depois do ponto de declaracao e sao registrados com diagnosticos exportados comuns
- nomes `trigger` viram bindings depois do ponto de declaracao
- `entry long` e `entry short` sao aliases de compatibilidade para
  `entry1 long` e `entry1 short`
- `entry1`, `entry2` e `entry3` sao declaracoes de sinal de entrada em estagios
  para backtest
- `exit long` e `exit short` continuam sendo saidas discricionarias unicas para
  a posicao inteira
- `cooldown long|short = <bars>` bloqueia novas entradas do mesmo lado pelas
  proximas `<bars>` barras de execucao apos um fechamento completo desse lado
- `max_bars_in_trade long|short = <bars>` forca uma saida market do mesmo lado
  na proxima abertura de execucao quando a operacao atingir `<bars>` barras de
  execucao em aberto
- ambos os controles declarativos exigem em v1 uma expressao escalar inteira
  nao negativa resolvida em compilacao
- `order entry ...` e `order exit ...` anexam um template de execucao a um role
  de sinal correspondente
- `protect`, `protect_after_target1..3` e `target1..3` declaram saidas
  anexadas em estagios que so armam enquanto a posicao correspondente estiver
  aberta
- `size entry1..3 long|short` permite dimensionar opcionalmente um fill de
  entrada em estagio com semantica `capital_fraction(x)` / fracao numerica nua
  legada, ou `risk_pct(pct, stop_price)` para dimensionamento por risco
- `size target1..3 long|short` permite dimensionar opcionalmente um fill de
  `target` em estagio como fracao da posicao aberta
- no maximo uma declaracao `order` e permitida por role de sinal
- no maximo uma declaracao e permitida por role em estagio
- se um role de sinal nao tiver declaracao `order` explicita, o backtester usa
  exige uma declaracao explicita `order ...`
- `size entry ...` e `size target ...` exigem uma declaracao correspondente
  `order ...` em estagio ou `target ...` anexado para o mesmo role
- `risk_pct(...)` e valido apenas em declaracoes de tamanho de entrada em
  estagio na v1
- saidas anexadas em estagios sao sequenciais: apenas o proximo target e o
  protect atual ficam ativos ao mesmo tempo
- `position.*` esta disponivel apenas dentro de declaracoes `protect` e
  `target`
- `position_event.*` esta disponivel em qualquer lugar onde `series<bool>` seja
  valido e serve para ancorar logica a fills reais de backtest
- os campos atuais de `position_event` sao:
  `long_entry_fill`, `short_entry_fill`, `long_exit_fill`, `short_exit_fill`,
  `long_protect_fill`, `short_protect_fill`, `long_target_fill`,
  `short_target_fill`, `long_signal_exit_fill`, `short_signal_exit_fill`,
  `long_reversal_exit_fill`, `short_reversal_exit_fill`,
  `long_liquidation_fill` e `short_liquidation_fill`
- campos de fill em estagio tambem estao disponiveis:
  `long_entry1_fill` .. `long_entry3_fill`, `short_entry1_fill` ..
  `short_entry3_fill`, `long_target1_fill` .. `long_target3_fill` e
  `short_target1_fill` .. `short_target3_fill`
- `last_exit.*`, `last_long_exit.*` e `last_short_exit.*` estao disponiveis em
  qualquer lugar onde expressoes normais sejam validas
- os campos atuais de `last_*_exit` sao `kind`, `stage`, `side`, `price`,
  `time`, `bar_index`, `realized_pnl`, `realized_return` e `bars_held`
- `last_*_exit.kind` inclui `exit_kind.liquidation` alem dos tipos de saida
  existentes
- scripts legados no estilo `trigger long_entry = ...` continuam suportados
  como ponte de compatibilidade quando nao ha sinais de primeira classe

## Escopo Condicional

`if` introduz dois escopos filhos:

```palmscript
if spot.close > spot.open {
    let x = 1
} else {
    let x = 0
}
```

Regras:

- a condicao deve se avaliar como `bool`, `series<bool>` ou `na`
- ambos os ramos tem escopos independentes
- bindings criados dentro de um ramo nao ficam visiveis fora do `if`

## Metadados De Otimizacao Em `input`

`input`s numericos podem declarar metadados de busca diretamente:

```palmscript
input fast_len = 21 optimize(int, 8, 34, 1)
input atr_mult = 2.5 optimize(float, 1.5, 4.0, 0.25)
input weekly_bias = 21 optimize(choice, 13, 21, 34)
```

Regras:

- `optimize(int, low, high[, step])` exige um valor padrao inteiro dentro do intervalo inclusivo e alinhado ao passo
- `optimize(float, low, high[, step])` exige um valor padrao finito dentro do intervalo inclusivo
- `optimize(choice, v1, v2, ...)` exige que o valor padrao seja uma das opcoes numericas listadas
- esses metadados apenas descrevem o espaco de busca do otimizador; eles nao mudam o valor compilado do `input`

## Latest Portfolio Additions

- PalmScript now reserves `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group`.
- These declarations are top-level only and compile-time only.
- Portfolio mode activates when backtest-oriented CLI commands receive repeated `--execution-source` flags.
- Portfolio mode shares one equity ledger across the selected aliases and blocks only the new entries that would exceed the configured caps.

## Latest Execution Additions

- PalmScript now reserves `execution` as a top-level declaration separate from `source`.
- `execution exec = bybit.usdt_perps("BTCUSDT")` declares an execution target without creating new market series.
- Matching `source` and `execution` aliases may mirror each other when the template and symbol are the same.
- Order constructors now accept named arguments, and `venue = exec` binds that order role to a declared execution alias.
- Positional and named order arguments cannot be mixed in the same order constructor call.
- Execution-oriented CLI modes now require at least one declared `execution` target.
