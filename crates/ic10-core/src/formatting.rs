use crate::{Document, LineKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FormatOptions {
    pub indent_size: usize,
    pub insert_spaces: bool,
    pub directives_at_column_zero: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            indent_size: 2,
            insert_spaces: true,
            directives_at_column_zero: true,
        }
    }
}

pub fn format_document(document: &Document, options: FormatOptions) -> String {
    let indent = if options.insert_spaces {
        " ".repeat(options.indent_size)
    } else {
        "\t".to_owned()
    };
    let eol = if document.source().contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let trailing_eol = document.source().ends_with('\n');
    let mut output = Vec::with_capacity(document.lines().len());

    for line in document.lines() {
        let comment = line
            .comment_span
            .map(|span| &document.source()[span.start..span.end]);
        let formatted = match &line.kind {
            LineKind::Empty if line.comment_span.is_some() => String::new(),
            LineKind::Empty => document.source()[line.span.start..line.span.end].to_owned(),
            LineKind::Label { name } => format!("{}:", name.text),
            LineKind::Instruction { mnemonic, operands } => {
                let at_column_zero = options.directives_at_column_zero
                    && matches!(mnemonic.text.as_str(), "alias" | "define");
                let mut code = if at_column_zero {
                    String::new()
                } else {
                    indent.clone()
                };
                code.push_str(&mnemonic.text);
                for operand in operands {
                    code.push(' ');
                    code.push_str(&operand.text);
                }
                code
            }
        };
        if let Some(comment) = comment {
            if formatted.trim().is_empty() {
                output.push(comment.to_owned());
            } else {
                output.push(format!("{} {}", formatted.trim_end(), comment));
            }
        } else {
            output.push(formatted.trim_end().to_owned());
        }
    }

    if trailing_eol && output.last().is_some_and(String::is_empty) {
        output.pop();
    }
    let mut result = output.join(eol);
    if trailing_eol {
        result.push_str(eol);
    }
    result
}
