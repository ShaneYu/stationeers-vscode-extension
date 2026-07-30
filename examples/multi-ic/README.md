# Multi-IC vending request

This example exercises three IC10 programs and a shared simulated world:

- `supplier-setup.ic10` creates the supplier's stack database as alternating
  vendor-name and item-prefab hashes.
- `item-supplier.ic10` reads requests from `Channel0` on the shared power
  cable, finds the corresponding vendor name, and activates only that named
  vending machine with `sbn`.
- `item-requester.ic10` publishes an item hash, waits for the stack at its
  digital chute valve, opens the valve for one tick, then closes it.

`ingot-supplier.icsim` contains named Iron and Gold vending machines,
separate supplier/requester data networks, a shared power cable, and two chute
segments separated by the requester's valve.

## Preparing the supplier stack

In game, load `supplier-setup.ic10` into the supplier housing and let it run
once. In the extension, pause the simulator and use **Save stack** in the
**IC10 State** view. Then switch that housing to `item-supplier.ic10`.

The committed scenario already contains the resulting stack and `sp` value so
the three cases in `ingot-supplier.ictest` are reproducible:

1. request iron and prove gold remains stored;
2. request gold and prove iron remains stored;
3. request an unknown hash and prove neither machine vends.

## Simulator abstraction

The simulator models the exact active behaviour needed by this example:

- a standard vending machine exports its selected or first occupied stack when
  `Activate` is non-zero;
- a digital chute valve holds one stack and passes it while `Open` is non-zero;
- a chute outlet increments `ExportCount` and keeps the last exported item in
  slot 0 as an assertion-friendly observation latch.

This is deterministic automation-test behaviour, not complete Stationeers
machine or chute physics. Empty vending machines, chute congestion beyond the
single valve slot, stack splitting, power loss, and loose world items are not
modelled yet.
