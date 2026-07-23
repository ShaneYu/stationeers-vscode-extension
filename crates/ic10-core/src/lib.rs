//! A small, error-tolerant parser for IC10's line-oriented source format.
//!
//! The parser intentionally has no LSP or editor dependencies. It is cheap
//! enough to run on every document change and can later be replaced internally
//! without changing the language-server protocol layer.

use std::collections::BTreeMap;

use ic10_data::KnowledgeBase;

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
}

#[derive(Clone, Debug)]
pub struct Document {
    source: String,
    lines: Vec<ParsedLine>,
    symbols: BTreeMap<String, Symbol>,
    diagnostics: Vec<Diagnostic>,
}

impl Document {
    pub fn parse(source: impl Into<String>, knowledge: &KnowledgeBase) -> Self {
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

            analyze_line(&kind, knowledge, &mut symbols, &mut diagnostics);
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

        let maximum_lines = knowledge.language.architecture.maximum_program_lines;
        if lines.len() > maximum_lines {
            let first_excess = lines[maximum_lines].span;
            diagnostics.push(Diagnostic {
                span: first_excess,
                severity: Severity::Warning,
                code: "program-too-long",
                message: format!(
                    "IC10 programs are limited to {maximum_lines} lines; this document has {}.",
                    lines.len()
                ),
            });
        }

        analyze_label_references(
            &lines,
            &symbols,
            &knowledge.language.constants,
            &mut diagnostics,
        );

        Self {
            source,
            lines,
            symbols,
            diagnostics,
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

fn analyze_line(
    kind: &LineKind,
    knowledge: &KnowledgeBase,
    symbols: &mut BTreeMap<String, Symbol>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match kind {
        LineKind::Empty => {}
        LineKind::Label { name } => {
            if !is_identifier(&name.text) {
                diagnostics.push(Diagnostic {
                    span: name.span,
                    severity: Severity::Error,
                    code: "invalid-label",
                    message: format!("`{}` is not a valid label name.", name.text),
                });
                return;
            }
            insert_symbol(
                Symbol {
                    name: name.text.clone(),
                    kind: SymbolKind::Label,
                    span: name.span,
                    value: None,
                },
                symbols,
                diagnostics,
            );
        }
        LineKind::Instruction { mnemonic, operands } => {
            let Some(instruction) = knowledge.instruction(&mnemonic.text) else {
                diagnostics.push(Diagnostic {
                    span: mnemonic.span,
                    severity: Severity::Error,
                    code: "unknown-instruction",
                    message: format!("Unknown IC10 instruction `{}`.", mnemonic.text),
                });
                return;
            };
            if instruction.deprecated {
                diagnostics.push(Diagnostic {
                    span: mnemonic.span,
                    severity: Severity::Hint,
                    code: "deprecated-instruction",
                    message: format!("`{}` is deprecated.", mnemonic.text),
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
                });
            }
            let symbol_kind = match mnemonic.text.as_str() {
                "define" => Some(SymbolKind::Define),
                "alias" => Some(SymbolKind::Alias),
                "label" => Some(SymbolKind::Label),
                _ => None,
            };
            if let (Some(symbol_kind), Some(name)) = (symbol_kind, operands.first()) {
                insert_symbol(
                    Symbol {
                        name: name.text.clone(),
                        kind: symbol_kind,
                        span: name.span,
                        value: operands.get(1).map(|value| value.text.clone()),
                    },
                    symbols,
                    diagnostics,
                );
            }
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
        });
    } else {
        symbols.insert(symbol.name.clone(), symbol);
    }
}

fn analyze_label_references(
    lines: &[ParsedLine],
    symbols: &BTreeMap<String, Symbol>,
    constants: &BTreeMap<String, ic10_data::Constant>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for line in lines {
        let LineKind::Instruction { mnemonic, operands } = &line.kind else {
            continue;
        };
        if !is_absolute_branch(&mnemonic.text) {
            continue;
        }
        let Some(target) = operands.last() else {
            continue;
        };
        if is_identifier(&target.text)
            && !symbols.contains_key(&target.text)
            && !constants.contains_key(&target.text)
            && !is_register(&target.text)
        {
            diagnostics.push(Diagnostic {
                span: target.span,
                severity: Severity::Warning,
                code: "undefined-label",
                message: format!("No label or define named `{}` exists.", target.text),
            });
        }
    }
}

fn is_absolute_branch(mnemonic: &str) -> bool {
    matches!(mnemonic, "j" | "jal")
        || (mnemonic.starts_with('b')
            && !mnemonic.starts_with("br")
            && !matches!(mnemonic, "bdse" | "bdns" | "bdnvl" | "bdnvs"))
}

fn is_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn is_register(value: &str) -> bool {
    matches!(value, "ra" | "sp")
        || value
            .strip_prefix('r')
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| number <= 15)
}

#[cfg(test)]
mod tests {
    use ic10_data::KnowledgeBase;

    use super::{Document, LineKind, Severity, SymbolKind};

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
