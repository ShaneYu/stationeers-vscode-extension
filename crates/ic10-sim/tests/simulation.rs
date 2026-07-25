use std::path::PathBuf;

use ic10_sim::{Scalar, Scenario, Simulator};
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
    let mut simulator = Simulator::from_scenario_path(&example("ingot-supplier.ic10sim.json"))
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
