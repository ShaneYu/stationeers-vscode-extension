use std::path::PathBuf;

use ic10_sim::{
    BehaviourCatalogEntry, BehaviourKind, CpuState, EffectActor, EffectTarget, Scalar, Scenario,
    ScheduledAction, Simulator, behaviour_catalog,
};
use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples")
        .join("multi-ic")
        .join(name)
}

fn assert_golden_scalar(actual: f64, expected: &Value, location: &str) {
    match expected {
        Value::Number(value) => {
            assert_eq!(actual, value.as_f64().expect("golden number"), "{location}")
        }
        Value::String(value) if value == "NaN" => assert!(actual.is_nan(), "{location}"),
        Value::String(value) if value == "Infinity" => {
            assert_eq!(actual, f64::INFINITY, "{location}")
        }
        Value::String(value) if value == "-Infinity" => {
            assert_eq!(actual, f64::NEG_INFINITY, "{location}")
        }
        Value::String(value) if value == "-0" => {
            assert_eq!(actual, 0.0, "{location}");
            assert!(actual.is_sign_negative(), "{location}");
        }
        _ => panic!("invalid golden scalar at {location}: {expected}"),
    }
}

fn material_handling_fixture() -> Simulator {
    let mut simulator =
        Simulator::from_scenario_path(&example("ingot-supplier.stationeerssim.json"))
            .expect("material handling fixture");
    for cpu in &mut simulator.cpus {
        cpu.state = CpuState::Halted;
    }
    simulator
}

fn model_tick(simulator: &mut Simulator) {
    simulator.step_world_tick().expect("behaviour tick");
}

fn clear_slot(simulator: &mut Simulator, device: usize, slot: usize) {
    for value in simulator.world.devices[device]
        .slots
        .get_mut(&slot)
        .expect("slot")
        .values_mut()
    {
        *value = 0.0;
    }
}

fn copy_slot(
    simulator: &mut Simulator,
    from_device: usize,
    from_slot: usize,
    to_device: usize,
    to_slot: usize,
) {
    let source = simulator.world.devices[from_device].slots[&from_slot].clone();
    for (field, value) in source {
        if let Some(target) = simulator.world.devices[to_device]
            .slots
            .get_mut(&to_slot)
            .expect("destination slot")
            .get_mut(&field)
        {
            *target = value;
        }
    }
}

#[test]
fn every_catalogued_conformance_fixture_executes_as_golden_data() {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalog: Value =
        serde_json::from_str(include_str!("../../../data/conformance/fixtures.json"))
            .expect("fixture catalog");
    for (fixture_id, fixture) in catalog["fixtures"].as_object().expect("fixtures") {
        let scenario = repository.join(fixture["scenario"].as_str().expect("scenario path"));
        let mut simulator = Simulator::from_scenario_path(&scenario).expect(fixture_id);
        for _ in 0..fixture["ticks"].as_u64().expect("ticks") {
            simulator.step_world_tick().expect(fixture_id);
        }
        let expected = &fixture["expected"];
        if let Some(cpus) = expected["cpus"].as_object() {
            for (cpu_id, cpu_expected) in cpus {
                let cpu = simulator
                    .cpus
                    .iter()
                    .find(|cpu| &cpu.id == cpu_id)
                    .expect("golden CPU");
                for (register, value) in cpu_expected["registers"].as_object().into_iter().flatten()
                {
                    assert_golden_scalar(
                        cpu.register(register).expect("golden register"),
                        value,
                        &format!("{fixture_id}.cpus.{cpu_id}.{register}"),
                    );
                }
            }
        }
        if let Some(devices) = expected["devices"].as_object() {
            for (device_id, device_expected) in devices {
                let device = simulator
                    .world
                    .device_index(device_id)
                    .map(|index| &simulator.world.devices[index])
                    .expect("golden device");
                for (field, value) in device_expected["fields"].as_object().into_iter().flatten() {
                    assert_golden_scalar(
                        device.fields[field],
                        value,
                        &format!("{fixture_id}.devices.{device_id}.{field}"),
                    );
                }
            }
        }
        if let Some(networks) = expected["networks"].as_object() {
            for (network_id, network_expected) in networks {
                let network = simulator
                    .world
                    .network_index(network_id)
                    .map(|index| &simulator.world.networks[index])
                    .expect("golden network");
                for (channel, value) in network_expected["channels"]
                    .as_object()
                    .into_iter()
                    .flatten()
                {
                    let index = channel
                        .strip_prefix("Channel")
                        .and_then(|value| value.parse::<usize>().ok())
                        .expect("golden channel");
                    assert_golden_scalar(
                        network.channels[index],
                        value,
                        &format!("{fixture_id}.networks.{network_id}.{channel}"),
                    );
                }
            }
        }
    }
}

#[test]
fn multiple_ics_share_connection_channels_but_keep_separate_data_networks() {
    let mut simulator =
        Simulator::from_scenario_path(&fixture("multi-ic.ic10sim.json")).expect("scenario");

    simulator.step_world_tick().expect("world tick");

    let shared = simulator
        .world
        .network_index("shared-power")
        .expect("network");
    let supplier_data = simulator
        .world
        .network_index("supplier-data")
        .expect("network");
    let requester_data = simulator
        .world
        .network_index("requester-data")
        .expect("network");
    assert_eq!(simulator.world.networks[shared].channels[0], 42.0);
    assert!(simulator.world.networks[supplier_data].channels[0].is_nan());
    assert!(simulator.world.networks[requester_data].channels[0].is_nan());
    assert_eq!(simulator.cpus[1].registers[0], 42.0);
    assert_eq!(simulator.cpus[1].registers[1], 1.0);
    let light = simulator
        .world
        .device_index("status-light")
        .expect("status light");
    assert_eq!(simulator.world.devices[light].fields["On"], 1.0);
}

#[test]
fn initial_registers_and_stack_are_editable_runtime_state() {
    let mut simulator =
        Simulator::from_scenario_path(&fixture("multi-ic.ic10sim.json")).expect("scenario");

    assert_eq!(simulator.cpus[0].registers[2], 7.0);
    assert_eq!(simulator.cpus[0].stack[12], 99.0);
    simulator.cpus[0].set_register("r2", 8.0).expect("register");
    simulator.cpus[0].stack[12] = 100.0;
    assert_eq!(simulator.cpus[0].registers[2], 8.0);
    assert_eq!(simulator.cpus[0].stack[12], 100.0);
}

#[test]
fn device_slots_and_memory_are_editable_runtime_state() {
    let mut simulator =
        Simulator::from_scenario_path(&fixture("multi-ic.ic10sim.json")).expect("scenario");
    let sorter = simulator
        .world
        .device_index("sorter")
        .expect("runtime sorter");

    assert_eq!(simulator.world.devices[sorter].slots[&0]["Class"], 19.0);
    assert_eq!(simulator.world.devices[sorter].slots[&0]["Quantity"], 5.0);
    assert_eq!(simulator.world.devices[sorter].memory[3], 77.0);
    simulator.world.devices[sorter]
        .slots
        .get_mut(&0)
        .expect("slot")
        .insert("Quantity".to_owned(), 6.0);
    simulator.world.devices[sorter].memory[3] = 88.0;
    assert_eq!(simulator.world.devices[sorter].slots[&0]["Quantity"], 6.0);
    assert_eq!(simulator.world.devices[sorter].memory[3], 88.0);
}

#[test]
fn named_vending_request_crosses_a_digital_chute_valve() {
    let mut simulator =
        Simulator::from_scenario_path(&example("ingot-supplier.stationeerssim.json"))
            .expect("vending scenario");

    for _ in 0..8 {
        simulator.step_world_tick().expect("vending world tick");
    }

    let iron = simulator.world.device_index("iron-vendor").expect("iron");
    let gold = simulator.world.device_index("gold-vendor").expect("gold");
    let valve = simulator
        .world
        .device_index("delivery-valve")
        .expect("valve");
    let outlet = simulator
        .world
        .device_index("delivery-outlet")
        .expect("outlet");
    assert_eq!(simulator.world.devices[iron].slots[&2]["Occupied"], 0.0);
    assert_eq!(simulator.world.devices[gold].slots[&2]["Quantity"], 50.0);
    assert_eq!(simulator.world.devices[valve].fields["Open"], 0.0);
    assert_eq!(
        simulator.world.devices[outlet].slots[&0]["OccupantHash"],
        -1_301_215_609.0
    );
    assert_eq!(simulator.world.devices[outlet].fields["ExportCount"], 1.0);
    assert_eq!(simulator.cpus[1].registers[2], 1.0);
}

#[test]
fn vending_model_handles_empty_congested_and_unconnected_outputs() {
    for case in ["empty", "congested", "unconnected"] {
        let mut simulator = material_handling_fixture();
        let vendor = simulator.world.device_index("iron-vendor").unwrap();
        let valve = simulator.world.device_index("delivery-valve").unwrap();
        if case == "empty" {
            clear_slot(&mut simulator, vendor, 2);
        } else if case == "congested" {
            copy_slot(&mut simulator, vendor, 2, valve, 0);
        } else {
            simulator.world.devices[vendor].connections.remove(&1);
        }
        simulator
            .set_device_field("iron-vendor", "Activate", 1.0)
            .unwrap();
        let before = simulator.world.devices[vendor].slots[&2].clone();
        model_tick(&mut simulator);
        assert_eq!(
            simulator.world.devices[vendor].fields["Activate"], 0.0,
            "{case}"
        );
        if case != "empty" {
            assert_eq!(simulator.world.devices[vendor].slots[&2], before, "{case}");
        }
    }
}

#[test]
fn digital_valve_model_handles_empty_congested_and_unconnected_outputs() {
    for case in ["empty", "congested", "unconnected"] {
        let mut simulator = material_handling_fixture();
        let vendor = simulator.world.device_index("iron-vendor").unwrap();
        let valve = simulator.world.device_index("delivery-valve").unwrap();
        let outlet = simulator.world.device_index("delivery-outlet").unwrap();
        if case != "empty" {
            copy_slot(&mut simulator, vendor, 2, valve, 0);
        }
        if case == "congested" {
            copy_slot(&mut simulator, vendor, 2, outlet, 0);
        } else if case == "unconnected" {
            simulator.world.devices[valve].connections.remove(&1);
        }
        simulator
            .set_device_field("delivery-valve", "Open", 1.0)
            .unwrap();
        let before = simulator.world.devices[valve].slots[&0].clone();
        model_tick(&mut simulator);
        assert_eq!(simulator.world.devices[valve].slots[&0], before, "{case}");
    }
}

#[test]
fn chute_outlet_model_fixtures_cover_success_empty_congested_and_unconnected() {
    for case in ["success", "empty", "congested", "unconnected"] {
        let mut simulator = material_handling_fixture();
        let vendor = simulator.world.device_index("iron-vendor").unwrap();
        let outlet = simulator.world.device_index("delivery-outlet").unwrap();
        if case != "empty" {
            copy_slot(&mut simulator, vendor, 2, outlet, 0);
        }
        if case == "unconnected" {
            simulator.world.devices[outlet].connections.clear();
        }
        let before = simulator.world.devices[outlet].slots[&0].clone();
        model_tick(&mut simulator);
        assert_eq!(simulator.world.devices[outlet].slots[&0], before, "{case}");
    }
}

#[test]
fn passive_model_fixtures_cover_success_empty_congested_and_unconnected() {
    for case in ["success", "empty", "congested", "unconnected"] {
        let mut simulator = material_handling_fixture();
        let passive = simulator.world.device_index("supplier").unwrap();
        if case == "empty" {
            simulator.world.devices[passive]
                .fields
                .insert("On".to_owned(), 0.0);
        } else {
            simulator.world.devices[passive]
                .fields
                .insert("On".to_owned(), 1.0);
        }
        if case == "unconnected" {
            simulator.world.devices[passive].connections.clear();
        }
        let before_fields = simulator.world.devices[passive].fields.clone();
        let before_connections = simulator.world.devices[passive].connections.clone();
        model_tick(&mut simulator);
        assert_eq!(
            simulator.world.devices[passive].fields, before_fields,
            "{case}"
        );
        assert_eq!(
            simulator.world.devices[passive].connections, before_connections,
            "{case}"
        );
    }
}

#[test]
fn checked_in_behaviour_catalog_matches_runtime_descriptors() {
    let checked_in: Vec<BehaviourCatalogEntry> =
        serde_json::from_str(include_str!("../../../docs/behaviour-catalog.json"))
            .expect("catalog JSON");
    assert_eq!(checked_in, behaviour_catalog());

    let simulator = material_handling_fixture();
    for (id, expected) in [
        ("iron-vendor", BehaviourKind::VendingMachine),
        ("delivery-valve", BehaviourKind::DigitalChuteValve),
        ("delivery-outlet", BehaviourKind::ChuteOutlet),
        ("supplier", BehaviourKind::Passive),
    ] {
        let device = simulator.world.device_index(id).unwrap();
        assert_eq!(simulator.behaviour(device).unwrap().kind, expected, "{id}");
    }
}

#[test]
fn behaviour_descriptors_distinguish_modelled_and_passive_devices() {
    let simulator = Simulator::from_scenario_path(&example("ingot-supplier.stationeerssim.json"))
        .expect("vending scenario");
    let vendor = simulator.world.device_index("iron-vendor").expect("vendor");
    let housing = simulator.world.device_index("supplier").expect("housing");

    let vendor_model = simulator.behaviour(vendor).expect("vendor behaviour");
    assert!(vendor_model.modelled);
    assert_eq!(vendor_model.kind, BehaviourKind::VendingMachine);
    assert_eq!(vendor_model.model, "builtin.material-handling");
    assert_eq!(vendor_model.version, 1);

    let housing_model = simulator.behaviour(housing).expect("passive behaviour");
    assert!(!housing_model.modelled);
    assert_eq!(housing_model.kind, BehaviourKind::Passive);
}

#[test]
fn scheduled_behaviour_events_are_stable_journalled_and_reversible() {
    let mut simulator =
        Simulator::from_scenario_path(&fixture("multi-ic.ic10sim.json")).expect("scenario");
    let light = simulator
        .world
        .device_index("status-light")
        .expect("status light");
    simulator.set_journaling(true);
    simulator
        .behaviour_runtime_mut()
        .schedule(
            1,
            light,
            ScheduledAction::SetField {
                device: light,
                field: "On".to_owned(),
                value: 1.0,
            },
        )
        .expect("first event");
    simulator
        .behaviour_runtime_mut()
        .schedule(
            1,
            light,
            ScheduledAction::SetField {
                device: light,
                field: "On".to_owned(),
                value: 1.0,
            },
        )
        .expect("second event");
    let initial = simulator.snapshot();

    simulator.step_world_tick().expect("world tick");
    let first_hash = simulator.state_hash();
    assert_eq!(simulator.world.devices[light].fields["On"], 1.0);
    let effects = simulator.take_effects();
    let writes = effects
        .writes
        .iter()
        .filter(|write| {
            matches!(
                write.target,
                EffectTarget::DeviceField { device, .. } if device == light
            ) && matches!(write.actor, EffectActor::Behaviour { device, version: 1, .. } if device == light)
        })
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].before_bits, 1.0_f64.to_bits());
    assert_eq!(writes[0].after_bits, 1.0_f64.to_bits());
    assert!(writes[0].attempted);
    assert_eq!(writes[1].after_bits, 1.0_f64.to_bits());

    simulator.restore(&initial).expect("restore");
    simulator.step_world_tick().expect("replayed world tick");
    assert_eq!(simulator.state_hash(), first_hash);
}

#[test]
fn stateful_model_counters_restore_and_replay_across_a_behaviour_tick() {
    let mut simulator = material_handling_fixture();
    let vendor = simulator.world.device_index("iron-vendor").unwrap();
    simulator
        .set_device_field("iron-vendor", "Activate", 1.0)
        .unwrap();
    let checkpoint = simulator.snapshot();
    let before_hash = simulator.state_hash();

    model_tick(&mut simulator);
    let forward_hash = simulator.state_hash();
    assert_eq!(simulator.behaviour_state(vendor).unwrap().activations, 1);
    assert_eq!(simulator.behaviour_state(vendor).unwrap().transfers, 1);
    assert_ne!(forward_hash, before_hash);

    simulator.restore(&checkpoint).expect("reset to checkpoint");
    assert_eq!(simulator.behaviour_state(vendor).unwrap().activations, 0);
    assert_eq!(simulator.behaviour_state(vendor).unwrap().transfers, 0);
    assert_eq!(simulator.state_hash(), before_hash);
    model_tick(&mut simulator);
    assert_eq!(simulator.state_hash(), forward_hash);
}

#[test]
fn instruction_journal_records_exact_actor_and_unchanged_store() {
    let mut simulator =
        Simulator::from_scenario_path(&fixture("multi-ic.ic10sim.json")).expect("scenario");
    simulator.set_journaling(true);
    simulator
        .set_register_as(0, 0, 42.0, EffectActor::Scenario)
        .expect("seed register");
    simulator.take_effects();

    let event = simulator.step_instruction(0).expect("move instruction");
    assert_eq!(event.line, 1);
    let effects = simulator.take_effects();
    let write = effects
        .writes
        .iter()
        .find(|write| {
            matches!(
                write.target,
                EffectTarget::Register {
                    cpu: 0,
                    register: 0
                }
            )
        })
        .expect("register write");
    assert_eq!(write.before_bits, 42.0_f64.to_bits());
    assert_eq!(write.after_bits, 42.0_f64.to_bits());
    assert!(write.attempted);
    assert!(matches!(
        write.actor,
        EffectActor::Ic {
            cpu: 0,
            line: 1,
            ..
        }
    ));
}

#[test]
fn behaviour_failures_include_device_model_and_version() {
    let mut simulator =
        Simulator::from_scenario_path(&fixture("multi-ic.ic10sim.json")).expect("scenario");
    let error = simulator
        .behaviour_runtime_mut()
        .schedule(
            u64::MAX,
            usize::MAX,
            ScheduledAction::SetField {
                device: usize::MAX,
                field: "On".to_owned(),
                value: 1.0,
            },
        )
        .expect_err("invalid device should fail");
    let message = error.to_string();
    assert!(message.contains("device `#"));
    assert!(message.contains("unknown@0"));
    assert!(message.contains("schedule failed"));
}

#[test]
fn device_pins_must_share_the_housing_data_cable() {
    let scenario_path = fixture("multi-ic.ic10sim.json");
    let mut scenario = Scenario::load(&scenario_path).expect("scenario");
    scenario.devices[1]
        .ic
        .as_mut()
        .expect("requester IC")
        .pins
        .insert("d0".to_owned(), "status-light".to_owned());

    let error =
        Simulator::from_scenario(scenario, scenario_path.parent().expect("fixture directory"))
            .expect_err("cross-network pin should fail");

    assert!(
        error
            .to_string()
            .contains("not on the housing's data cable")
    );
}

#[test]
fn connection_types_must_match_network_media_and_cable_role() {
    let scenario_path = fixture("multi-ic.ic10sim.json");
    let mut scenario = Scenario::load(&scenario_path).expect("scenario");
    scenario.devices[1]
        .connections
        .insert("1".to_owned(), "requester-data".to_owned());

    let error =
        Simulator::from_scenario(scenario, scenario_path.parent().expect("fixture directory"))
            .expect_err("power connection should reject data-only cable");

    assert!(
        error
            .to_string()
            .contains("connection `1` (Power) is not compatible with network `requester-data`")
    );
}

#[test]
fn device_initial_state_must_be_supported_by_its_prefab() {
    let scenario_path = fixture("multi-ic.ic10sim.json");
    let mut unknown_field = Scenario::load(&scenario_path).expect("scenario");
    unknown_field.devices[2]
        .fields
        .insert("Mode".to_owned(), Scalar::Number(1.0));
    let error = Simulator::from_scenario(
        unknown_field,
        scenario_path.parent().expect("fixture directory"),
    )
    .expect_err("wall light should reject unsupported Mode");
    assert!(
        error
            .to_string()
            .contains("does not support logic field `Mode`")
    );

    let mut unsupported_memory = Scenario::load(&scenario_path).expect("scenario");
    unsupported_memory.devices[2]
        .memory
        .insert("0".to_owned(), Scalar::Number(1.0));
    let error = Simulator::from_scenario(
        unsupported_memory,
        scenario_path.parent().expect("fixture directory"),
    )
    .expect_err("wall light should reject device memory");
    assert!(
        error
            .to_string()
            .contains("does not expose addressable memory")
    );
}

#[test]
fn manual_stepping_can_continue_past_yield() {
    let mut simulator =
        Simulator::from_scenario_path(&fixture("multi-ic.ic10sim.json")).expect("scenario");

    for _ in 0..4 {
        simulator.step_instruction(0).expect("instruction");
    }
    assert!(matches!(
        simulator.cpus[0].state,
        ic10_sim::CpuState::WaitingUntil(_)
    ));

    let jump = simulator
        .step_instruction(0)
        .expect("manual step should execute jump after yield");
    assert_eq!(jump.line, 5);
    assert_eq!(simulator.cpus[0].current_line(), Some(1));
}

#[test]
fn arithmetic_stack_selection_and_relative_branches_execute_together() {
    let mut simulator =
        Simulator::from_scenario_path(&fixture("instructions.ic10sim.json")).expect("scenario");

    simulator.step_world_tick().expect("world tick");

    assert_eq!(simulator.cpus[0].registers[1], 5.0);
    assert_eq!(simulator.cpus[0].registers[2], 5.0);
    assert_eq!(simulator.cpus[0].registers[3], 1.0);
    assert_eq!(simulator.cpus[0].registers[4], 9.0);
}

#[test]
fn documented_ieee754_cases_are_deterministic() {
    let mut simulator =
        Simulator::from_scenario_path(&fixture("ieee754.ic10sim.json")).expect("scenario");

    simulator.step_world_tick().expect("world tick");

    assert!(simulator.cpus[0].registers[0].is_nan());
    assert_eq!(simulator.cpus[0].registers[1], f64::INFINITY);
    assert_eq!(simulator.cpus[0].registers[2], f64::NEG_INFINITY);
    assert!(simulator.cpus[0].registers[3].is_sign_negative());
    assert_eq!(simulator.cpus[0].registers[3], 0.0);
}

#[test]
fn multiple_ics_are_scheduled_in_stable_shared_world_order() {
    let mut first =
        Simulator::from_scenario_path(&fixture("multi-ic.ic10sim.json")).expect("scenario");
    let mut second =
        Simulator::from_scenario_path(&fixture("multi-ic.ic10sim.json")).expect("scenario");

    let first_events = first.step_world_tick().expect("first world tick");
    let second_events = second.step_world_tick().expect("second world tick");
    let first_order: Vec<_> = first_events
        .iter()
        .map(|event| (event.cpu, event.line))
        .collect();
    let second_order: Vec<_> = second_events
        .iter()
        .map(|event| (event.cpu, event.line))
        .collect();

    assert_eq!(first_order, second_order);
    assert_eq!(
        first_order,
        vec![
            (0, 1),
            (0, 2),
            (0, 3),
            (0, 4),
            (1, 1),
            (1, 2),
            (1, 6),
            (1, 7)
        ]
    );
    assert_eq!(first.cpus[1].registers[0], 42.0);
    assert_eq!(first.cpus[1].registers[1], 1.0);
}

#[test]
fn mutable_checkpoint_restores_and_replays_to_the_same_hash() {
    let mut simulator =
        Simulator::from_scenario_path(&fixture("multi-ic.ic10sim.json")).expect("scenario");
    simulator.set_seed(73);
    let checkpoint = simulator.snapshot();
    let initial_hash = simulator.state_hash();

    for _ in 0..6 {
        simulator.scheduler_step().expect("scheduled instruction");
    }
    let forward_hash = simulator.state_hash();
    assert_ne!(forward_hash, initial_hash);

    simulator.restore(&checkpoint).expect("restore checkpoint");
    assert_eq!(simulator.state_hash(), initial_hash);
    for _ in 0..6 {
        simulator.scheduler_step().expect("replayed instruction");
    }
    assert_eq!(simulator.state_hash(), forward_hash);
}

#[test]
fn test_driver_state_is_checkpointed_hashed_and_journalled() {
    let mut simulator =
        Simulator::from_scenario_path(&fixture("multi-ic.ic10sim.json")).expect("scenario");
    simulator.set_journaling(true);
    let actor = simulator.scripted_driver_actor("mock-vendor", 2);
    let checkpoint = simulator.snapshot();
    let initial_hash = simulator.state_hash();
    simulator.set_test_driver_state("mock-vendor", b"pending:tick=4".to_vec(), actor);
    assert_ne!(simulator.state_hash(), initial_hash);
    assert_eq!(
        simulator.test_driver_state("mock-vendor"),
        Some(b"pending:tick=4".as_slice())
    );
    let effects = simulator.take_effects();
    assert!(effects.writes.iter().any(|write| {
        write.actor == actor && matches!(write.target, ic10_sim::EffectTarget::DriverState { .. })
    }));
    simulator.restore(&checkpoint).expect("restore checkpoint");
    assert_eq!(simulator.state_hash(), initial_hash);
    assert_eq!(simulator.test_driver_state("mock-vendor"), None);
}

#[test]
fn test_driver_slot_move_uses_the_shared_journalled_world_api() {
    let mut simulator =
        Simulator::from_scenario_path(&example("ingot-supplier.stationeerssim.json"))
            .expect("scenario");
    simulator.set_journaling(true);
    let source = simulator.world.device_index("iron-vendor").unwrap();
    let destination = simulator.world.device_index("delivery-outlet").unwrap();
    let actor = simulator.scripted_driver_actor("mock-vendor", 0);
    simulator
        .move_slot_item_as(source, 2, destination, 0, actor)
        .expect("move slot item");
    assert_eq!(simulator.world.devices[source].slots[&2]["Quantity"], 0.0);
    assert_eq!(
        simulator.world.devices[destination].slots[&0]["OccupantHash"],
        -1_301_215_609.0
    );
    let effects = simulator.take_effects();
    assert!(effects.writes.iter().any(|write| write.actor == actor));
}

#[test]
fn newer_scenario_versions_warn_without_blocking_execution() {
    let scenario_path = fixture("instructions.ic10sim.json");
    let mut scenario = Scenario::load(&scenario_path).expect("scenario");
    scenario.game_version = Some("0.2.9999.1".to_owned());

    let simulator =
        Simulator::from_scenario(scenario, scenario_path.parent().expect("fixture directory"))
            .expect("newer version should not block simulation");

    assert_eq!(simulator.compatibility_warnings.len(), 1);
    assert!(simulator.compatibility_warnings[0].contains("newer than bundled"));
}
