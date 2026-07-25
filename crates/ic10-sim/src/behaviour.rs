use std::collections::BTreeMap;

use crate::world::World;

const VENDING_MACHINE: &str = "StructureVendingMachine";
const CHUTE_OUTLET: &str = "StructureChuteOutlet";

/// Runs the small, deterministic active-device subset currently supported by
/// the simulator. Device order and slot order are stable because both are
/// scenario/BTreeMap ordered.
pub(crate) fn run_tick(world: &mut World) {
    vend_requested_stacks(world);
    pass_open_digital_valves(world);
}

fn vend_requested_stacks(world: &mut World) {
    for vendor in 0..world.devices.len() {
        if world.devices[vendor].prefab != VENDING_MACHINE
            || field(world, vendor, "Activate") == 0.0
        {
            continue;
        }

        let Some(output) = world.devices[vendor].connections.get(&1).copied() else {
            reset_activation(world, vendor);
            continue;
        };
        let Some(receiver) = digital_valve_on_input(world, output) else {
            reset_activation(world, vendor);
            continue;
        };
        if slot_occupied(world, receiver, 0) {
            reset_activation(world, vendor);
            continue;
        }

        let requested_slot = field(world, vendor, "DispenseSlot")
            .is_finite()
            .then(|| field(world, vendor, "DispenseSlot") as usize)
            .filter(|slot| slot_occupied(world, vendor, *slot));
        let source_slot = requested_slot.or_else(|| {
            world.devices[vendor]
                .slots
                .iter()
                .find_map(|(slot, _)| slot_occupied(world, vendor, *slot).then_some(*slot))
        });

        if let Some(source_slot) = source_slot {
            let item = take_slot(world, vendor, source_slot);
            put_slot(world, receiver, 0, item);
            increment_field(world, vendor, "ExportCount");
            set_field(world, receiver, "Quantity", 1.0);
        }
        reset_activation(world, vendor);
    }
}

fn pass_open_digital_valves(world: &mut World) {
    for valve in 0..world.devices.len() {
        if !is_digital_chute_valve(&world.devices[valve].prefab)
            || field(world, valve, "Open") == 0.0
            || !slot_occupied(world, valve, 0)
        {
            continue;
        }
        let Some(output) = world.devices[valve].connections.get(&1).copied() else {
            continue;
        };
        let Some(outlet) = chute_outlet_on_input(world, output) else {
            continue;
        };

        let item = take_slot(world, valve, 0);
        // The outlet slot is a deterministic "last exported item" latch. It
        // makes item identity observable to tests after the item leaves the
        // simulated chute network.
        put_slot(world, outlet, 0, item);
        increment_field(world, outlet, "ExportCount");
        set_field(world, valve, "Quantity", 0.0);
    }
}

fn digital_valve_on_input(world: &World, network: usize) -> Option<usize> {
    world
        .devices
        .iter()
        .enumerate()
        .find_map(|(index, device)| {
            (is_digital_chute_valve(&device.prefab)
                && device.connections.get(&0).copied() == Some(network))
            .then_some(index)
        })
}

fn chute_outlet_on_input(world: &World, network: usize) -> Option<usize> {
    world
        .devices
        .iter()
        .enumerate()
        .find_map(|(index, device)| {
            (device.prefab == CHUTE_OUTLET && device.connections.get(&0).copied() == Some(network))
                .then_some(index)
        })
}

fn is_digital_chute_valve(prefab: &str) -> bool {
    prefab.starts_with("StructureChuteDigitalValve")
}

fn field(world: &World, device: usize, name: &str) -> f64 {
    world.devices[device]
        .fields
        .get(name)
        .copied()
        .unwrap_or(0.0)
}

fn set_field(world: &mut World, device: usize, name: &str, value: f64) {
    if let Some(field) = world.devices[device].fields.get_mut(name) {
        *field = value;
    }
}

fn increment_field(world: &mut World, device: usize, name: &str) {
    if let Some(field) = world.devices[device].fields.get_mut(name) {
        *field += 1.0;
    }
}

fn reset_activation(world: &mut World, vendor: usize) {
    set_field(world, vendor, "Activate", 0.0);
}

fn slot_occupied(world: &World, device: usize, slot: usize) -> bool {
    world.devices[device]
        .slots
        .get(&slot)
        .is_some_and(|fields| {
            fields.get("Occupied").copied().unwrap_or(0.0) > 0.0
                || fields.get("Quantity").copied().unwrap_or(0.0) > 0.0
        })
}

fn take_slot(world: &mut World, device: usize, slot: usize) -> BTreeMap<String, f64> {
    let fields = world.devices[device]
        .slots
        .get_mut(&slot)
        .expect("known device slot");
    let item = fields.clone();
    for value in fields.values_mut() {
        *value = 0.0;
    }
    item
}

fn put_slot(world: &mut World, device: usize, slot: usize, item: BTreeMap<String, f64>) {
    let fields = world.devices[device]
        .slots
        .get_mut(&slot)
        .expect("known device slot");
    for (name, value) in item {
        if fields.contains_key(&name) {
            fields.insert(name, value);
        }
    }
}
