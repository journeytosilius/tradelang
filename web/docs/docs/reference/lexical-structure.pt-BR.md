# Estrutura Lexica

O codigo-fonte de PalmScript e tokenizado antes do parsing. O lexer deve
preservar a ordem do codigo e os spans, e deve rejeitar caracteres
desconhecidos e literais de intervalo invalidos.

## Palavras-Chave

Os seguintes tokens sao palavras-chave reservadas:

- `fn`
- `let`
- `interval`
- `source`
- `use`
- `const`
- `input`
- `export`
- `regime`
- `trigger`
- `entry`
- `exit`
- `protect`
- `target`
- `order`
- `if`
- `else`
- `and`
- `or`
- `true`
- `false`
- `na`

Essas palavras-chave nao devem ser usadas onde um identificador e exigido.

## Identificadores

Um identificador:

- deve comecar com uma letra ASCII ou `_`
- pode continuar com letras ASCII, digitos ou `_`

Exemplos:

- `trend`
- `_tmp1`
- `weekly_basis`

## Literais

### Literais Numericos

Literais numericos sao interpretados como `f64`.

Formas aceitas:

- `1`
- `14`
- `3.5`

Formas rejeitadas:

- notacao exponencial como `1e6`
- formas com ponto inicial como `.5`

Numeros negativos sao expressos pelo operador unario `-`, nao por um token
separado de literal assinado.

### Literais Booleanos

Literais booleanos sao:

- `true`
- `false`

### Literal De Valor Ausente

`na` e o literal de valor ausente.

### Literais String

Literais string atualmente sao aceitos apenas onde a gramatica os permite nas
declaracoes `source`.

Eles:

- sao delimitados por `"`
- podem conter escapes basicos para `"`, `\\`, `\\n`, `\\r` e `\\t`
- nao devem atravessar uma nova linha nao escapada

## Comentarios

Apenas comentarios de linha unica sao suportados:

```palmscript
// regime de tendencia
let fast = ema(spot.close, 5)
```

Um token `/` isolado e o operador aritmetico de divisao. `//` inicia um
comentario de linha unica.

## Separadores De Instrucao

Instrucoes sao separadas por:

- novas linhas
- ponto e virgula

Novas linhas dentro de parenteses ou colchetes nao encerram uma instrucao.

## Literais De Intervalo

Literais de intervalo diferenciam maiusculas de minusculas. O conjunto aceito
e definido em [Tabela De Intervalos](intervals.md).

Por exemplo:

- `1w` e valido
- `1M` e valido
- `1W` e invalido

## Nota Sobre `optimize`

`optimize` agora e uma palavra reservada. Ela e usada como sufixo de metadados em declaracoes `input ... optimize(...)` e nao pode ser reutilizada como identificador comum.
