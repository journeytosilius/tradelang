# Compiler Pipeline

PalmScript maintains strict layer boundaries:

```text
source text
-> tokens
-> AST
-> semantic analysis
-> bytecode
-> VM execution
```

The parser emits AST only. Bytecode generation and runtime behavior happen later in dedicated layers.

## Main Stages

1. lexer tokenizes source
2. parser builds AST
3. semantic analysis validates types, scopes, intervals, outputs, and function use
4. compiler assigns slots, history requirements, update masks, outputs, and instructions
5. runtime and VM execute compiled bytecode over bounded history

## Why This Matters

The CLI, language server, and VS Code extension all depend on these same stages. Tooling stays thin because the language rules live in the library rather than in duplicated wrappers.
