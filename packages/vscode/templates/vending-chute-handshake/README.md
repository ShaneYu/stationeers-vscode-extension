# Vending and chute handshake

Target: Stationeers `0.2.6403.27689`.

Two ICs coordinate across the power cable: the requester publishes an item
hash, the supplier resolves its configured name/hash pair and activates the
matching vendor, and the requester pulses a digital chute valve on arrival.
The requester's `d0` is `delivery-valve`; its `db:1` and the supplier's `db:1`
share `shared-power`. The vendor uses `supplier-data` plus `supply-chute`; the
valve bridges `supply-chute` to `delivered-chute`.

The supplier stack is preconfigured in the scenario to keep this template
directly runnable. It assumes a stocked vendor and full-stack vending. No
sorter is present; the name is deliberately explicit about the implemented flow.

Open `vending.ic10sim.json`, select either IC, and choose **Debug**. Run
`ic10 test vending.ic10test.json`; positive delivery and unknown-request safety
are both covered.
