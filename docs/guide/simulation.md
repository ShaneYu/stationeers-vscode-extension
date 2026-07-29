# Simulation and debugging

The simulator is a deterministic shared-world model for IC10 programs. It is
especially useful for debugging logic, network coordination, device access,
and repeatable tests before loading code into a live save.

## Start a session

1. Run **IC10: Create Simulation Environment** or open a template.
2. Add IC housings, devices, networks, and program paths.
3. Configure numbered data connections, pins, registers, and initial stack.
4. Select an IC and press F5, or use the environment editor's **Debug** action.

Each IC is a debug thread in the same world. Normal stepping advances one
instruction; **IC10: Step World Tick** advances every eligible IC through one
coordinated game tick.

## Inspect state

While paused, inspect and edit registers, stack cells, device fields, slots,
addressable memory, and cable-network channels. Watches, conditions, logpoints,
hover evaluation, and scenario assertions use the same expression grammar.

Use the **IC10 State** view for a compact register and stack editor. A saved
stack can become the housing's sparse initial state for a later program.

## Know the boundary

The simulator includes a narrow, test-oriented behaviour pack. It does not
automatically reproduce every recipe, atmospheric system, or active game
physics. See the [simulator guide](../simulator.md) and
[compatibility report](../simulator-compatibility.md) for details.
