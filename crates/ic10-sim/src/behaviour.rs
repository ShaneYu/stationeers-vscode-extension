use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::journal::{EffectActor, EffectJournal, EffectTarget};
use crate::world::World;

const MATERIAL_HANDLING_MODEL: &str = "builtin.material-handling";
const MATERIAL_HANDLING_VERSION: u32 = 1;
const VENDING_MACHINE: &str = "StructureVendingMachine";
const CHUTE_OUTLET: &str = "StructureChuteOutlet";

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BehaviourKind {
    Passive,
    VendingMachine,
    DigitalChuteValve,
    ChuteOutlet,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BehaviourDescriptor {
    pub model: String,
    pub version: u32,
    pub kind: BehaviourKind,
    pub modelled: bool,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BehaviourCatalogEntry {
    pub selector: BehaviourSelector,
    pub descriptor: BehaviourDescriptor,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum BehaviourSelector {
    Exact(String),
    Prefix(String),
    Fallback,
}

/// Runtime-owned catalogue used by tooling and verified documentation.
///
/// Selector order is significant: the first match wins.
pub fn behaviour_catalog() -> Vec<BehaviourCatalogEntry> {
    vec![
        catalog_entry(
            BehaviourSelector::Exact(VENDING_MACHINE.to_owned()),
            BehaviourKind::VendingMachine,
            true,
        ),
        catalog_entry(
            BehaviourSelector::Prefix("StructureChuteDigitalValve".to_owned()),
            BehaviourKind::DigitalChuteValve,
            true,
        ),
        catalog_entry(
            BehaviourSelector::Exact(CHUTE_OUTLET.to_owned()),
            BehaviourKind::ChuteOutlet,
            true,
        ),
        BehaviourCatalogEntry {
            selector: BehaviourSelector::Fallback,
            descriptor: passive_descriptor(),
        },
    ]
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BehaviourState {
    /// Model-private counters intentionally live outside ordinary logic
    /// fields. They prove that checkpoints restore behaviour-owned state.
    pub activations: u64,
    pub transfers: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
struct EventKey {
    tick: u64,
    phase: u8,
    device: usize,
    sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ScheduledAction {
    SetField {
        device: usize,
        field: String,
        value: f64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct WriteNotice {
    device: usize,
    field: String,
    before_bits: u64,
    after_bits: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BehaviourRuntime {
    descriptors: Vec<BehaviourDescriptor>,
    states: Vec<BehaviourState>,
    scheduled: BTreeMap<EventKey, ScheduledAction>,
    pending_writes: VecDeque<WriteNotice>,
    next_sequence: u64,
    last_error: Option<String>,
}

impl BehaviourRuntime {
    pub fn build(world: &World) -> Self {
        let descriptors = world
            .devices
            .iter()
            .map(|device| descriptor_for_prefab(&device.prefab))
            .collect::<Vec<_>>();
        Self {
            states: vec![BehaviourState::default(); descriptors.len()],
            descriptors,
            scheduled: BTreeMap::new(),
            pending_writes: VecDeque::new(),
            next_sequence: 0,
            last_error: None,
        }
    }

    pub fn descriptor(&self, device: usize) -> Option<&BehaviourDescriptor> {
        self.descriptors.get(device)
    }

    pub fn state(&self, device: usize) -> Option<&BehaviourState> {
        self.states.get(device)
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub(crate) fn deterministic_bytes(&self) -> Vec<u8> {
        fn usize_value(bytes: &mut Vec<u8>, value: usize) {
            bytes.extend_from_slice(&(value as u64).to_le_bytes());
        }
        fn text(bytes: &mut Vec<u8>, value: &str) {
            usize_value(bytes, value.len());
            bytes.extend_from_slice(value.as_bytes());
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"IC10-BEHAVIOUR\0");
        usize_value(&mut bytes, self.descriptors.len());
        for (descriptor, state) in self.descriptors.iter().zip(&self.states) {
            text(&mut bytes, &descriptor.model);
            bytes.extend_from_slice(&descriptor.version.to_le_bytes());
            bytes.push(descriptor.kind as u8);
            bytes.push(u8::from(descriptor.modelled));
            usize_value(&mut bytes, descriptor.dependencies.len());
            for dependency in &descriptor.dependencies {
                text(&mut bytes, dependency);
            }
            bytes.extend_from_slice(&state.activations.to_le_bytes());
            bytes.extend_from_slice(&state.transfers.to_le_bytes());
        }
        usize_value(&mut bytes, self.scheduled.len());
        for (key, action) in &self.scheduled {
            bytes.extend_from_slice(&key.tick.to_le_bytes());
            bytes.push(key.phase);
            bytes.extend_from_slice(&(key.device as u64).to_le_bytes());
            bytes.extend_from_slice(&key.sequence.to_le_bytes());
            match action {
                ScheduledAction::SetField {
                    device,
                    field,
                    value,
                } => {
                    bytes.push(0);
                    bytes.extend_from_slice(&(*device as u64).to_le_bytes());
                    text(&mut bytes, field);
                    bytes.extend_from_slice(&value.to_bits().to_le_bytes());
                }
            }
        }
        usize_value(&mut bytes, self.pending_writes.len());
        for notice in &self.pending_writes {
            bytes.extend_from_slice(&(notice.device as u64).to_le_bytes());
            text(&mut bytes, &notice.field);
            bytes.extend_from_slice(&notice.before_bits.to_le_bytes());
            bytes.extend_from_slice(&notice.after_bits.to_le_bytes());
        }
        bytes.extend_from_slice(&self.next_sequence.to_le_bytes());
        match &self.last_error {
            Some(error) => {
                bytes.push(1);
                text(&mut bytes, error);
            }
            None => bytes.push(0),
        }
        bytes
    }

    pub fn notify_field_write(&mut self, device: usize, field: &str, before: f64, after: f64) {
        self.pending_writes.push_back(WriteNotice {
            device,
            field: field.to_owned(),
            before_bits: before.to_bits(),
            after_bits: after.to_bits(),
        });
    }

    pub fn schedule(
        &mut self,
        tick: u64,
        device: usize,
        action: ScheduledAction,
    ) -> Result<(), BehaviourError> {
        let descriptor = self
            .descriptor(device)
            .cloned()
            .ok_or_else(|| BehaviourError {
                device: format!("#{device}"),
                model: "unknown".to_owned(),
                version: 0,
                hook: "schedule",
                message: "device index is out of range".to_owned(),
            })?;
        match &action {
            ScheduledAction::SetField { device, field, .. } => {
                let Some(target) = self.descriptor(*device) else {
                    return Err(BehaviourError {
                        device: format!("#{device}"),
                        model: descriptor.model.clone(),
                        version: descriptor.version,
                        hook: "schedule",
                        message: "action target device index is out of range".to_owned(),
                    });
                };
                if field.is_empty() {
                    return Err(BehaviourError {
                        device: format!("#{device}"),
                        model: target.model.clone(),
                        version: target.version,
                        hook: "schedule",
                        message: "action target field is empty".to_owned(),
                    });
                }
            }
        }
        let key = EventKey {
            tick,
            phase: 0,
            device,
            sequence: self.next_sequence,
        };
        self.next_sequence = self.next_sequence.wrapping_add(1);
        if self.scheduled.insert(key, action).is_some() {
            return Err(BehaviourError {
                device: format!("#{device}"),
                model: descriptor.model.clone(),
                version: descriptor.version,
                hook: "schedule",
                message: "stable event key collided".to_owned(),
            });
        }
        Ok(())
    }

    pub fn tick_start(
        &mut self,
        world: &mut World,
        journal: &mut EffectJournal,
        tick: u64,
    ) -> Result<(), BehaviourError> {
        let due = self
            .scheduled
            .range(
                ..=EventKey {
                    tick,
                    phase: u8::MAX,
                    device: usize::MAX,
                    sequence: u64::MAX,
                },
            )
            .map(|(key, action)| (key.clone(), action.clone()))
            .collect::<Vec<_>>();
        for (key, action) in due {
            self.scheduled.remove(&key);
            match action {
                ScheduledAction::SetField {
                    device,
                    field,
                    value,
                } => {
                    let mut api = BehaviourWorld::new(self, world, journal, device, "tickStart")?;
                    api.set_field(device, &field, value)?;
                }
            }
        }
        Ok(())
    }

    pub fn tick_end(
        &mut self,
        world: &mut World,
        journal: &mut EffectJournal,
    ) -> Result<(), BehaviourError> {
        // Write notifications are drained in attempted-write order, including
        // unchanged writes. Built-ins currently need no immediate reaction,
        // but this is the deterministic lifecycle seam used by future packs.
        while let Some(notice) = self.pending_writes.pop_front() {
            if notice.device >= self.descriptors.len() {
                return self.fail(world, notice.device, "fieldWrite", "unknown device");
            }
            let _ = (notice.field, notice.before_bits, notice.after_bits);
        }
        for device in 0..self.descriptors.len() {
            match self.descriptors[device].kind {
                BehaviourKind::VendingMachine => self.vend(world, journal, device)?,
                BehaviourKind::DigitalChuteValve => self.pass_valve(world, journal, device)?,
                BehaviourKind::Passive | BehaviourKind::ChuteOutlet => {}
            }
        }
        Ok(())
    }

    fn vend(
        &mut self,
        world: &mut World,
        journal: &mut EffectJournal,
        vendor: usize,
    ) -> Result<(), BehaviourError> {
        if field(world, vendor, "Activate") == 0.0 {
            return Ok(());
        }
        self.states[vendor].activations = self.states[vendor].activations.wrapping_add(1);
        record_state(
            journal,
            vendor,
            "activations",
            self.states[vendor].activations,
        );

        let receiver = world.devices[vendor]
            .connections
            .get(&1)
            .copied()
            .and_then(|network| digital_valve_on_input(world, network));
        let Some(receiver) = receiver else {
            return self.reset_activation(world, journal, vendor);
        };
        if slot_occupied(world, receiver, 0) {
            return self.reset_activation(world, journal, vendor);
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
            let mut api = BehaviourWorld::new(self, world, journal, vendor, "tickEnd")?;
            api.move_slot(vendor, source_slot, receiver, 0)?;
            api.increment_field(vendor, "ExportCount")?;
            api.set_field(receiver, "Quantity", 1.0)?;
            api.runtime.states[vendor].transfers =
                api.runtime.states[vendor].transfers.wrapping_add(1);
            record_state(
                api.journal,
                vendor,
                "transfers",
                api.runtime.states[vendor].transfers,
            );
        }
        self.reset_activation(world, journal, vendor)
    }

    fn pass_valve(
        &mut self,
        world: &mut World,
        journal: &mut EffectJournal,
        valve: usize,
    ) -> Result<(), BehaviourError> {
        if field(world, valve, "Open") == 0.0 || !slot_occupied(world, valve, 0) {
            return Ok(());
        }
        let outlet = world.devices[valve]
            .connections
            .get(&1)
            .copied()
            .and_then(|network| chute_outlet_on_input(world, network));
        let Some(outlet) = outlet else {
            return Ok(());
        };
        if slot_occupied(world, outlet, 0) {
            return Ok(());
        }
        let mut api = BehaviourWorld::new(self, world, journal, valve, "tickEnd")?;
        api.move_slot(valve, 0, outlet, 0)?;
        api.increment_field(outlet, "ExportCount")?;
        api.set_field(valve, "Quantity", 0.0)?;
        api.runtime.states[valve].transfers = api.runtime.states[valve].transfers.wrapping_add(1);
        record_state(
            api.journal,
            valve,
            "transfers",
            api.runtime.states[valve].transfers,
        );
        Ok(())
    }

    fn reset_activation(
        &mut self,
        world: &mut World,
        journal: &mut EffectJournal,
        vendor: usize,
    ) -> Result<(), BehaviourError> {
        BehaviourWorld::new(self, world, journal, vendor, "tickEnd")?
            .set_field(vendor, "Activate", 0.0)
    }

    fn fail<T>(
        &mut self,
        world: &World,
        device: usize,
        hook: &'static str,
        message: &str,
    ) -> Result<T, BehaviourError> {
        let descriptor = self
            .descriptors
            .get(device)
            .cloned()
            .unwrap_or_else(passive_descriptor);
        let error = BehaviourError {
            device: world
                .devices
                .get(device)
                .map_or_else(|| format!("#{device}"), |item| item.id.clone()),
            model: descriptor.model,
            version: descriptor.version,
            hook,
            message: message.to_owned(),
        };
        self.last_error = Some(error.to_string());
        Err(error)
    }
}

struct BehaviourWorld<'a> {
    runtime: &'a mut BehaviourRuntime,
    world: &'a mut World,
    journal: &'a mut EffectJournal,
    actor: EffectActor,
    hook: &'static str,
}

impl<'a> BehaviourWorld<'a> {
    fn new(
        runtime: &'a mut BehaviourRuntime,
        world: &'a mut World,
        journal: &'a mut EffectJournal,
        actor_device: usize,
        hook: &'static str,
    ) -> Result<Self, BehaviourError> {
        let descriptor =
            runtime
                .descriptor(actor_device)
                .cloned()
                .ok_or_else(|| BehaviourError {
                    device: format!("#{actor_device}"),
                    model: "unknown".to_owned(),
                    version: 0,
                    hook,
                    message: "device index is out of range".to_owned(),
                })?;
        let model = journal.intern(&descriptor.model);
        Ok(Self {
            runtime,
            world,
            journal,
            actor: EffectActor::Behaviour {
                device: actor_device,
                model,
                version: descriptor.version,
            },
            hook,
        })
    }

    fn set_field(&mut self, device: usize, name: &str, value: f64) -> Result<(), BehaviourError> {
        let Some(target) = self.world.devices.get(device) else {
            return Err(BehaviourError {
                device: format!("#{device}"),
                model: "unknown".to_owned(),
                version: 0,
                hook: self.hook,
                message: "device index is out of range".to_owned(),
            });
        };
        let Some(before) = target.fields.get(name).copied() else {
            return self.runtime.fail(
                self.world,
                device,
                self.hook,
                &format!("missing field `{name}`"),
            );
        };
        self.world.devices[device]
            .fields
            .insert(name.to_owned(), value);
        let field = self.journal.intern(name);
        self.journal.write(
            self.actor,
            EffectTarget::DeviceField { device, field },
            before,
            value,
        );
        self.runtime.notify_field_write(device, name, before, value);
        Ok(())
    }

    fn increment_field(&mut self, device: usize, name: &str) -> Result<(), BehaviourError> {
        let before = field(self.world, device, name);
        self.set_field(device, name, before + 1.0)
    }

    fn move_slot(
        &mut self,
        from_device: usize,
        from_slot: usize,
        to_device: usize,
        to_slot: usize,
    ) -> Result<(), BehaviourError> {
        let Some(source_device) = self.world.devices.get(from_device) else {
            return Err(BehaviourError {
                device: format!("#{from_device}"),
                model: MATERIAL_HANDLING_MODEL.to_owned(),
                version: MATERIAL_HANDLING_VERSION,
                hook: self.hook,
                message: "source device index is out of range".to_owned(),
            });
        };
        let Some(destination_device) = self.world.devices.get(to_device) else {
            return Err(BehaviourError {
                device: format!("#{to_device}"),
                model: MATERIAL_HANDLING_MODEL.to_owned(),
                version: MATERIAL_HANDLING_VERSION,
                hook: self.hook,
                message: "destination device index is out of range".to_owned(),
            });
        };
        let item = source_device
            .slots
            .get(&from_slot)
            .cloned()
            .ok_or_else(|| BehaviourError {
                device: source_device.id.clone(),
                model: MATERIAL_HANDLING_MODEL.to_owned(),
                version: MATERIAL_HANDLING_VERSION,
                hook: self.hook,
                message: format!("source slot {from_slot} is missing"),
            })?;
        let Some(destination) = destination_device.slots.get(&to_slot) else {
            return self.runtime.fail(
                self.world,
                to_device,
                self.hook,
                &format!("destination slot {to_slot} is missing"),
            );
        };
        if let Some(field) = item.keys().find(|name| !destination.contains_key(*name)) {
            return self.runtime.fail(
                self.world,
                to_device,
                self.hook,
                &format!("destination slot {to_slot} has no field `{field}`"),
            );
        }
        // All bounds and schema checks above precede mutation, so a failed
        // transfer cannot partially clear the source item.
        for (name, value) in item {
            let source_before = self.world.devices[from_device].slots[&from_slot][&name];
            self.world.devices[from_device]
                .slots
                .get_mut(&from_slot)
                .expect("validated slot")
                .insert(name.clone(), 0.0);
            let field = self.journal.intern(&name);
            self.journal.write(
                self.actor,
                EffectTarget::DeviceSlot {
                    device: from_device,
                    slot: from_slot as u16,
                    field,
                },
                source_before,
                0.0,
            );
            if let Some(destination_before) = self.world.devices[to_device].slots[&to_slot]
                .get(&name)
                .copied()
            {
                self.world.devices[to_device]
                    .slots
                    .get_mut(&to_slot)
                    .expect("validated slot")
                    .insert(name.clone(), value);
                self.journal.write(
                    self.actor,
                    EffectTarget::DeviceSlot {
                        device: to_device,
                        slot: to_slot as u16,
                        field,
                    },
                    destination_before,
                    value,
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BehaviourError {
    pub device: String,
    pub model: String,
    pub version: u32,
    pub hook: &'static str,
    pub message: String,
}

impl fmt::Display for BehaviourError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "device `{}` behaviour {}@{} {} failed: {}",
            self.device, self.model, self.version, self.hook, self.message
        )
    }
}

impl std::error::Error for BehaviourError {}

fn descriptor_for_prefab(prefab: &str) -> BehaviourDescriptor {
    if prefab == VENDING_MACHINE {
        active_descriptor(BehaviourKind::VendingMachine)
    } else if prefab.starts_with("StructureChuteDigitalValve") {
        active_descriptor(BehaviourKind::DigitalChuteValve)
    } else if prefab == CHUTE_OUTLET {
        active_descriptor(BehaviourKind::ChuteOutlet)
    } else {
        passive_descriptor()
    }
}

fn catalog_entry(
    selector: BehaviourSelector,
    kind: BehaviourKind,
    modelled: bool,
) -> BehaviourCatalogEntry {
    BehaviourCatalogEntry {
        selector,
        descriptor: if modelled {
            active_descriptor(kind)
        } else {
            passive_descriptor()
        },
    }
}

fn active_descriptor(kind: BehaviourKind) -> BehaviourDescriptor {
    BehaviourDescriptor {
        model: MATERIAL_HANDLING_MODEL.to_owned(),
        version: MATERIAL_HANDLING_VERSION,
        kind,
        modelled: true,
        dependencies: vec!["fields".to_owned(), "slots".to_owned(), "chute".to_owned()],
    }
}

fn passive_descriptor() -> BehaviourDescriptor {
    BehaviourDescriptor {
        model: "passive".to_owned(),
        version: 1,
        kind: BehaviourKind::Passive,
        modelled: false,
        dependencies: Vec::new(),
    }
}

fn record_state(journal: &mut EffectJournal, device: usize, name: &str, after: u64) {
    let key = journal.intern(name);
    let model = journal.intern(MATERIAL_HANDLING_MODEL);
    journal.write_bits(
        EffectActor::Behaviour {
            device,
            model,
            version: MATERIAL_HANDLING_VERSION,
        },
        EffectTarget::BehaviourState { device, key },
        after.saturating_sub(1),
        after,
    );
}

fn field(world: &World, device: usize, name: &str) -> f64 {
    world.devices[device]
        .fields
        .get(name)
        .copied()
        .unwrap_or(0.0)
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

fn digital_valve_on_input(world: &World, network: usize) -> Option<usize> {
    world
        .devices
        .iter()
        .enumerate()
        .find_map(|(index, device)| {
            (device.prefab.starts_with("StructureChuteDigitalValve")
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
