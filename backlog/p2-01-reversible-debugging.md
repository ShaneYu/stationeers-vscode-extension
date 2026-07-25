# P2.01 — Reversible debugging, tracing, and analysis

## Goal

Exploit deterministic simulation to answer not only “what is the value?” but
“when, why, and by which IC did it change?”

## Trace model

Record compact events for:

- executed instruction and source location;
- register and stack writes;
- device field, slot, and memory reads/writes;
- network channel reads/writes;
- scheduler transitions, yield, sleep, wake, halt, and error;
- scenario test stimuli;
- breakpoint and assertion stops.

Events reference interned device, field, source, and expression IDs to avoid
repeating strings.

## State history

Use periodic snapshots plus reversible deltas:

- a configurable in-memory ring buffer;
- deterministic restore to any retained event;
- per-tick checkpoints for fast seeking;
- bounded memory with a visible retained-history duration;
- optional trace export/import for bug reports.

Do not clone the complete world after every instruction.

## Debugger features

Implement:

- step back;
- reverse continue;
- previous/next write to the selected value;
- previous error, breakpoint, yield, or tick;
- restart from a retained checkpoint;
- timeline filters by IC, device, network, event type, and source;
- value history charts for selected numeric values;
- side-by-side state diff between two stops.

## Coverage and profiling

Collect:

- source lines/instructions executed;
- branch outcomes;
- operations per tick and per IC;
- device/network read and write counts;
- maximum stack pointer;
- time spent at the operation-budget ceiling;
- values that oscillate or are repeatedly written without change.

Display coverage in editor gutters and profiling summaries in a native view or
webview only where standard VS Code surfaces are insufficient.

## Performance requirements

- Tracing is off or minimal during ordinary runs unless requested.
- Recording uses preallocated/batched structures on hot paths.
- History has explicit event and memory limits.
- The debugger exposes dropped/coalesced diagnostic events rather than hiding
  them.
- Benchmarks cover one, ten, and many-IC worlds with and without tracing.

## Acceptance criteria

- [ ] Step-back restores all CPU, world, network, scheduling, and random state.
- [ ] Replaying forward from a checkpoint produces the original state hash.
- [ ] Previous-write identifies the IC and source instruction responsible.
- [ ] History memory is bounded and configurable.
- [ ] Exported traces include tool/game-data version and redact absolute paths
      by default.
- [ ] Coverage and operation profiles are deterministic.
- [ ] Trace-enabled benchmarks have an agreed and documented overhead budget.
- [ ] DAP `stepBack` and `reverseContinue` work through standard VS Code UI.

## Dependencies

- [P1.03](p1-03-debugger-power-features.md) evaluator and mature DAP session
  control.
- [P1.02](p1-02-scenario-tests-and-cli.md) uses traces for failure provenance.

## Non-goals

- Unlimited recording.
- Reverse execution of a live game before the bridge can provide authoritative
  snapshots and events.

## Decisions

- State history uses checkpoints plus deltas, not full copies per instruction.
