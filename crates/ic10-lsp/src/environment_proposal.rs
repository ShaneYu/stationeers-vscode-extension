use std::collections::{BTreeMap, BTreeSet};

use ic10_core::{
    Document, LineKind, LiteralMacroKind, Span, SymbolKind, parse_literal_macro,
    parse_numeric_literal, stationeers_crc32,
};
use ic10_data::{Device, KnowledgeBase};
use serde::Serialize;
use tower_lsp::lsp_types::Url;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentProposal {
    pub schema_version: u32,
    pub source_uri: Url,
    pub preview_only: bool,
    pub housing: HousingProposal,
    pub devices: Vec<DeviceProposal>,
    pub batch_groups: Vec<BatchGroupProposal>,
    pub networks: Vec<NetworkProposal>,
    pub unresolved: Vec<UnresolvedAssumption>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HousingProposal {
    pub suggested_id: String,
    pub suggested_name: String,
    pub program_uri: Url,
    pub prefab: PrefabCandidate,
    pub required_fields: Vec<FieldRequirement>,
    pub channels: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProposal {
    pub reference: String,
    pub aliases: Vec<String>,
    pub suggested_id: String,
    pub pin: Option<u8>,
    pub candidates: Vec<PrefabCandidate>,
    pub required_fields: Vec<FieldRequirement>,
    pub required_slot_fields: Vec<String>,
    pub requires_memory: bool,
    pub confidence: f32,
    pub reasons: Vec<String>,
    pub evidence: Vec<SourceEvidence>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchGroupProposal {
    pub prefab_hash_expression: String,
    pub name_hash_expression: Option<String>,
    pub suggested_name: Option<String>,
    pub candidates: Vec<PrefabCandidate>,
    pub required_fields: Vec<FieldRequirement>,
    pub confidence: f32,
    pub reasons: Vec<String>,
    pub evidence: Vec<SourceEvidence>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProposal {
    pub suggested_id: String,
    pub kind: String,
    pub cable_role: Option<String>,
    pub participants: Vec<String>,
    pub channels: Vec<u8>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefabCandidate {
    pub prefab_name: String,
    pub prefab_hash: i32,
    pub display_name: String,
    pub confidence: f32,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldRequirement {
    pub name: String,
    pub read: bool,
    pub write: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceEvidence {
    pub line: usize,
    pub start_character: usize,
    pub end_character: usize,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedAssumption {
    pub code: String,
    pub message: String,
    pub reference: Option<String>,
    pub evidence: Option<SourceEvidence>,
}

#[derive(Default)]
struct Requirement {
    aliases: BTreeSet<String>,
    pin: Option<u8>,
    fields: BTreeMap<String, (bool, bool)>,
    slot_fields: BTreeSet<String>,
    memory: bool,
    evidence: Vec<SourceEvidence>,
    reasons: BTreeSet<String>,
}

#[derive(Default)]
struct BatchRequirement {
    name_hash: Option<String>,
    fields: BTreeMap<String, (bool, bool)>,
    evidence: Vec<SourceEvidence>,
}

pub fn propose_environment(
    source_uri: Url,
    document: &Document,
    knowledge: &KnowledgeBase,
) -> EnvironmentProposal {
    let mut references = BTreeMap::<String, Requirement>::new();
    let mut batches = BTreeMap::<String, BatchRequirement>::new();
    let mut unresolved = Vec::new();
    let mut housing_fields = BTreeMap::<String, (bool, bool)>::new();
    let mut channels = BTreeSet::new();
    let aliases = device_aliases(document);

    for (alias, target) in &aliases {
        if let Some(pin) = direct_pin(target) {
            let requirement = references.entry(target.clone()).or_default();
            requirement.pin = Some(pin);
            requirement.aliases.insert(alias.clone());
            requirement
                .reasons
                .insert(format!("Alias `{alias}` binds `{target}`."));
        }
    }

    for line in document.lines() {
        let LineKind::Instruction { mnemonic, operands } = &line.kind else {
            continue;
        };
        let Some(instruction) = knowledge.instruction(&mnemonic.text) else {
            continue;
        };
        let evidence = evidence(document.source(), line.number, line.code_span);
        let device_operand = instruction
            .operands
            .iter()
            .position(|operand| operand.label == "device");
        let logic_operand = instruction.operands.iter().position(|operand| {
            matches!(operand.operand_type.as_str(), "logicType" | "logicSlotType")
        });
        let slot_access = instruction
            .operands
            .iter()
            .any(|operand| operand.operand_type == "slotIndex");
        let memory_access = instruction.category == "memory" && device_operand.is_some();
        let is_write = mnemonic.text.starts_with('s') || mnemonic.text.starts_with("put");
        let is_read = !is_write;

        if let Some(device_index) = device_operand
            && let Some(token) = operands.get(device_index)
        {
            let resolved = resolve_alias(&token.text, &aliases);
            if resolved.starts_with("db") {
                if let Some(logic_index) = logic_operand
                    && let Some(logic) = operands.get(logic_index)
                {
                    if let Some(channel) = logic
                        .text
                        .strip_prefix("Channel")
                        .and_then(|v| v.parse().ok())
                    {
                        channels.insert(channel);
                    } else {
                        add_field(&mut housing_fields, &logic.text, is_read, is_write);
                    }
                }
            } else if direct_pin(&resolved).is_some() {
                let requirement = references.entry(resolved.clone()).or_default();
                requirement.pin = direct_pin(&resolved);
                requirement.evidence.push(evidence.clone());
                if memory_access {
                    requirement.memory = true;
                    requirement
                        .reasons
                        .insert("Program accesses device memory.".to_owned());
                }
                if let Some(logic_index) = logic_operand
                    && let Some(logic) = operands.get(logic_index)
                {
                    if slot_access {
                        requirement.slot_fields.insert(logic.text.clone());
                    } else {
                        add_field(&mut requirement.fields, &logic.text, is_read, is_write);
                    }
                }
            } else {
                unresolved.push(UnresolvedAssumption {
                    code: "dynamic-device-reference".to_owned(),
                    message: format!(
                        "`{}` resolves at runtime; a concrete device cannot be proposed safely.",
                        token.text
                    ),
                    reference: Some(token.text.clone()),
                    evidence: Some(evidence.clone()),
                });
            }
        }

        let hash_index = instruction
            .operands
            .iter()
            .position(|operand| operand.operand_type == "deviceHash");
        if let Some(hash_index) = hash_index
            && let Some(hash) = operands.get(hash_index)
        {
            let group = batches.entry(hash.text.clone()).or_default();
            group.evidence.push(evidence.clone());
            if let Some(name_index) = instruction
                .operands
                .iter()
                .position(|operand| operand.operand_type == "nameHash")
            {
                group.name_hash = operands.get(name_index).map(|value| value.text.clone());
            }
            if let Some(logic_index) = logic_operand
                && let Some(logic) = operands.get(logic_index)
            {
                add_field(&mut group.fields, &logic.text, is_read, is_write);
            }
        }
    }

    let mut devices = references
        .into_iter()
        .map(|(reference, requirement)| {
            let candidates = candidates_for_requirement(&requirement, knowledge);
            let confidence = candidates
                .first()
                .map_or(0.25, |candidate| candidate.confidence);
            if candidates.is_empty() {
                unresolved.push(UnresolvedAssumption {
                    code: "unresolved-device-prefab".to_owned(),
                    message: format!(
                        "No Stationpedia prefab satisfies every observed use of `{reference}`."
                    ),
                    reference: Some(reference.clone()),
                    evidence: requirement.evidence.first().cloned(),
                });
            } else if confidence < 0.9 {
                unresolved.push(UnresolvedAssumption {
                    code: "ambiguous-device-prefab".to_owned(),
                    message: format!(
                        "Choose the intended prefab for `{reference}` from the ranked candidates."
                    ),
                    reference: Some(reference.clone()),
                    evidence: requirement.evidence.first().cloned(),
                });
            }
            DeviceProposal {
                suggested_id: requirement
                    .aliases
                    .iter()
                    .next()
                    .map(|alias| safe_id(alias))
                    .unwrap_or_else(|| format!("device-{reference}")),
                aliases: requirement.aliases.into_iter().collect(),
                pin: requirement.pin,
                required_fields: fields(requirement.fields),
                required_slot_fields: requirement.slot_fields.into_iter().collect(),
                requires_memory: requirement.memory,
                reasons: requirement.reasons.into_iter().collect(),
                evidence: requirement.evidence,
                reference,
                candidates,
                confidence,
            }
        })
        .collect::<Vec<_>>();
    devices.sort_by_key(|device| device.pin);

    let batch_groups = batches
        .into_iter()
        .map(|(expression, requirement)| {
            let resolved = resolve_value(&expression, document);
            let candidates = resolved
                .and_then(|hash| knowledge.device_by_hash(hash))
                .map(|device| vec![exact_candidate(device, "Exact batch prefab hash.")])
                .unwrap_or_else(|| {
                    compatible_devices(&requirement.fields, &BTreeSet::new(), false, knowledge)
                });
            let confidence = candidates
                .first()
                .map_or(0.25, |candidate| candidate.confidence);
            if candidates.is_empty() {
                unresolved.push(UnresolvedAssumption {
                    code: "unresolved-batch-prefab".to_owned(),
                    message: format!(
                        "Batch prefab expression `{expression}` is unresolved or incompatible."
                    ),
                    reference: Some(expression.clone()),
                    evidence: requirement.evidence.first().cloned(),
                });
            } else if confidence < 0.9 {
                unresolved.push(UnresolvedAssumption {
                    code: "ambiguous-batch-prefab".to_owned(),
                    message: format!(
                        "Choose the intended batch prefab for expression `{expression}`."
                    ),
                    reference: Some(expression.clone()),
                    evidence: requirement.evidence.first().cloned(),
                });
            }
            let suggested_name = requirement
                .name_hash
                .as_deref()
                .and_then(|value| resolve_hash_name(value, document));
            if requirement.name_hash.is_some() && suggested_name.is_none() {
                unresolved.push(UnresolvedAssumption {
                    code: "unresolved-batch-name".to_owned(),
                    message: format!(
                        "Name-hash expression `{}` cannot be converted back to a device label.",
                        requirement.name_hash.as_deref().unwrap_or_default()
                    ),
                    reference: requirement.name_hash.clone(),
                    evidence: requirement.evidence.first().cloned(),
                });
            }
            BatchGroupProposal {
                prefab_hash_expression: expression,
                name_hash_expression: requirement.name_hash,
                suggested_name,
                candidates,
                required_fields: fields(requirement.fields),
                confidence,
                reasons: vec![
                    "Batch operations require a shared data network with matching prefabs."
                        .to_owned(),
                ],
                evidence: requirement.evidence,
            }
        })
        .collect::<Vec<_>>();

    let stem = source_uri
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| name.strip_suffix(".ic10"))
        .unwrap_or("controller");
    let housing_id = format!("{}-housing", safe_id(stem));
    let mut participants = vec![housing_id.clone()];
    participants.extend(devices.iter().map(|device| device.suggested_id.clone()));
    for (index, _) in batch_groups.iter().enumerate() {
        participants.push(format!("batch-group-{}", index + 1));
    }
    let networks = (!participants.is_empty() && (participants.len() > 1 || !channels.is_empty()))
        .then(|| NetworkProposal {
            suggested_id: format!("{}-data", safe_id(stem)),
            kind: "cable".to_owned(),
            cable_role: Some("data".to_owned()),
            participants,
            channels: channels.iter().copied().collect(),
            reason: "Device, batch, or db channel operations require a shared data network."
                .to_owned(),
        })
        .into_iter()
        .collect();
    let housing_device = knowledge
        .device_by_name("StructureCircuitHousing")
        .expect("embedded Stationpedia includes the IC housing");
    EnvironmentProposal {
        schema_version: 1,
        source_uri: source_uri.clone(),
        preview_only: true,
        housing: HousingProposal {
            suggested_id: housing_id,
            suggested_name: title(stem),
            program_uri: source_uri,
            prefab: exact_candidate(housing_device, "IC10 programs run in an IC housing."),
            required_fields: fields(housing_fields),
            channels: channels.into_iter().collect(),
        },
        devices,
        batch_groups,
        networks,
        unresolved,
    }
}

fn device_aliases(document: &Document) -> BTreeMap<String, String> {
    document
        .symbols()
        .values()
        .filter(|symbol| symbol.kind == SymbolKind::Alias)
        .filter_map(|symbol| {
            symbol
                .value
                .as_ref()
                .map(|value| (symbol.name.clone(), value.clone()))
        })
        .collect()
}

fn resolve_alias(value: &str, aliases: &BTreeMap<String, String>) -> String {
    let mut current = value;
    for _ in 0..16 {
        let Some(next) = aliases.get(current) else {
            break;
        };
        current = next;
    }
    current.to_owned()
}

fn resolve_value(value: &str, document: &Document) -> Option<i32> {
    let value = document
        .symbol(value)
        .filter(|symbol| symbol.kind == SymbolKind::Define)
        .and_then(|symbol| symbol.value.as_deref())
        .unwrap_or(value);
    if let Some(literal) = parse_literal_macro(value)
        && literal.kind == LiteralMacroKind::Hash
    {
        return Some(stationeers_crc32(&literal.value) as i32);
    }
    parse_numeric_literal(value).and_then(|value| {
        value
            .is_finite()
            .then_some(value as i64)
            .and_then(|value| i32::try_from(value).ok())
    })
}

fn resolve_hash_name(value: &str, document: &Document) -> Option<String> {
    let value = document
        .symbol(value)
        .filter(|symbol| symbol.kind == SymbolKind::Define)
        .and_then(|symbol| symbol.value.as_deref())
        .unwrap_or(value);
    parse_literal_macro(value)
        .filter(|literal| literal.kind == LiteralMacroKind::Hash)
        .map(|literal| literal.value)
}

fn candidates_for_requirement(
    requirement: &Requirement,
    knowledge: &KnowledgeBase,
) -> Vec<PrefabCandidate> {
    let mut candidates = compatible_devices(
        &requirement.fields,
        &requirement.slot_fields,
        requirement.memory,
        knowledge,
    );
    for candidate in &mut candidates {
        let exact_alias = requirement.aliases.iter().any(|alias| {
            candidate.display_name.eq_ignore_ascii_case(alias)
                || candidate.prefab_name.eq_ignore_ascii_case(alias)
        });
        let partial_alias = requirement.aliases.iter().any(|alias| {
            let alias = alias.to_ascii_lowercase();
            candidate.prefab_name.to_ascii_lowercase().contains(&alias)
                || candidate.display_name.to_ascii_lowercase().contains(&alias)
        });
        if exact_alias {
            candidate.confidence = 0.95;
            candidate.reason =
                "Stationpedia access metadata and the source alias match exactly.".to_owned();
        } else if partial_alias {
            candidate.confidence = 0.8;
            candidate.reason =
                "Stationpedia access metadata and the source alias both match.".to_owned();
        }
    }
    candidates.sort_by(|left, right| {
        right
            .confidence
            .total_cmp(&left.confidence)
            .then_with(|| left.prefab_name.cmp(&right.prefab_name))
    });
    candidates.truncate(12);
    candidates
}

fn compatible_devices(
    required_fields: &BTreeMap<String, (bool, bool)>,
    slot_fields: &BTreeSet<String>,
    memory: bool,
    knowledge: &KnowledgeBase,
) -> Vec<PrefabCandidate> {
    let mut devices = knowledge
        .all_devices()
        .filter(|device| {
            (!memory || device.memory.is_some())
                && required_fields.iter().all(|(field, (read, write))| {
                    device
                        .logic_types
                        .get(field)
                        .is_some_and(|access| (!read || access.read) && (!write || access.write))
                })
                && slot_fields.iter().all(|field| {
                    device
                        .slots
                        .values()
                        .any(|slot| slot.logic_types.contains_key(field))
                })
        })
        .collect::<Vec<_>>();
    devices.sort_by(|left, right| left.prefab_name.cmp(&right.prefab_name));
    let confidence = if devices.len() == 1 { 0.9 } else { 0.55 };
    devices
        .into_iter()
        .map(|device| PrefabCandidate {
            prefab_name: device.prefab_name.clone(),
            prefab_hash: device.prefab_hash,
            display_name: device.display_name.clone(),
            confidence,
            reason:
                "Stationpedia access metadata satisfies the observed fields, slots, and memory."
                    .to_owned(),
        })
        .collect()
}

fn exact_candidate(device: &Device, reason: &str) -> PrefabCandidate {
    PrefabCandidate {
        prefab_name: device.prefab_name.clone(),
        prefab_hash: device.prefab_hash,
        display_name: device.display_name.clone(),
        confidence: 1.0,
        reason: reason.to_owned(),
    }
}

fn add_field(fields: &mut BTreeMap<String, (bool, bool)>, name: &str, read: bool, write: bool) {
    fields
        .entry(name.to_owned())
        .and_modify(|access| {
            access.0 |= read;
            access.1 |= write;
        })
        .or_insert((read, write));
}

fn fields(fields: BTreeMap<String, (bool, bool)>) -> Vec<FieldRequirement> {
    fields
        .into_iter()
        .map(|(name, (read, write))| FieldRequirement { name, read, write })
        .collect()
}

fn direct_pin(value: &str) -> Option<u8> {
    value
        .strip_prefix('d')
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|pin| *pin < 6)
}

fn safe_id(value: &str) -> String {
    let id = value
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let normalized = id
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if normalized.is_empty() {
        "controller".to_owned()
    } else {
        normalized
    }
}

fn title(value: &str) -> String {
    value
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + characters.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn evidence(source: &str, line: usize, span: Span) -> SourceEvidence {
    let line_start = source[..span.start.min(source.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    SourceEvidence {
        line,
        start_character: source[line_start..span.start].encode_utf16().count(),
        end_character: source[line_start..span.end].encode_utf16().count(),
        text: source[span.start..span.end].to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use ic10_core::Document;
    use ic10_data::KnowledgeBase;
    use tower_lsp::lsp_types::Url;

    use super::propose_environment;

    fn proposal(source: &str) -> super::EnvironmentProposal {
        let knowledge = KnowledgeBase::load_embedded().unwrap();
        let document = Document::parse(source, &knowledge);
        propose_environment(
            Url::parse("vscode-remote://ssh-remote+station/work/%F0%9F%9A%80.ic10").unwrap(),
            &document,
            &knowledge,
        )
    }

    #[test]
    fn proposes_pin_candidates_from_semantic_alias_and_field_access() {
        let proposal = proposal("alias LED d0\ns LED On 1\n");
        let lamp = &proposal.devices[0];
        assert_eq!(lamp.reference, "d0");
        assert_eq!(lamp.pin, Some(0));
        assert_eq!(lamp.aliases, ["LED"]);
        assert!(
            lamp.candidates
                .iter()
                .any(|candidate| candidate.prefab_name == "StructureDiode"),
            "{:?}",
            lamp.candidates
        );
        assert!(lamp.required_fields[0].write);
        assert!(proposal.unresolved.is_empty());
    }

    #[test]
    fn reports_dynamic_device_references_as_explicit_assumptions() {
        let proposal = proposal("l r1 r0 Temperature\n");
        assert!(proposal.devices.is_empty());
        assert_eq!(proposal.unresolved[0].code, "dynamic-device-reference");
        assert_eq!(proposal.unresolved[0].reference.as_deref(), Some("r0"));
    }

    #[test]
    fn resolves_batch_prefab_and_name_groups_from_document_symbols() {
        let proposal = proposal(
            "define LED 1944485013\ndefine Named HASH(\"warning\")\nlbn r0 LED Named On Average\n",
        );
        let batch = &proposal.batch_groups[0];
        assert_eq!(batch.prefab_hash_expression, "LED");
        assert_eq!(batch.name_hash_expression.as_deref(), Some("Named"));
        assert_eq!(batch.suggested_name.as_deref(), Some("warning"));
        assert_eq!(batch.candidates[0].prefab_name, "StructureDiode");
        assert_eq!(batch.candidates[0].confidence, 1.0);
        assert_eq!(proposal.networks[0].kind, "cable");
    }

    #[test]
    fn records_db_channels_without_misclassifying_them_as_logic_fields() {
        let proposal = proposal("l r0 db:1 Channel0\ns db:1 Channel1 r0\n");
        assert_eq!(proposal.housing.channels, [0, 1]);
        assert!(proposal.housing.required_fields.is_empty());
        assert_eq!(proposal.networks[0].channels, [0, 1]);
    }

    #[test]
    fn proposes_multiple_pinned_devices_on_one_ranked_network() {
        let proposal =
            proposal("alias sensor d0\nalias valve d1\nl r0 sensor Temperature\ns valve On r0\n");
        assert_eq!(proposal.devices.len(), 2);
        assert_eq!(
            proposal
                .devices
                .iter()
                .map(|device| device.pin)
                .collect::<Vec<_>>(),
            [Some(0), Some(1)]
        );
        assert_eq!(proposal.networks.len(), 1);
        assert_eq!(proposal.networks[0].participants.len(), 3);
    }

    #[test]
    fn preserves_remote_uri_and_reports_utf16_evidence_columns() {
        let proposal = proposal("lb r0 HASH(\"🚀\") On Average\n");
        assert_eq!(
            proposal.source_uri.as_str(),
            "vscode-remote://ssh-remote+station/work/%F0%9F%9A%80.ic10"
        );
        assert!(!proposal.housing.suggested_id.contains('/'));
        assert!(proposal.housing.suggested_id.ends_with("-housing"));
        let evidence = &proposal.batch_groups[0].evidence[0];
        assert_eq!(evidence.start_character, 0);
        assert_eq!(
            evidence.end_character,
            "lb r0 HASH(\"🚀\") On Average".encode_utf16().count()
        );
    }
}
