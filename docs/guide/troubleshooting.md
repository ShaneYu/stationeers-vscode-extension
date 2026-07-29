# Troubleshooting

## The language server did not start

Open **View: Output**, select **Stationeers Toolkit**, and run **IC10: Restart
Language Server**. If `ic10.server.path` is configured, confirm it points to a
native executable for the host running the extension.

## Language features are missing

Confirm the file ends in `.ic10` and the status bar language mode is **IC10**.
If the file is referenced by multiple environments, use **IC10: Select
Simulation Context** to choose one or return to document-only analysis.

## The simulator differs from the game

The simulator is deterministic and intentionally scoped. Check the
[compatibility report](../simulator-compatibility.md), reduce the scenario to
the supported device behaviours, and report a reproducible case with the
scenario file attached.

## Collect a protocol trace

Set `ic10.trace.server` to `messages` or `verbose`, reproduce the issue, and
include only the relevant output in a bug report. Review it first because
protocol traces can contain source text.
