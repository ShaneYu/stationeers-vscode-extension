# IC10 editing

The language server understands incomplete source, so editor assistance keeps
working while a program is being written.

## Supported intelligence

- Completion for instructions, operands, registers, devices, macros, constants,
  enums, labels, and hashes.
- Hover documentation for instructions, devices, fields, slots, registers,
  symbols, prefab names, and numeric hashes.
- Signature help, definitions, references, highlights, rename, symbols, and
  folding for labels, defines, and aliases.
- Typed operand validation, unused/dead-code hints, semantic tokens, inlay
  hints, formatting, and safe quick fixes.

## Environment-aware editing

When an open program is referenced by a simulation environment, the selected IC
housing adds device-aware completion, hover information, and diagnostics. If a
program is used by multiple housings, the toolkit asks you to select the active
context instead of guessing.

Use **IC10: Select Simulation Context** to change that selection or return to
document-only analysis.

## Useful source conventions

```ic10
define Sensor HASH("StructureVolumePump")
alias pump d0

main:
  l r0 pump Activate
  beq r0 1 running
  j main
running:
  yield
  j main
```

The editor treats quoted `HASH` and `STR` literals correctly when removing
comments. Relative branch destinations are adjusted when the
**IC10: Remove All Comments** command removes physical lines.
