//! Deterministic IC10 deployment builds.
//!
//! The crate deliberately has no CLI, editor, filesystem, or protocol
//! dependencies. A CLI crate can call [`build`] and decide where to write the
//! returned code and JSON sidecars; editor integrations use the same API.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use ic10_core::{Document, LineKind, Severity, SymbolKind, parse_numeric_literal};
use ic10_data::KnowledgeBase;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OptimizationLevel {
    None,
    #[default]
    Readable,
    Compact,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BuildOptions {
    pub optimization: OptimizationLevel,
    /// If supplied, this must exactly match the versioned embedded data.
    pub game_version: Option<String>,
    /// Optional caller-owned source identity used by debugger sidecar readers.
    pub source_path: Option<String>,
    /// An opaque environment name recorded for reproducibility. Environment
    /// validation remains the responsibility of the caller that owns it.
    pub environment: Option<String>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            optimization: OptimizationLevel::Readable,
            game_version: None,
            source_path: None,
            environment: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMapEntry {
    pub generated_line: usize,
    pub source_line: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildMetadata {
    pub source_sha256: String,
    pub tool_version: String,
    pub game_data_version: String,
    pub options: BuildOptions,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialLimit {
    pub name: String,
    pub value: Option<usize>,
    pub unit: String,
    pub source: String,
    pub game_data_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildReport {
    pub source_lines: usize,
    pub generated_lines: usize,
    pub source_bytes: usize,
    pub generated_bytes: usize,
    pub saved_lines: isize,
    pub saved_bytes: isize,
    pub adjusted_relative_branches: usize,
    pub adjusted_absolute_branches: usize,
    pub substituted_defines: usize,
    pub replaced_labels: usize,
    pub shortened_aliases: usize,
    pub limits: Vec<OfficialLimit>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildOutput {
    pub code: String,
    pub source_map: Vec<SourceMapEntry>,
    pub metadata: BuildMetadata,
    pub report: BuildReport,
}

impl BuildOutput {
    pub fn source_map_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(&self.source_map)
    }

    pub fn metadata_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(&self.metadata)
    }

    pub fn report_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(&self.report)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildDiagnostic {
    pub code: String,
    pub message: String,
    pub source_line: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildError {
    pub diagnostics: Vec<BuildDiagnostic>,
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}",
            self.diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )
    }
}

impl std::error::Error for BuildError {}

#[derive(Clone, Debug)]
struct OutputLine {
    source_line: usize,
    text: String,
    mnemonic: Option<String>,
    operands: Vec<String>,
}

/// Builds deployable code entirely in memory. The input is only borrowed and
/// this function performs no filesystem operations.
pub fn build(
    source: &str,
    options: &BuildOptions,
    knowledge: &KnowledgeBase,
) -> Result<BuildOutput, BuildError> {
    if let Some(requested) = options.game_version.as_deref()
        && requested != knowledge.language.game_version
    {
        return Err(error(
            "game-version-mismatch",
            format!(
                "Selected Stationeers version {requested} does not match official generated data {}.",
                knowledge.language.game_version
            ),
            None,
        ));
    }

    let document = Document::parse(source.to_owned(), knowledge);
    let validation: Vec<_> = document
        .diagnostics()
        .iter()
        .filter(|diagnostic| {
            diagnostic.severity == Severity::Error && diagnostic.code != "program-line-limit"
        })
        .map(|diagnostic| BuildDiagnostic {
            code: diagnostic.code.to_owned(),
            message: diagnostic.message.clone(),
            source_line: document
                .line_at_offset(diagnostic.span.start)
                .map(|line| line.number + 1),
        })
        .collect();
    if !validation.is_empty() {
        return Err(BuildError {
            diagnostics: validation,
        });
    }

    if options.optimization == OptimizationLevel::None {
        return finish_exact(source, options, knowledge);
    }

    let raw_lines = source_lines(source);
    let mut removed = BTreeSet::new();
    let mut output = Vec::new();
    let mut substituted_defines = 0;
    let mut replaced_labels = 0;
    let mut label_replaced_lines = BTreeSet::new();
    let mut shortened_aliases = 0;

    for (index, line) in document.lines().iter().enumerate().take(raw_lines.len()) {
        let code = line.comment_span.map_or(raw_lines[index], |span| {
            raw_lines[index]
                .get(..span.start.saturating_sub(line.span.start))
                .unwrap_or(raw_lines[index])
        });
        let code = code.trim_end();
        if code.trim().is_empty() {
            removed.insert(index);
            continue;
        }
        let (mnemonic, operands) = match &line.kind {
            LineKind::Instruction { mnemonic, operands } => (
                Some(mnemonic.text.to_ascii_lowercase()),
                operands.iter().map(|token| token.text.clone()).collect(),
            ),
            LineKind::Label { .. } | LineKind::Empty => (None, Vec::new()),
        };
        output.push(OutputLine {
            source_line: index,
            text: code.to_owned(),
            mnemonic,
            operands,
        });
    }

    if options.optimization == OptimizationLevel::Compact {
        let raw_defines: BTreeMap<_, _> = document
            .symbols()
            .values()
            .filter(|symbol| symbol.kind == SymbolKind::Define)
            .filter_map(|symbol| Some((symbol.name.clone(), symbol.value.clone()?)))
            .collect();
        let defines: BTreeMap<_, _> = raw_defines
            .keys()
            .filter_map(|name| {
                resolve_define(name, &raw_defines).map(|value| (name.clone(), value))
            })
            .collect();
        let private_aliases: Vec<_> = document
            .symbols()
            .values()
            .filter(|symbol| symbol.kind == SymbolKind::Alias && symbol.name.starts_with('_'))
            .map(|symbol| symbol.name.clone())
            .collect();
        let occupied: BTreeSet<_> = document.symbols().keys().cloned().collect();
        let aliases: BTreeMap<_, _> = private_aliases
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let mut candidate = format!("a{index}");
                while occupied.contains(&candidate) {
                    candidate.insert(0, 'a');
                }
                (name, candidate)
            })
            .collect();

        for line in &mut output {
            if line.mnemonic.as_deref() == Some("define")
                && line
                    .operands
                    .first()
                    .is_some_and(|name| defines.contains_key(name))
            {
                removed.insert(line.source_line);
                line.text.clear();
                continue;
            }
            let mut replacements = defines.clone();
            replacements.extend(aliases.clone());
            let changed = replace_tokens(&line.text, &replacements);
            substituted_defines += line
                .operands
                .iter()
                .filter(|operand| defines.contains_key(*operand))
                .count();
            shortened_aliases += line
                .operands
                .iter()
                .filter(|operand| aliases.contains_key(*operand))
                .count();
            line.text = changed;
            line.operands = line
                .operands
                .iter()
                .map(|operand| replacements.get(operand).unwrap_or(operand).clone())
                .collect();
        }
        output.retain(|line| !line.text.is_empty());

        let removable_labels: BTreeSet<_> = document
            .symbols()
            .values()
            .filter(|symbol| {
                symbol.kind == SymbolKind::Label
                    && document.occurrences_for(&symbol.name).all(|occurrence| {
                        occurrence.declaration
                            || document
                                .line_at_offset(occurrence.span.start)
                                .is_some_and(|line| {
                                    matches!(
                                        &line.kind,
                                        LineKind::Instruction { mnemonic, .. }
                                            if ic10_core::is_absolute_branch(&mnemonic.text)
                                    )
                                })
                    })
            })
            .map(|symbol| symbol.name.clone())
            .collect();
        for symbol in document.symbols().values().filter(|symbol| {
            symbol.kind == SymbolKind::Label && removable_labels.contains(&symbol.name)
        }) {
            removed.insert(symbol.declaration_line);
        }
        output.retain(|line| !removed.contains(&line.source_line));
        let line_map = generated_line_map(raw_lines.len(), &output);
        for line in &mut output {
            if line
                .mnemonic
                .as_deref()
                .is_some_and(ic10_core::is_absolute_branch)
                && let Some(target) = line.operands.last()
                && removable_labels.contains(target)
                && let Some(symbol) = document.symbol(target)
            {
                let generated_target = translate_target(symbol.declaration_line, &line_map)
                    .ok_or_else(|| {
                        error(
                            "unmappable-label",
                            format!("Label `{target}` has no generated destination."),
                            Some(line.source_line + 1),
                        )
                    })?;
                let replacements = BTreeMap::from([(target.clone(), generated_target.to_string())]);
                line.text = replace_tokens(&line.text, &replacements);
                line.operands.pop();
                line.operands.push(generated_target.to_string());
                replaced_labels += 1;
                label_replaced_lines.insert(line.source_line);
            }
        }
    }

    let line_map = generated_line_map(raw_lines.len(), &output);
    let topology_changed = output.len() != raw_lines.len();
    let mut adjusted_relative_branches = 0;
    let mut adjusted_absolute_branches = 0;
    for (generated_index, line) in output.iter_mut().enumerate() {
        let Some(mnemonic) = line.mnemonic.as_deref() else {
            continue;
        };
        let relative = mnemonic == "jr" || mnemonic.starts_with("br");
        let absolute = ic10_core::is_absolute_branch(mnemonic);
        if !relative && !absolute {
            continue;
        }
        let Some(token) = line.operands.last().cloned() else {
            continue;
        };
        if absolute
            && (label_replaced_lines.contains(&line.source_line)
                || document
                    .symbol(&token)
                    .is_some_and(|symbol| symbol.kind == SymbolKind::Label))
        {
            continue;
        }
        let Some(value) =
            parse_numeric_literal(&token).filter(|value| value.is_finite() && value.fract() == 0.0)
        else {
            if topology_changed {
                return Err(error(
                    if relative {
                        "unsafe-relative-branch"
                    } else {
                        "unsafe-absolute-branch"
                    },
                    format!(
                        "Cannot preserve {} branch `{mnemonic}` with non-literal target `{token}` while lines are removed.",
                        if relative { "relative" } else { "absolute" },
                    ),
                    Some(line.source_line + 1),
                ));
            }
            continue;
        };
        let original_target = if relative {
            line.source_line as isize + value as isize
        } else {
            value as isize
        };
        if original_target < 0 || original_target as usize >= raw_lines.len() {
            return Err(error(
                "branch-out-of-range",
                format!("Branch target {original_target} is outside the source."),
                Some(line.source_line + 1),
            ));
        }
        let generated_target =
            translate_target(original_target as usize, &line_map).ok_or_else(|| {
                error(
                    "unmappable-branch",
                    "Branch target has no generated destination.".to_owned(),
                    Some(line.source_line + 1),
                )
            })?;
        let adjusted = if relative {
            generated_target as isize - generated_index as isize
        } else {
            generated_target as isize
        };
        if adjusted != value as isize {
            let replacements = BTreeMap::from([(token.clone(), adjusted.to_string())]);
            line.text = replace_last_token(&line.text, &token, &adjusted.to_string());
            line.operands.pop();
            line.operands.push(replacements[&token].clone());
            if relative {
                adjusted_relative_branches += 1;
            } else {
                adjusted_absolute_branches += 1;
            }
        }
    }

    let code = if output.is_empty() {
        String::new()
    } else {
        format!(
            "{}\n",
            output
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    finish(
        source,
        code,
        output.iter().map(|line| line.source_line).collect(),
        options,
        knowledge,
        adjusted_relative_branches,
        adjusted_absolute_branches,
        substituted_defines,
        replaced_labels,
        shortened_aliases,
    )
}

fn finish_exact(
    source: &str,
    options: &BuildOptions,
    knowledge: &KnowledgeBase,
) -> Result<BuildOutput, BuildError> {
    let count = source_lines(source).len();
    finish(
        source,
        source.to_owned(),
        (0..count).collect(),
        options,
        knowledge,
        0,
        0,
        0,
        0,
        0,
    )
}

#[allow(clippy::too_many_arguments)]
fn finish(
    source: &str,
    code: String,
    source_lines: Vec<usize>,
    options: &BuildOptions,
    knowledge: &KnowledgeBase,
    adjusted_relative_branches: usize,
    adjusted_absolute_branches: usize,
    substituted_defines: usize,
    replaced_labels: usize,
    shortened_aliases: usize,
) -> Result<BuildOutput, BuildError> {
    let generated_lines = if code.is_empty() {
        0
    } else {
        code.lines().count()
    };
    let maximum = knowledge.language.architecture.maximum_program_lines;
    if generated_lines > maximum {
        return Err(error(
            "official-program-line-limit",
            format!(
                "Built program has {generated_lines} lines; official generated data for {} limits programs to {maximum}.",
                knowledge.language.game_version
            ),
            source_lines.get(maximum).map(|line| line + 1),
        ));
    }
    let source_line_count = if source.is_empty() {
        0
    } else {
        source.lines().count()
    };
    let source_bytes = source.len();
    let generated_bytes = code.len();
    Ok(BuildOutput {
        code,
        source_map: source_lines
            .into_iter()
            .take(generated_lines)
            .enumerate()
            .map(|(generated, source)| SourceMapEntry {
                generated_line: generated + 1,
                source_line: source + 1,
            })
            .collect(),
        metadata: BuildMetadata {
            source_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
            tool_version: TOOL_VERSION.to_owned(),
            game_data_version: knowledge.language.game_version.clone(),
            options: options.clone(),
        },
        report: BuildReport {
            source_lines: source_line_count,
            generated_lines,
            source_bytes,
            generated_bytes,
            saved_lines: source_line_count as isize - generated_lines as isize,
            saved_bytes: source_bytes as isize - generated_bytes as isize,
            adjusted_relative_branches,
            adjusted_absolute_branches,
            substituted_defines,
            replaced_labels,
            shortened_aliases,
            limits: vec![
                OfficialLimit {
                    name: "programLines".to_owned(),
                    value: Some(maximum),
                    unit: "lines".to_owned(),
                    source: "generated Stationpedia architecture data".to_owned(),
                    game_data_version: knowledge.language.game_version.clone(),
                },
                OfficialLimit {
                    name: "programBytes".to_owned(),
                    value: None,
                    unit: "bytes".to_owned(),
                    source: "unknown: no official generated limit is available".to_owned(),
                    game_data_version: knowledge.language.game_version.clone(),
                },
                OfficialLimit {
                    name: "bytesPerLine".to_owned(),
                    value: None,
                    unit: "bytes".to_owned(),
                    source: "unknown: no official generated limit is available".to_owned(),
                    game_data_version: knowledge.language.game_version.clone(),
                },
            ],
        },
    })
}

fn source_lines(source: &str) -> Vec<&str> {
    if source.is_empty() {
        Vec::new()
    } else {
        source
            .split_terminator('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .collect()
    }
}

fn generated_line_map(source_len: usize, output: &[OutputLine]) -> Vec<Option<usize>> {
    let mut result = vec![None; source_len];
    for (generated, line) in output.iter().enumerate() {
        result[line.source_line] = Some(generated);
    }
    result
}

fn translate_target(target: usize, line_map: &[Option<usize>]) -> Option<usize> {
    line_map.get(target..)?.iter().find_map(|line| *line)
}

fn replace_tokens(source: &str, replacements: &BTreeMap<String, String>) -> String {
    let mut result = String::with_capacity(source.len());
    let mut token = String::new();
    for character in source.chars().chain(std::iter::once(' ')) {
        if character.is_whitespace() {
            if !token.is_empty() {
                result.push_str(replacements.get(&token).unwrap_or(&token));
                token.clear();
            }
            result.push(character);
        } else {
            token.push(character);
        }
    }
    result.pop();
    result
}

fn replace_last_token(source: &str, old: &str, new: &str) -> String {
    let end = source.trim_end().len();
    let start = source[..end].rfind(old).unwrap_or(end);
    format!(
        "{}{}{}",
        &source[..start],
        new,
        &source[start + old.len()..]
    )
}

fn resolve_define(name: &str, defines: &BTreeMap<String, String>) -> Option<String> {
    let mut current = name;
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(current) {
            return None;
        }
        let value = defines.get(current)?;
        match defines.get(value) {
            Some(_) => current = value,
            None => return Some(value.clone()),
        }
    }
}

fn error(code: &str, message: String, source_line: Option<usize>) -> BuildError {
    BuildError {
        diagnostics: vec![BuildDiagnostic {
            code: code.to_owned(),
            message,
            source_line,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn knowledge() -> KnowledgeBase {
        KnowledgeBase::load_embedded().expect("embedded data")
    }

    #[test]
    fn readable_golden_preserves_relative_destinations_and_maps_lines() {
        let source =
            "# heading\nstart:\nmove r0 1 # inline\nbrne r0 0 3\n# skipped\nmove r1 2\nyield\n";
        let built = build(source, &BuildOptions::default(), &knowledge()).expect("build");

        assert_eq!(
            built.code,
            "start:\nmove r0 1\nbrne r0 0 2\nmove r1 2\nyield\n"
        );
        assert_eq!(
            built
                .source_map
                .iter()
                .map(|entry| entry.source_line)
                .collect::<Vec<_>>(),
            vec![2, 3, 4, 6, 7]
        );
        assert_eq!(built.report.adjusted_relative_branches, 1);
    }

    #[test]
    fn compact_golden_substitutes_defines_labels_and_private_aliases() {
        let source = "define Limit 10\nalias _sensor d0\nstart:\nmove r0 Limit\nl r1 _sensor Pressure\nj start\n";
        let built = build(
            source,
            &BuildOptions {
                optimization: OptimizationLevel::Compact,
                ..BuildOptions::default()
            },
            &knowledge(),
        )
        .expect("build");

        assert_eq!(
            built.code,
            "alias a0 d0\nmove r0 10\nl r1 a0 Pressure\nj 1\n"
        );
        assert_eq!(built.report.replaced_labels, 1);
        assert_eq!(built.report.substituted_defines, 1);
        assert!(built.report.shortened_aliases >= 2);
    }

    #[test]
    fn none_is_byte_for_byte_and_metadata_is_reproducible() {
        let source = "move r0 -0 # keep\r\n";
        let options = BuildOptions {
            optimization: OptimizationLevel::None,
            ..BuildOptions::default()
        };
        let first = build(source, &options, &knowledge()).expect("build");
        let second = build(source, &options, &knowledge()).expect("build");
        assert_eq!(first, second);
        assert_eq!(first.code, source);
        assert_eq!(first.metadata.source_sha256.len(), 64);
        assert_eq!(first.report.limits[1].value, None);
    }

    #[test]
    fn refuses_dynamic_relative_offsets_when_topology_changes() {
        let error = build(
            "alias offset r0\n# removed\njr offset\n",
            &BuildOptions::default(),
            &knowledge(),
        )
        .expect_err("unsafe build");
        assert_eq!(error.diagnostics[0].code, "unsafe-relative-branch");
    }

    #[test]
    fn readable_rewrites_literal_absolute_targets() {
        let built = build(
            "# removed\nmove r0 1\nj 3\nyield\n",
            &BuildOptions::default(),
            &knowledge(),
        )
        .expect("build");
        assert_eq!(built.code, "move r0 1\nj 2\nyield\n");
        assert_eq!(built.report.adjusted_absolute_branches, 1);
    }

    #[test]
    fn official_limit_is_hard_only_after_transformation() {
        let source = std::iter::repeat_n("move r0 0", 129)
            .collect::<Vec<_>>()
            .join("\n");
        let error = build(&source, &BuildOptions::default(), &knowledge())
            .expect_err("generated program exceeds official limit");
        assert_eq!(error.diagnostics[0].code, "official-program-line-limit");
    }

    proptest! {
        #[test]
        fn comment_removal_preserves_forward_relative_targets(
            before in 0usize..12,
            removed in 0usize..12,
            after in 1usize..12,
        ) {
            let mut lines = vec!["move r0 0".to_owned(); before];
            let branch_line = lines.len();
            let target = branch_line + removed + 1;
            lines.push(format!("jr {}", target as isize - branch_line as isize));
            lines.extend((0..removed).map(|_| "# comment".to_owned()));
            lines.push("yield".to_owned());
            lines.extend((0..after).map(|_| "move r1 1".to_owned()));
            let source = format!("{}\n", lines.join("\n"));
            let built = build(&source, &BuildOptions::default(), &knowledge()).expect("build");
            let generated_branch = before;
            let offset = built.code.lines().nth(generated_branch)
                .and_then(|line| line.split_whitespace().last())
                .and_then(|value| value.parse::<usize>().ok())
                .expect("literal offset");
            prop_assert_eq!(generated_branch + offset, before + 1);
        }

        #[test]
        fn compact_define_substitution_preserves_integer_values(value in any::<i32>()) {
            let source = format!("define Value {value}\nmove r0 Value\n");
            let built = build(
                &source,
                &BuildOptions {
                    optimization: OptimizationLevel::Compact,
                    ..BuildOptions::default()
                },
                &knowledge(),
            ).expect("compact build");
            let output_value = built.code
                .split_whitespace()
                .last()
                .and_then(|token| token.parse::<i32>().ok());
            prop_assert_eq!(output_value, Some(value));
        }
    }
}
