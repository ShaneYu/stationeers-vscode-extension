//! Typed access to the Stationpedia data generated at development time.
//!
//! The JSON is embedded in the final language-server binary. Users of the VS
//! Code extension never need Stationeers, Python, or a `.env` file at runtime.

use std::collections::BTreeMap;

use serde::Deserialize;

const INSTRUCTIONS_JSON: &str = include_str!("../../../data/generated/instructions.json");
const DEVICES_JSON: &str = include_str!("../../../data/generated/devices.json");
const RESOURCES_JSON: &str = include_str!("../../../data/generated/resources.json");
const HOVER_OVERRIDES_JSON: &str = include_str!("../../../data/overrides/hover.json");

#[derive(Debug)]
pub struct KnowledgeBase {
    pub language: LanguageReference,
    pub devices: DeviceReference,
    pub resources: ResourceReference,
    pub hover: HoverOverrides,
}

impl KnowledgeBase {
    pub fn load_embedded() -> Result<Self, serde_json::Error> {
        Ok(Self {
            language: serde_json::from_str(INSTRUCTIONS_JSON)?,
            devices: serde_json::from_str(DEVICES_JSON)?,
            resources: serde_json::from_str(RESOURCES_JSON)?,
            hover: serde_json::from_str(HOVER_OVERRIDES_JSON)?,
        })
    }

    pub fn instruction(&self, name: &str) -> Option<&Instruction> {
        self.language.instructions.get(name)
    }

    pub fn device_by_name(&self, name: &str) -> Option<&Device> {
        self.devices
            .devices
            .get(name)
            .or_else(|| self.devices.other_logicables.get(name))
    }

    pub fn device_by_hash(&self, prefab_hash: i32) -> Option<&Device> {
        self.all_devices()
            .find(|device| device.prefab_hash == prefab_hash)
    }

    pub fn all_devices(&self) -> impl Iterator<Item = &Device> {
        self.devices
            .devices
            .values()
            .chain(self.devices.other_logicables.values())
    }

    pub fn enum_value(&self, name: &str) -> Option<(&str, &EnumValue)> {
        self.language.enums.iter().find_map(|(enum_name, listing)| {
            listing
                .values
                .get(name)
                .map(|value| (enum_name.as_str(), value))
        })
    }

    pub fn resource_by_name(&self, name: &str) -> Option<&Resource> {
        self.resources.resources.get(name)
    }

    pub fn resource_by_hash(&self, prefab_hash: i32) -> Option<&Resource> {
        self.resources
            .resources
            .values()
            .find(|resource| resource.prefab_hash == prefab_hash)
    }

    pub fn reagent_by_name(&self, name: &str) -> Option<&Reagent> {
        self.resources.reagents.get(name)
    }

    pub fn reagent_by_hash(&self, hash: i32) -> Option<&Reagent> {
        self.resources
            .reagents
            .values()
            .find(|reagent| reagent.hash == hash)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageReference {
    pub schema_version: u32,
    pub game_version: String,
    pub architecture: Architecture,
    pub instructions: BTreeMap<String, Instruction>,
    pub constants: BTreeMap<String, Constant>,
    pub enums: BTreeMap<String, EnumListing>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Architecture {
    pub numeric_storage: String,
    pub general_registers: String,
    pub return_address_register: String,
    pub stack_pointer_register: String,
    pub stack_size: u32,
    pub device_pins: String,
    pub base_device: String,
    pub maximum_program_lines: usize,
    pub maximum_instructions_per_tick: u32,
    pub tick_seconds: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instruction {
    pub category: String,
    pub syntax: String,
    pub description: String,
    pub deprecated: bool,
    pub operands: Vec<Operand>,
}

#[derive(Debug, Deserialize)]
pub struct Operand {
    pub label: String,
    #[serde(rename = "type")]
    pub operand_type: String,
    pub display: String,
}

#[derive(Debug, Deserialize)]
pub struct Constant {
    pub value: serde_json::Value,
    pub description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumListing {
    pub display_name: String,
    pub values: BTreeMap<String, EnumValue>,
}

#[derive(Debug, Deserialize)]
pub struct EnumValue {
    pub value: serde_json::Value,
    pub deprecated: bool,
    pub description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceReference {
    pub schema_version: u32,
    pub game_version: String,
    pub devices: BTreeMap<String, Device>,
    pub other_logicables: BTreeMap<String, Device>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub prefab_name: String,
    pub prefab_hash: i32,
    pub display_name: String,
    pub description: String,
    pub image: Option<String>,
    pub logic_types: BTreeMap<String, LogicAccess>,
    pub slots: BTreeMap<String, DeviceSlot>,
    pub modes: BTreeMap<String, serde_json::Value>,
    pub connections: Vec<DeviceConnection>,
    pub memory: Option<DeviceMemory>,
}

#[derive(Debug, Deserialize)]
pub struct LogicAccess {
    pub read: bool,
    pub write: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSlot {
    pub name: String,
    #[serde(rename = "type")]
    pub slot_type: String,
    pub class: Option<serde_json::Value>,
    pub hash: Option<i32>,
    pub logic_types: BTreeMap<String, LogicAccess>,
}

#[derive(Debug, Deserialize)]
pub struct DeviceConnection {
    #[serde(rename = "type")]
    pub connection_type: serde_json::Value,
    pub role: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct DeviceMemory {
    pub size: Option<u32>,
    pub access: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReference {
    pub schema_version: u32,
    pub game_version: String,
    pub resources: BTreeMap<String, Resource>,
    pub reagents: BTreeMap<String, Reagent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub prefab_name: String,
    pub prefab_hash: i32,
    pub display_name: String,
    pub description: String,
    pub image: Option<String>,
    pub kind: String,
    pub slot_class: Option<String>,
    pub slot_class_value: Option<i32>,
    pub sorting_class: Option<String>,
    pub sorting_class_value: Option<i32>,
    pub max_quantity: Option<f64>,
    pub reagents: BTreeMap<String, f64>,
    pub gases: Vec<ResourceGas>,
}

#[derive(Debug, Deserialize)]
pub struct ResourceGas {
    #[serde(rename = "type")]
    pub gas_type: String,
    pub quantity: f64,
    pub temperature: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reagent {
    pub name: String,
    pub id: i32,
    pub hash: i32,
    pub unit: String,
    pub is_organic: bool,
    pub sources: BTreeMap<String, f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverOverrides {
    pub schema_version: u32,
    pub colors: BTreeMap<String, HoverColor>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct HoverColor {
    pub value: i32,
    pub foreground: String,
    pub background: String,
}

#[cfg(test)]
mod tests {
    use super::KnowledgeBase;

    #[test]
    fn embedded_data_is_valid_and_cross_versioned() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data should deserialize");

        assert_eq!(knowledge.language.schema_version, 1);
        assert_eq!(knowledge.devices.schema_version, 1);
        assert_eq!(knowledge.resources.schema_version, 1);
        assert_eq!(knowledge.hover.schema_version, 1);
        assert_eq!(
            knowledge.language.game_version,
            knowledge.devices.game_version
        );
        assert_eq!(
            knowledge.language.game_version,
            knowledge.resources.game_version
        );
        assert!(knowledge.language.instructions.len() > 100);
        assert!(knowledge.devices.devices.len() > 400);
        assert!(knowledge.resources.resources.len() > 700);
        assert!(
            knowledge
                .resource_by_name("ItemIntegratedCircuit10")
                .is_some()
        );
        assert_eq!(knowledge.resources.reagents.len(), 46);
        assert_eq!(knowledge.hover.colors.len(), 12);
    }

    #[test]
    fn indexes_prefabs_by_name_and_hash() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data should deserialize");
        let device = knowledge
            .device_by_name("StructureAccessBridge")
            .expect("known device should exist");

        assert_eq!(device.prefab_hash, 1_298_920_475);
        assert_eq!(
            knowledge
                .device_by_hash(device.prefab_hash)
                .map(|value| value.prefab_name.as_str()),
            Some("StructureAccessBridge")
        );
    }

    #[test]
    fn indexes_resources_and_reagents_as_distinct_hash_domains() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data should deserialize");
        let ingot = knowledge
            .resource_by_name("ItemIronIngot")
            .expect("iron ingot should exist");
        let reagent = knowledge
            .reagent_by_name("Iron")
            .expect("iron reagent should exist");

        assert_eq!(ingot.prefab_hash, -1_301_215_609);
        assert_eq!(ingot.slot_class.as_deref(), Some("Ingot"));
        assert_eq!(ingot.slot_class_value, Some(19));
        assert_eq!(ingot.sorting_class.as_deref(), Some("Resources"));
        assert_eq!(ingot.sorting_class_value, Some(3));
        assert_eq!(reagent.hash, -666_742_878);
        assert_ne!(ingot.prefab_hash, reagent.hash);
        assert_eq!(
            knowledge
                .resource_by_hash(ingot.prefab_hash)
                .map(|resource| resource.prefab_name.as_str()),
            Some("ItemIronIngot")
        );
        assert_eq!(
            knowledge
                .reagent_by_hash(reagent.hash)
                .map(|value| value.name.as_str()),
            Some("Iron")
        );
    }

    #[test]
    fn loads_hand_maintained_hover_palette_outside_generated_data() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data should deserialize");
        let blue = knowledge
            .hover
            .colors
            .get("Blue")
            .expect("blue hover style should exist");

        assert_eq!(blue.value, 0);
        assert_eq!(blue.background, "#2563EB80");
        assert!(knowledge.hover.keywords.iter().any(|value| value == "jal"));
    }
}
