use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use ic10_core::{Document, LineKind, Severity, pack_stationeers_string, parse_literal_macro};
use ic10_data::KnowledgeBase;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Operation {
    pub line: usize,
    pub mnemonic: String,
    pub operands: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Program {
    pub source_path: PathBuf,
    pub debug_source_path: PathBuf,
    pub source: String,
    pub operations: BTreeMap<usize, Operation>,
    pub labels: BTreeMap<String, usize>,
    pub defines: BTreeMap<String, String>,
    pub aliases: BTreeMap<String, String>,
    generated_to_source: BTreeMap<usize, usize>,
}

impl Program {
    pub fn compile(
        source_path: PathBuf,
        source: String,
        knowledge: &KnowledgeBase,
    ) -> Result<Self, CompileError> {
        let document = Document::parse(source.clone(), knowledge);
        let diagnostics: Vec<_> = document
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.severity == Severity::Error)
            .map(|diagnostic| diagnostic.message.clone())
            .collect();
        if !diagnostics.is_empty() {
            return Err(CompileError { diagnostics });
        }

        let mut operations = BTreeMap::new();
        let mut labels = BTreeMap::new();
        let mut defines = BTreeMap::new();
        let mut aliases = BTreeMap::new();

        for line in document.lines() {
            match &line.kind {
                LineKind::Label { name } => {
                    labels.insert(name.text.clone(), line.number);
                }
                LineKind::Instruction { mnemonic, operands } => {
                    let values: Vec<_> = operands.iter().map(|value| value.text.clone()).collect();
                    match mnemonic.text.as_str() {
                        "define" if values.len() >= 2 => {
                            defines.insert(values[0].clone(), values[1].clone());
                        }
                        "alias" if values.len() >= 2 => {
                            aliases.insert(values[0].clone(), values[1].clone());
                        }
                        _ => {
                            operations.insert(
                                line.number,
                                Operation {
                                    line: line.number,
                                    mnemonic: mnemonic.text.to_ascii_lowercase(),
                                    operands: values,
                                },
                            );
                        }
                    }
                }
                LineKind::Empty => {}
            }
        }

        let (debug_source_path, generated_to_source) = load_build_mapping(&source_path);
        Ok(Self {
            source_path,
            debug_source_path,
            source,
            operations,
            labels,
            defines,
            aliases,
            generated_to_source,
        })
    }

    pub fn operation_at_or_after(&self, line: usize) -> Option<&Operation> {
        self.operations.range(line..).next().map(|(_, value)| value)
    }

    pub fn debug_line(&self, generated_line: usize) -> usize {
        self.generated_to_source
            .get(&generated_line)
            .copied()
            .unwrap_or(generated_line)
    }

    pub fn generated_line(&self, source_line: usize) -> Option<usize> {
        self.generated_to_source
            .iter()
            .find_map(|(generated, source)| (*source == source_line).then_some(*generated))
            .or_else(|| self.generated_to_source.is_empty().then_some(source_line))
    }

    pub fn resolve_alias<'a>(&'a self, value: &'a str) -> &'a str {
        let mut current = value;
        for _ in 0..32 {
            let Some(next) = self.aliases.get(current) else {
                break;
            };
            current = next;
        }
        current
    }

    pub fn resolve_number(&self, value: &str, knowledge: &KnowledgeBase) -> Result<f64, String> {
        let mut current = self.resolve_alias(value);
        for _ in 0..32 {
            let Some(next) = self.defines.get(current) else {
                break;
            };
            current = next;
        }
        if let Some(line) = self.labels.get(current) {
            return Ok(*line as f64);
        }
        if let Some(literal) = parse_literal_macro(current) {
            return match literal.kind {
                ic10_core::LiteralMacroKind::Hash => {
                    Ok(ic10_core::stationeers_crc32(&literal.value) as i32 as f64)
                }
                ic10_core::LiteralMacroKind::String => pack_stationeers_string(&literal.value)
                    .map(|value| value as f64)
                    .map_err(|error| format!("invalid STR literal: {error:?}")),
            };
        }
        if let Some(constant) = knowledge.language.constants.get(current)
            && let Some(value) = json_number(&constant.value)
        {
            return Ok(value);
        }
        if let Some((_, value)) = knowledge.enum_value(current)
            && let Some(value) = json_number(&value.value)
        {
            return Ok(value);
        }
        parse_number(current).ok_or_else(|| format!("cannot resolve numeric operand `{value}`"))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildMapEntry {
    generated_line: usize,
    source_line: usize,
}

#[derive(Deserialize)]
struct BuildMetadata {
    options: BuildMetadataOptions,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildMetadataOptions {
    source_path: Option<String>,
}

fn load_build_mapping(artifact: &Path) -> (PathBuf, BTreeMap<usize, usize>) {
    let Some(name) = artifact.file_name().and_then(|name| name.to_str()) else {
        return (artifact.to_owned(), BTreeMap::new());
    };
    let map_path = artifact.with_file_name(format!("{name}.map.json"));
    let metadata_path = artifact.with_file_name(format!("{name}.metadata.json"));
    let entries: Vec<BuildMapEntry> = fs::read_to_string(map_path)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    let metadata_source_path = fs::read_to_string(metadata_path)
        .ok()
        .and_then(|json| serde_json::from_str::<BuildMetadata>(&json).ok())
        .and_then(|metadata| metadata.options.source_path)
        .map(PathBuf::from)
        .unwrap_or_else(|| artifact.to_owned());
    let mapping: BTreeMap<_, _> = entries
        .into_iter()
        .filter_map(|entry| {
            Some((
                entry.generated_line.checked_sub(1)?,
                entry.source_line.checked_sub(1)?,
            ))
        })
        .collect();
    let debug_source_path = if mapping.is_empty() {
        artifact.to_owned()
    } else {
        metadata_source_path
    };
    (debug_source_path, mapping)
}

pub fn parse_number(value: &str) -> Option<f64> {
    ic10_core::parse_numeric_literal(value)
}

fn json_number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|value| value as f64))
        .or_else(|| value.as_str().and_then(parse_number))
}

#[derive(Debug)]
pub struct CompileError {
    pub diagnostics: Vec<String>,
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.diagnostics.join("; "))
    }
}

impl std::error::Error for CompileError {}

#[cfg(test)]
mod source_map_tests {
    use super::*;

    #[test]
    fn loads_generated_to_source_debug_mapping_from_sidecars() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let directory = temporary.path();
        let artifact = directory.join("program.ic10");
        fs::write(&artifact, "move r0 1\n").expect("artifact");
        fs::write(
            directory.join("program.ic10.map.json"),
            r#"[{"generatedLine":1,"sourceLine":4}]"#,
        )
        .expect("map");
        let original = directory.join("source.ic10");
        fs::write(
            directory.join("program.ic10.metadata.json"),
            format!(
                r#"{{"options":{{"sourcePath":{}}}}}"#,
                serde_json::to_string(&original.to_string_lossy()).expect("path JSON")
            ),
        )
        .expect("metadata");

        let knowledge = KnowledgeBase::load_embedded().expect("knowledge");
        let program =
            Program::compile(artifact, "move r0 1\n".to_owned(), &knowledge).expect("program");
        assert_eq!(program.debug_source_path, original);
        assert_eq!(program.debug_line(0), 3);
        assert_eq!(program.generated_line(3), Some(0));
    }
}
