# Environment format

Simulation environments use `*.stationeerssim.json` and are designed to be
source-controlled. They describe IC housings, program paths, devices, pins,
numbered connections, cable and power networks, registers, stack values, and
initial world state.

The visual editor is the safest way to create and update them. Advanced users
can inspect the JSON directly; validation prevents invalid form state from
overwriting a valid file.

See the [simulator guide](../simulator.md) for the model and
[workspace formats](../live-integration/workspace-formats.md) for path and
multi-root rules.
