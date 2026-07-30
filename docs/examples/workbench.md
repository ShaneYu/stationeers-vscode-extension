# Scenario Workbench

The Scenario Workbench is the shared example for the testing, simulation, and
debugging guides. It models a small mixed IC10/Lua vending system:

```text
IC10 requester → shared data network → Lua supplier → vending machine
       ↑                                                    ↓
       └──────────── delivery chute and valve ←────────────┘
```

## Open the example

The source folder is
[`examples/scenario-workbench/`](https://github.com/ShaneYu/stationeers-vscode-extension/tree/main/examples/scenario-workbench).
Open the repository root in VS Code, then open:

```text
examples/scenario-workbench/testing/workbench.icsim
examples/scenario-workbench/testing/workbench.ictest
```

The simulation file is the world layout. The test file contains a passing iron
request, a parameterized iron/gold request, initial values, a timeline event,
exact/eventual/always assertions, and final snapshots.

## Learn from a failure

The separate
`testing/failures/intentional-failure.ictest` fixture expects the
gold vendor's stock to be zero even though the simulation starts it at 50. Run
it when you want to practise reading a failure, opening the relevant state, or
capturing a debugging screenshot. It is intentionally excluded from the
passing example command described in the README.

## Related guides

- [Scenario testing](/guide/scenario-testing)
- [Simulation and debugging](/guide/simulation)
- [Debugging the workbench](/guide/debugging)
