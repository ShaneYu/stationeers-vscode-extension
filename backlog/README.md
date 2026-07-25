# IC10 Toolkit backlog

This directory turns the product roadmap into an ordered set of implementation
plans. The objective is to evolve the extension from an editor and simulator
into a complete IC10 engineering workbench:

```text
author -> validate -> test -> debug -> build -> deploy -> observe
```

## How to use this backlog

- Work in the order shown below. The number after the priority breaks ties
  within that priority.
- Keep each document current as design decisions are made. Record material
  decisions in its **Decisions** section before implementation diverges from
  the plan.
- Check an acceptance criterion only after the implementation, automated
  tests, user documentation, and changelog entry are complete.
- Split an item into smaller pull requests when useful, but do not mark the
  item complete until its full acceptance criteria are met.
- New instruction semantics must follow the evidence policy in
  [P0.02](p0-02-simulator-conformance.md). We do not infer Stationeers
  behaviour from conventional MIPS behaviour.

## Ordered roadmap

| Order | Priority | Item | Outcome |
| ---: | :---: | --- | --- |
| 1 | P0 | [Language correctness](p0-01-language-correctness.md) ✅ | Typed operands, symbol analysis, unused-code hints, control-flow diagnostics, and standard LSP affordances |
| 2 | P0 | [Simulator conformance](p0-02-simulator-conformance.md) | Evidence-backed instruction semantics and a visible compatibility report |
| 3 | P1 | [Environment-aware intelligence](p1-01-environment-aware-intelligence.md) | Editing assistance understands the devices and networks in the selected simulation |
| 4 | P1 | [Scenario tests and CLI](p1-02-scenario-tests-and-cli.md) | Repeatable IC10 tests run in VS Code and CI |
| 5 | P1 | [Debugger power features](p1-03-debugger-power-features.md) | Conditional, hit-count, log, data, and exception breakpoints with richer evaluation |
| 6 | P1 | [Deployment build pipeline](p1-04-deployment-build-pipeline.md) | Readable source produces safe, compact code for the game without modifying the source |
| 7 | P2 | [Reversible debugging and analysis](p2-01-reversible-debugging.md) | Trace, change history, coverage, profiling, and step-back |
| 8 | P2 | [Device behaviour framework](p2-02-device-behaviour-framework.md) | Common machines evolve deterministically without attempting a full game reimplementation |
| 9 | P2 | [Topology and environment UX](p2-03-environment-topology-and-templates.md) | Large simulated stations are easy to construct and understand |
| 10 | P3 | [Live game bridge](p3-01-live-game-bridge.md) | Safe, conflict-aware IC discovery, pull, push, observation, and eventual live debugging |
| 11 | P4 | [Platform quality and ecosystem](p4-01-platform-quality-and-ecosystem.md) | Extension-host tests, accessibility, compatibility, migrations, and maintainable releases |

## Priority definitions

- **P0 — Trust:** correctness work that must precede broader automation.
- **P1 — Necessity:** closes the everyday author/test/debug/deploy loop.
- **P2 — Differentiation:** high-value capabilities that distinguish the
  toolkit from ordinary editors and emulators.
- **P3 — Strategic moat:** game integration requiring a separately shipped
  companion and careful security/performance engineering.
- **P4 — Scale and polish:** hardens the complete product and contributor
  experience.

## Definition of done

Every backlog item must include:

1. Protocol-neutral implementation in the appropriate Rust core or simulator
   crate where practical.
2. Unit tests for edge cases and transport/integration tests for its LSP, DAP,
   CLI, or VS Code surface.
3. User-facing documentation and an `Unreleased` changelog entry.
4. Explicit behaviour for unsupported, stale, or version-mismatched data.
5. No material regression in activation time, editing latency, simulation
   determinism, or debug responsiveness.
6. Schema or protocol migration notes when persisted files or bridge messages
   change.

## Product principles

- Prefer precise, subtle diagnostics to noisy guesses.
- Never claim simulator fidelity without reproducible evidence.
- Keep source files readable; generate deployment artefacts separately.
- Make headless workflows first-class so important systems can be tested in CI.
- Keep the live bridge local-only by default and require explicit authority for
  multiplayer writes.
- Use deltas, subscriptions, and bounded work instead of periodic full-world
  snapshots.
