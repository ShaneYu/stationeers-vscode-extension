# Local Lua simulator profile

P3-09A changeset A selects the pure-module profile
`stationeerslua-0.9.5.0-lua5.2-pure-module-v1`. This is a narrow local
execution target, not a claim of StationeersLua VM compatibility and not a
claim that Stationeers host APIs are implemented.

The embedded runtime selection is `mlua` **0.12.0**, pinned exactly, with
`lua52` and `vendored` enabled. It supplies Lua 5.2 semantics and packages the
Lua implementation with the Rust build. The selection and trade-offs are
recorded in [ADR-0001](../architecture/adr-0001-pure-lua-module-runtime.md).

## Supported in changeset A

The runner executes pure workspace `.lua` modules only. A custom
Rust-owned `require()` resolver searches explicitly configured,
test-relative module roots in deterministic order and caches successful
loads. It does not use arbitrary filesystem searchers. Pure language features
and the safe standard-library subset—base operations, `string`, `table`,
`math`, and Lua 5.2 `bit32`—are the compatibility surface.

The runner contract includes deterministic module ordering, structured
compile/runtime/module/limit failures, source locations, bounded captured
output, filtering, and nonzero failure status. Limits cover instructions,
recursion/call depth, memory where enforceable, output bytes, module count,
aggregate source bytes, and wall time as a last-resort safety cutoff.
Fixture configuration can lower but cannot raise the host ceilings:
10,000,000 instructions, 30 seconds, 64 MiB memory, 1 MiB captured output,
256 modules, 4 MiB aggregate source, and 512 calls. Absolute paths, parent
traversal, and canonical symlink/junction escapes outside the test directory
are rejected.

Select this mode explicitly in a neutral scenario test:

```json
{
  "name": "arithmetic module",
  "focusProgram": "arithmetic-tests",
  "execution": {
    "kind": "luaModule",
    "profile": "stationeerslua-0.9.5.0-lua5.2-pure-module-v1",
    "moduleRoots": ["."]
  }
}
```

The selected Lua program is the test entry. It uses ordinary Lua `assert`
calls and may import pure modules through the controlled resolver. Normal test
name filtering, human/JSON/JUnit output, source locations, and CI exit status
come from the existing runner.

## Core host mock profile

The opt-in `LuaModuleRunner::run_with_host` path adds the narrow core host
surface recorded in the machine-readable manifest. `device.get(name)` and
`device.getReferenceId(name)` use scenario device ids and decimal ReferenceIds;
the name is the only documented lookup form. `ic.get(pin, field)` and
`ic.set(pin, field, value)` resolve configured scenario pins and use the same
validated field access as IC10. A device proxy exposes `get`, `set`, `slot(i)`
(`get`/`set`), `memory(i)`, and `setMemory(i, value)`. Missing devices, pins,
fields, slots, memory addresses, and read/write violations return stable
`[lua-*]` errors. `print` and `log` are captured in the run result in call
order. This profile is world-attached but scheduler-free: lifecycle callbacks,
`tick`, `yield`, and `sleep` remain unsupported.

## Explicitly unsupported

Outside the opt-in core host mock, Stationeers host APIs are not enabled. In
particular, the following remain unsupported: batch/network I/O, `tick`,
`yield`, `sleep`, scheduler or real-time access,
persistence, coroutines, events, callbacks, messaging, peer discovery,
Stationeers enums,
hashes, generated game libraries, library-chip `require()`, HTTP, and random
services. Full Lua-chip execution, mixed IC10/Lua worlds, and local Lua
debugging are also outside changeset A.

P3-09B adds the VM-neutral scheduler boundary needed by later world adapters,
but does not enable Lua-chip execution. An otherwise structurally valid
scenario with any world-attached Lua program—including a mixed IC10/Lua
world—fails with
`lua-runtime-unavailable` before an IC10 instruction, Lua source, or world tick
can execute. This prevents incomplete mixed worlds from appearing successful.

The sandbox denies `io`, `os`, `debug`, unrestricted `load`, `dofile`,
`loadfile`, `pcall`, `xpcall`, package native loaders, filesystem, process, environment,
dynamic-library, native-code, and network access. The `library.lua` editor
annotations are editor metadata, not verified runtime API evidence.

The machine-readable profile is checked in at
[`lua-simulator-profile.json`](lua-simulator-profile.json). This profile is
separate from the StationeersLua remote Pull/Compare/Push integration provided
by P3-08.
