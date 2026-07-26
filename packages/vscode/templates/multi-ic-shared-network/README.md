# Multi-IC shared-network coordination

Target: Stationeers `0.2.6403.27689`.

The publisher's `d0` reads `published-value`; the subscriber's `d0` writes
`received-value`. Both are `StructureLogicMemory` devices. Both housings and
memories share the `shared` power-and-data network, with Channel0 as the bus.

Open `shared.ic10sim.json` and debug either IC to observe coordinated
scheduling. Run `ic10 test shared.ic10test.json`. Tests cover the default,
alternate, and cleared channel values; multiplayer latency/collisions are not.
