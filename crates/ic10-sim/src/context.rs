use std::collections::{BTreeMap, BTreeSet};

use ic10_core::{
    Document, LineKind, LiteralMacroKind, Span, SymbolKind, parse_literal_macro,
    parse_numeric_literal, stationeers_crc32,
};
use ic10_data::{Device, KnowledgeBase};

use crate::{DeviceSpec, Scenario};

/// A transport-independent identity for an IC program.
///
/// Hosts provide canonical URI strings. Keeping URI parsing out of this crate
/// lets the same context model serve native paths, VS Code remote URIs, tests,
/// and future simulator front ends.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ProgramUri(pub String);

#[derive(Clone, Debug)]
pub struct AnalysisContext {
    pub scenario_uri: String,
    pub scenario_version: i64,
    pub ic_id: String,
    pub program_uri: ProgramUri,
    pub housing: DeviceSpec,
    pub pins: BTreeMap<String, DeviceSpec>,
    pub reachable_devices: Vec<DeviceSpec>,
    pub game_version: Option<String>,
}

impl AnalysisContext {
    pub fn label(&self) -> String {
        let name = if self.housing.name.is_empty() {
            self.ic_id.as_str()
        } else {
            self.housing.name.as_str()
        };
        format!("{name} · {}", self.scenario_uri)
    }

    pub fn device_for_reference<'a>(
        &'a self,
        token: &str,
        document: &Document,
    ) -> Option<&'a DeviceSpec> {
        let resolved = resolve_alias(token, document);
        let resolved = resolved.split(':').next().unwrap_or(resolved);
        if resolved == "db" {
            return Some(&self.housing);
        }
        self.pins.get(resolved)
    }

    pub fn device_for_reference_id<'a>(
        &'a self,
        token: &str,
        document: &Document,
    ) -> Option<&'a DeviceSpec> {
        let reference_id = i32::try_from(resolve_integer(token, document)?).ok()?;
        std::iter::once(&self.housing)
            .chain(self.reachable_devices.iter())
            .find(|device| device.reference_id == Some(reference_id))
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScenarioIndex {
    scenarios: BTreeMap<String, (i64, Vec<AnalysisContext>)>,
    programs: BTreeMap<ProgramUri, Vec<AnalysisContext>>,
}

impl ScenarioIndex {
    pub fn update<F>(
        &mut self,
        scenario_uri: String,
        version: i64,
        scenario: Scenario,
        resolve_program: F,
        knowledge: &KnowledgeBase,
    ) where
        F: Fn(&str) -> ProgramUri,
    {
        if self
            .scenarios
            .get(&scenario_uri)
            .is_some_and(|(cached_version, _)| *cached_version >= version)
        {
            return;
        }
        self.remove(&scenario_uri);
        let contexts = build_contexts(
            &scenario_uri,
            version,
            &scenario,
            resolve_program,
            knowledge,
        );
        for context in &contexts {
            self.programs
                .entry(context.program_uri.clone())
                .or_default()
                .push(context.clone());
        }
        self.scenarios.insert(scenario_uri, (version, contexts));
    }

    pub fn remove(&mut self, scenario_uri: &str) {
        let Some((_, old)) = self.scenarios.remove(scenario_uri) else {
            return;
        };
        for context in old {
            if let Some(contexts) = self.programs.get_mut(&context.program_uri) {
                contexts.retain(|candidate| {
                    candidate.scenario_uri != context.scenario_uri
                        || candidate.ic_id != context.ic_id
                });
                if contexts.is_empty() {
                    self.programs.remove(&context.program_uri);
                }
            }
        }
    }

    pub fn contexts(&self, program_uri: &ProgramUri) -> &[AnalysisContext] {
        self.programs
            .get(program_uri)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

fn build_contexts<F>(
    scenario_uri: &str,
    version: i64,
    scenario: &Scenario,
    resolve_program: F,
    knowledge: &KnowledgeBase,
) -> Vec<AnalysisContext>
where
    F: Fn(&str) -> ProgramUri,
{
    let devices: BTreeMap<_, _> = scenario
        .devices
        .iter()
        .map(|device| (device.id.as_str(), device))
        .collect();
    let networks: BTreeMap<_, _> = scenario
        .networks
        .iter()
        .map(|network| (network.id.as_str(), network))
        .collect();
    let mut contexts = Vec::new();
    for housing in &scenario.devices {
        let program = if let Some(program_id) = &housing.program {
            scenario
                .programs
                .iter()
                .find(|program| {
                    &program.id == program_id
                        && program.language == crate::scenario::ProgramLanguage::Ic10
                })
                .map(|program| &program.path)
        } else {
            housing.ic.as_ref().and_then(|ic| ic.program.as_ref())
        };
        let Some(program) = program else { continue };
        let data_networks = data_networks(housing, knowledge)
            .filter_map(|connection| housing.connections.get(&connection.to_string()))
            .filter(|network| {
                networks.get(network.as_str()).is_some_and(|network| {
                    network.kind.eq_ignore_ascii_case("cable")
                        && matches!(
                            network.cable_role.as_deref().unwrap_or("powerAndData"),
                            "data" | "powerAndData"
                        )
                })
            })
            .cloned()
            .collect::<BTreeSet<_>>();
        let reachable_devices = scenario
            .devices
            .iter()
            .filter(|device| {
                knowledge
                    .device_by_name(&device.prefab)
                    .is_some_and(|metadata| {
                        device.connections.iter().any(|(connection, network)| {
                            let Some(connection) = connection
                                .parse::<usize>()
                                .ok()
                                .and_then(|index| metadata.connections.get(index))
                            else {
                                return false;
                            };
                            connection
                                .connection_type
                                .as_str()
                                .is_some_and(|kind| kind.to_ascii_lowercase().contains("data"))
                                && data_networks.contains(network)
                        })
                    })
            })
            .cloned()
            .collect();
        let pins = housing
            .ic
            .iter()
            .flat_map(|ic| &ic.pins)
            .filter_map(|(pin, id)| {
                devices
                    .get(id.as_str())
                    .map(|device| (pin.clone(), (*device).clone()))
            })
            .collect();
        contexts.push(AnalysisContext {
            scenario_uri: scenario_uri.to_owned(),
            scenario_version: version,
            ic_id: housing.id.clone(),
            program_uri: resolve_program(&program.to_string_lossy()),
            housing: housing.clone(),
            pins,
            reachable_devices,
            game_version: scenario.game_version.clone(),
        });
    }
    contexts
}

fn data_networks<'a>(
    housing: &'a DeviceSpec,
    knowledge: &'a KnowledgeBase,
) -> impl Iterator<Item = usize> + 'a {
    knowledge
        .device_by_name(&housing.prefab)
        .into_iter()
        .flat_map(|device| device.connections.iter().enumerate())
        .filter(|(_, connection)| {
            connection
                .connection_type
                .as_str()
                .is_some_and(|kind| kind.to_ascii_lowercase().contains("data"))
        })
        .map(|(index, _)| index)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextDiagnostic {
    pub span: Span,
    pub code: &'static str,
    pub message: String,
    pub target: Option<EnvironmentTarget>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvironmentTarget {
    pub scenario_uri: String,
    pub ic_id: String,
    pub device_id: Option<String>,
    pub property: Option<String>,
}

pub fn validate_context(
    document: &Document,
    context: &AnalysisContext,
    knowledge: &KnowledgeBase,
) -> Vec<ContextDiagnostic> {
    let mut diagnostics = Vec::new();
    let prefix = format!("[{}]", context.label());
    if let Some(version) = &context.game_version
        && version != &knowledge.devices.game_version
    {
        diagnostics.push(ContextDiagnostic {
            span: Span::default(),
            code: "environment-game-version",
            message: format!(
                "{prefix} Environment targets Stationeers {version}; bundled official data is {}. Environment field assumptions are not treated as authoritative.",
                knowledge.devices.game_version
            ),
            target: Some(target(context, None, Some("gameVersion"))),
        });
    }

    for line in document.lines() {
        let LineKind::Instruction { mnemonic, operands } = &line.kind else {
            continue;
        };
        match mnemonic.text.as_str() {
            "l" | "s" | "ls" | "ss" | "lr" | "get" | "put" | "bdnvl" | "bdnvs" | "ld" | "sd" => {
                let device_index = match mnemonic.text.as_str() {
                    "l" | "ls" | "lr" | "get" | "ld" => 1,
                    _ => 0,
                };
                let Some(device_token) = operands.get(device_index) else {
                    continue;
                };
                let resolved_reference = resolve_alias(&device_token.text, document);
                let raw_reference = resolved_reference
                    .split(':')
                    .next()
                    .unwrap_or(resolved_reference);
                let by_reference_id = matches!(mnemonic.text.as_str(), "ld" | "sd");
                let device = if by_reference_id {
                    context.device_for_reference_id(&device_token.text, document)
                } else {
                    context.device_for_reference(&device_token.text, document)
                };
                if device.is_none() && is_direct_device(raw_reference) {
                    diagnostics.push(ContextDiagnostic {
                        span: device_token.span,
                        code: "environment-unassigned-device",
                        message: format!(
                            "{prefix} `{raw_reference}` is not assigned on IC housing `{}`.",
                            context.ic_id
                        ),
                        target: Some(target(context, None, Some(raw_reference))),
                    });
                    continue;
                }
                if device.is_none()
                    && by_reference_id
                    && let Some(reference_id) = resolve_integer(&device_token.text, document)
                {
                    diagnostics.push(ContextDiagnostic {
                        span: device_token.span,
                        code: "environment-unknown-reference-id",
                        message: format!(
                            "{prefix} No reachable device has explicit ReferenceId `{reference_id}`."
                        ),
                        target: Some(target(context, None, Some("devices"))),
                    });
                    continue;
                }
                let Some(device) = device else { continue };
                if device.id != context.housing.id
                    && !context
                        .reachable_devices
                        .iter()
                        .any(|candidate| candidate.id == device.id)
                {
                    diagnostics.push(context_error(
                        context,
                        device,
                        device_token.span,
                        "environment-unreachable-device",
                        format!(
                            "{prefix} {} is assigned to `{raw_reference}` but is not reachable on the housing data cable.",
                            display_name(device)
                        ),
                        Some("connections".to_owned()),
                    ));
                    continue;
                }
                validate_direct_operation(
                    document,
                    context,
                    knowledge,
                    &prefix,
                    mnemonic.text.as_str(),
                    operands,
                    device,
                    &mut diagnostics,
                );
            }
            "lb" | "lbn" | "lbs" | "lbns" | "sb" | "sbn" | "sbs" => {
                validate_batch(
                    document,
                    context,
                    knowledge,
                    &prefix,
                    mnemonic.text.as_str(),
                    operands,
                    &mut diagnostics,
                );
            }
            _ => {}
        }
    }
    diagnostics
}

#[allow(clippy::too_many_arguments)]
fn validate_direct_operation(
    document: &Document,
    context: &AnalysisContext,
    knowledge: &KnowledgeBase,
    prefix: &str,
    mnemonic: &str,
    operands: &[ic10_core::Token],
    device: &DeviceSpec,
    diagnostics: &mut Vec<ContextDiagnostic>,
) {
    let Some(metadata) = knowledge.device_by_name(&device.prefab) else {
        return;
    };
    let device_index = match mnemonic {
        "l" | "ls" | "lr" | "get" | "ld" => 1,
        _ => 0,
    };
    if let Some(device_token) = operands.get(device_index) {
        let resolved = resolve_alias(&device_token.text, document);
        if let Some(connection) = resolved
            .split_once(':')
            .and_then(|(_, suffix)| suffix.parse::<usize>().ok())
        {
            if connection >= metadata.connections.len() {
                diagnostics.push(context_error(
                    context,
                    device,
                    device_token.span,
                    "environment-invalid-connection",
                    format!(
                        "{prefix} {} has no connection `{connection}`.",
                        display_name(device)
                    ),
                    Some(format!("connections.{connection}")),
                ));
            } else if !device.connections.contains_key(&connection.to_string()) {
                diagnostics.push(context_error(
                    context,
                    device,
                    device_token.span,
                    "environment-unattached-connection",
                    format!(
                        "{prefix} {} connection `{connection}` is not attached to a network.",
                        display_name(device)
                    ),
                    Some(format!("connections.{connection}")),
                ));
            }
        }
    }
    let (field_index, slot_index, memory_index, write) = match mnemonic {
        "l" => (Some(2), None, None, false),
        "s" => (Some(1), None, None, true),
        "ls" => (Some(3), Some(2), None, false),
        "ss" => (Some(2), Some(1), None, true),
        "bdnvl" => (Some(1), None, None, false),
        "bdnvs" => (Some(1), None, None, true),
        "ld" => (Some(2), None, None, false),
        "sd" => (Some(1), None, None, true),
        "get" => (None, None, Some(2), false),
        "put" => (None, None, Some(1), true),
        _ => (None, None, None, false),
    };
    if let Some(index) = field_index
        && let Some(field) = operands.get(index)
    {
        if field
            .text
            .strip_prefix("Channel")
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|channel| channel < 8)
        {
            if !operands
                .get(device_index)
                .is_some_and(|device| resolve_alias(&device.text, document).contains(':'))
            {
                diagnostics.push(context_error(
                    context,
                    device,
                    field.span,
                    "environment-channel-needs-connection",
                    format!(
                        "{prefix} `{}` requires a numbered device connection such as `db:0`.",
                        field.text
                    ),
                    Some("connections".to_owned()),
                ));
            }
            return;
        }
        if let Some(slot_index) = slot_index {
            let slot = operands
                .get(slot_index)
                .and_then(|value| resolve_integer(&value.text, document));
            let slot_key = slot.map(|value| value.to_string());
            let slot_metadata = slot_key.as_ref().and_then(|slot| metadata.slots.get(slot));
            if slot.is_some() && slot_metadata.is_none() {
                diagnostics.push(context_error(
                    context,
                    device,
                    operands[slot_index].span,
                    "environment-invalid-slot",
                    format!(
                        "{prefix} {} has no slot `{}`.",
                        display_name(device),
                        slot.unwrap()
                    ),
                    Some(format!("slots.{}", slot.unwrap())),
                ));
            } else if let Some(slot_metadata) = slot_metadata {
                validate_access(
                    context,
                    device,
                    field,
                    &slot_metadata.logic_types,
                    write,
                    prefix,
                    diagnostics,
                    Some(format!("slots.{}.{}", slot.unwrap(), field.text)),
                );
            }
        } else {
            validate_access(
                context,
                device,
                field,
                &metadata.logic_types,
                write,
                prefix,
                diagnostics,
                Some(format!("fields.{}", field.text)),
            );
            if write
                && field.text == "Mode"
                && metadata.logic_types.contains_key("Mode")
                && let Some(value_token) = operands.get(2)
                && let Some(value) = resolve_integer(&value_token.text, document)
                && !metadata
                    .modes
                    .values()
                    .any(|mode| mode.as_i64() == Some(value))
            {
                diagnostics.push(context_error(
                    context,
                    device,
                    value_token.span,
                    "environment-invalid-mode",
                    format!(
                        "{prefix} `{value}` is not a supported mode on {}.",
                        display_name(device)
                    ),
                    Some("fields.Mode".to_owned()),
                ));
            }
        }
    }
    if let Some(index) = memory_index
        && let Some(address) = operands.get(index)
        && let Some(address_value) = resolve_integer(&address.text, document)
    {
        let size = metadata
            .memory
            .as_ref()
            .and_then(|memory| memory.size)
            .unwrap_or(0);
        let required_access = if write { "write" } else { "read" };
        let memory_access = metadata
            .memory
            .as_ref()
            .and_then(|memory| memory.access.as_ref())
            .and_then(serde_json::Value::as_str);
        let exposes_access = memory_access.is_some_and(|access| {
            access.eq_ignore_ascii_case("ReadWrite")
                || access.to_ascii_lowercase().contains(required_access)
        });
        if size > 0 && memory_access.is_some() && !exposes_access {
            diagnostics.push(context_error(
                context,
                device,
                address.span,
                if write {
                    "environment-read-only-memory"
                } else {
                    "environment-write-only-memory"
                },
                format!(
                    "{prefix} Addressable memory is not {required_access}able on {}.",
                    display_name(device)
                ),
                Some(format!("memory.{address_value}")),
            ));
        }
        if address_value < 0 || address_value >= i64::from(size) {
            diagnostics.push(context_error(
                context,
                device,
                address.span,
                "environment-invalid-memory-address",
                format!(
                    "{prefix} {} exposes {} memory address{}; `{address_value}` is invalid.",
                    display_name(device),
                    size,
                    if size == 1 { "" } else { "es" }
                ),
                Some(format!("memory.{address_value}")),
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_access(
    context: &AnalysisContext,
    device: &DeviceSpec,
    field: &ic10_core::Token,
    fields: &BTreeMap<String, ic10_data::LogicAccess>,
    write: bool,
    prefix: &str,
    diagnostics: &mut Vec<ContextDiagnostic>,
    property: Option<String>,
) {
    let Some(access) = fields.get(&field.text) else {
        diagnostics.push(context_error(
            context,
            device,
            field.span,
            "environment-unsupported-field",
            format!(
                "{prefix} {} does not expose logic field `{}` in bundled official data.",
                display_name(device),
                field.text
            ),
            property,
        ));
        return;
    };
    if (write && !access.write) || (!write && !access.read) {
        diagnostics.push(context_error(
            context,
            device,
            field.span,
            if write {
                "environment-read-only-field"
            } else {
                "environment-write-only-field"
            },
            format!(
                "{prefix} `{}` is not {} on {}.",
                field.text,
                if write { "writable" } else { "readable" },
                display_name(device)
            ),
            property,
        ));
    }
}

fn validate_batch(
    document: &Document,
    context: &AnalysisContext,
    knowledge: &KnowledgeBase,
    prefix: &str,
    mnemonic: &str,
    operands: &[ic10_core::Token],
    diagnostics: &mut Vec<ContextDiagnostic>,
) {
    let read = mnemonic.starts_with('l');
    let named = mnemonic.contains("bn");
    let slotted = mnemonic.contains("bs") || mnemonic.ends_with("bns");
    let hash_index = usize::from(read);
    let Some(hash_token) = operands.get(hash_index) else {
        return;
    };
    let Some(prefab_hash) = resolve_hash(&hash_token.text, document, knowledge) else {
        return;
    };
    let mut matches = context
        .reachable_devices
        .iter()
        .filter(|device| {
            knowledge
                .device_by_name(&device.prefab)
                .is_some_and(|metadata| metadata.prefab_hash == prefab_hash)
        })
        .collect::<Vec<_>>();
    if named {
        let Some(name_token) = operands.get(hash_index + 1) else {
            return;
        };
        if let Some(name_hash) = resolve_hash(&name_token.text, document, knowledge) {
            matches.retain(|device| stationeers_crc32(&display_name(device)) as i32 == name_hash);
        }
    }
    if matches.is_empty() {
        diagnostics.push(ContextDiagnostic {
            span: hash_token.span,
            code: "environment-empty-batch",
            message: format!(
                "{prefix} This batch selector matches no devices reachable on the housing data cable."
            ),
            target: Some(target(context, None, Some("devices"))),
        });
        return;
    }
    let field_index = if read {
        if named { 3 } else { 2 }
    } else if named {
        2
    } else {
        1
    } + usize::from(slotted);
    let slot_index = slotted.then_some(if read {
        if named { 3 } else { 2 }
    } else if named {
        2
    } else {
        1
    });
    let Some(field) = operands.get(field_index) else {
        return;
    };
    for device in matches {
        let Some(metadata) = knowledge.device_by_name(&device.prefab) else {
            continue;
        };
        let fields = if let Some(slot_index) = slot_index {
            let Some(slot) = operands
                .get(slot_index)
                .and_then(|token| resolve_integer(&token.text, document))
            else {
                continue;
            };
            let Some(slot_metadata) = metadata.slots.get(&slot.to_string()) else {
                diagnostics.push(context_error(
                    context,
                    device,
                    operands[slot_index].span,
                    "environment-invalid-slot",
                    format!("{prefix} {} has no slot `{slot}`.", display_name(device)),
                    Some(format!("slots.{slot}")),
                ));
                continue;
            };
            &slot_metadata.logic_types
        } else {
            &metadata.logic_types
        };
        validate_access(
            context,
            device,
            field,
            fields,
            !read,
            prefix,
            diagnostics,
            Some(format!("fields.{}", field.text)),
        );
    }
}

pub fn valid_logic_fields<'a>(
    context: &'a AnalysisContext,
    document: &Document,
    device_token: &str,
    write: bool,
    knowledge: &'a KnowledgeBase,
) -> Option<Vec<&'a str>> {
    context
        .device_for_reference(device_token, document)
        .and_then(|device| knowledge.device_by_name(&device.prefab))
        .map(|metadata| {
            metadata
                .logic_types
                .iter()
                .filter(|(_, access)| if write { access.write } else { access.read })
                .map(|(name, _)| name.as_str())
                .collect()
        })
}

pub fn valid_operation_logic_fields<'a>(
    context: &'a AnalysisContext,
    document: &Document,
    mnemonic: &str,
    operands: &[ic10_core::Token],
    knowledge: &'a KnowledgeBase,
) -> Option<Vec<&'a str>> {
    let (device_index, slot_index, write, by_reference_id) = match mnemonic {
        "l" => (1, None, false, false),
        "s" => (0, None, true, false),
        "ls" => (1, Some(2), false, false),
        "ss" => (0, Some(1), true, false),
        "bdnvl" => (0, None, false, false),
        "bdnvs" => (0, None, true, false),
        "ld" => (1, None, false, true),
        "sd" => (0, None, true, true),
        _ => return None,
    };
    let device_token = operands.get(device_index)?;
    let device = if by_reference_id {
        context.device_for_reference_id(&device_token.text, document)?
    } else {
        context.device_for_reference(&device_token.text, document)?
    };
    let metadata = knowledge.device_by_name(&device.prefab)?;
    let fields = if let Some(slot_index) = slot_index {
        let slot = operands
            .get(slot_index)
            .and_then(|token| resolve_integer(&token.text, document))?;
        &metadata.slots.get(&slot.to_string())?.logic_types
    } else {
        &metadata.logic_types
    };
    Some(
        fields
            .iter()
            .filter(|(_, access)| if write { access.write } else { access.read })
            .map(|(name, _)| name.as_str())
            .collect(),
    )
}

fn resolve_alias<'a>(value: &'a str, document: &'a Document) -> &'a str {
    let mut current = value;
    for _ in 0..32 {
        let Some(symbol) = document.symbol(current) else {
            break;
        };
        if symbol.kind != SymbolKind::Alias {
            break;
        }
        let Some(next) = symbol.value.as_deref() else {
            break;
        };
        current = next;
    }
    current
}

fn resolve_hash(value: &str, document: &Document, knowledge: &KnowledgeBase) -> Option<i32> {
    let mut current = resolve_alias(value, document);
    for _ in 0..32 {
        let Some(symbol) = document.symbol(current) else {
            break;
        };
        if symbol.kind != SymbolKind::Define {
            break;
        }
        current = symbol.value.as_deref()?;
    }
    if let Some(literal) = parse_literal_macro(current)
        && literal.kind == LiteralMacroKind::Hash
    {
        return Some(stationeers_crc32(&literal.value) as i32);
    }
    parse_numeric_literal(current)
        .or_else(|| {
            knowledge
                .language
                .constants
                .get(current)
                .and_then(|constant| constant.value.as_f64())
        })
        .filter(|value| value.is_finite() && value.fract() == 0.0)
        .map(|value| value as i32)
}

fn resolve_integer(value: &str, document: &Document) -> Option<i64> {
    let mut current = resolve_alias(value, document);
    for _ in 0..32 {
        let Some(symbol) = document.symbol(current) else {
            break;
        };
        if symbol.kind != SymbolKind::Define {
            break;
        }
        current = symbol.value.as_deref()?;
    }
    parse_numeric_literal(current)
        .filter(|value| value.is_finite() && value.fract() == 0.0)
        .map(|value| value as i64)
}

fn is_direct_device(value: &str) -> bool {
    value == "db"
        || value
            .strip_prefix('d')
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|value| value < 6)
}

fn display_name(device: &DeviceSpec) -> String {
    if device.name.is_empty() {
        device.id.clone()
    } else {
        device.name.clone()
    }
}

fn context_error(
    context: &AnalysisContext,
    device: &DeviceSpec,
    span: Span,
    code: &'static str,
    message: String,
    property: Option<String>,
) -> ContextDiagnostic {
    ContextDiagnostic {
        span,
        code,
        message,
        target: Some(target(
            context,
            Some(device.id.clone()),
            property.as_deref(),
        )),
    }
}

fn target(
    context: &AnalysisContext,
    device_id: Option<String>,
    property: Option<&str>,
) -> EnvironmentTarget {
    EnvironmentTarget {
        scenario_uri: context.scenario_uri.clone(),
        ic_id: context.ic_id.clone(),
        device_id,
        property: property.map(str::to_owned),
    }
}

pub fn context_device_markdown(
    context: &AnalysisContext,
    device: &DeviceSpec,
    metadata: &Device,
    asset_uri: Option<&str>,
) -> String {
    let mut markdown = format!(
        "### {}\n\n**Environment:** `{}`  \n**Prefab:** `{}`  \n**Stable ID:** `{}`",
        display_name(device),
        context.label(),
        device.prefab,
        device.id
    );
    if let Some(image) = &metadata.image
        && let Some(asset_uri) = asset_uri
    {
        markdown.push_str(&format!(
            "\n\n<img src=\"{asset_uri}/{image}\" alt=\"{}\" width=\"96\">",
            display_name(device)
        ));
    }
    if !device.connections.is_empty() {
        markdown.push_str("\n\n**Connections:** ");
        markdown.push_str(
            &device
                .connections
                .iter()
                .map(|(connection, network)| format!("`{connection}` → `{network}`"))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    if !device.fields.is_empty() {
        markdown.push_str("\n\n**Initial fields:** ");
        markdown.push_str(
            &device
                .fields
                .iter()
                .map(|(field, value)| format!("`{field}` = `{value:?}`"))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    markdown
}

#[cfg(test)]
mod tests {
    use ic10_core::Document;
    use ic10_data::KnowledgeBase;

    use super::*;

    fn scenario(program: &str) -> Scenario {
        serde_json::from_str(&format!(
            r#"{{
              "networks":[{{"id":"data","kind":"cable","cableRole":"data"}}],
              "devices":[
                {{"id":"ic","prefab":"StructureCircuitHousing","name":"Main","connections":{{"0":"data"}},"ic":{{"program":"{program}","pins":{{"d0":"light","d1":"lathe"}}}}}},
                {{"id":"light","prefab":"StructureWallLight","name":"Outside Light","referenceId":77,"connections":{{"0":"data"}}}},
                {{"id":"lathe","prefab":"StructureAutolathe","name":"Workshop Lathe","connections":{{"0":"data"}}}}
              ]
            }}"#
        ))
        .unwrap()
    }

    #[test]
    fn indexes_zero_one_and_multiple_contexts_without_choosing() {
        let knowledge = KnowledgeBase::load_embedded().unwrap();
        let mut index = ScenarioIndex::default();
        assert!(
            index
                .contexts(&ProgramUri("file:///a.ic10".into()))
                .is_empty()
        );
        index.update(
            "file:///one.ic10sim.json".into(),
            1,
            scenario("a.ic10"),
            |_| ProgramUri("file:///a.ic10".into()),
            &knowledge,
        );
        index.update(
            "vscode-remote://host/root/two.ic10sim.json".into(),
            2,
            scenario("../a.ic10"),
            |_| ProgramUri("file:///a.ic10".into()),
            &knowledge,
        );
        assert_eq!(
            index.contexts(&ProgramUri("file:///a.ic10".into())).len(),
            2
        );
        index.remove("file:///one.ic10sim.json");
        assert_eq!(
            index.contexts(&ProgramUri("file:///a.ic10".into())).len(),
            1
        );
    }

    #[test]
    fn aliases_follow_pins_and_access_matches_device_metadata() {
        let knowledge = KnowledgeBase::load_embedded().unwrap();
        let mut index = ScenarioIndex::default();
        index.update(
            "file:///simulation.ic10sim.json".into(),
            1,
            scenario("main.ic10"),
            |_| ProgramUri("file:///main.ic10".into()),
            &knowledge,
        );
        let document = Document::parse(
            "alias lamp d0\nalias lathe d1\nalias channel d0:99\ns lamp On 1\nl r0 lamp Setting\ns lamp PrefabHash 2\nl r1 d2 On\nl r2 channel Channel0\nls r3 lamp 0 Occupied\nls r4 lathe 0 Occupied\nss lathe 0 Occupied 1\nlb r5 HASH(\"StructureAutolathe\") On Average\nlbn r6 HASH(\"StructureWallLight\") HASH(\"Missing\") On Average\nbdnvl lamp Power done\nbdnvs lamp Power done\nld r7 77 Power\nsd 77 Power 1\ndone:\n",
            &knowledge,
        );
        let diagnostics = validate_context(
            &document,
            &index.contexts(&ProgramUri("file:///main.ic10".into()))[0],
            &knowledge,
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "environment-unsupported-field")
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "environment-read-only-field")
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "environment-unassigned-device")
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "environment-invalid-connection")
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "environment-invalid-slot")
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "environment-empty-batch")
                .count(),
            2
        );
        let fields = valid_logic_fields(
            &index.contexts(&ProgramUri("file:///main.ic10".into()))[0],
            &document,
            "lamp",
            true,
            &knowledge,
        )
        .unwrap();
        assert!(fields.contains(&"On"));
        assert!(!fields.contains(&"Setting"));

        let context = &index.contexts(&ProgramUri("file:///main.ic10".into()))[0];
        let operations = document
            .lines()
            .iter()
            .filter_map(|line| match &line.kind {
                LineKind::Instruction { mnemonic, operands } => {
                    Some((mnemonic.text.as_str(), operands.as_slice()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for (mnemonic, contains, excludes) in [
            ("s", "On", "Power"),
            ("l", "Power", "Setting"),
            ("bdnvl", "Power", "Setting"),
            ("bdnvs", "On", "Power"),
            ("ld", "Power", "Setting"),
            ("sd", "On", "Power"),
        ] {
            let (_, operands) = operations
                .iter()
                .find(|(name, _)| *name == mnemonic)
                .expect("operation");
            let fields =
                valid_operation_logic_fields(context, &document, mnemonic, operands, &knowledge)
                    .expect("known operation fields");
            assert!(
                fields.contains(&contains),
                "{mnemonic} should include {contains}"
            );
            assert!(
                !fields.contains(&excludes),
                "{mnemonic} should exclude {excludes}"
            );
        }
        let (_, ls_operands) = operations
            .iter()
            .find(|(name, operands)| {
                *name == "ls" && operands.get(1).is_some_and(|value| value.text == "lathe")
            })
            .expect("slotted load operation");
        let fields =
            valid_operation_logic_fields(context, &document, "ls", ls_operands, &knowledge)
                .expect("known slot load fields");
        assert!(fields.contains(&"Occupied"));

        let (_, ss_operands) = operations
            .iter()
            .find(|(name, _)| *name == "ss")
            .expect("slotted store operation");
        let fields =
            valid_operation_logic_fields(context, &document, "ss", ss_operands, &knowledge)
                .expect("known slot store fields");
        assert!(
            fields.is_empty(),
            "the autolathe import slot exposes no writable fields"
        );
    }

    #[test]
    fn renamed_scenario_replaces_old_cache_entry() {
        let knowledge = KnowledgeBase::load_embedded().unwrap();
        let mut index = ScenarioIndex::default();
        index.update(
            "file:///old.ic10sim.json".into(),
            1,
            scenario("old.ic10"),
            |_| ProgramUri("file:///old.ic10".into()),
            &knowledge,
        );
        index.remove("file:///old.ic10sim.json");
        index.update(
            "file:///new.ic10sim.json".into(),
            2,
            scenario("new.ic10"),
            |_| ProgramUri("file:///new.ic10".into()),
            &knowledge,
        );
        assert!(
            index
                .contexts(&ProgramUri("file:///old.ic10".into()))
                .is_empty()
        );
        assert_eq!(
            index.contexts(&ProgramUri("file:///new.ic10".into())).len(),
            1
        );
    }

    #[test]
    fn stale_scenario_versions_do_not_replace_the_cache() {
        let knowledge = KnowledgeBase::load_embedded().unwrap();
        let mut index = ScenarioIndex::default();
        index.update(
            "file:///simulation.ic10sim.json".into(),
            10,
            scenario("new.ic10"),
            |_| ProgramUri("file:///new.ic10".into()),
            &knowledge,
        );
        index.update(
            "file:///simulation.ic10sim.json".into(),
            9,
            scenario("old.ic10"),
            |_| ProgramUri("file:///old.ic10".into()),
            &knowledge,
        );
        assert_eq!(
            index.contexts(&ProgramUri("file:///new.ic10".into())).len(),
            1
        );
        assert!(
            index
                .contexts(&ProgramUri("file:///old.ic10".into()))
                .is_empty()
        );
    }
}
