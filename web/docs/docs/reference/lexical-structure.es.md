# Estructura Lexica

El codigo fuente de PalmScript se tokeniza antes del parseo. El lexer debe
preservar el orden y los spans del codigo, y debe rechazar caracteres
desconocidos y literales de intervalo invalidos.

## Palabras Clave

Los siguientes tokens son palabras clave reservadas:

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

Estas palabras clave no deben usarse donde se requiera un identificador.

## Identificadores

Un identificador:

- debe comenzar con una letra ASCII o `_`
- puede continuar con letras ASCII, digitos o `_`

Ejemplos:

- `trend`
- `_tmp1`
- `weekly_basis`

## Literales

### Literales Numericos

Los literales numericos se parsean como `f64`.

Formas aceptadas:

- `1`
- `14`
- `3.5`

Formas rechazadas:

- notacion exponencial como `1e6`
- formas con punto inicial como `.5`

Los numeros negativos se expresan mediante el `-` unario, no como un token
separado de literal con signo.

### Literales Booleanos

Los literales booleanos son:

- `true`
- `false`

### Literal De Valor Faltante

`na` es el literal de valor faltante.

### Literales String

Los literales string actualmente se aceptan solo donde la gramatica los permite
en declaraciones `source`.

Ellos:

- se delimitan con `"`
- pueden contener escapes basicos para `"`, `\\`, `\\n`, `\\r` y `\\t`
- no deben cruzar una nueva linea sin escape

## Comentarios

Solo se soportan comentarios de una linea:

```palmscript
// regimen de tendencia
let fast = ema(spot.close, 5)
```

Un token `/` aislado es el operador aritmetico de division. `//` inicia un
comentario de una sola linea.

## Separadores De Sentencias

Las sentencias se separan por:

- nuevas lineas
- punto y coma

Las nuevas lineas dentro de parentesis o corchetes no terminan una sentencia.

## Literales De Intervalo

Los literales de intervalo distinguen mayusculas y minusculas. El conjunto
aceptado se define en [Tabla De Intervalos](intervals.md).

Por ejemplo:

- `1w` es valido
- `1M` es valido
- `1W` es invalido

## Nota Sobre `optimize`

`optimize` ahora es una palabra reservada. Se usa como sufijo de metadata en declaraciones `input ... optimize(...)` y no puede reutilizarse como identificador ordinario.
