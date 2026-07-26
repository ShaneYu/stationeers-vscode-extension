# One-door airlock

Target: Stationeers `0.2.6403.27689`.

`controller` maps `d0` to the chamber `StructureGasSensor` and `d1` to the
`StructureGlassDoor`. Both use `data`; powered devices also use `power`. The
door opens below 10 kPa and closes when pressure returns.

Open `airlock.ic10sim.json`, select `controller`, and choose **Debug**. Run
`ic10 test airlock.ic10test.json`. Atmospheric flow is intentionally driven by
timeline changes to the sensor's read-only `Pressure`.
