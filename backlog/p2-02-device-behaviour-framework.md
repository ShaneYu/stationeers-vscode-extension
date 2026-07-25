# P2.02 — Device behaviour framework

## Goal

Model the active behaviour needed for useful automation tests without
attempting to reproduce the complete Stationeers simulation.

## Framework

Create deterministic device behaviours selected by prefab or capability:

- lifecycle hooks for tick start/end and field/slot/memory writes;
- scheduled events with stable ordering;
- access only through a narrow world API;
- serializable private state for snapshots and reverse debugging;
- explicit behaviour version;
- declared fields, slots, networks, and other dependencies;
- deterministic random input supplied by the simulator;
- no wall clock, threads, file access, or hidden global state.

Unknown devices remain passive and are labelled as such.

## Initial behaviour packs

Prioritise common IC10 workflows:

1. switches, buttons, dials, sensors, displays, lights, and doors;
2. batteries and simple power-state transitions;
3. sorters, stackers, vending machines, chutes, and inventory movement;
4. active/passive vents and simplified pressure exchange;
5. basic production-machine state transitions;
6. selected rocket/trading devices after concrete scenarios require them.

Each model should document its abstraction and known deviations. Simplified
physics must not be presented as exact.

## Scripted fallback

Scenario-test timeline events remain the universal fallback. Users should be
able to model an unsupported machine as an external test driver without
writing Rust:

- set a field at a tick;
- react to a field write;
- move a slot item;
- publish a network-channel value;
- schedule a later response.

A constrained declarative rule format is preferable to arbitrary executable
scripts for determinism and security.

## Extension model

Consider external behaviour packs only after the built-in interface is stable.
If supported, packs require:

- versioned manifests;
- strict compatibility ranges;
- sandboxed/declarative execution;
- deterministic conformance tests;
- clear trust prompts.

Do not load arbitrary native libraries into the extension or simulator.

## Acceptance criteria

- [x] The first deterministic pack covers the standard vending machine,
      digital chute valve, and chute outlet workflow used by
      `examples/multi-ic`.
- [x] The pack has an end-to-end simulator test and documented deviations.
- [ ] Behaviour state participates in snapshots, reset, tests, and step-back.
- [ ] Event ordering is deterministic across operating systems.
- [ ] Passive devices are visibly distinguished from modelled devices.
- [ ] Every built-in behaviour has fixtures and a known-deviations document.
- [ ] Scripted stimuli can stand in for unsupported active behaviour.
- [ ] Behaviour failures identify the device and model version.
- [ ] No behaviour depends on wall-clock timing.
- [ ] Performance benchmarks cover many passive and active devices.

## Dependencies

- [P0.02](p0-02-simulator-conformance.md) fidelity reporting.
- [P1.02](p1-02-scenario-tests-and-cli.md) stimuli and assertions.
- [P2.01](p2-01-reversible-debugging.md) serializable history.

## Non-goals

- Complete atmospherics, thermodynamics, recipes, character needs, or world
  simulation.
- Silent approximation of complex game behaviour.

## Decisions

- Common deterministic abstractions and scripted stimuli take priority over
  full game-physics fidelity.
- The initial vending/chute slice is stateless beyond ordinary world
  fields/slots. The chute outlet's slot 0 is an explicit last-exported-item
  observation latch for scenario assertions.
