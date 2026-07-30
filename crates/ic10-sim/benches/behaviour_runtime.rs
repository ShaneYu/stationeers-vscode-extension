use std::fs;
use std::hint::black_box;
use std::time::Instant;

use ic10_sim::Simulator;
use serde_json::{Value, json};

const DEVICE_COUNT: usize = 600;
const TICKS: usize = 100;
const RUNS: usize = 3;

#[derive(Clone, Copy)]
enum Mix {
    Passive,
    Active,
    Mixed,
}

fn main() {
    println!("case,devices,ticks,tracing,median_ns_per_tick,state_hash");
    for mix in [Mix::Passive, Mix::Active, Mix::Mixed] {
        for tracing in [false, true] {
            let mut samples = Vec::new();
            let mut hashes = Vec::new();
            for _ in 0..RUNS {
                let (elapsed, hash) = run(mix, tracing);
                samples.push(elapsed);
                hashes.push(hash);
            }
            samples.sort_unstable();
            assert!(
                hashes.windows(2).all(|pair| pair[0] == pair[1]),
                "benchmark replay hashes differ"
            );
            let median = samples[RUNS / 2] / TICKS as u128;
            let budget = if tracing { 350_000 } else { 250_000 };
            assert!(
                median <= budget,
                "{} tracing={tracing} median {median} ns/tick exceeds {budget} ns/tick",
                name(mix)
            );
            println!(
                "{},{DEVICE_COUNT},{TICKS},{tracing},{},{}",
                name(mix),
                median,
                hashes[0]
            );
        }
    }
}

fn run(mix: Mix, tracing: bool) -> (u128, u64) {
    let directory = tempfile::tempdir().expect("benchmark directory");
    fs::write(directory.path().join("idle.ic10"), "yield\nj 0\n").expect("program");
    let scenario = scenario(mix);
    let scenario_path = directory.path().join("benchmark.icsim");
    fs::write(
        &scenario_path,
        serde_json::to_vec(&scenario).expect("scenario JSON"),
    )
    .expect("scenario");
    let mut simulator = Simulator::from_scenario_path(&scenario_path).expect("simulator");
    simulator.set_journaling(tracing);
    let vendors = simulator
        .world
        .devices
        .iter()
        .filter(|device| device.prefab == "StructureVendingMachine")
        .map(|device| device.id.clone())
        .collect::<Vec<_>>();
    let started = Instant::now();
    for _ in 0..TICKS {
        for vendor in &vendors {
            simulator
                .set_device_field(vendor, "Activate", 1.0)
                .expect("activate vendor");
        }
        black_box(simulator.step_world_tick().expect("world tick"));
        if tracing {
            black_box(simulator.take_effects());
        }
    }
    (started.elapsed().as_nanos(), simulator.state_hash())
}

fn scenario(mix: Mix) -> Value {
    let mut devices = vec![json!({
        "id": "controller",
        "prefab": "StructureCircuitHousing",
        "connections": {"0": "data"},
        "ic": {"program": "idle.ic10"}
    })];
    for index in 0..DEVICE_COUNT {
        let prefab = match mix {
            Mix::Passive => "StructureWallLight",
            Mix::Active => active_prefab(index),
            Mix::Mixed if index % 2 == 0 => "StructureWallLight",
            Mix::Mixed => active_prefab(index / 2),
        };
        devices.push(json!({
            "id": format!("device-{index}"),
            "prefab": prefab
        }));
    }
    json!({
        "schemaVersion": 1,
        "networks": [{"id": "data", "kind": "cable", "cableRole": "data"}],
        "devices": devices
    })
}

fn active_prefab(index: usize) -> &'static str {
    match index % 3 {
        0 => "StructureVendingMachine",
        1 => "StructureChuteDigitalValveLeft",
        _ => "StructureChuteOutlet",
    }
}

fn name(mix: Mix) -> &'static str {
    match mix {
        Mix::Passive => "passive",
        Mix::Active => "active",
        Mix::Mixed => "mixed",
    }
}
