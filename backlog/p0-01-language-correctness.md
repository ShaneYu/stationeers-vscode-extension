# P0.01 — Language correctness

## Goal

Make the language server trustworthy enough that a clean Problems panel is a
meaningful statement about an IC10 program, while keeping suggestions subtle
and avoiding false-positive noise.

## Current state

The parser is tolerant and fast, but semantic checks mostly cover instruction
existence, operand count, literal macros, duplicate symbols, missing symbolic
branch targets, and the program line limit. Symbol navigation is
document-local. Rename is already implemented.

## Scope

### Typed operands

Introduce protocol-neutral operand classifications in `ic10-core` and validate
every operand against the generated instruction signature:

- writable register;
- numeric value or register;
- device reference, including indirect references and connection suffixes;
- logic type and slot logic type;
- batch mode;
- label/branch target;
- identifier introduced by `define` or `alias`;
- hash, packed string, integer, binary, hexadecimal, and decimal literal;
- slot and memory address where statically knowable.

Diagnostics must point to the individual operand rather than the complete
instruction. Incomplete input should not produce cascades of speculative
errors.

### Symbol resolution

Build a resolved symbol model for labels, defines, and aliases:

- distinguish declarations from references;
- follow alias and define chains;
- detect cycles;
- reject invalid declaration targets;
- provide find-references and document highlights;
- keep rename based on resolved identity rather than matching token text;
- expose workspace symbols for locating declarations across independent IC10
  files without pretending that IC10 has cross-file imports.

### Unused and dead code suggestions

Add hint-level diagnostics for:

- labels with no symbolic branch or jump reference;
- aliases with no reference outside their declaration;
- defines with no reference outside their declaration;
- unreachable instructions and labels;
- values written to a register and definitely overwritten before being read,
  when this can be proven without path-sensitive speculation.

Unused diagnostics should use the LSP `Unnecessary` diagnostic tag so editors
render them as a subdued fade or subtle squiggle. They must not be errors.

Rules for controlling noise:

- Ignore appearances in comments and quoted strings.
- Do not count a declaration as its own use.
- Treat dynamic numeric/register jumps conservatively; they may suppress
  unreachable-code conclusions for affected regions.
- Allow a leading underscore to intentionally suppress an unused-symbol hint.
- Add a setting with `off`, `hint`, and `warning`, defaulting to `hint`.
- Offer safe code actions to remove an unused declaration or unreachable
  instruction without silently renumbering numeric branches.

### Control-flow and value analysis

Build a compact control-flow graph covering absolute and relative jumps,
conditional branches, branch-and-link instructions, `jr`, `yield`, `sleep`,
`hcf`, and fall-through.

Use it to detect:

- unreachable code;
- branches with missing or invalid targets;
- constant branch conditions;
- loops that definitely consume the full per-tick operation budget without
  yielding or sleeping;
- provable stack underflow or overflow;
- obvious use-before-write register reads;
- possible return-address clobbering in nested call patterns;
- constant divide/modulo-by-zero and statically invalid addresses.

All path-sensitive results should be hints or warnings unless failure is
certain.

### Standard LSP affordances

Implement in this order:

1. references and document highlights;
2. code actions and quick fixes;
3. semantic tokens for instructions, declaration kinds, references, registers,
   device references, logic fields, hashes, and deprecated instructions;
4. folding ranges for label-delimited regions;
5. optional inlay hints for resolved aliases, hashes, and line destinations;
6. a conservative formatter with configurable indentation and spacing.

TextMate remains the fallback highlighter when the server is unavailable.

### Program budget

Show the exact limits supported by the current game data:

- physical/program lines used;
- instructions executed per tick where statically estimable;
- total encoded program size and per-line limits only after those limits are
  available from generated game data or another official source.

Do not encode community folklore as a hard diagnostic. Unknown limits should
be labelled unknown rather than guessed.

## Implementation sequence

1. Define operand and resolved-symbol data structures in `ic10-core`.
2. Convert existing diagnostics to use the resolved model.
3. Add typed validation with golden fixtures for every operand kind.
4. Add reference collection and unused-symbol hints.
5. Add the control-flow graph and conservative data-flow passes.
6. Expose references, highlights, code actions, semantic tokens, folding, and
   inlay hints through `ic10-lsp`.
7. Add formatter and budget UI after semantics are stable.

## Acceptance criteria

- [ ] Every generated instruction operand kind has a validator and fixtures.
- [ ] Invalid operand diagnostics select only the offending token.
- [ ] Labels, aliases, and defines support references, highlights, definition,
      and identity-safe rename.
- [ ] Unused declarations render as hint-level unnecessary code by default.
- [ ] `_name` declarations suppress unused hints.
- [ ] Unreachable-code analysis is conservative around dynamic jumps.
- [ ] Quick fixes never change numeric branch behaviour silently.
- [ ] Formatting is idempotent and preserves program meaning.
- [ ] UTF-16 position and incomplete-line tests cover every new LSP feature.
- [ ] LSP transport smoke tests exercise the new advertised capabilities.

## Non-goals

- Cross-file linking or includes that do not exist in IC10.
- A parser rewrite solely to provide incremental parsing; programs are small
  enough for the existing full-document model.
- Aggressive style warnings that make valid compact IC10 unpleasant to edit.

## Decisions

- Unused declarations are hints tagged `Unnecessary`, not warnings or errors.
- A leading underscore is the initial intentional-unused convention.
