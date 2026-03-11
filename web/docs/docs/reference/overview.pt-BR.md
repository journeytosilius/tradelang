# Visao Geral Da Referencia

Esta secao e a definicao normativa de PalmScript como documentada publicamente.

Se uma pagina de guia e uma pagina de referencia divergirem, a referencia e a
fonte autoritativa.

## O Que Esta Secao Define

- estrutura lexica
- gramatica
- regras de declaracoes e escopo
- tipos e valores
- semantica de series e indexacao
- semantica de avaliacao
- regras de intervalos e fontes
- contratos de builtins e indicadores
- semantica de saidas
- classes de diagnosticos

## Implementado Hoje

A superficie atual de PalmScript inclui:

- exatamente uma diretiva base `interval <...>` de nivel superior por script
- um ou mais aliases `source` nomeados por script executavel
- series qualificadas por fonte como `spot.close` ou `hl.1h.close`
- intervalos suplementares por meio de `use <alias> <interval>`
- declaracoes `fn` de nivel superior com corpo de expressao
- `let`, `const`, `input`, desestruturacao de tuplas, `export`, `trigger`, `entry` / `exit` de primeira classe e `order`
- indexacao de series apenas com literais, literais enum tipados `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>` e `exit_kind.<variant>`, e logica booleana deterministica de tres valores
- uma superficie estilo TA-Lib onde alguns nomes sao executaveis hoje e nomes reservados adicionais sao expostos por diagnosticos

## Limites Atuais

- `interval`, `source`, `use`, `fn`, `const`, `input`, `export`, `trigger`, `entry`, `exit` e `order` sao apenas de nivel superior
- identificadores de mercado soltos como `close` nao sao validos em scripts executaveis
- intervalos superiores exigem `use <alias> <interval>`
- apenas identificadores sao invocaveis
- literais string sao validos apenas dentro de declaracoes `source`
- indexacao de series exige um literal inteiro nao negativo
- resultados tuple-valued de builtins precisam ser desestruturados com `let (...) = ...` antes de qualquer outro uso

## Como Ler

- comece com [Estrutura Lexica](lexical-structure.md) e [Gramatica](grammar.md) para a sintaxe aceita
- use [Declaracoes e Escopo](declarations-and-scope.md) para regras de bindings e visibilidade
- use [Semantica De Avaliacao](evaluation-semantics.md) e [Intervalos e Fontes](intervals-and-sources.md) para o significado da linguagem
- use [Builtins](builtins.md), [Indicadores](indicators.md) e [Saidas](outputs.md) para o comportamento de callables e saidas
