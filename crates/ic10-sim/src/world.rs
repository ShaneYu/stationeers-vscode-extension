use std::collections::{BTreeMap, HashMap};
use std::fmt;

use ic10_core::stationeers_crc32;
use ic10_data::KnowledgeBase;

use crate::scenario::{DeviceSpec, NetworkSpec};

#[derive(Clone, Debug)]
pub struct Network {
    pub id: String,
    pub kind: String,
    pub cable_role: String,
    pub channels: [f64; 8],
}

#[derive(Clone, Debug)]
pub struct Device {
    pub id: String,
    pub prefab: String,
    pub prefab_hash: i32,
    pub name: String,
    pub name_hash: i32,
    pub reference_id: i32,
    pub fields: BTreeMap<String, f64>,
    pub slots: BTreeMap<usize, BTreeMap<String, f64>>,
    pub memory: Vec<f64>,
    pub connections: BTreeMap<usize, usize>,
}

#[derive(Debug)]
pub struct World {
    pub networks: Vec<Network>,
    pub devices: Vec<Device>,
    network_ids: HashMap<String, usize>,
    device_ids: HashMap<String, usize>,
    reference_ids: HashMap<i32, usize>,
}

impl World {
    pub fn build(
        networks: &[NetworkSpec],
        devices: &[DeviceSpec],
        knowledge: &KnowledgeBase,
    ) -> Result<Self, WorldError> {
        let mut world = Self {
            networks: Vec::new(),
            devices: Vec::new(),
            network_ids: HashMap::new(),
            device_ids: HashMap::new(),
            reference_ids: HashMap::new(),
        };

        for specification in networks {
            if world.network_ids.contains_key(&specification.id) {
                return Err(WorldError(format!(
                    "duplicate network id `{}`",
                    specification.id
                )));
            }
            let mut channels = [f64::NAN; 8];
            let cable_role = if specification.kind.eq_ignore_ascii_case("cable") {
                let role = specification
                    .cable_role
                    .as_deref()
                    .unwrap_or("powerAndData");
                if !matches!(role, "data" | "power" | "powerAndData") {
                    return Err(WorldError(format!(
                        "cable network `{}` has invalid cableRole `{role}`",
                        specification.id
                    )));
                }
                role.to_owned()
            } else {
                if specification.cable_role.is_some() {
                    return Err(WorldError(format!(
                        "non-cable network `{}` cannot define cableRole",
                        specification.id
                    )));
                }
                String::new()
            };
            for (name, value) in &specification.channels {
                let index = channel_index(name).ok_or_else(|| {
                    WorldError(format!(
                        "network `{}` has invalid channel `{name}`",
                        specification.id
                    ))
                })?;
                channels[index] = value.as_f64().map_err(WorldError)?;
            }
            let index = world.networks.len();
            world.network_ids.insert(specification.id.clone(), index);
            world.networks.push(Network {
                id: specification.id.clone(),
                kind: specification.kind.clone(),
                cable_role,
                channels,
            });
        }

        let mut next_reference_id = 1_i32;
        for specification in devices {
            if world.device_ids.contains_key(&specification.id) {
                return Err(WorldError(format!(
                    "duplicate device id `{}`",
                    specification.id
                )));
            }
            let metadata = knowledge
                .device_by_name(&specification.prefab)
                .ok_or_else(|| {
                    WorldError(format!("unknown device prefab `{}`", specification.prefab))
                })?;
            let reference_id = specification.reference_id.unwrap_or_else(|| {
                while world.reference_ids.contains_key(&next_reference_id) {
                    next_reference_id += 1;
                }
                let value = next_reference_id;
                next_reference_id += 1;
                value
            });
            if world.reference_ids.contains_key(&reference_id) {
                return Err(WorldError(format!(
                    "duplicate ReferenceId `{reference_id}`"
                )));
            }
            let name = if specification.name.is_empty() {
                metadata.display_name.clone()
            } else {
                specification.name.clone()
            };
            let name_hash = stationeers_crc32(&name) as i32;
            let mut fields: BTreeMap<_, _> = metadata
                .logic_types
                .keys()
                .map(|name| (name.clone(), 0.0))
                .collect();
            fields.insert("PrefabHash".to_owned(), metadata.prefab_hash as f64);
            fields.insert("NameHash".to_owned(), name_hash as f64);
            fields.insert("ReferenceId".to_owned(), reference_id as f64);
            for (field, value) in &specification.fields {
                if !metadata.logic_types.contains_key(field) {
                    return Err(WorldError(format!(
                        "device `{}` does not support logic field `{field}`",
                        specification.id
                    )));
                }
                fields.insert(field.clone(), value.as_f64().map_err(WorldError)?);
            }

            let mut slots: BTreeMap<usize, BTreeMap<String, f64>> = BTreeMap::new();
            for (slot, metadata) in &metadata.slots {
                let Ok(index) = slot.parse::<usize>() else {
                    continue;
                };
                slots.insert(
                    index,
                    metadata
                        .logic_types
                        .keys()
                        .map(|name| (name.clone(), 0.0))
                        .collect(),
                );
            }
            for (slot, fields) in &specification.slots {
                let index = slot.parse::<usize>().map_err(|_| {
                    WorldError(format!(
                        "device `{}` has invalid slot `{slot}`",
                        specification.id
                    ))
                })?;
                let slot_metadata = metadata.slots.get(slot).ok_or_else(|| {
                    WorldError(format!(
                        "device `{}` does not have slot `{slot}`",
                        specification.id
                    ))
                })?;
                let values = slots
                    .get_mut(&index)
                    .expect("metadata slot was initialized");
                for (field, value) in fields {
                    if !slot_metadata.logic_types.contains_key(field) {
                        return Err(WorldError(format!(
                            "device `{}` slot `{slot}` does not support logic field `{field}`",
                            specification.id
                        )));
                    }
                    values.insert(field.clone(), value.as_f64().map_err(WorldError)?);
                }
            }
            let metadata_size = metadata
                .memory
                .as_ref()
                .and_then(|memory| memory.size)
                .unwrap_or(0) as usize;
            if metadata_size == 0 && !specification.memory.is_empty() {
                return Err(WorldError(format!(
                    "device `{}` does not expose addressable memory",
                    specification.id
                )));
            }
            let mut memory = vec![0.0; metadata_size];
            for (address, value) in &specification.memory {
                let address = address.parse::<usize>().map_err(|_| {
                    WorldError(format!(
                        "device `{}` has invalid memory address `{address}`",
                        specification.id
                    ))
                })?;
                if address >= metadata_size {
                    return Err(WorldError(format!(
                        "device `{}` memory address {address} exceeds {}",
                        specification.id,
                        metadata_size.saturating_sub(1)
                    )));
                }
                memory[address] = value.as_f64().map_err(WorldError)?;
            }

            let index = world.devices.len();
            world.device_ids.insert(specification.id.clone(), index);
            world.reference_ids.insert(reference_id, index);
            world.devices.push(Device {
                id: specification.id.clone(),
                prefab: specification.prefab.clone(),
                prefab_hash: metadata.prefab_hash,
                name,
                name_hash,
                reference_id,
                fields,
                slots,
                memory,
                connections: BTreeMap::new(),
            });
        }

        for specification in devices {
            let device_index = world
                .device_index(&specification.id)
                .expect("inserted device");
            for (connection, network) in &specification.connections {
                let connection = connection.parse::<usize>().map_err(|_| {
                    WorldError(format!(
                        "device `{}` has invalid connection `{connection}`",
                        specification.id
                    ))
                })?;
                let network_index = world.network_index(network).ok_or_else(|| {
                    WorldError(format!(
                        "device `{}` references unknown network `{network}`",
                        specification.id
                    ))
                })?;
                let metadata = knowledge
                    .device_by_name(&specification.prefab)
                    .expect("validated device prefab");
                let connection_metadata =
                    metadata.connections.get(connection).ok_or_else(|| {
                        WorldError(format!(
                            "device `{}` does not have connection `{connection}`",
                            specification.id
                        ))
                    })?;
                let connection_type =
                    connection_metadata
                        .connection_type
                        .as_str()
                        .ok_or_else(|| {
                            WorldError(format!(
                                "device `{}` connection `{connection}` has an invalid type",
                                specification.id
                            ))
                        })?;
                if !network_accepts_connection(&world.networks[network_index], connection_type) {
                    return Err(WorldError(format!(
                        "device `{}` connection `{connection}` ({connection_type}) is not compatible with network `{network}`",
                        specification.id
                    )));
                }
                world.devices[device_index]
                    .connections
                    .insert(connection, network_index);
            }
        }
        Ok(world)
    }

    pub fn device_index(&self, id: &str) -> Option<usize> {
        self.device_ids.get(id).copied()
    }

    pub fn device_by_reference(&self, reference_id: i32) -> Option<usize> {
        self.reference_ids.get(&reference_id).copied()
    }

    pub fn network_index(&self, id: &str) -> Option<usize> {
        self.network_ids.get(id).copied()
    }

    pub fn devices_on_network(&self, network: usize) -> impl Iterator<Item = usize> + '_ {
        self.devices
            .iter()
            .enumerate()
            .filter(move |(_, device)| device.connections.values().any(|value| *value == network))
            .map(|(index, _)| index)
    }

    pub fn read_field(
        &self,
        device: usize,
        connection: Option<usize>,
        field: &str,
        knowledge: &KnowledgeBase,
    ) -> Result<f64, String> {
        if let Some(channel) = channel_index(field) {
            let connection = connection
                .ok_or_else(|| format!("`{field}` requires a device connection such as `db:0`"))?;
            let network = self.devices[device]
                .connections
                .get(&connection)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "device `{}` connection {connection} is not attached",
                        self.devices[device].id
                    )
                })?;
            if self.networks[network].kind != "cable" {
                return Err(format!(
                    "network `{}` is not a cable network",
                    self.networks[network].id
                ));
            }
            return Ok(self.networks[network].channels[channel]);
        }
        let target = &self.devices[device];
        let metadata = knowledge
            .device_by_name(&target.prefab)
            .ok_or_else(|| format!("unknown prefab `{}`", target.prefab))?;
        let access = metadata
            .logic_types
            .get(field)
            .ok_or_else(|| format!("{} does not expose `{field}`", target.name))?;
        if !access.read {
            return Err(format!("`{field}` is not readable on {}", target.name));
        }
        target
            .fields
            .get(field)
            .copied()
            .ok_or_else(|| format!("{} has no value for `{field}`", target.name))
    }

    pub fn write_field(
        &mut self,
        device: usize,
        connection: Option<usize>,
        field: &str,
        value: f64,
        knowledge: &KnowledgeBase,
    ) -> Result<(), String> {
        if let Some(channel) = channel_index(field) {
            let connection = connection
                .ok_or_else(|| format!("`{field}` requires a device connection such as `db:0`"))?;
            let network = self.devices[device]
                .connections
                .get(&connection)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "device `{}` connection {connection} is not attached",
                        self.devices[device].id
                    )
                })?;
            if self.networks[network].kind != "cable" {
                return Err(format!(
                    "network `{}` is not a cable network",
                    self.networks[network].id
                ));
            }
            self.networks[network].channels[channel] = value;
            return Ok(());
        }
        let target = &mut self.devices[device];
        let metadata = knowledge
            .device_by_name(&target.prefab)
            .ok_or_else(|| format!("unknown prefab `{}`", target.prefab))?;
        let access = metadata
            .logic_types
            .get(field)
            .ok_or_else(|| format!("{} does not expose `{field}`", target.name))?;
        if !access.write {
            return Err(format!("`{field}` is not writable on {}", target.name));
        }
        target.fields.insert(field.to_owned(), value);
        Ok(())
    }
}

fn network_accepts_connection(network: &Network, connection_type: &str) -> bool {
    match connection_type {
        "Data" => {
            network.kind.eq_ignore_ascii_case("cable")
                && matches!(network.cable_role.as_str(), "data" | "powerAndData")
        }
        "Power" => {
            network.kind.eq_ignore_ascii_case("cable")
                && matches!(network.cable_role.as_str(), "power" | "powerAndData")
        }
        "PowerAndData" => network.kind.eq_ignore_ascii_case("cable"),
        "Chute" => network.kind.eq_ignore_ascii_case("chute"),
        "Pipe" => network.kind.eq_ignore_ascii_case("gas"),
        "PipeLiquid" => network.kind.eq_ignore_ascii_case("liquid"),
        _ => false,
    }
}

pub fn channel_index(value: &str) -> Option<usize> {
    value
        .strip_prefix("Channel")
        .or(Some(value))
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value < 8)
}

#[derive(Debug)]
pub struct WorldError(pub String);

impl fmt::Display for WorldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for WorldError {}
