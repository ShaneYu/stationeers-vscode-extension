# Pure Lua module tests

This example runs Lua 5.2 without Unity, a game process, or Stationeers host
APIs:

```text
cargo run -p ic10-runner -- test examples/lua-modules/pure-modules.stationeerstest.json
```

The test entry uses ordinary Lua `assert` calls and the sandboxed workspace
`require()` resolver. `execution.kind` is explicit so Lua programs attached to
simulated devices remain unsupported until the later shared-world changesets.
