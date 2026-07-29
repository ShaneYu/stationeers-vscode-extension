# Stateful Lua mock profile

P3-09D adds the world-free profile
`stationeerslua-0.9.5.0-lua5.2-stateful-mock-v1`. It is a deterministic Rust
service API for focused simulator fixtures; it is not full Lua-chip execution
and does not claim undocumented StationeersLua lifecycle semantics.

`LuaStatefulMock` owns four explicit services:

- `PersistedState` stores JSON key/value pairs in sorted-key order. Values
  survive both `reload()` and `power_cycle()`.
- `VirtualClock` starts at zero and advances only by finite, non-negative
  test-supplied seconds. Reload and power-cycle reset it to zero.
- `DeterministicRandom` uses a recorded seed and a fixed xorshift64* stream.
  Reload and power-cycle restart that stream from the seed.
- `Lifecycle` records reload and power-cycle counts.

The API is intentionally Rust-owned until Stationeers host function names and
return semantics have fixtures or sanitized game evidence. Calling
`LuaStatefulMock::unsupported` for `events`, `messaging`,
`libraryChipRequire`, or `http` returns a stable named error and performs no
side effect. Workspace `.lua` `require()` remains supported by the existing
pure-module runner; library-chip loading is a separate unsupported capability.

The profile is suitable for replay and unit tests, but it is not attached to
the existing IC10 scheduler or world-attached Lua slots. P3-09B's fail-closed
behavior for Lua chips and mixed worlds is unchanged.
