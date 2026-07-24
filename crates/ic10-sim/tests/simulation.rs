use std::path::PathBuf;

use ic10_sim::{Scalar, Scenario, Simulator};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
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
