# P0.02 — Simulator conformance

## Goal

Make every simulator-fidelity claim evidence-backed, versioned, and
reproducible.

## Instruction evidence policy

An instruction may be implemented or changed only when at least one of these is
available:

1. generated Stationpedia data from a known Stationeers build; or
2. an official RocketWerkz source such as release notes or published
   documentation when the Stationpedia export is incomplete.

Conventional MIPS behaviour, community wikis, existing emulators, and other
mods can suggest test cases but are not authoritative sources for semantics.

Every instruction implementation must record:

- Stationeers build/game version;
- evidence source and extraction path;
- syntax and operand types;
- edge cases that the source defines;
- observed-game evidence for behaviour the documentation leaves ambiguous.

Ambiguous behaviour remains explicitly unsupported until it can be measured.

## Status of the July 2026 instructions

The generated Stationpedia data for build `0.2.6403.27689` contains:

- `rol r? a(r?|num) b(r?|num)`;
- `ror r? a(r?|num) b(r?|num)`;
- `clamp r? a(r?|num) min(r?|num) max(r?|num)`;
- `sgn r? a(r?|num)`.

The simulator already contains implementations for all four. They should be
classified as **implemented, awaiting conformance audit** until the tests below
confirm the documented and ambiguous edge cases. No new semantics should be
invented during that audit.

Required audit cases include:

- rotation width and masking of negative, fractional, very large, and negative
  rotation counts;
- signed/unsigned conversion and values outside exact IEEE-754 integer range;
- `clamp` with equal or reversed bounds, NaN, infinities, and negative zero;
- `sgn` with positive/negative values, both zero signs, NaN, and infinities.

Where Stationpedia does not specify an edge case, capture the real game's
result before locking the simulator behaviour.

## Conformance framework

Create a machine-readable instruction matrix generated from
`instructions.json`. Each instruction records:

- `supported`, `partial`, `unsupported`, or `unverified`;
- source game version;
- simulator test fixture IDs;
- known deviations;
- active device behaviour dependencies.

Add golden execution fixtures for all 154 generated instructions. Cover:

- direct and indirect registers;
- aliases and defines;
- normal values, fractions, negative values, signed zero, NaN, and infinities;
- minimum/maximum addresses and out-of-range errors;
- absolute and relative branch boundaries;
- empty and populated batch selections;
- readable, writable, and forbidden device fields;
- sleep/yield tick scheduling;
- deterministic random behaviour.

Use generated parameterized tests where instruction families share semantics,
but retain named regression tests for past bugs.

## Differential harness

Design a small capture format for results obtained from the real game:

```json
{
  "gameVersion": "0.2.6403.27689",
  "program": "examples/conformance/rol.ic10",
  "inputs": {},
  "observed": {
    "registers": { "r0": 1 },
    "error": null
  }
}
```

The initial capture process may be manual. The live bridge should eventually
run and collect these fixtures automatically. Captures must never include
proprietary game assemblies or data beyond the minimal observed values needed
for the test.

## Compatibility reporting

Generate a user-visible report from the matrix:

- game data version bundled with the extension;
- complete and partial instruction counts;
- known unsupported instructions (`lr` and `rmap` at the time this plan was
  written);
- active device behaviours that are modelled or passive;
- link to known deviations.

Show a concise warning when a scenario targets a game version newer than the
bundled data. Do not block editing or execution solely because of a version
mismatch.

## Acceptance criteria

- [x] Every generated instruction appears in the conformance matrix.
- [x] Every `supported` instruction has at least one execution fixture.
- [ ] Shared edge-case suites cover IEEE-754 and integer-conversion behaviour.
- [ ] `rol`, `ror`, `clamp`, and `sgn` have documented evidence and real-game
      captures for unspecified edge cases before being marked verified.
- [x] Unknown semantics fail explicitly instead of returning invented data.
- [x] The matrix and user-facing compatibility report are generated in CI.
- [x] A game-data update cannot silently remove or add an instruction without a
      conformance review failure.
- [x] Simulator regression tests include multi-IC scheduling and shared-world
      ordering.

## Implementation status

Partially complete. The generated matrix covers all 154 instructions from
Stationpedia build `0.2.6403.27689`, and CI rejects stale output, unknown
overrides, unsupported statuses, missing fixture IDs, and `supported` entries
without fixtures. The compatibility report, non-blocking newer-version warning,
explicit unsupported-operation failures, IEEE-754 storage fixture, and stable
multi-IC shared-world scheduling regression are implemented.

The remaining two criteria are evidence-blocked rather than implementation
blocked. `examples/conformance/` contains a minimal capture schema, workflow,
and programs for `rol`, `ror`, `clamp`, and `sgn`. Their undocumented
integer-conversion, masking, NaN, infinity, reversed-bound, and signed-zero
behaviour must be captured from the real game before those results can be
encoded as golden expectations. Until then the four instructions remain
`unverified`, and conversion cases are deliberately not asserted from Rust or
conventional MIPS behaviour.

The documented Stationpedia subset now has the named
`stationpedia-operators` golden fixture: positive/negative/zero/NaN `sgn`,
the published `clamp` examples and NaN propagation, and exactly representable
64-bit wrapping cases for `rol`/`ror`. This improves regression coverage
without claiming that the still-uncaptured conversion and edge semantics are
verified.

## Non-goals

- Claiming complete Stationeers physics fidelity.
- Treating another emulator as an authoritative oracle.
- Blocking releases until every active machine behaviour is implemented.

## Decisions

- Stationpedia or another official RocketWerkz source is required before
  implementing instruction semantics.
- Real-game observations resolve ambiguities but are version-specific.
