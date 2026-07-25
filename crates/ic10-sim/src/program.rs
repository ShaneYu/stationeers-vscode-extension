use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use ic10_core::{Document, LineKind, Severity, pack_stationeers_string, parse_literal_macro};
use ic10_data::KnowledgeBase;

#[derive(Clone, Debug)]
pub struct Operation {
    pub line: usize,
    pub mnemonic: String,
    pub operands: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Program {
    pub source_path: PathBuf,
    pub source: String,
    pub operations: BTreeMap<usize, Operation>,
    pub labels: BTreeMap<String, usize>,
    pub defines: BTreeMap<String, String>,
    pub aliases: BTreeMap<String, String>,
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

        Ok(Self {
            source_path,
            source,
            operations,
            labels,
            defines,
            aliases,
        })
    }

    pub fn operation_at_or_after(&self, line: usize) -> Option<&Operation> {
        self.operations.range(line..).next().map(|(_, value)| value)
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

pub fn parse_number(value: &str) -> Option<f64> {
    ic10_core::parse_numeric_literal(value)
}

fn json_number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|value| value as f64))
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
