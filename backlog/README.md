# Stationeers Toolkit backlog

This directory turns the product roadmap into an ordered set of implementation
plans. The objective is to evolve the extension from an editor and simulator
into a complete IC10 and Lua engineering workbench:

```text
author -> validate -> test -> debug -> build -> deploy -> observe
```

## How to use this backlog

- Work in dependency order. The roadmap order breaks ties only when two items
  are otherwise ready.
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

## AI execution contract

This backlog is written for autonomous coding agents. An agent implementing an
item must:

1. Read this file, the complete target item, every listed dependency, and any
   repository instruction file before changing code.
2. Check the worktree and preserve unrelated user changes. Work on one backlog
   item at a time unless the target item explicitly defines independent
   changesets.
3. Treat checked-in code, generated game data, recorded game probes, and linked
   upstream contracts as evidence. Never invent Stationeers types, event names,
   RPCs, prefab construction steps, or StationeersLua endpoints.
4. Implement the smallest vertical slice that satisfies the item. Do not
   opportunistically start a blocked successor.
5. Add tests with the implementation and run the narrowest relevant checks
   before the repository-wide checks. Use the commands in the item as a
   starting point, but verify them against the current manifests.
6. Keep schemas, fixtures, protocol examples, user documentation, architecture
   documentation, and the `Unreleased` changelog synchronized with behaviour.
7. Record material discoveries in the item's **Decisions** section or in the
   durable evidence/ADR requested by that item. Do not leave important findings
   only in an agent transcript.
8. Stop rather than guess when a listed stop condition is reached. Report the
   exact missing evidence, the probes already attempted, and the smallest
   decision or external action needed to continue.
9. Finish with a completion packet containing changed files, user-visible
   behaviour, validation commands and results, manual-game evidence, remaining
   uncertainties, and the next unblocked backlog item.

Effort is expressed as bounded deliverables and dependency gates rather than
calendar estimates. A task that cannot be verified without a running game is
not complete merely because its code compiles.

## Ordered roadmap

| Order | Priority | Item | Outcome |
| ---: | :---: | --- | --- |
| 1 | P0 | [Language correctness](p0-01-language-correctness.md) ✅ | Typed operands, symbol analysis, unused-code hints, control-flow diagnostics, and standard LSP affordances |
| 2 | P0 | [Simulator conformance](p0-02-simulator-conformance.md) | Evidence-backed instruction semantics and a visible compatibility report |
| 3 | P1 | [Environment-aware intelligence](p1-01-environment-aware-intelligence.md) ✅ | Editing assistance understands the devices and networks in the selected simulation |
| 4 | P1 | [Scenario tests and CLI](p1-02-scenario-tests-and-cli.md) | Repeatable IC10 tests run in VS Code and CI |
| 5 | P1 | [Debugger power features](p1-03-debugger-power-features.md) ✅ | Conditional, hit-count, log, data, and exception breakpoints with richer evaluation |
| 6 | P1 | [Deployment build pipeline](p1-04-deployment-build-pipeline.md) | Readable source produces safe, compact code for the game without modifying the source |
| 7 | P2 | [Reversible debugging and analysis](p2-01-reversible-debugging.md) | Trace, change history, coverage, profiling, and step-back |
| 8 | P2 | [Device behaviour framework](p2-02-device-behaviour-framework.md) | Common machines evolve deterministically without attempting a full game reimplementation |
| 9 | P2 | [Topology and environment UX](p2-03-environment-topology-and-templates.md) | Large simulated stations are easy to construct and understand |
| 10 | P3 | [Live game and Lua integration](p3-00-live-integration-epic.md) | RemoteNetwork-scoped discovery, safe IC10 deployment, optional StationeersLua debugging, and local Lua testing |
| 11 | P4 | [Platform quality and ecosystem](p4-01-platform-quality-and-ecosystem.md) | Extension-host tests, accessibility, compatibility, migrations, and maintainable releases |

## Current execution position (2026-07-29)

The live IC10 and Lua source workflows now have working vertical slices:

- RemoteNetwork discovery, named in-memory chip tabs, IC10 pull, conditional
  push, manual-save push, stale-target/conflict rejection, and read-only
  compare are implemented.
- Merge and force-push are intentionally deferred. Compare is inspection-only;
  recovery from a conflict is Pull/Compare, then edit and retry.
- P3.08 now correlates globally discovered Lua chips with StationeersLua's
  current editor/wireless scope, shows per-chip accessibility, and supports
  Pull, read-only Compare, and explicitly best-effort Push. Live probes against
  StationeersLua `0.9.5.0` validated wireless `mode=chip` Lua reads and writes.
  Exact-editor `mode=editor_then_chip` synchronization is accepted as an
  upstream StationeersLua multiplayer/editor-detection limitation and will be
  revisited after the author's fix. Atomic conflict preconditions remain
  unavailable.
- The next unblocked backlog slice is P3.09 changeset A: the evidence-backed
  StationeersLua API profile and sandboxed pure-module runner. P3.08 Lua
  debugging, multiplayer relay, build/export mappings, and final release
  hardening remain later work.

The detailed state and decisions are recorded in [P3.04](p3-04-bridge-protocol-readonly.md),
[P3.05](p3-05-vscode-live-network-explorer.md), [P3.06](p3-06-conflict-safe-ic10-sync.md),
and [P3.08](p3-08-stationeers-lua-integration.md).

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

1. Protocol- and editor-neutral implementation in the appropriate shared core
   where practical.
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
- Keep IC10 authoring, simulation, and testing fully usable when the optional
  StationeersLua mod is absent.
- Use the `sumneko.lua` extension for general Lua 5.2 language intelligence;
  this extension owns the Stationeers-specific annotations, workflows, tests,
  live integration, and debugger UX.
- Do not require or delegate to the StationeersLua VS Code extension. Integrate
  directly with the game mod's documented public services.
- Treat `RemoteNetwork` labels as player-authored discovery scopes, not durable
  authority or global entity identity.
