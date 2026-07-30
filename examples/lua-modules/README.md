# Pure Lua module tests

This example runs Lua 5.2 without Unity, a game process, or Stationeers host
APIs:

```text
cargo run -p ic10-runner -- test examples/lua-modules/pure-modules.ictest
```

The test entry uses ordinary Lua `assert` calls and the sandboxed workspace
`require()` resolver. World-attached Lua programs use the same resolver; see
`examples/mixed-ic-lua` for a Lua supplier requiring shared program logic.
