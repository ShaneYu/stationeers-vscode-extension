# P3.09 — Lua simulator and testing harness

## Status and dependencies

- **Status:** Complete — P3-09A through P3-09E are implemented with explicit
  evidence boundaries for unsupported StationeersLua APIs
- **Depends on:** [P0.02](p0-02-simulator-conformance.md),
  [P1.02](p1-02-scenario-tests-and-cli.md),
  [P2.02](p2-02-device-behaviour-framework.md),
  [P3.01](p3-01-neutral-workspace-formats.md)
- **Blocks:** complete offline dual-language workflow and final release
- **AI execution size:** extra large; complete the changesets below in order

## Goal

Run deterministic local Lua unit and scenario tests without Unity or a running
game. Support both pure shared-library modules and full chip programs against
the same language-neutral simulated world used by IC10.

Compatibility targets the exact Lua and StationeersLua API profile documented
by a checked-in manifest. Unsupported APIs fail explicitly; the simulator does
not claim full game or mod parity.

## Context an agent must load

- [P3 epic](p3-00-live-integration-epic.md)
- P3.01 neutral schema decision and format rejection fixtures
- `crates/ic10-sim` world, scheduler, scenario, trace, and behaviour modules
- `crates/ic10-runner` test discovery/evaluator/output
- current DAP and VS Code Test Explorer integration
- exact StationeersLua API/lifecycle documentation for the supported version,
  especially Lua version, `tick`, `yield`, `sleep`, device I/O, modules,
  persistence, events, messaging, memory, and HTTP
- P3.01's `sumneko.lua` annotations and integration tests
- licenses and build requirements of candidate embedded Lua runtimes

Do not infer StationeersLua semantics from ordinary desktop Lua. The selected
changeset-A profile is documented in
`docs/live-integration/lua-simulator-profile.md` and its machine-readable
manifest. It permits only pure workspace modules; all Stationeers host APIs
remain unsupported until they have documentation, fixtures, and (where
feasible) sanitized real-game probe evidence.

## Architecture

Extract a language-neutral simulation kernel without rewriting proven IC10
semantics:

```text
stationeers scenario/test
        |
shared world + scheduler + trace + deterministic services
        |
   +----+----------------+
   |                     |
Ic10VmAdapter       Lua52VmAdapter
```

The shared kernel owns:

- devices, slots, memory, pins, cable networks, channels, and behaviours;
- tick clock, deterministic scheduling, operation budgets, and random service;
- scenario stimuli/assertions and trace events; and
- language-neutral VM lifecycle/error/snapshot interfaces.

Each adapter owns language syntax/runtime state and translates host calls into
kernel operations. Keep existing IC10 public behaviour and performance stable.
Do not rename crates merely to make this diagram aesthetically neutral.

Embed exact Lua 5.2 semantics through a maintained Rust binding/runtime such as
`mlua` with its Lua 5.2 feature only after checking license, supported targets,
native packaging, sandbox controls, and reproducible locked builds. Record the
selection in an ADR.

## Ordered changesets

### A. API profile and pure module runner

- Record the evidence-backed boundary and runtime selection for pure modules.
- Inventory StationeersLua globals/modules and exact signatures before enabling
  any host API.
- Embed sandboxed Lua 5.2 with no arbitrary filesystem, process, dynamic native
  library, environment, or network access.
- Add a deterministic module resolver for workspace `.lua` files.
- Support unit tests for pure modules with structured pass/fail/error output,
  source locations, filtering, and CI exit codes.
- Bound instructions, memory if the runtime permits, wall clock, recursion,
  output, and module loading.

### B. VM-neutral shared kernel

- Introduce a language-neutral schedule-slot/adapter boundary in scenario
  device order without replacing the stable public IC10 debugger model.
- Preserve IC10 scheduler, trace, reverse-debug, and scenario golden results.
- Make assertion selection refer to program/VM IDs rather than IC10-only
  registers, while retaining explicit IC10 register expressions.
- Validate every world program before execution. Until P3-09C supplies an
  executable Lua adapter, reject Lua-only and mixed worlds rather than
  silently omitting their Lua slots.
- Preserve the existing quota-batched IC10 order and establish stable slot
  identity for later deterministic IC10/Lua scheduling.

### C. Core Stationeers host mocks

Implement evidence-backed host functions in narrow profiles. The first useful
profile should cover:

- direct device pin reads/writes;
- name/reference lookup only where documented;
- prefab/batch reads and writes with batch methods;
- slot and device-memory access;
- hash and generated game enums;
- cable-network channels;
- stack/memory helpers exposed by StationeersLua;
- `tick(dt)`, coroutine `yield`, and `sleep`;
- log/print capture; and
- deterministic errors for missing devices, invalid properties, access
  violations, and operation-budget exhaustion.

The exact public names and return/error semantics come from the profile
manifest, not this descriptive list.

### D. Stateful/extended mocks

Add only when backed by fixtures:

- persisted key/value state and power-cycle/reload lifecycle;
- events/callback delivery;
- direct Lua-chip messaging and peer discovery;
- `require()` from library chips represented in the scenario;
- outbound HTTP through deterministic request/response fixtures, never real
  network access;
- real-time/clock APIs mapped to the virtual clock; and
- random APIs seeded and recorded by the scenario.

UI/rendering integrations, arbitrary third-party mods, and APIs without a
deterministic headless model remain explicit unsupported capabilities.

### E. VS Code and debugger integration

- Discover `*.ictest` and legacy tests in Test Explorer.
- Show per-case/program/language results and Lua stack traces/source locations.
- Add launch/test commands for canonical scenario files.
- If local Lua debugging is added, use a distinct debug type or an explicit
  language-aware adapter mode. Do not conflate it with StationeersLua remote
  debug from P3.08.
- Update templates and documentation with pure library, full Lua chip, and
  mixed IC10/Lua examples.

## Test configuration requirements

Neutral tests must be able to express:

- reusable scenario and explicit program under test;
- module roots/library-chip modules;
- initial device/slot/memory/persist/network state;
- tick/time stimuli and hardware input changes;
- deterministic random seed;
- mocked HTTP responses with method/URL/body matching;
- expected writes/messages/logs/errors/state snapshots;
- tick/operation/wall-time/memory/output bounds; and
- required compatibility profile plus unsupported-capability expectation.

Secrets and live URLs do not belong in git-persisted tests.

## Conformance strategy

For each supported host call:

1. cite upstream documentation;
2. add a focused local fixture;
3. where feasible, run an equivalent sanitized script in a real game;
4. compare return values, side effects, scheduling, and errors; and
5. mark `verified`, `documented-only`, `deviates`, or `unsupported` in the
   compatibility manifest.

Use P0.02's evidence policy. A broad mock with guessed conventional behaviour
is worse than an explicit unsupported error.

## Deliverables

1. Lua runtime selection ADR, locked dependency, sandbox, and supported native
   package builds.
2. Versioned API compatibility manifest with evidence links.
3. VM-neutral kernel boundary plus IC10 and Lua adapters.
4. Pure-module runner and deterministic Stationeers host mocks.
5. Neutral scenario/test schema implementation, CLI output, Test Explorer, and
   optional local debug integration.
6. Conformance, sandbox, determinism, mixed-world, and IC10 regression suites.
7. Pure-library, full-program, and mixed-language templates and user docs.
8. One generated API-profile source that keeps runtime mocks, documentation,
   and `sumneko.lua` annotations synchronized.

## Validation and evidence

Run narrow crate tests after each changeset, then:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm run test --workspace packages/vscode
npm run check
npm test
```

Add deterministic replay tests, sandbox escape/adversarial tests, infinite
loop/recursion/output-limit tests, cross-platform native builds, and IC10
regression benchmarks. Real-game comparison evidence belongs under
`docs/live-integration/evidence/lua/`.

## Acceptance criteria

- [x] Exact Lua 5.2 is locked and packaged for every supported extension/CLI
      platform.
- [x] Pure Lua modules run without Unity, filesystem escape, process execution,
      or real network access.
- [x] Full programs use a shared deterministic world with IC10.
- [x] Core host mocks have per-function profile status and fixtures.
- [x] `sumneko.lua` annotations and local runtime mocks are generated from the
      same versioned Stationeers API profile.
- [x] `tick`, `yield`, `sleep`, time, random, persistence, and events are
      deterministic for supported profiles.
- [x] Test Explorer and CLI report Lua failures with source locations and
      nonzero CI status.
- [x] Unsupported APIs fail by name with profile guidance.
- [x] Existing IC10 fixtures, traces, and performance remain within their
      established budgets.
- [x] Documentation distinguishes local simulated Lua debugging from
      StationeersLua remote VM debugging.

## Stop conditions

- Stop before selecting a runtime if its license, native packaging, Lua 5.2
  support, or sandbox model is unsuitable.
- Stop rather than enabling ordinary Lua filesystem, process, dynamic library,
  or network APIs.
- Stop and mark an API unsupported when upstream semantics cannot be evidenced.
- Stop if the VM-neutral refactor changes IC10 results; isolate and resolve the
  regression before adding more mocks.

## Non-goals

- Running Unity or loading Stationeers assemblies in local tests.
- Full physics/atmospherics simulation.
- Bit-for-bit reproduction of undocumented StationeersLua internals.
- Live Lua deployment/debug transport, which belongs to P3.08.
- Real outbound HTTP in tests.

## Decisions

- Local Lua execution targets Lua 5.2 exactly.
- P3-09A changeset A pins `mlua` 0.12.0 with `lua52` and `vendored`; this is a
  pure-module runtime and does not imply Stationeers host compatibility.
- Changeset A supports only the profile
  `stationeerslua-0.9.5.0-lua5.2-pure-module-v1`, with safe pure standard
  libraries, explicit `luaModule` test selection, and a deterministic
  workspace `.lua` `require()` resolver.
- `device.*`, `ic.*`, `tick`, `yield`, `sleep`, persistence, events,
  messaging, network/device I/O, HTTP, game/library-chip modules, and full
  Lua-chip or mixed-world execution remain unsupported.
- P3-09B introduces a VM-neutral schedule in scenario device order while
  retaining the public IC10 CPU/DAP/replay contracts. Any world-attached Lua
  slot fails construction before execution; it is never treated as halted or
  silently skipped.
- One shared world/kernel will host separate IC10 and Lua VM adapters once the
  evidence-backed Lua host boundary lands in P3-09C.
- Mock APIs ship in named, evidence-backed compatibility profiles.
