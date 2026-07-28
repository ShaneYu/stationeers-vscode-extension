# Batch production

Target: Stationeers `0.2.6403.27689`.

`d0` is a `StructureLogicMemory` order queue and `d1` is a real
`StructureAutolathe`. The autolathe connects to `input`/`output` chutes plus
`data` and `power`. A positive `Setting` drives `Activate`.

Open `batch.stationeerssim.json`, select `controller`, and choose **Debug**. Run
`ic10 test batch.stationeerstest.json`. The test covers idle, demand, and clearing;
reagents, recipes, and physical production completion are not modeled.
