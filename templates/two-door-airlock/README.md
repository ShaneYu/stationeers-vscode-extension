# Two-door airlock

Target: Stationeers `0.2.6403.27689`.

`d0` is the chamber `StructureGasSensor`, `d1` the exterior
`StructureGlassDoor`, and `d2` the interior door. Devices share `data` and
powered devices share `power`. Complementary `Open` commands enforce exclusion.

Open `airlock.ic10sim.json`, select `controller`, and choose **Debug**. Run
`ic10 test airlock.ic10test.json`. Pressure transitions are deterministic
stimuli; real door control and the interlock are executed by the simulator.
