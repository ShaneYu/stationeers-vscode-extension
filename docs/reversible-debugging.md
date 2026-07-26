# Reversible debugging and trace analysis

IC10 debug sessions can record deterministic execution history when enabled. VS Code's
standard **Step Back** and **Reverse Continue** controls restore CPU registers,
stack, device and slot state, device memory, networks, scheduler position, tick,
sleep state, and each IC's random stream.

History is not a complete world clone per instruction. The adapter stores a
mutable-state checkpoint every 10,000 events and compact replay records and write
deltas between checkpoints. Replay verifies a stable state hash at every event;
a mismatch stops navigation instead of silently showing an incorrect state.

## Configuration

The following settings provide explicit bounds:

- `ic10.debug.history.enabled`: disables all history/profiling hot-path work.
- `ic10.debug.history.events`: maximum retained events (default 20,000).
- `ic10.debug.history.checkpointInterval`: events between checkpoints
  (default 10,000).
- `ic10.debug.history.memoryMiB`: approximate retained-history ceiling
  (default 64 MiB).

The same values can be overridden per `launch.json` configuration with
`enableHistory`, `historyEvents`, `checkpointInterval`, and
`historyMemoryMiB`. The IC10 State view
shows retained events, ticks, the configured limits, and the number dropped.
Its timeline requests only the newest page; `ic10/getTrace` also accepts
`offset`, `limit`, `tail`, and `summaryOnly` for bounded consumers.

## Timeline and analysis

The **IC10 State** debug view includes a filtered timeline. Filter by stop/event
type or select a register, stack cell, device field/slot/memory cell, or network
channel. Previous and Next navigate writes or events and the value history shows
the retained numeric changes. **Compare** produces a side-by-side-friendly JSON
state delta between two event numbers.

Run **IC10: Show Trace, Coverage, and Profile** for the complete deterministic
summary and coverage highlighting on visible IC10 editors. It includes executed
lines, branch outcomes, per-IC and per-tick operation counts, device/network
access counts, maximum stack pointers, operation-budget ceiling ticks,
oscillation candidates, and redundant writes. Data-breakpoint expressions use
the same canonical target syntax as the timeline.

Run **IC10: Export Debug Trace** to create an `*.ic10trace.json` bug-report
artifact. Exported traces contain the trace schema, toolkit version, bundled
game-data version, coverage/profile data, dropped-event diagnostics, and event
records. Absolute source paths are replaced with stable, collision-free source
IDs by default. The
adapter's `ic10/importTrace` request validates and loads exported analysis
without executing it.

## Performance budget

History is off when `enableHistory` is false. When enabled, typed reads and
attempted writes are recorded at simulator mutation points into bounded event
storage; full mutable-state copies occur
only at the checkpoint interval. The agreed interactive-debugging budget is:

- no-history runtime: within 5% of an ordinary simulator run;
- trace runtime target: no more than 2x an ordinary simulator run (up to 3x
  only with a documented product justification);
- retained memory: event ring plus at most
  `ceil(historyEvents / checkpointInterval) + 1` mutable checkpoints.

Before a release, benchmark 1, 10, and 50 identical deterministic IC workloads
for at least 10,000 operations in both modes. Record median wall time, peak
resident memory, retained/dropped events, and the final state hash. Both modes
must produce the same hash and coverage/profile results must match across three
runs. Regressions beyond the budget block release.

The benchmark executes 100,000 scheduled instructions for 1, 10, and 50 IC
worlds, takes the median of three runs, and verifies matching final state
hashes. Run it with:

`cargo test -p ic10-dap benchmark_trace_overhead_for_one_ten_and_many_ic_worlds --release -- --ignored --nocapture`

The accepted full-trace budget is 3x because reversible debugging deliberately
retains every typed read and attempted write—including unchanged writes—plus
periodic complete mutable-state checkpoints. Records remain raw and interned on
the execution path; target names, formatted numbers, coverage, profiling, and
oscillation analysis are produced only for paged trace requests and exports.

Do not publish benchmark numbers unless the benchmark exercises the same
no-history and history-enabled paths used by a shipped DAP session, enforces
the configured event and memory limits, and records the requested wall-time,
resident-memory, retained/dropped-event, and final-hash evidence. Until that
release benchmark is recorded, the performance acceptance criterion remains
open.
