# Bytecode Model

PalmScript compiles scripts to a deterministic bytecode program.

## Program Metadata

Compiled programs carry more than raw instructions. They also encode:

- slot layout
- output declarations
- external input declarations
- base interval
- declared supplemental intervals
- per-slot history requirements

That metadata is what lets the runtime bind market feeds, validate histories, and materialize outputs without reparsing source.

## Instruction Philosophy

Instructions are designed for:

- deterministic execution
- predictable stack effects
- low-overhead dispatch in the VM hot path

The VM uses direct opcode matching instead of dynamic dispatch or trait-object execution.
