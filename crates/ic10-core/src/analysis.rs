use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use ic10_data::KnowledgeBase;

use crate::{
    AnalysisOptions, Diagnostic, LineKind, ParsedLine, ProgramBudget, Severity, Span, Symbol,
    SymbolKind, SymbolOccurrence, UnusedDiagnosticLevel, is_absolute_branch, is_identifier,
    is_register, parse_literal_macro,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperandKind {
    Register,
    RegisterOrNumber,
    DeviceRegisterOrId,
    LogicType,
    DeviceHash,
    LogicSlotType,
    SlotIndex,
    BatchMode,
    Integer,
    RegisterOrId,
    Device,
    NameHash,
    String,
    Number,
    RegisterOrDevice,
    ReagentMode,
}

impl OperandKind {
    pub fn from_generated(value: &str) -> Option<Self> {
        Some(match value {
            "r?" => Self::Register,
            "r?|num" => Self::RegisterOrNumber,
            "d?|r?|id" => Self::DeviceRegisterOrId,
            "logicType" => Self::LogicType,
            "deviceHash" => Self::DeviceHash,
            "logicSlotType" => Self::LogicSlotType,
            "slotIndex" => Self::SlotIndex,
            "batchMode" => Self::BatchMode,
            "int" => Self::Integer,
            "r?|id" => Self::RegisterOrId,
            "d?" => Self::Device,
            "nameHash" => Self::NameHash,
            "str" => Self::String,
            "num" => Self::Number,
            "r?|d?" => Self::RegisterOrDevice,
            "reagentMode" => Self::ReagentMode,
            _ => return None,
        })
    }

    fn description(self) -> &'static str {
        match self {
            Self::Register => "a writable register",
            Self::RegisterOrNumber => "a register or numeric value",
            Self::DeviceRegisterOrId => "a device, register, or reference id",
            Self::LogicType => "a logic type",
            Self::DeviceHash => "a device hash",
            Self::LogicSlotType => "a slot logic type",
            Self::SlotIndex => "a non-negative slot index",
            Self::BatchMode => "a batch mode",
            Self::Integer => "an integer or branch label",
            Self::RegisterOrId => "a register or reference id",
            Self::Device => "a device reference",
            Self::NameHash => "a name hash",
            Self::String => "an identifier",
            Self::Number => "a numeric value",
            Self::RegisterOrDevice => "a register or device reference",
            Self::ReagentMode => "a reagent mode",
        }
    }
}

pub(super) struct AnalysisResult {
    pub diagnostics: Vec<Diagnostic>,
    pub occurrences: Vec<SymbolOccurrence>,
    pub budget: ProgramBudget,
}

pub(super) fn analyze(
    lines: &[ParsedLine],
    symbols: &BTreeMap<String, Symbol>,
    knowledge: &KnowledgeBase,
    options: AnalysisOptions,
) -> AnalysisResult {
    let mut diagnostics = Vec::new();
    let occurrences = collect_occurrences(lines, symbols);

    validate_symbol_cycles(symbols, &mut diagnostics);
    validate_operands(lines, symbols, knowledge, &mut diagnostics);
    add_unused_symbol_diagnostics(symbols, &occurrences, options, &mut diagnostics);
    analyze_control_flow(lines, symbols, knowledge, options, &mut diagnostics);
    analyze_straight_line_values(lines, symbols, knowledge, &mut diagnostics);

    let physical_lines = if lines.len() == 1
        && lines[0].span == Span::default()
        && matches!(lines[0].kind, LineKind::Empty)
    {
        0
    } else if lines.last().is_some_and(|line| {
        line.span.start == line.span.end && matches!(line.kind, LineKind::Empty)
    }) {
        lines.len().saturating_sub(1)
    } else {
        lines.len()
    };
    let program_lines = lines
        .iter()
        .take(physical_lines)
        .filter(|line| !matches!(line.kind, LineKind::Empty))
        .count();
    let maximum_program_lines = knowledge.language.architecture.maximum_program_lines;
    if physical_lines > maximum_program_lines {
        diagnostics.push(Diagnostic {
            span: lines[maximum_program_lines].span,
            severity: Severity::Warning,
            code: "program-too-long",
            message: format!(
                "IC10 programs are limited to {maximum_program_lines} physical lines; this document has {physical_lines}."
            ),
            unnecessary: false,
        });
    }

    let budget = ProgramBudget {
        physical_lines,
        program_lines,
        maximum_program_lines,
        estimated_operations_per_tick: estimate_operations(lines),
        maximum_operations_per_tick: knowledge
            .language
            .architecture
            .maximum_instructions_per_tick,
    };

    AnalysisResult {
        diagnostics,
        occurrences,
        budget,
    }
}

fn collect_occurrences(
    lines: &[ParsedLine],
    symbols: &BTreeMap<String, Symbol>,
) -> Vec<SymbolOccurrence> {
    let mut occurrences = Vec::new();
    for symbol in symbols.values() {
        occurrences.push(SymbolOccurrence {
            name: symbol.name.clone(),
            kind: symbol.kind,
            span: symbol.span,
            declaration: true,
        });
    }
    for line in lines {
        let LineKind::Instruction { operands, .. } = &line.kind else {
            continue;
        };
        for operand in operands {
            let Some(symbol) = symbols.get(&operand.text) else {
                continue;
            };
            if operand.span == symbol.span {
                continue;
            }
            occurrences.push(SymbolOccurrence {
                name: symbol.name.clone(),
                kind: symbol.kind,
                span: operand.span,
                declaration: false,
            });
        }
    }
    occurrences.sort_by_key(|occurrence| occurrence.span.start);
    occurrences
}

fn validate_symbol_cycles(symbols: &BTreeMap<String, Symbol>, diagnostics: &mut Vec<Diagnostic>) {
    for symbol in symbols.values() {
        let mut seen = BTreeSet::new();
        let mut current = symbol;
        while let Some(next_name) = current.value.as_deref() {
            let Some(next) = symbols.get(next_name) else {
                break;
            };
            if !seen.insert(current.name.as_str()) {
                diagnostics.push(Diagnostic {
                    span: symbol.value_span.unwrap_or(symbol.span),
                    severity: Severity::Error,
                    code: "symbol-cycle",
                    message: format!(
                        "`{}` is part of a circular alias or define chain.",
                        symbol.name
                    ),
                    unnecessary: false,
                });
                break;
            }
            current = next;
        }
    }
}

fn validate_operands(
    lines: &[ParsedLine],
    symbols: &BTreeMap<String, Symbol>,
    knowledge: &KnowledgeBase,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for line in lines {
        let LineKind::Instruction { mnemonic, operands } = &line.kind else {
            continue;
        };
        let Some(instruction) = knowledge.instruction(&mnemonic.text) else {
            continue;
        };
        for (operand, generated) in operands.iter().zip(&instruction.operands) {
            let Some(kind) = OperandKind::from_generated(&generated.operand_type) else {
                diagnostics.push(Diagnostic {
                    span: operand.span,
                    severity: Severity::Error,
                    code: "unsupported-operand-kind",
                    message: format!(
                        "The generated operand type `{}` has no validator.",
                        generated.operand_type
                    ),
                    unnecessary: false,
                });
                continue;
            };
            if !operand_is_valid(&operand.text, kind, symbols, knowledge)
                && !looks_incomplete(&operand.text, kind, symbols, knowledge)
            {
                diagnostics.push(Diagnostic {
                    span: operand.span,
                    severity: Severity::Error,
                    code: "invalid-operand",
                    message: format!(
                        "`{}` expects {}; `{}` is not valid here.",
                        generated.label,
                        kind.description(),
                        operand.text
                    ),
                    unnecessary: false,
                });
            }
        }

        if is_branch(&mnemonic.text)
            && let Some(target) = operands.last()
            && is_identifier(&target.text)
            && !symbols.contains_key(&target.text)
            && !knowledge.language.constants.contains_key(&target.text)
            && !is_register(&target.text)
        {
            diagnostics.push(Diagnostic {
                span: target.span,
                severity: Severity::Warning,
                code: "undefined-label",
                message: format!("No label or define named `{}` exists.", target.text),
                unnecessary: false,
            });
        }
    }
}

fn operand_is_valid(
    token: &str,
    kind: OperandKind,
    symbols: &BTreeMap<String, Symbol>,
    knowledge: &KnowledgeBase,
) -> bool {
    match kind {
        OperandKind::Register => resolves_to_register(token, symbols),
        OperandKind::RegisterOrNumber | OperandKind::Number => {
            resolves_to_register(token, symbols)
                || resolves_to_number(token, symbols, knowledge, &mut HashSet::new())
        }
        OperandKind::DeviceRegisterOrId => {
            resolves_to_device(token, symbols)
                || resolves_to_register(token, symbols)
                || resolves_to_number(token, symbols, knowledge, &mut HashSet::new())
        }
        OperandKind::LogicType => {
            resolves_to_number(token, symbols, knowledge, &mut HashSet::new())
                || enum_contains(knowledge, "LogicType", token)
        }
        OperandKind::LogicSlotType => {
            resolves_to_number(token, symbols, knowledge, &mut HashSet::new())
                || enum_contains(knowledge, "LogicSlotType", token)
        }
        OperandKind::BatchMode => {
            resolves_to_number(token, symbols, knowledge, &mut HashSet::new())
                || enum_contains(knowledge, "LogicBatchMethod", token)
        }
        OperandKind::ReagentMode => {
            resolves_to_number(token, symbols, knowledge, &mut HashSet::new())
                || enum_contains(knowledge, "LogicReagentMode", token)
        }
        OperandKind::DeviceHash | OperandKind::NameHash => {
            resolves_to_number(token, symbols, knowledge, &mut HashSet::new())
                || parse_literal_macro(token).is_some()
        }
        OperandKind::SlotIndex => {
            resolves_to_register(token, symbols)
                || resolve_number(token, symbols, knowledge)
                    .is_some_and(|value| value >= 0.0 && value.fract() == 0.0)
        }
        OperandKind::Integer => {
            resolves_to_register(token, symbols)
                || symbols
                    .get(token)
                    .is_some_and(|symbol| symbol.kind == SymbolKind::Label)
                || resolve_number(token, symbols, knowledge)
                    .is_some_and(|value| value.fract() == 0.0)
        }
        OperandKind::RegisterOrId => {
            resolves_to_register(token, symbols)
                || resolves_to_number(token, symbols, knowledge, &mut HashSet::new())
        }
        OperandKind::Device => resolves_to_device(token, symbols),
        OperandKind::String => is_identifier(token),
        OperandKind::RegisterOrDevice => {
            resolves_to_register(token, symbols) || resolves_to_device(token, symbols)
        }
    }
}

fn looks_incomplete(
    token: &str,
    kind: OperandKind,
    symbols: &BTreeMap<String, Symbol>,
    knowledge: &KnowledgeBase,
) -> bool {
    if token.is_empty()
        || matches!(token, "r" | "rr" | "d" | "dr" | "+" | "-" | "$" | "%")
        || (token.starts_with("HASH(") && !token.ends_with(')'))
        || (token.starts_with("STR(") && !token.ends_with(')'))
    {
        return true;
    }
    let mut candidates = symbols.keys().map(String::as_str).collect::<Vec<_>>();
    candidates.extend(knowledge.language.constants.keys().map(String::as_str));
    match kind {
        OperandKind::LogicType => candidates.extend(enum_names(knowledge, "LogicType")),
        OperandKind::LogicSlotType => candidates.extend(enum_names(knowledge, "LogicSlotType")),
        OperandKind::BatchMode => candidates.extend(enum_names(knowledge, "LogicBatchMethod")),
        OperandKind::ReagentMode => candidates.extend(enum_names(knowledge, "LogicReagentMode")),
        _ => {}
    }
    candidates
        .into_iter()
        .any(|candidate| candidate.starts_with(token))
}

fn enum_contains(knowledge: &KnowledgeBase, enum_name: &str, token: &str) -> bool {
    knowledge
        .language
        .enums
        .get(enum_name)
        .is_some_and(|listing| listing.values.contains_key(token))
}

fn enum_names<'a>(knowledge: &'a KnowledgeBase, enum_name: &str) -> Vec<&'a str> {
    knowledge
        .language
        .enums
        .get(enum_name)
        .map(|listing| listing.values.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

fn resolves_to_register(token: &str, symbols: &BTreeMap<String, Symbol>) -> bool {
    if is_register(token) {
        return true;
    }
    resolve_symbol_value(token, symbols).is_some_and(|value| value != token && is_register(value))
}

fn resolves_to_device(token: &str, symbols: &BTreeMap<String, Symbol>) -> bool {
    let value = resolve_symbol_value(token, symbols).unwrap_or(token);
    let base = value.split_once(':').map_or(value, |(base, _)| base);
    if base == "db" {
        return true;
    }
    if base
        .strip_prefix('d')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| number <= 5)
    {
        return true;
    }
    base.strip_prefix('d').is_some_and(is_register)
}

fn resolve_symbol_value<'a>(
    token: &'a str,
    symbols: &'a BTreeMap<String, Symbol>,
) -> Option<&'a str> {
    let mut current = token;
    let mut seen = HashSet::new();
    while let Some(symbol) = symbols.get(current) {
        if !seen.insert(current) {
            return None;
        }
        current = symbol.value.as_deref()?;
    }
    Some(current)
}

fn resolves_to_number(
    token: &str,
    symbols: &BTreeMap<String, Symbol>,
    knowledge: &KnowledgeBase,
    seen: &mut HashSet<String>,
) -> bool {
    if parse_numeric_literal(token).is_some()
        || parse_literal_macro(token).is_some()
        || knowledge.language.constants.contains_key(token)
        || knowledge.enum_value(token).is_some()
    {
        return true;
    }
    let Some(symbol) = symbols.get(token) else {
        return false;
    };
    if symbol.kind == SymbolKind::Label || !seen.insert(token.to_owned()) {
        return false;
    }
    symbol
        .value
        .as_deref()
        .is_some_and(|value| resolves_to_number(value, symbols, knowledge, seen))
}

fn resolve_number(
    token: &str,
    symbols: &BTreeMap<String, Symbol>,
    knowledge: &KnowledgeBase,
) -> Option<f64> {
    if let Some(value) = parse_numeric_literal(token) {
        return Some(value);
    }
    if let Some(literal) = parse_literal_macro(token) {
        return match literal.kind {
            crate::LiteralMacroKind::Hash => Some(crate::stationeers_crc32(&literal.value) as f64),
            crate::LiteralMacroKind::String => crate::pack_stationeers_string(&literal.value)
                .ok()
                .map(|value| value as f64),
        };
    }
    if let Some(constant) = knowledge.language.constants.get(token) {
        return constant.value.as_f64();
    }
    if let Some((_, value)) = knowledge.enum_value(token) {
        return value.value.as_f64();
    }
    let resolved = resolve_symbol_value(token, symbols)?;
    (resolved != token)
        .then(|| resolve_number(resolved, symbols, knowledge))
        .flatten()
}

pub fn parse_numeric_literal(value: &str) -> Option<f64> {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        "nan" => return Some(f64::NAN),
        "inf" | "+inf" | "infinity" | "+infinity" => return Some(f64::INFINITY),
        "-inf" | "-infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let (negative, unsigned) = value
        .strip_prefix('-')
        .map_or((false, value), |value| (true, value));
    let integer = if let Some(hex) = unsigned
        .strip_prefix('$')
        .or_else(|| unsigned.strip_prefix("0x"))
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16).ok()
    } else if let Some(binary) = unsigned
        .strip_prefix('%')
        .or_else(|| unsigned.strip_prefix("0b"))
        .or_else(|| unsigned.strip_prefix("0B"))
    {
        i64::from_str_radix(binary, 2).ok()
    } else {
        return value.parse::<f64>().ok();
    }?;
    Some(if negative {
        -(integer as f64)
    } else {
        integer as f64
    })
}

fn add_unused_symbol_diagnostics(
    symbols: &BTreeMap<String, Symbol>,
    occurrences: &[SymbolOccurrence],
    options: AnalysisOptions,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let severity = match options.unused {
        UnusedDiagnosticLevel::Off => return,
        UnusedDiagnosticLevel::Hint => Severity::Hint,
        UnusedDiagnosticLevel::Warning => Severity::Warning,
    };
    for symbol in symbols.values() {
        if symbol.name.starts_with('_')
            || occurrences
                .iter()
                .any(|occurrence| occurrence.name == symbol.name && !occurrence.declaration)
        {
            continue;
        }
        let kind = match symbol.kind {
            SymbolKind::Label => "label",
            SymbolKind::Define => "define",
            SymbolKind::Alias => "alias",
        };
        diagnostics.push(Diagnostic {
            span: symbol.span,
            severity,
            code: match symbol.kind {
                SymbolKind::Label => "unused-label",
                SymbolKind::Define => "unused-define",
                SymbolKind::Alias => "unused-alias",
            },
            message: format!("The {kind} `{}` is never used.", symbol.name),
            unnecessary: true,
        });
    }
}

fn analyze_control_flow(
    lines: &[ParsedLine],
    symbols: &BTreeMap<String, Symbol>,
    knowledge: &KnowledgeBase,
    options: AnalysisOptions,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if lines.is_empty() {
        return;
    }
    let mut edges = vec![Vec::new(); lines.len()];
    let mut dynamic_target = false;
    for (index, line) in lines.iter().enumerate() {
        let next = (index + 1 < lines.len()).then_some(index + 1);
        let LineKind::Instruction { mnemonic, operands } = &line.kind else {
            if let Some(next) = next {
                edges[index].push(next);
            }
            continue;
        };
        let name = mnemonic.text.as_str();
        if matches!(name, "hcf") {
            continue;
        }
        if !is_branch(name) {
            if let Some(next) = next {
                edges[index].push(next);
            }
            continue;
        }
        let target = operands
            .last()
            .and_then(|operand| branch_target(index, name, &operand.text, symbols, knowledge));
        if target.is_some_and(|target| target >= lines.len())
            && let Some(operand) = operands.last()
        {
            diagnostics.push(Diagnostic {
                span: operand.span,
                severity: Severity::Error,
                code: "invalid-branch-target",
                message: format!(
                    "Branch destination {} is outside this {}-line program.",
                    target.unwrap_or_default(),
                    lines.len()
                ),
                unnecessary: false,
            });
        }
        if let Some(target) = target.filter(|target| *target < lines.len()) {
            edges[index].push(target);
            if is_unconditional_branch(name)
                && target <= index
                && !lines[target..=index].iter().any(is_tick_boundary)
            {
                diagnostics.push(Diagnostic {
                    span: mnemonic.span,
                    severity: Severity::Warning,
                    code: "tight-loop",
                    message: format!(
                        "This loop has no yield or sleep and can consume the {} operation per-tick budget.",
                        knowledge.language.architecture.maximum_instructions_per_tick
                    ),
                    unnecessary: false,
                });
            }
        } else if operands.last().is_some_and(|operand| {
            resolves_to_register(&operand.text, symbols)
                || symbols
                    .get(&operand.text)
                    .is_some_and(|symbol| symbol.kind == SymbolKind::Alias)
        }) {
            dynamic_target = true;
        }
        if (!is_unconditional_branch(name) || is_link_branch(name))
            && let Some(next) = next
        {
            edges[index].push(next);
        }

        if let Some(constant) = constant_branch_result(name, operands, symbols, knowledge) {
            diagnostics.push(Diagnostic {
                span: mnemonic.span,
                severity: Severity::Hint,
                code: "constant-branch",
                message: format!(
                    "This branch condition is always {} for the supplied constants.",
                    if constant { "true" } else { "false" }
                ),
                unnecessary: false,
            });
        }
    }

    analyze_return_address_clobbering(lines, symbols, knowledge, diagnostics);

    if dynamic_target || options.unused == UnusedDiagnosticLevel::Off {
        return;
    }
    let mut reachable = vec![false; lines.len()];
    let mut queue = VecDeque::from([0]);
    while let Some(index) = queue.pop_front() {
        if reachable[index] {
            continue;
        }
        reachable[index] = true;
        queue.extend(edges[index].iter().copied());
    }
    let severity = match options.unused {
        UnusedDiagnosticLevel::Warning => Severity::Warning,
        UnusedDiagnosticLevel::Hint => Severity::Hint,
        UnusedDiagnosticLevel::Off => return,
    };
    for (index, line) in lines.iter().enumerate() {
        if reachable[index] || matches!(line.kind, LineKind::Empty) {
            continue;
        }
        diagnostics.push(Diagnostic {
            span: line.code_span,
            severity,
            code: "unreachable-code",
            message: "This line cannot be reached from the program entry point.".to_owned(),
            unnecessary: true,
        });
    }
}

fn branch_target(
    line: usize,
    mnemonic: &str,
    token: &str,
    symbols: &BTreeMap<String, Symbol>,
    knowledge: &KnowledgeBase,
) -> Option<usize> {
    if let Some(symbol) = symbols.get(token)
        && symbol.kind == SymbolKind::Label
    {
        return Some(symbol.declaration_line);
    }
    let value = resolve_number(token, symbols, knowledge)?;
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    let value = value as isize;
    if mnemonic == "jr" || mnemonic.starts_with("br") {
        line.checked_add_signed(value)
    } else {
        usize::try_from(value).ok()
    }
}

fn constant_branch_result(
    mnemonic: &str,
    operands: &[crate::Token],
    symbols: &BTreeMap<String, Symbol>,
    knowledge: &KnowledgeBase,
) -> Option<bool> {
    let name = mnemonic
        .strip_prefix("br")
        .or_else(|| mnemonic.strip_prefix('b'))?
        .strip_suffix("al")
        .unwrap_or_else(|| {
            mnemonic
                .strip_prefix("br")
                .or_else(|| mnemonic.strip_prefix('b'))
                .unwrap_or_default()
        });
    let condition_operands = operands.get(..operands.len().saturating_sub(1))?;
    let values = condition_operands
        .iter()
        .map(|operand| resolve_number(&operand.text, symbols, knowledge))
        .collect::<Option<Vec<_>>>()?;
    match (name, values.as_slice()) {
        ("eq", [a, b]) => Some(a == b),
        ("ne", [a, b]) => Some(a != b),
        ("gt", [a, b]) => Some(a > b),
        ("ge", [a, b]) => Some(a >= b),
        ("lt", [a, b]) => Some(a < b),
        ("le", [a, b]) => Some(a <= b),
        ("eqz", [a]) => Some(*a == 0.0),
        ("nez", [a]) => Some(*a != 0.0),
        ("gtz", [a]) => Some(*a > 0.0),
        ("gez", [a]) => Some(*a >= 0.0),
        ("ltz", [a]) => Some(*a < 0.0),
        ("lez", [a]) => Some(*a <= 0.0),
        ("nan", [a]) => Some(a.is_nan()),
        _ => None,
    }
}

fn analyze_straight_line_values(
    lines: &[ParsedLine],
    symbols: &BTreeMap<String, Symbol>,
    knowledge: &KnowledgeBase,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut written = HashSet::new();
    let mut unread_writes: HashMap<String, Span> = HashMap::new();
    let mut entry_block = true;
    let stack_size = knowledge.language.architecture.stack_size as i64;
    let mut stack_pointer = Some(0_i64);

    for line in lines {
        let LineKind::Instruction { mnemonic, operands } = &line.kind else {
            if matches!(line.kind, LineKind::Label { .. }) {
                entry_block = false;
                unread_writes.clear();
                stack_pointer = None;
            }
            continue;
        };
        let Some(instruction) = knowledge.instruction(&mnemonic.text) else {
            continue;
        };
        if is_branch(&mnemonic.text) {
            entry_block = false;
            unread_writes.clear();
            stack_pointer = None;
        }

        for (index, operand) in operands.iter().enumerate() {
            let Some(register) = resolved_direct_register(&operand.text, symbols) else {
                continue;
            };
            let is_write = instruction
                .operands
                .get(index)
                .is_some_and(|operand| operand.operand_type == "r?");
            if is_write {
                if let Some(previous) = unread_writes.insert(register.clone(), operand.span) {
                    diagnostics.push(Diagnostic {
                        span: previous,
                        severity: Severity::Hint,
                        code: "unused-register-write",
                        message: format!(
                            "This value written to `{register}` is overwritten before it is read."
                        ),
                        unnecessary: true,
                    });
                }
                written.insert(register);
            } else {
                unread_writes.remove(&register);
                if entry_block
                    && !matches!(register.as_str(), "sp" | "ra")
                    && !written.contains(&register)
                {
                    diagnostics.push(Diagnostic {
                        span: operand.span,
                        severity: Severity::Hint,
                        code: "register-before-write",
                        message: format!("`{register}` is read before this program writes to it."),
                        unnecessary: false,
                    });
                    written.insert(register);
                }
            }
        }

        if matches!(mnemonic.text.as_str(), "div" | "mod")
            && let Some(divisor) = operands.last()
            && resolve_number(&divisor.text, symbols, knowledge) == Some(0.0)
        {
            diagnostics.push(Diagnostic {
                span: divisor.span,
                severity: Severity::Error,
                code: "division-by-zero",
                message: "The divisor is statically zero.".to_owned(),
                unnecessary: false,
            });
        }

        if let Some(address_index) = instruction
            .operands
            .iter()
            .position(|operand| operand.label == "address")
            && let Some(address) = operands.get(address_index)
            && let Some(value) = resolve_number(&address.text, symbols, knowledge)
            && (value.fract() != 0.0
                || value < 0.0
                || (mnemonic.text == "poke" && value >= stack_size as f64))
        {
            diagnostics.push(Diagnostic {
                span: address.span,
                severity: Severity::Error,
                code: "invalid-address",
                message: if mnemonic.text == "poke" {
                    format!(
                        "Stack address must be an integer from 0 through {}.",
                        stack_size - 1
                    )
                } else {
                    "A device memory address must be a non-negative integer.".to_owned()
                },
                unnecessary: false,
            });
        }

        match mnemonic.text.as_str() {
            "move" if operands.first().is_some_and(|operand| operand.text == "sp") => {
                stack_pointer = operands
                    .get(1)
                    .and_then(|operand| resolve_number(&operand.text, symbols, knowledge))
                    .filter(|value| value.fract() == 0.0)
                    .map(|value| value as i64);
            }
            "push" => {
                if let Some(pointer) = stack_pointer.as_mut() {
                    if *pointer >= stack_size {
                        diagnostics.push(Diagnostic {
                            span: mnemonic.span,
                            severity: Severity::Error,
                            code: "stack-overflow",
                            message: format!(
                                "This push exceeds the {stack_size}-value IC10 stack."
                            ),
                            unnecessary: false,
                        });
                    } else {
                        *pointer += 1;
                    }
                }
            }
            "pop" => {
                if let Some(pointer) = stack_pointer.as_mut() {
                    if *pointer <= 0 {
                        diagnostics.push(Diagnostic {
                            span: mnemonic.span,
                            severity: Severity::Error,
                            code: "stack-underflow",
                            message: "This pop reads below the start of the IC10 stack.".to_owned(),
                            unnecessary: false,
                        });
                    } else {
                        *pointer -= 1;
                    }
                }
            }
            "peek" => {
                if stack_pointer == Some(0) {
                    diagnostics.push(Diagnostic {
                        span: mnemonic.span,
                        severity: Severity::Error,
                        code: "stack-underflow",
                        message: "This peek reads an empty IC10 stack.".to_owned(),
                        unnecessary: false,
                    });
                }
            }
            _ => {}
        }
    }
}

fn analyze_return_address_clobbering(
    lines: &[ParsedLine],
    symbols: &BTreeMap<String, Symbol>,
    knowledge: &KnowledgeBase,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (caller_line, line) in lines.iter().enumerate() {
        let LineKind::Instruction { mnemonic, operands } = &line.kind else {
            continue;
        };
        if !is_link_branch(&mnemonic.text) {
            continue;
        }
        let Some(target) = operands
            .last()
            .and_then(|operand| {
                branch_target(
                    caller_line,
                    &mnemonic.text,
                    &operand.text,
                    symbols,
                    knowledge,
                )
            })
            .filter(|target| *target < lines.len())
        else {
            continue;
        };
        let mut saved_return_address = false;
        for nested_line in lines.iter().skip(target) {
            match &nested_line.kind {
                LineKind::Label { .. } if nested_line.number != target => break,
                LineKind::Instruction { mnemonic, operands } => {
                    if mnemonic.text == "push"
                        && operands.first().is_some_and(|operand| {
                            resolved_direct_register(&operand.text, symbols)
                                .is_some_and(|register| register == "ra")
                        })
                    {
                        saved_return_address = true;
                    } else if is_link_branch(&mnemonic.text) && !saved_return_address {
                        diagnostics.push(Diagnostic {
                            span: mnemonic.span,
                            severity: Severity::Warning,
                            code: "return-address-clobber",
                            message: "This nested call can overwrite `ra`; save `ra` before calling and restore it before returning.".to_owned(),
                            unnecessary: false,
                        });
                        break;
                    } else if mnemonic.text == "j"
                        && operands.first().is_some_and(|operand| {
                            resolved_direct_register(&operand.text, symbols)
                                .is_some_and(|register| register == "ra")
                        })
                    {
                        break;
                    }
                }
                LineKind::Empty | LineKind::Label { .. } => {}
            }
        }
    }
}

fn resolved_direct_register(token: &str, symbols: &BTreeMap<String, Symbol>) -> Option<String> {
    let resolved = resolve_symbol_value(token, symbols).unwrap_or(token);
    (is_register(resolved) && !resolved.starts_with("rr")).then(|| resolved.to_owned())
}

fn estimate_operations(lines: &[ParsedLine]) -> Option<u32> {
    if lines.iter().any(|line| {
        matches!(
            &line.kind,
            LineKind::Instruction { mnemonic, .. } if is_branch(&mnemonic.text)
        )
    }) {
        return None;
    }
    let mut current = 0_u32;
    let mut maximum = 0_u32;
    for line in lines {
        let LineKind::Instruction { mnemonic, .. } = &line.kind else {
            continue;
        };
        if matches!(mnemonic.text.as_str(), "alias" | "define") {
            continue;
        }
        current = current.saturating_add(1);
        maximum = maximum.max(current);
        if matches!(mnemonic.text.as_str(), "yield" | "sleep") {
            current = 0;
        } else if mnemonic.text == "hcf" {
            break;
        }
    }
    Some(maximum)
}

fn is_branch(mnemonic: &str) -> bool {
    mnemonic == "jr" || is_absolute_branch(mnemonic) || mnemonic.starts_with("br")
}

fn is_unconditional_branch(mnemonic: &str) -> bool {
    matches!(mnemonic, "j" | "jr")
}

fn is_link_branch(mnemonic: &str) -> bool {
    mnemonic == "jal" || mnemonic.ends_with("al")
}

fn is_tick_boundary(line: &ParsedLine) -> bool {
    matches!(
        &line.kind,
        LineKind::Instruction { mnemonic, .. }
            if matches!(mnemonic.text.as_str(), "yield" | "sleep" | "hcf")
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ic10_data::KnowledgeBase;

    use crate::{
        AnalysisOptions, Document, FormatOptions, Severity, UnusedDiagnosticLevel,
        parse_numeric_literal,
    };

    use super::{OperandKind, operand_is_valid};

    fn knowledge() -> KnowledgeBase {
        KnowledgeBase::load_embedded().expect("embedded data")
    }

    #[test]
    fn every_generated_operand_kind_has_a_validator() {
        let knowledge = knowledge();
        let kinds = knowledge
            .language
            .instructions
            .values()
            .flat_map(|instruction| &instruction.operands)
            .map(|operand| operand.operand_type.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(kinds.len(), 16);
        for kind in kinds {
            assert!(
                OperandKind::from_generated(kind).is_some(),
                "missing validator for {kind}"
            );
        }
    }

    #[test]
    fn every_operand_kind_has_valid_and_invalid_fixtures() {
        let knowledge = knowledge();
        let symbols = Default::default();
        let fixtures = [
            (OperandKind::Register, "r0", "d0"),
            (OperandKind::RegisterOrNumber, "1.5", "d0"),
            (OperandKind::DeviceRegisterOrId, "d0", "not_a_device"),
            (OperandKind::LogicType, "Setting", "not_a_logic_type"),
            (
                OperandKind::DeviceHash,
                "HASH(\"StructureAutolathe\")",
                "d0",
            ),
            (OperandKind::LogicSlotType, "Damage", "not_slot_logic"),
            (OperandKind::SlotIndex, "0", "-1"),
            (OperandKind::BatchMode, "Average", "not_batch_mode"),
            (OperandKind::Integer, "2", "1.5"),
            (OperandKind::RegisterOrId, "r0", "d0"),
            (OperandKind::Device, "d0", "r0"),
            (OperandKind::NameHash, "HASH(\"Main\")", "d0"),
            (OperandKind::String, "Main", "1"),
            (OperandKind::Number, "$ff", "d0"),
            (OperandKind::RegisterOrDevice, "db", "1"),
            (OperandKind::ReagentMode, "Contents", "not_reagent_mode"),
        ];
        for (kind, valid, invalid) in fixtures {
            assert!(
                operand_is_valid(valid, kind, &symbols, &knowledge),
                "{valid} should be valid for {kind:?}"
            );
            assert!(
                !operand_is_valid(invalid, kind, &symbols, &knowledge),
                "{invalid} should be invalid for {kind:?}"
            );
        }
    }

    #[test]
    fn numeric_literals_cover_ic10_hex_binary_and_decimal() {
        assert_eq!(parse_numeric_literal("$ff"), Some(255.0));
        assert_eq!(parse_numeric_literal("-%10"), Some(-2.0));
        assert_eq!(parse_numeric_literal("1.25e2"), Some(125.0));
    }

    #[test]
    fn validates_only_the_invalid_operand_span() {
        let source = "add r0 nope 2\n";
        let document = Document::parse(source, &knowledge());
        let diagnostic = document
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code == "invalid-operand")
            .expect("invalid operand");
        assert_eq!(&source[diagnostic.span.start..diagnostic.span.end], "nope");
    }

    #[test]
    fn tracks_resolved_references_and_unused_declarations() {
        let document = Document::parse(
            "define used 2\ndefine unused 3\nalias sensor d0\nstart:\nadd r0 used 1\nj start\n",
            &knowledge(),
        );
        assert_eq!(document.occurrences_for("used").count(), 2);
        assert_eq!(document.occurrences_for("start").count(), 2);
        assert!(document.diagnostics().iter().any(|diagnostic| {
            diagnostic.code == "unused-define"
                && diagnostic.unnecessary
                && diagnostic.severity == Severity::Hint
        }));
        assert!(
            document
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "unused-alias")
        );
    }

    #[test]
    fn underscore_and_setting_suppress_unused_hints() {
        let underscored = Document::parse("define _reserved 1\n", &knowledge());
        assert!(
            !underscored
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code.starts_with("unused-"))
        );

        let off = Document::parse_with_options(
            "define unused 1\n",
            &knowledge(),
            AnalysisOptions {
                unused: UnusedDiagnosticLevel::Off,
            },
        );
        assert!(
            !off.diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code.starts_with("unused-"))
        );
    }

    #[test]
    fn dynamic_jump_suppresses_unreachable_hints() {
        let document = Document::parse("j r0\nmove r1 1\n", &knowledge());
        assert!(
            !document
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "unreachable-code")
        );
    }

    #[test]
    fn catches_certain_value_and_stack_failures() {
        let document = Document::parse("div r0 1 0\npop r1\npoke 512 1\nj 999\n", &knowledge());
        let codes = document
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        assert!(codes.contains(&"division-by-zero"));
        assert!(codes.contains(&"stack-underflow"));
        assert!(codes.contains(&"invalid-address"));
        assert!(codes.contains(&"invalid-branch-target"));
    }

    #[test]
    fn warns_when_a_nested_call_can_clobber_ra() {
        let document = Document::parse(
            "jal outer\nhcf\nouter:\njal inner\nj ra\ninner:\nj ra\n",
            &knowledge(),
        );
        assert!(
            document
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "return-address-clobber")
        );
    }

    #[test]
    fn formatter_is_idempotent_and_preserves_comments() {
        let knowledge = knowledge();
        let document = Document::parse("  start:  # loop\nadd   r0  1  2\n", &knowledge);
        let once = document.format(FormatOptions::default());
        let twice = Document::parse(once.clone(), &knowledge).format(FormatOptions::default());
        assert_eq!(once, twice);
        assert!(once.contains("# loop"));
        assert!(once.contains("  add r0 1 2"));
    }
}
