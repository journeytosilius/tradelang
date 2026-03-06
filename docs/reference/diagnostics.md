# Diagnostics and Error Classes

PalmScript surfaces several classes of user-visible errors.

## Compile Errors

Compile-time diagnostics include:

- lexer and parser errors
- invalid `interval` / `use` declarations
- type errors
- invalid identifiers
- illegal function usage

These surface through:

- `palmscript check`
- `palmscript run csv` before execution
- `palmscript dump-bytecode`
- `palmscript-lsp`
- the VS Code extension

## CSV Mode Data Preparation Errors

The data-preparation layer can fail before runtime with errors such as:

- `CannotInferInputInterval`
- `MissingBaseIntervalDeclaration`
- `RawIntervalTooCoarse`
- `UnsupportedRollupPath`
- `InsufficientDataForInterval`
- `IncompleteRollupBucket`
- `UnsortedInputBars`
- `DuplicateInputBarTime`

These happen after successful compilation but before VM execution.

## Runtime Errors

Runtime errors include:

- feed compatibility problems
- history-cap violations
- execution-limit violations

The runtime fails deterministically rather than silently degrading semantics.
