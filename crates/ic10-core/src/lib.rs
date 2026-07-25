//! A small, error-tolerant parser for IC10's line-oriented source format.
//!
//! The parser intentionally has no LSP or editor dependencies. It is cheap
//! enough to run on every document change and can later be replaced internally
//! without changing the language-server protocol layer.

use std::collections::BTreeMap;

use ic10_data::KnowledgeBase;

mod analysis;
mod formatting;

pub use analysis::{OperandKind, parse_numeric_literal};
pub use formatting::{FormatOptions, format_document};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset <= self.end
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiteralMacroKind {
    Hash,
    String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiteralMacro {
    pub kind: LiteralMacroKind,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackedStringError {
    NonAscii,
    TooLong { length: usize },
}

pub const MAX_PACKED_STRING_BYTES: usize = 6;

pub fn parse_literal_macro(token: &str) -> Option<LiteralMacro> {
    let (kind, encoded) = if let Some(value) = token
        .strip_prefix("HASH(\"")
        .and_then(|value| value.strip_suffix("\")"))
    {
        (LiteralMacroKind::Hash, value)
    } else if let Some(value) = token
        .strip_prefix("STR(\"")
        .and_then(|value| value.strip_suffix("\")"))
    {
        (LiteralMacroKind::String, value)
    } else {
        return None;
    };
    Some(LiteralMacro {
        kind,
        value: decode_macro_string(encoded)?,
    })
}

pub fn stationeers_crc32(value: &str) -> u32 {
    let mut checksum = u32::MAX;
    for byte in value.bytes() {
        checksum ^= u32::from(byte);
        for _ in 0..8 {
            checksum = if checksum & 1 == 1 {
                (checksum >> 1) ^ 0xEDB8_8320
            } else {
                checksum >> 1
            };
        }
    }
    !checksum
}

pub fn pack_stationeers_string(value: &str) -> Result<u64, PackedStringError> {
    if !value.is_ascii() {
        return Err(PackedStringError::NonAscii);
    }
    if value.len() > MAX_PACKED_STRING_BYTES {
        return Err(PackedStringError::TooLong {
            length: value.len(),
        });
    }
    Ok(value
        .bytes()
        .fold(0_u64, |packed, byte| (packed << 8) | u64::from(byte)))
}

fn decode_macro_string(value: &str) -> Option<String> {
    let mut decoded = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => match characters.next()? {
                '\\' => decoded.push('\\'),
                '"' => decoded.push('"'),
                'n' => decoded.push('\n'),
                'r' => decoded.push('\r'),
                't' => decoded.push('\t'),
                escaped => decoded.push(escaped),
            },
            '"' => return None,
            other => decoded.push(other),
        }
    }
    Some(decoded)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LineKind {
    Empty,
    Label {
        name: Token,
    },
    Instruction {
        mnemonic: Token,
        operands: Vec<Token>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedLine {
    pub number: usize,
    pub span: Span,
    pub code_span: Span,
    pub comment_span: Option<Span>,
    pub kind: LineKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolKind {
    Label,
    Define,
    Alias,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub value: Option<String>,
    pub value_span: Option<Span>,
    pub declaration_line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolOccurrence {
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub declaration: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub span: Span,
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub unnecessary: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnusedDiagnosticLevel {
    Off,
    Hint,
    Warning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisOptions {
    pub unused: UnusedDiagnosticLevel,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            unused: UnusedDiagnosticLevel::Hint,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramBudget {
    pub physical_lines: usize,
    pub program_lines: usize,
    pub maximum_program_lines: usize,
    pub estimated_operations_per_tick: Option<u32>,
    pub maximum_operations_per_tick: u32,
}

#[derive(Clone, Debug)]
pub struct Document {
    source: String,
    lines: Vec<ParsedLine>,
    symbols: BTreeMap<String, Symbol>,
    occurrences: Vec<SymbolOccurrence>,
    diagnostics: Vec<Diagnostic>,
    budget: ProgramBudget,
}

impl Document {
    pub fn parse(source: impl Into<String>, knowledge: &KnowledgeBase) -> Self {
        Self::parse_with_options(source, knowledge, AnalysisOptions::default())
    }

    pub fn parse_with_options(
        source: impl Into<String>,
        knowledge: &KnowledgeBase,
        options: AnalysisOptions,
    ) -> Self {
        let source = source.into();
        let mut lines = Vec::new();
        let mut symbols = BTreeMap::new();
        let mut diagnostics = Vec::new();
        let mut offset = 0;

        for (line_number, line_with_ending) in source.split_inclusive('\n').enumerate() {
            let text = line_with_ending
                .strip_suffix('\n')
                .unwrap_or(line_with_ending)
                .strip_suffix('\r')
                .unwrap_or_else(|| {
                    line_with_ending
                        .strip_suffix('\n')
                        .unwrap_or(line_with_ending)
                });
            let line_end = offset + text.len();
            let comment_relative = comment_start(text);
            let code = &text[..comment_relative.unwrap_or(text.len())];
            let tokens = tokenize(code, offset);
            let code_span = Span::new(offset, offset + code.len());
            let comment_span = comment_relative.map(|start| Span::new(offset + start, line_end));
            let kind = classify_line(tokens);

            lines.push(ParsedLine {
                number: line_number,
                span: Span::new(offset, line_end),
                code_span,
                comment_span,
                kind,
            });
            offset += line_with_ending.len();
        }

        if source.is_empty() {
            lines.push(ParsedLine {
                number: 0,
                span: Span::default(),
                code_span: Span::default(),
                comment_span: None,
                kind: LineKind::Empty,
            });
        } else if source.ends_with('\n') {
            lines.push(ParsedLine {
                number: lines.len(),
                span: Span::new(source.len(), source.len()),
                code_span: Span::new(source.len(), source.len()),
                comment_span: None,
                kind: LineKind::Empty,
            });
        }

        collect_declarations(&lines, knowledge, &mut symbols, &mut diagnostics);
        let analysis = analysis::analyze(&lines, &symbols, knowledge, options);
        diagnostics.extend(analysis.diagnostics);

        Self {
            source,
            lines,
            symbols,
            occurrences: analysis.occurrences,
            diagnostics,
            budget: analysis.budget,
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn lines(&self) -> &[ParsedLine] {
        &self.lines
    }

    pub fn symbols(&self) -> &BTreeMap<String, Symbol> {
        &self.symbols
    }

    pub fn symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn occurrences(&self) -> &[SymbolOccurrence] {
        &self.occurrences
    }

    pub fn occurrences_for(&self, name: &str) -> impl Iterator<Item = &SymbolOccurrence> {
        self.occurrences
            .iter()
            .filter(move |occurrence| occurrence.name == name)
    }

    pub fn symbol_occurrence_at_offset(&self, offset: usize) -> Option<&SymbolOccurrence> {
        self.occurrences
            .iter()
            .find(|occurrence| occurrence.span.contains(offset))
    }

    pub const fn budget(&self) -> ProgramBudget {
        self.budget
    }

    pub fn format(&self, options: FormatOptions) -> String {
        format_document(self, options)
    }

    pub fn line_at_offset(&self, offset: usize) -> Option<&ParsedLine> {
        self.lines.iter().find(|line| line.span.contains(offset))
    }

    pub fn token_at_offset(&self, offset: usize) -> Option<&Token> {
        match &self.line_at_offset(offset)?.kind {
            LineKind::Empty => None,
            LineKind::Label { name } => name.span.contains(offset).then_some(name),
            LineKind::Instruction { mnemonic, operands } => {
                if mnemonic.span.contains(offset) {
                    return Some(mnemonic);
                }
                operands.iter().find(|token| token.span.contains(offset))
            }
        }
    }

    pub fn instruction_at_offset(&self, offset: usize) -> Option<(&Token, &[Token])> {
        match &self.line_at_offset(offset)?.kind {
            LineKind::Instruction { mnemonic, operands } => Some((mnemonic, operands)),
            LineKind::Empty | LineKind::Label { .. } => None,
        }
    }
}

fn comment_start(line: &str) -> Option<usize> {
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '#' if !quoted => return Some(index),
            _ => {}
        }
    }
    None
}

fn tokenize(code: &str, base_offset: usize) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut token_start = None;
    let mut quoted = false;
    let mut escaped = false;

    for (index, character) in code.char_indices() {
        if character.is_whitespace() && !quoted {
            if let Some(start) = token_start.take() {
                tokens.push(Token {
                    text: code[start..index].to_owned(),
                    span: Span::new(base_offset + start, base_offset + index),
                });
            }
            continue;
        }
        token_start.get_or_insert(index);
        if escaped {
            escaped = false;
        } else if character == '\\' && quoted {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        }
    }

    if let Some(start) = token_start {
        tokens.push(Token {
            text: code[start..].to_owned(),
            span: Span::new(base_offset + start, base_offset + code.len()),
        });
    }
    tokens
}

fn classify_line(mut tokens: Vec<Token>) -> LineKind {
    if tokens.is_empty() {
        return LineKind::Empty;
    }
    if tokens.len() == 1 && tokens[0].text.ends_with(':') {
        let mut name = tokens.remove(0);
        name.text.pop();
        name.span.end = name.span.end.saturating_sub(1);
        return LineKind::Label { name };
    }
    let mnemonic = tokens.remove(0);
    LineKind::Instruction {
        mnemonic,
        operands: tokens,
    }
}

fn collect_declarations(
    lines: &[ParsedLine],
    knowledge: &KnowledgeBase,
    symbols: &mut BTreeMap<String, Symbol>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for line in lines {
        match &line.kind {
            LineKind::Empty => {}
            LineKind::Label { name } => {
                if !is_identifier(&name.text) {
                    diagnostics.push(Diagnostic {
                        span: name.span,
                        severity: Severity::Error,
                        code: "invalid-label",
                        message: format!("`{}` is not a valid label name.", name.text),
                        unnecessary: false,
                    });
                    continue;
                }
                insert_symbol(
                    Symbol {
                        name: name.text.clone(),
                        kind: SymbolKind::Label,
                        span: name.span,
                        value: None,
                        value_span: None,
                        declaration_line: line.number,
                    },
                    symbols,
                    diagnostics,
                );
            }
            LineKind::Instruction { mnemonic, operands } => {
                analyze_literal_macros(operands, diagnostics);
                let Some(instruction) = knowledge.instruction(&mnemonic.text) else {
                    diagnostics.push(Diagnostic {
                        span: mnemonic.span,
                        severity: Severity::Error,
                        code: "unknown-instruction",
                        message: format!("Unknown IC10 instruction `{}`.", mnemonic.text),
                        unnecessary: false,
                    });
                    continue;
                };
                if instruction.deprecated {
                    diagnostics.push(Diagnostic {
                        span: mnemonic.span,
                        severity: Severity::Hint,
                        code: "deprecated-instruction",
                        message: format!("`{}` is deprecated.", mnemonic.text),
                        unnecessary: false,
                    });
                }
                if operands.len() != instruction.operands.len() {
                    diagnostics.push(Diagnostic {
                        span: Span::new(
                            mnemonic.span.start,
                            operands
                                .last()
                                .map_or(mnemonic.span.end, |operand| operand.span.end),
                        ),
                        severity: Severity::Error,
                        code: "operand-count",
                        message: format!(
                            "`{}` expects {} operand{}, but {} {} provided.",
                            mnemonic.text,
                            instruction.operands.len(),
                            if instruction.operands.len() == 1 {
                                ""
                            } else {
                                "s"
                            },
                            operands.len(),
                            if operands.len() == 1 { "was" } else { "were" },
                        ),
                        unnecessary: false,
                    });
                }
                let symbol_kind = match mnemonic.text.as_str() {
                    "define" => Some(SymbolKind::Define),
                    "alias" => Some(SymbolKind::Alias),
                    _ => None,
                };
                if let (Some(symbol_kind), Some(name)) = (symbol_kind, operands.first()) {
                    if !is_identifier(&name.text) {
                        diagnostics.push(Diagnostic {
                            span: name.span,
                            severity: Severity::Error,
                            code: "invalid-symbol",
                            message: format!("`{}` is not a valid symbol name.", name.text),
                            unnecessary: false,
                        });
                        continue;
                    }
                    insert_symbol(
                        Symbol {
                            name: name.text.clone(),
                            kind: symbol_kind,
                            span: name.span,
                            value: operands.get(1).map(|value| value.text.clone()),
                            value_span: operands.get(1).map(|value| value.span),
                            declaration_line: line.number,
                        },
                        symbols,
                        diagnostics,
                    );
                }
            }
        }
    }
}

fn analyze_literal_macros(operands: &[Token], diagnostics: &mut Vec<Diagnostic>) {
    for operand in operands {
        let looks_like_macro =
            operand.text.starts_with("HASH(") || operand.text.starts_with("STR(");
        let Some(literal) = parse_literal_macro(&operand.text) else {
            if looks_like_macro {
                diagnostics.push(Diagnostic {
                    span: operand.span,
                    severity: Severity::Error,
                    code: "malformed-literal-macro",
                    message: format!(
                        "`{}` must use a quoted literal such as `{}(\"text\")`.",
                        operand.text,
                        if operand.text.starts_with("STR(") {
                            "STR"
                        } else {
                            "HASH"
                        }
                    ),
                    unnecessary: false,
                });
            }
            continue;
        };
        if literal.kind != LiteralMacroKind::String {
            continue;
        }
        match pack_stationeers_string(&literal.value) {
            Ok(_) => {}
            Err(PackedStringError::NonAscii) => diagnostics.push(Diagnostic {
                span: operand.span,
                severity: Severity::Error,
                code: "non-ascii-string-literal",
                message: "`STR` supports ASCII characters only because each character occupies one byte."
                    .to_owned(),
                unnecessary: false,
            }),
            Err(PackedStringError::TooLong { length }) => diagnostics.push(Diagnostic {
                span: operand.span,
                severity: Severity::Error,
                code: "string-literal-too-long",
                message: format!(
                    "`STR` supports at most {MAX_PACKED_STRING_BYTES} characters; this literal has {length}."
                ),
                unnecessary: false,
            }),
        }
    }
}

fn insert_symbol(
    symbol: Symbol,
    symbols: &mut BTreeMap<String, Symbol>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(previous) = symbols.get(&symbol.name) {
        diagnostics.push(Diagnostic {
            span: symbol.span,
            severity: Severity::Warning,
            code: "duplicate-symbol",
            message: format!(
                "`{}` is already defined at byte offset {}.",
                symbol.name, previous.span.start
            ),
            unnecessary: false,
        });
    } else {
        symbols.insert(symbol.name.clone(), symbol);
    }
}

pub fn is_absolute_branch(mnemonic: &str) -> bool {
    matches!(mnemonic, "j" | "jal")
        || (mnemonic.starts_with('b')
            && !mnemonic.starts_with("br")
            && !matches!(mnemonic, "bdse" | "bdns" | "bdnvl" | "bdnvs"))
}

pub fn is_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

pub fn is_register(value: &str) -> bool {
    let direct = value.trim_start_matches('r');
    matches!(value, "ra" | "sp")
        || (!direct.is_empty()
            && direct.len() < value.len()
            && direct.parse::<u8>().is_ok_and(|number| number <= 17))
}

#[cfg(test)]
mod tests {
    use ic10_data::KnowledgeBase;

    use super::{
        Document, LineKind, LiteralMacro, LiteralMacroKind, PackedStringError, Severity,
        SymbolKind, pack_stationeers_string, parse_literal_macro, stationeers_crc32,
    };

    fn knowledge() -> KnowledgeBase {
        KnowledgeBase::load_embedded().expect("embedded data")
    }

    #[test]
    fn parses_labels_comments_and_quoted_hashes() {
        let document = Document::parse(
            "start:\nmove r0 HASH(\"A#B\") # a comment\nj start\n",
            &knowledge(),
        );

        assert!(matches!(document.lines()[0].kind, LineKind::Label { .. }));
        assert_eq!(
            document.symbol("start").map(|symbol| symbol.kind),
            Some(SymbolKind::Label)
        );
        assert!(document.lines()[1].comment_span.is_some());
        assert!(
            document
                .diagnostics()
                .iter()
                .all(|diagnostic| diagnostic.severity != Severity::Error)
        );
    }

    #[test]
    fn computes_hash_and_packed_string_literals() {
        assert_eq!(
            parse_literal_macro("HASH(\"Iron\")"),
            Some(LiteralMacro {
                kind: LiteralMacroKind::Hash,
                value: "Iron".to_owned(),
            })
        );
        assert_eq!(stationeers_crc32("Iron"), 3_628_224_418);
        assert_eq!(stationeers_crc32("Iron") as i32, -666_742_878);
        assert_eq!(stationeers_crc32("StructureGasAnalyser"), 1_876_147_318);

        assert_eq!(
            parse_literal_macro("STR(\"Hello!\")"),
            Some(LiteralMacro {
                kind: LiteralMacroKind::String,
                value: "Hello!".to_owned(),
            })
        );
        assert_eq!(pack_stationeers_string("Hello!"), Ok(0x48_65_6C_6C_6F_21));
        assert_eq!(
            pack_stationeers_string("Too long"),
            Err(PackedStringError::TooLong { length: 8 })
        );
        assert_eq!(
            pack_stationeers_string("°"),
            Err(PackedStringError::NonAscii)
        );
    }

    #[test]
    fn reports_invalid_packed_string_literals() {
        let document = Document::parse(
            "move r0 STR(\"1234567\")\nmove r1 STR(\"°\")\nmove r2 HASH(oops)\n",
            &knowledge(),
        );
        let codes: Vec<_> = document
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect();

        assert!(codes.contains(&"string-literal-too-long"));
        assert!(codes.contains(&"non-ascii-string-literal"));
        assert!(codes.contains(&"malformed-literal-macro"));
    }

    #[test]
    fn reports_unknown_instructions_and_missing_labels() {
        let document = Document::parse("wat r0\nj nowhere\n", &knowledge());
        let codes: Vec<_> = document
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect();

        assert!(codes.contains(&"unknown-instruction"));
        assert!(codes.contains(&"undefined-label"));
    }

    #[test]
    fn reports_operand_count_without_rejecting_incomplete_source() {
        let document = Document::parse("add r0 1\n", &knowledge());

        assert!(
            document
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "operand-count")
        );
    }

    #[test]
    fn resolves_define_and_alias_symbols() {
        let document = Document::parse("define target 4\nalias sensor d0\n", &knowledge());

        assert_eq!(
            document.symbol("target").map(|symbol| symbol.kind),
            Some(SymbolKind::Define)
        );
        assert_eq!(
            document.symbol("sensor").map(|symbol| symbol.kind),
            Some(SymbolKind::Alias)
        );
    }
}
