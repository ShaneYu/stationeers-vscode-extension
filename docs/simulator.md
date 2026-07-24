# IC10 simulation and debugging

The extension can execute one or more IC10 programs against a shared,
source-controlled simulation environment. The simulator and debug adapter run
locally and do not require Stationeers.

## Create an environment

1. Open the IC10 program that should become the first controller.
2. Run **IC10: Create Simulation Environment**.
3. Save the suggested `simulation.ic10sim.json` file.
4. Add networks and devices in the visual environment editor.
5. Enable **Runs an IC10 program** on each IC housing and choose its program
   path.
6. Attach the housing and its devices to the same data cable, then assign the
   housing's `d0` through `d5` pins.

The editor only offers pin targets whose data-capable connection is on the
housing's data cable. Removing or changing that connection clears pins that
are no longer reachable. Program paths offer workspace IC10 files while still
allowing a manually entered or externally browsed path. Renaming a referenced
IC10 file or folder through VS Code updates simulation files automatically.

The scenario remains ordinary JSON and has schema validation when opened as
text. It is intended to be committed beside the programs it exercises.

The repository's `examples/multi-ic/ingot-supplier.ic10sim.json` scenario is a
small supplier/requester handshake with separate data networks and a shared
power cable.

## Start debugging

Select an IC housing in the visual environment editor and press its **Debug**
button or F5. With one housing, the button starts it directly; with several,
choose a housing from the adjacent selector first. From an `.ic10` editor, F5
finds the environments that reference that exact program and asks which
housing to focus when the program is used more than once.

A single debug session still represents the complete simulated world. The
selected housing is the initial paused thread, and every other housing appears
as another thread in the Call Stack view.

Normal source breakpoints work in every program referenced by the scenario.
When one IC reaches a breakpoint, every IC pauses so device and network state
remain consistent.

The standard debugger exposes these scopes:

- **Registers**: `r0` through `r15`, `ra`, and `sp`;
- **Stack**: all 512 cells with indexed paging;
- **CPU**: line, tick, run state, operation count, and error;
- **Pins**: `d0` through `d5` and `db`;
- **Devices**: runtime logic fields, inventory slots, and addressable memory
  for every scenario device;
- **Networks**: `Channel0` through `Channel7` for each cable network.

Leaf values in Registers, Stack, device fields, device slots, device memory,
and Networks can be edited while the simulation is paused. The **IC10 State**
debug view provides a denser editable register and stack grid.

Use the normal debugger Step command to execute one instruction on the selected
IC. Use **IC10: Step World Tick** or the debug toolbar's sync button to run
every eligible IC for one 0.5-second world tick.

Useful watch expressions include:

```text
r3
stack[42]
tick
device("requester").Setting
device("sorter").slot[0].Quantity
device("sorter").memory[3]
network("shared-power").Channel0
```

## Scenario concepts

### Networks and connections

Networks are independent objects. A device's numbered connection is attached
to a network by ID, so one device can participate in several networks. Cable
networks declare a `cableRole` of `data`, `power`, or `powerAndData`; the
environment editor only offers networks compatible with each device
connection. Chute, gas-pipe, and liquid-pipe connections are likewise limited
to their matching network kind. Cable networks own eight shared channels and
initialize them to `NaN` unless the scenario specifies another value.

References such as `db:1 Channel0` and `d0:0 Channel3` resolve the target
device's numbered connection and then access the attached cable network.

### Device fields

The editor obtains fields, slots, connection layouts, access modes, prefab
hashes, and images from the generated Stationpedia data. Values entered in a
scenario are initial/test-driver state, so read-only outputs can be seeded.
Instructions executed by an IC still obey the device's real read/write access.
Unsupported fields and slots are rejected instead of becoming invisible custom
state.

Inventory slots include an optional searchable item preset. Items are filtered
by the slot class and can be found by display name, prefab name, or hash.
Selecting one initializes reliable fields such as occupant/prefab hash,
occupied state, quantity, damage, known maximum quantity, and the numeric
`Class` and `SortingClass` values exported by the game.

Labeller names produce `NameHash` automatically. `PrefabHash`, `NameHash`, and
an automatically assigned `ReferenceId` are available at runtime.

Only prefabs with non-zero addressable memory show a **Device memory** editor.
The stored map is sparse: omitted addresses start at zero and only explicitly
initialized cells appear in the scenario.

### IC state

Each IC declaration selects a program and can initialize pins, registers, and
sparse stack cells. The visual editor uses unique row inputs for initial
registers and stack addresses, while the underlying file remains compact
sparse JSON. After running a setup-only program, use **Save stack** in the
**IC10 State** debug view to replace that housing's sparse initial stack with
its current non-zero runtime cells. The housing can then be switched to its
operational program without manually copying the setup data.

Special IEEE-754 values that JSON cannot represent directly use the strings
`"NaN"`, `"Infinity"`, `"-Infinity"`, and `"-0"`.

Stable device IDs, network IDs, and explicit `ReferenceId` values must be
unique. The visual editor reports duplicates before debugging and prevents new
duplicate values. Disabled ICs remain part of the shared device world but are
not offered as debug launch targets.

## Current fidelity

The simulator currently covers the CPU, deterministic tick budgets, sleep and
yield scheduling, arithmetic, selection, bitwise operations, absolute and
relative branches, stack and device memory, direct and pin device access,
slots, batch reads/writes, and connection-based cable channels.

Device fields are passive unless an IC writes them or the debugger edits them.
Active machine physics—such as vending exports, chute item travel,
atmospherics, recipes, and reagent mapping—does not yet evolve automatically.
Unsupported active-behaviour instructions stop the IC with an explicit error
instead of returning invented data.
