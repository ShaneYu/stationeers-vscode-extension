# Temperature and pressure control

Target: Stationeers `0.2.6403.27689`.

`d0` is a `StructureGasSensor`, `d1` a `StructureWallHeater`, and `d2` a
`StructureActiveVent`. Control devices share `data`; the vent also attaches to
the `gas` pipe. Heating starts below 290 K and venting above 120 kPa.

Open `control.stationeerssim.json`, select `controller`, and choose **Debug**. Run
`ic10 test control.stationeerstest.json`. Thermodynamic feedback is not solved, so
tests inject `Temperature` and `Pressure`, including recovery/clear transitions.
