# Documentacao Do PalmScript

PalmScript e uma linguagem para estrategias financeiras de series temporais.
Este site foca na linguagem em si: sintaxe, semantica, builtins e exemplos de
codigo.

## Mapa Da Documentacao

- `Aprenda` ensina a linguagem com exemplos curtos e fluxos executaveis.
- `Referencia` define a sintaxe aceita e a semantica da linguagem.

## Comece Aqui

- Se voce e novo em PalmScript: [Visao Geral De Aprenda](learn/overview.md)
- Se quer seu primeiro script executavel: [Inicio Rapido](learn/quickstart.md)
- Se precisa da definicao formal da linguagem: [Visao Geral Da Referencia](reference/overview.md)
- Se esta procurando contratos de indicadores: [Visao Geral De Indicadores](reference/indicators.md)

A demo hospedada do IDE mantem uma interface minima: um editor, uma shell em
React e TypeScript com Monaco, seletores de data sobre o historico disponivel
de BTCUSDT, diagnosticos em tempo real, snippets de autocompletado para
callables, paineis de backtest e tabelas de trades e orders sem uma coluna de
JSON bruto. A barra superior mantem o logo do PalmScript dentro do cabecalho,
junto com um seletor de tema claro/escuro. O modo escuro usa uma shell
inspirada no VS Code com um tema estilo Dracula no editor.
A entrada hospedada e `/app/`. [https://palmscript.dev/app](https://palmscript.dev/app) redireciona para ela.

## Destaques Da Linguagem

PalmScript suporta:

- uma declaracao base obrigatoria `interval <...>`
- declaracoes `source` com nome para dados de mercado
- series qualificadas por fonte como `spot.close` e `perp.1h.close`
- declaracoes opcionais `use <alias> <interval>` para intervalos suplementares
- literais, aritmetica, comparacoes, operadores unarios, `and` e `or`
- `let`, `const`, `input`, desestruturacao de tuplas, `export` e `trigger`
- `if / else if / else`
- indexacao de series com deslocamentos literais
- indicadores, helpers de sinais, helpers de memoria de eventos e builtins estilo TA-Lib
- declaracoes de estrategia de primeira classe como `entry`, `exit`, `order`, `protect` e `target`

## Como Ler A Documentacao

Comece com `Aprenda` se voce vai escrever PalmScript pela primeira vez.

Use `Referencia` quando precisar de regras exatas para sintaxe, semantica,
builtins, intervalos ou saidas.

O titulo do cabecalho permanece como `PalmScript` durante o scroll e volta para
a pagina principal do site.
