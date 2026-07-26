# Filtration

Target: Stationeers `0.2.6403.27689`.

`d0` maps to a `StructureGasSensor`; `d1` maps to `StructureFiltration`.
The filtration unit connects to `data`, `power`, and the `input`, `output`, and
`waste` gas networks. It runs while `RatioPollutant` exceeds 1%.

Open `filtration.ic10sim.json`, select `controller`, and choose **Debug**. Run
`ic10 test filtration.ic10test.json`. Gas mixing and cartridge consumption are
not simulated; the timeline supplies contamination and clearing.
