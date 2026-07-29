# ADR-0003: Deterministic stateful Lua mock services

- **Status:** Accepted for P3-09D
- **Date:** 2026-07-29

## Decision

Add a narrow, world-free `LuaStatefulMock` service layer to `ic10-sim`. It
models only persisted JSON key/value state, a virtual clock, and seeded
randomness. Reload and power-cycle are explicit operations: both retain
persisted state, reset the clock and random stream, and increment their
respective lifecycle counters.

The service layer is separate from `LuaModuleRunner` and the P3-09B scheduler
boundary. This avoids changing pure-module behavior or implying that these
helpers are a complete StationeersLua chip adapter.

## Evidence boundary

The checked-in profile and pure-module fixtures provide evidence for workspace
`.lua` `require()` only. They do not provide Stationeers host semantics for
events, messaging, library-chip loading, or HTTP. Those capabilities therefore
return `lua-mock-unsupported-api` with the named capability and mock profile;
no guessed callback, peer, network, or live request behavior is implemented.

Focused Rust fixtures cover persistence across both lifecycle operations,
virtual-clock validation, seeded random replay, and unsupported capability
errors. Additional host APIs require new documentation and fixtures before
being enabled.
