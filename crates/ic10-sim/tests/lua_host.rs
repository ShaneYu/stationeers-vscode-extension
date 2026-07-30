use std::collections::BTreeMap;
use std::fs;

use ic10_data::KnowledgeBase;
use ic10_sim::{LuaHostMock, LuaModuleRunner, LuaRunLimits, Scenario, World};
use tempfile::tempdir;

#[test]
fn core_host_reads_writes_slots_memory_and_captures_logs() {
    let scenario =
        Scenario::load(std::path::Path::new("tests/fixtures/multi-ic.icsim")).expect("fixture");
    let knowledge = KnowledgeBase::load_embedded().expect("knowledge");
    let world = World::build(&scenario.networks, &scenario.devices, &knowledge).expect("world");
    let mut pins = BTreeMap::new();
    pins.insert("d0".to_owned(), "status-light".to_owned());
    let host = LuaHostMock::new(world, knowledge, pins);

    let directory = tempdir().expect("tempdir");
    let entry = directory.path().join("entry.lua");
    fs::write(
        &entry,
        r#"
        assert(ic.get("d0", "On") == 0)
        ic.set("d0", "On", 1)
        local light = device.get("status-light")
        assert(light:get("On") == 1)
        assert(device.getReferenceId("status-light") > 0)
        local sorter = device.get("sorter")
        assert(sorter:slot(0):get("Quantity") == 5)
        sorter:slot(0):set("Quantity", 6)
        assert(sorter:memory(3) == 77)
        sorter:setMemory(3, 88)
        print("done")
        log("host")
    "#,
    )
    .expect("source");

    let result = LuaModuleRunner::new()
        .run_with_host(
            &entry,
            &[directory.path().to_owned()],
            &LuaRunLimits::default(),
            &host,
        )
        .expect("host execution");
    assert_eq!(result.output, ["done", "host"]);
    assert_eq!(host.logs(), result.output);
    let world = host.world();
    assert_eq!(world.borrow().devices[2].fields["On"], 1.0);
    assert_eq!(world.borrow().devices[3].slots[&0]["Quantity"], 6.0);
    assert_eq!(world.borrow().devices[3].memory[3], 88.0);
}

#[test]
fn core_host_reads_and_writes_channel_logic_types() {
    let scenario =
        Scenario::load(std::path::Path::new("tests/fixtures/multi-ic.icsim")).expect("fixture");
    let knowledge = KnowledgeBase::load_embedded().expect("knowledge");
    let world = World::build(&scenario.networks, &scenario.devices, &knowledge).expect("world");
    let host = LuaHostMock::new(world, knowledge, BTreeMap::new()).with_housing(0);
    let directory = tempdir().expect("tempdir");
    let entry = directory.path().join("entry.lua");
    fs::write(
        &entry,
        r#"
        local LT = ic.enums.LogicType
        local base = ic.const.BASE_UNIT_INDEX
        ic.write(base, LT.Channel0 + 7, 0, 77)
        assert(ic.read(base, LT.Channel0 + 7, 0) == 77)
    "#,
    )
    .expect("source");

    LuaModuleRunner::new()
        .run_with_host(
            &entry,
            &[directory.path().to_owned()],
            &LuaRunLimits::default(),
            &host,
        )
        .expect("channel host execution");
    assert_eq!(host.world().borrow().networks[0].channels[7], 77.0);
}

#[test]
fn core_host_errors_are_named_and_deterministic() {
    let scenario =
        Scenario::load(std::path::Path::new("tests/fixtures/multi-ic.icsim")).expect("fixture");
    let knowledge = KnowledgeBase::load_embedded().expect("knowledge");
    let world = World::build(&scenario.networks, &scenario.devices, &knowledge).expect("world");
    let host = LuaHostMock::new(world, knowledge, BTreeMap::new());
    let directory = tempdir().expect("tempdir");
    let entry = directory.path().join("entry.lua");
    fs::write(&entry, "device.get('missing')\n").expect("source");
    let diagnostic = LuaModuleRunner::new()
        .run_with_host(
            &entry,
            &[directory.path().to_owned()],
            &LuaRunLimits::default(),
            &host,
        )
        .expect_err("missing device");
    assert_eq!(diagnostic.code, "lua-runtime-error");
    assert!(diagnostic.message.contains("[lua-missing-device]"));
}
