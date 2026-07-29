# Mixed IC10 and Lua vending request

This example puts two cooperating programs in one folder:

- `supplier.lua` is a Lua chip program. It requires `supplier_logic.lua`, reads
  `Channel0` from the base unit with `ic.read`, and activates the matching
  vending machine.
- `requester.ic10` is an IC10 program. It writes an item hash, waits for the
  delivered stack, and opens the digital valve for one tick.

The folder also contains focused tests for the Lua supplier decision module
and the IC10 requester, plus a mixed-language scenario test that runs both
programs in the same simulated world.

The runnable sources stay at the example root; simulation and test fixtures
live under `testing/`. The runner treats a directly nested `testing` folder as
the example workspace, so its `..` module/program paths remain bounded and
validated.

Run the tests with:

```text
cargo run -p ic10-runner -- test examples/mixed-ic-lua
```

The request is carried on `shared-data` as `Channel0`, so the example shows
the same `d0:0`/`Channel0` addressing model in both languages. The simulator
also resolves the module with the same sandboxed `require()` behavior used by
the Lua unit test.
