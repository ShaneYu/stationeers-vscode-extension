# Deterministic device behaviours

The simulator distinguishes modelled devices from passive
Stationpedia-backed devices. Unsupported devices retain their fields, slots,
memory, and connections, but do not evolve on their own. The environment
runtime inspector labels modelled devices with `model@version` and labels the
fallback as `passive`.

The runtime-owned machine-readable catalogue is
[`behaviour-catalog.json`](behaviour-catalog.json). A simulator integration
test deserializes that file, compares it for exact value equality with
`ic10_sim::behaviour_catalog()`, and resolves a real device for every selector.
Extension code can therefore consume the JSON without maintaining a second
unverified model list.

## Lifecycle and ordering

One world tick uses this stable order:

1. scenario/debugger stimuli for the current tick;
2. ICs in scenario order;
3. queued field-write notifications in attempted-write order;
4. behaviour tick-end hooks in scenario device order;
5. increment the world tick;
6. scheduled actions for the new tick, ordered by tick, phase, device ordinal,
   and insertion sequence;
7. behaviour tick-start hooks in scenario device order.

The runtime uses ordered collections and simulator ticks only. Behaviours
cannot access wall-clock time, threads, files, or global mutable state.

## Model and fixture matrix

All fixture links below are executable Rust integration tests in
[`simulation.rs`](../crates/ic10-sim/tests/simulation.rs).

| Kind | Selector | Model | Version | Declared dependencies | Executable fixtures |
| --- | --- | --- | ---: | --- | --- |
| Vending machine | exact `StructureVendingMachine` | `builtin.material-handling` | 1 | fields, slots, chute | `named_vending_request_crosses_a_digital_chute_valve` (success); `vending_model_handles_empty_congested_and_unconnected_outputs` |
| Digital chute valve | prefix `StructureChuteDigitalValve` | `builtin.material-handling` | 1 | fields, slots, chute | `named_vending_request_crosses_a_digital_chute_valve` (success); `digital_valve_model_handles_empty_congested_and_unconnected_outputs` |
| Chute outlet | exact `StructureChuteOutlet` | `builtin.material-handling` | 1 | fields, slots, chute | `named_vending_request_crosses_a_digital_chute_valve` (receives); `chute_outlet_model_fixtures_cover_success_empty_congested_and_unconnected` |
| Passive fallback | any unmatched prefab | `passive` | 1 | none | `behaviour_descriptors_distinguish_modelled_and_passive_devices`; `passive_model_fixtures_cover_success_empty_congested_and_unconnected` |

Cross-model fixtures also cover:

- stable scheduled ordering, attempted unchanged writes, journaling, restore,
  and replay in `scheduled_behaviour_events_are_stable_journalled_and_reversible`;
- real private activation/transfer counters crossing a checkpoint, reset, and
  deterministic replay in
  `stateful_model_counters_restore_and_replay_across_a_behaviour_tick`;
- device/model/version failure provenance in
  `behaviour_failures_include_device_model_and_version`;
- runtime/catalogue agreement in
  `checked_in_behaviour_catalog_matches_runtime_descriptors`.

## Known deviations

| Kind | Implemented abstraction | Known deviations from Stationeers |
| --- | --- | --- |
| Vending machine | A non-zero `Activate` moves one occupied slot to the first connected digital-valve input, increments `ExportCount`, and resets `Activate`. `DispenseSlot` is honoured when it identifies an occupied slot. | One activation moves one complete stack. There is no stack splitting, loose-item travel, multi-item queue, power check, animation, or vending delay. An empty, disconnected, or congested output performs no transfer and still resets `Activate`. |
| Digital chute valve | When `Open` is non-zero, its transit slot moves atomically to the connected chute outlet if that outlet is empty. | One transit slot is modelled. There is no spatial chute travel, pressure, item collision, or timing. A congested or disconnected outlet leaves the item in the valve. |
| Chute outlet | Slot 0 retains the last exported item as an assertion-visible observation latch. | The observed item is not emitted into a world, stacked, despawned, or consumed. Consequently an occupied latch represents congestion until a test clears it. |
| Passive fallback | Stationpedia fields, slots, memory, connections, and IC access remain available; no autonomous transition runs. | Any real active behaviour is intentionally absent. Use a constrained scenario scripted driver when a test needs that transition. Passive does not mean the real game device is inert. |

Recipes, reagents, atmospherics, production timing, and power availability are
outside this pack. These simplifications are never reported as exact game
physics.

## Performance benchmark

Run the deterministic executable benchmark with:

```text
cargo bench -p ic10-sim --bench behaviour_runtime
```

It builds three 600-device worlds (passive, active, and 50/50 mixed), advances
100 ticks, and measures three release-mode runs for tracing disabled and
enabled. The reported value is the median nanoseconds per tick. Every three-run
group must finish with identical state hashes or the benchmark fails.

Budget for this baseline is:

- at most 250,000 ns/tick for 600 devices without tracing;
- at most 350,000 ns/tick for 600 devices with effect tracing.

Reference run on 2026-07-25, Windows x64, Rust 1.90.0:

| World | No tracing median | Tracing median | Final hash |
| --- | ---: | ---: | ---: |
| 600 passive | 916 ns/tick | 1,261 ns/tick | `6161438875805727289` |
| 600 active | 82,586 ns/tick | 92,722 ns/tick | `165355500254446725` |
| 300 passive / 300 active | 31,604 ns/tick | 36,536 ns/tick | `8883749043896804581` |

Short timings can contain scheduler noise, so the numbers are a regression
baseline rather than a hardware-independent promise. The executable hash
assertion and budgets are the durable acceptance gates.
