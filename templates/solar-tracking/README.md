# Solar tracking

Target: Stationeers `0.2.6403.27689`.

`controller` uses `d0` for the `StructureDaylightSensor` and `d1` for the
`StructureSolarPanel`; all three share the `data` power-and-data network.
`SolarAngle` is copied to the panel's writable `Horizontal` field.

Open `solar.icsim` in the visual simulator, select `controller`, and
choose **Debug**. Run `ic10 test solar.ictest` from this directory.
Orbital motion is not simulated, so tests inject sensor angles and verify
demand, tracking, and clearing.
