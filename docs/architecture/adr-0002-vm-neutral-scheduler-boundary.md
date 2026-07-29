# ADR-0002: VM-neutral scheduler boundary

- **Status:** Accepted for P3-09B
- **Date:** 2026-07-29
- **Decision owners:** Stationeers Toolkit maintainers

## Context

The simulator scheduler currently addresses `Cpu` values directly. That model
is correct for IC10 but cannot safely admit another runtime. It is also deeply
coupled to IC10 debugger state: registers, stack, program counters, source
lines, journal actors, checkpoints, and replay records.

World-attached Lua programs do not yet have evidence-backed Stationeers host
semantics. In particular, lifecycle behavior for `tick`, `yield`, `sleep`,
device access, persistence, events, messaging, and library chips belongs to
P3-09C. Treating an unsupported Lua chip as halted or silently omitting it
would allow a mixed world to produce plausible but incomplete results.

## Decision

Introduce an internal VM-neutral schedule in scenario device order. A schedule
slot identifies its language-specific adapter and runtime index; the existing
IC10 adapter continues to own `Cpu` execution. The scheduler cursor addresses
slots, while the public IC10 `Cpu`, `CpuState`, `StepEvent`, manual stepping,
journal, DAP thread, and replay contracts remain unchanged.

The existing quota-batched ordering is preserved:

1. the current slot runs while ready and below its language-specific budget;
2. waiting, halted, faulted, or quota-exhausted slots yield to the next slot;
3. earlier-slot world writes are immediately visible to later slots;
4. after all slots, behaviours receive `tick_end`;
5. the world tick advances and behaviours receive `tick_start`; and
6. operation counters reset and sleeping programs wake.

P3-09B represents world-attached Lua programs as explicit unavailable adapter
slots during validation, then rejects simulator construction with
`lua-runtime-unavailable`. This applies to Lua-only and mixed worlds before any
program source executes, structural world construction, or any world tick
occurs. The P3-09A `luaModule` runner remains a separate world-free path.

Snapshots retain the scheduler-slot cursor. Immutable slot-to-runtime mappings
remain outside snapshots. For all-IC10 scenarios, slot indices equal the
existing CPU indices; the existing conformance, state-hash, trace, DAP smoke,
and replay suites verify compatibility.

## Consequences

The shared scheduler now has a stable adapter seam for P3-09C without requiring
a disruptive DAP migration in P3-09B. Mixed worlds can no longer silently skip
Lua chips.

Executable Lua schedule slots, Lua checkpoint state, Lua source events,
language-neutral debugger records, and Stationeers host calls remain deferred
until their semantics are evidenced and implemented. The local Lua module
runner does not imply simulated Lua-chip support.
