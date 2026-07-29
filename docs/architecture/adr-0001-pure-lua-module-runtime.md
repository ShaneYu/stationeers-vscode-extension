# ADR-0001: Sandboxed pure Lua module runtime

- **Status:** Accepted for P3-09A changeset A
- **Date:** 2026-07-29
- **Decision owners:** Stationeers IC10 Toolkit maintainers

## Context

P3-09A needs a deterministic local execution target for pure Lua modules.
The target language is Lua 5.2, while the StationeersLua host contract is not
yet sufficiently evidenced to execute full Lua chip programs locally. The
runtime must therefore be embedded and sandboxed, without implying that it is
the StationeersLua VM or that the shared simulated world is implemented.

## Decision

Use `mlua` **0.12.0**, pinned exactly, with only the `lua52` and `vendored`
features. This selects PUC Lua 5.2 semantics and builds the Lua implementation
as part of the Rust package, avoiding a user-installed Lua shared library.
The dependency and lockfile change are implementation work for P3-09A; this
ADR records the selection and its compatibility boundary.

Changeset A supports only deterministic workspace Lua modules. A Rust-owned
resolver loads `.lua` files from explicitly configured test-relative
module roots. It rejects absolute paths, traversal, root escape, duplicate
ambiguous modules, native loaders, and game/library-chip module loading. Module
resolution order, loaded-module reporting, and errors are stable.

The initial safe standard-library allowlist is limited to pure facilities:
base language operations, `string`, `table`, `math`, and Lua 5.2 `bit32`.
`coroutine` remains unavailable until coroutine lifecycle limits are covered.
`io`, `os`, `debug`, unrestricted `load`, `dofile`,
`loadfile`, `package` searchers, `package.loadlib`, and all host capabilities
are unavailable. `pcall` and `xpcall` are unavailable in this first profile so
hook-enforced instruction and deadline failures cannot be caught and retried.
No userdata exposes filesystem, process, environment,
dynamic-library, network, or native-code access.

Execution is bounded by instruction count, recursion/call depth, memory where
the selected runtime can enforce it, captured output bytes, module count,
aggregate module source bytes, and wall time as a last-resort safety guard.
Fixtures may lower but cannot raise immutable host ceilings of 10,000,000
instructions, 30 seconds, 64 MiB memory, 1 MiB output, 256 modules, 4 MiB
aggregate source, and 512 calls.
Each test uses a fresh state or an explicitly reset state. Results and errors
include stable module/source information and do not expose host paths outside
the workspace.

## Compatibility boundary

The selected profile is
`stationeerslua-0.9.5.0-lua5.2-pure-module-v1`. It is a language/runtime
selection, not evidence that StationeersLua host APIs are implemented. The
following remain unsupported until backed by documentation, focused fixtures,
and, where practical, sanitized real-game probes:

- `device.*`, `ic.*`, device I/O, references, slots, memory, and networks;
- `tick`, `yield`, `sleep`, real-time, clock, and scheduler integration;
- persistence, power-cycle state, events, callbacks, messaging, and peers;
- StationeersLua game/library-chip modules and mixed IC10/Lua execution;
- Stationeers host enums, hashes, generated game libraries, and HTTP;
- randomness unless later defined by a deterministic profile.

The editor `library.lua` annotations are not runtime evidence and do not alter
this boundary. Remote StationeersLua Pull/Compare/Push remains the separate
P3-08 live integration.

## Consequences

Pure-module tests run reproducibly without Unity or a running game, with
explicit failures for unsupported APIs. The runtime does not
run Lua programs attached to simulated devices, model Stationeers lifecycle
semantics, or provide local Lua debugging. Those belong to later P3-09
changesets and require the compatibility evidence described above.

The vendored build removes an end-user native Lua installation requirement but
requires a C toolchain for each supported Rust target and license notices for
`mlua`, `mlua-sys`, `lua-src`, and PUC Lua to remain represented in release
dependency reporting.
