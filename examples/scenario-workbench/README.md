# Scenario Workbench

This is the shared teaching example for the Stationeers Toolkit documentation.
It is deliberately small enough to understand, but includes both IC10 and Lua
programs, a reusable simulation, passing tests, parameterized tests, and an
intentionally failing test for learning how failures look.

## What is in this folder?

| File | Purpose |
| --- | --- |
| `requester.ic10` | IC10 program that waits for button presses, requests an item, and opens a delivery valve. |
| `supplier.lua` | Lua chip program that chooses a vending machine from the request. |
| `supplier_logic.lua` | Pure Lua module used by the Lua chip. |
| `testing/workbench.stationeerssim.json` | Reusable devices, networks, slots, and program paths. |
| `testing/workbench.stationeerstest.json` | Passing, parameterized scenario cases demonstrating the guide concepts. |
| `testing/failures/intentional-failure.stationeerstest.json` | Deliberately failing case for Test Explorer and debugger screenshots. |

## The story

The operator presses the iron, gold, or steel button. The IC10 requester writes
the selected item hash to the shared data network. The Lua supplier reads that
request, activates the matching vending machine, and clears the channel. The
requested item travels through the chute to the delivery outlet. The IC10
requester notices the item, opens the digital valve for one tick, records
completion in `r2`, waits for the button to be released, and returns to its
idle state. Steel has no vendor, so it demonstrates a safe unavailable request.

This gives the guides one consistent world to refer to:

- **Simulation** — inspect the three request buttons, two IC housings, vending
  machines, chute networks, slots, and shared data channel.
- **Initial state** — start with an iron or gold request and controlled slot
  contents.
- **Timeline** — inject an external valve reset while the scenario runs.
- **Assertions** — check exact values, eventual delivery, and invariants.
- **Parameters** — run the same request flow for iron and gold.
- **Snapshots** — record the final outlet and vendor state.
- **Debugging** — put a breakpoint in `requester.ic10` or inspect the Lua
  supplier's shared-world effects.
- **Failures** — open the separate fixture under `testing/failures/` to see an
  assertion failure without changing the passing examples.

## Run it

Open this folder in VS Code and open `testing/workbench.stationeerstest.json`
for the visual scenario editor. The checked-in simulation file is ready to
open; use **IC10: Create Simulation Environment** only when you want to create
a new environment.

Run the passing fixture with the bundled runner:

```text
ic10 test testing/workbench.stationeerstest.json
```

To see an intentional failure:

```text
ic10 test testing/failures/intentional-failure.stationeerstest.json
```

That failure is expected and should not be used as the success command in CI.
