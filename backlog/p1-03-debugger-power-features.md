# P1.03 — Debugger power features

## Goal

Fill the standard DAP gaps that matter for IC10 and make runtime failures easy
to understand without replacing VS Code's native debugger UI.

## Breakpoints

Implement:

- conditional source breakpoints;
- hit-count breakpoints;
- logpoints with expression interpolation;
- data breakpoints for registers, stack cells, device fields, slots, memory,
  and network channels;
- function/label breakpoints by symbolic label;
- exception filters for compile errors, invalid instructions/operands, missing
  devices, access violations, invalid addresses, and explicit `hcf`;
- verified/unverified breakpoint updates when source or scenarios change.

Conditions and log expressions use the shared evaluator. Invalid conditions
must produce an unverified breakpoint with an actionable message.

## Evaluation and variables

Replace the current collection of special-case watch parsers with a real small
expression grammar supporting:

- numbers, special IEEE-754 values, unary and arithmetic operators;
- comparisons, boolean operators, and parentheses;
- registers and resolved aliases/defines;
- `stack[address]`;
- device fields, slots, and memory;
- network channels;
- `tick`, current line, run state, and operation count;
- helpers such as `isnan`, `isfinite`, `abs`, and `changed`.

Use the same grammar for tests, breakpoint conditions, logpoints, hover
evaluation, and the Debug Console.

Variables should:

- mark writable values accurately;
- provide presentation hints and stable evaluate names;
- highlight values changed since the previous stop;
- expose friendly prefab, field, slot, and enum names;
- page large memory/stack collections without expensive eager materialisation.

## Session control

Implement:

- restart with the original launch/test configuration;
- hot reload while paused when source still compiles;
- run to cursor through DAP goto targets;
- explicit instruction-step and world-tick-step commands;
- meaningful `exceptionInfo`;
- clean distinction between program termination and debugger disconnect;
- optional single-thread continue, clearly warning that it departs from normal
  coordinated world scheduling.

Hot reload must preserve or reset CPU/world state only through an explicit user
choice.

## Inline debugging

Provide optional inline values for:

- referenced registers and aliases;
- direct device fields read or written on the current line;
- branch conditions and targets;
- current tick and operation budget.

Keep inline values off by default or sparse enough that short IC10 programs do
not become visually noisy.

## Acceptance criteria

- [x] Conditional, hit-count, log, label, data, and exception breakpoints work
      in single- and multi-IC scenarios.
- [x] Breakpoint expressions and test expressions use one evaluator.
- [x] Runtime errors implement `exceptionInfo` with category and details.
- [x] Restart and hot reload have explicit, tested state semantics.
- [x] Changed values are visually distinguishable after a stop.
- [x] Debug Console evaluation supports aliases and world objects.
- [x] Invalid breakpoint expressions are unverified rather than ignored.
- [x] DAP transport tests cover every advertised capability.
- [x] Existing instruction and coordinated tick stepping remain deterministic.

## Dependencies

- [P0.01](p0-01-language-correctness.md) resolved aliases and symbols.
- [P1.02](p1-02-scenario-tests-and-cli.md) shares the evaluator and debug-test
  launch model.

## Non-goals

- Pretending IC10 has conventional stack frames where none exist.
- Advertising a DAP capability before its requests and edge cases are tested.

## Decisions

- Prefer standard DAP requests and UI over additional custom webview controls.
- Full-world coordinated execution remains the default.
