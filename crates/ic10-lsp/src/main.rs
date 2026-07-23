use std::collections::HashMap;
use std::sync::Arc;

use ic10_core::{Document, LineKind, Severity, Span, SymbolKind};
use ic10_data::{Device, Instruction, KnowledgeBase};
use serde::Deserialize;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializationOptions {
    asset_uri: Option<String>,
}

struct Backend {
    client: Client,
    knowledge: Arc<KnowledgeBase>,
    documents: RwLock<HashMap<Url, Arc<Document>>>,
    asset_uri: RwLock<Option<String>>,
}

impl Backend {
    fn new(client: Client, knowledge: Arc<KnowledgeBase>) -> Self {
        Self {
            client,
            knowledge,
            documents: RwLock::new(HashMap::new()),
            asset_uri: RwLock::new(None),
        }
    }

    async fn document(&self, uri: &Url) -> Option<Arc<Document>> {
        self.documents.read().await.get(uri).cloned()
    }

    async fn update_document(&self, uri: Url, text: String) {
        let document = Arc::new(Document::parse(text, &self.knowledge));
        let diagnostics = document
            .diagnostics()
            .iter()
            .map(|diagnostic| Diagnostic {
                range: span_to_range(document.source(), diagnostic.span),
                severity: Some(match diagnostic.severity {
                    Severity::Error => DiagnosticSeverity::ERROR,
                    Severity::Warning => DiagnosticSeverity::WARNING,
                    Severity::Information => DiagnosticSeverity::INFORMATION,
                    Severity::Hint => DiagnosticSeverity::HINT,
                }),
                code: Some(NumberOrString::String(diagnostic.code.to_owned())),
                source: Some("ic10".to_owned()),
                message: diagnostic.message.clone(),
                ..Diagnostic::default()
            })
            .collect();
        self.documents.write().await.insert(uri.clone(), document);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    fn command_completions(&self) -> Vec<CompletionItem> {
        self.knowledge
            .language
            .instructions
            .iter()
            .map(|(name, instruction)| CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some(instruction.syntax.clone()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: instruction.description.clone(),
                })),
                deprecated: Some(instruction.deprecated),
                insert_text: Some(instruction_snippet(name, instruction)),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                sort_text: Some(format!("0-{name}")),
                ..CompletionItem::default()
            })
            .collect()
    }

    fn operand_completions(
        &self,
        document: &Document,
        mnemonic: &str,
        operand_type: &str,
    ) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        if operand_type.contains("r?") {
            for register in (0..=15)
                .map(|index| format!("r{index}"))
                .chain(["ra".to_owned(), "sp".to_owned()])
            {
                items.push(simple_completion(
                    register,
                    CompletionItemKind::VARIABLE,
                    "IC10 register",
                ));
            }
        }
        if operand_type.contains("d?") || operand_type.starts_with("device") {
            for device in (0..=5)
                .map(|index| format!("d{index}"))
                .chain(["db".to_owned()])
            {
                items.push(simple_completion(
                    device,
                    CompletionItemKind::VARIABLE,
                    "IC10 device reference",
                ));
            }
        }

        let enum_name = if operand_type.contains("logicSlotType") {
            Some("LogicSlotType")
        } else if operand_type.contains("logicType") {
            Some("LogicType")
        } else if operand_type.contains("batchMode") {
            Some("LogicBatchMethod")
        } else if operand_type.contains("reagentMode") {
            Some("LogicReagentMode")
        } else {
            None
        };
        if let Some(listing) = enum_name.and_then(|name| self.knowledge.language.enums.get(name)) {
            items.extend(listing.values.iter().map(|(name, value)| CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some(format!("{} = {}", listing.display_name, value.value)),
                documentation: Some(Documentation::String(value.description.clone())),
                deprecated: Some(value.deprecated),
                ..CompletionItem::default()
            }));
        }

        if operand_type.contains("deviceHash") {
            items.extend(self.knowledge.all_devices().map(|device| CompletionItem {
                label: device.prefab_name.clone(),
                label_details: Some(CompletionItemLabelDetails {
                    detail: Some(format!(" ({})", device.prefab_hash)),
                    description: Some(device.display_name.clone()),
                }),
                kind: Some(CompletionItemKind::REFERENCE),
                detail: Some(format!(
                    "{} · PrefabHash {}",
                    device.display_name, device.prefab_hash
                )),
                insert_text: Some(format!("HASH(\"{}\")", device.prefab_name)),
                filter_text: Some(format!(
                    "{} {} {}",
                    device.prefab_name, device.display_name, device.prefab_hash
                )),
                ..CompletionItem::default()
            }));
        }

        if operand_type.contains("num") || operand_type.contains("int") {
            items.extend(
                self.knowledge
                    .language
                    .constants
                    .iter()
                    .map(|(name, constant)| CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::CONSTANT),
                        detail: Some(constant.value.to_string()),
                        documentation: Some(Documentation::String(constant.description.clone())),
                        ..CompletionItem::default()
                    }),
            );
            let expects_label = is_absolute_branch(mnemonic);
            items.extend(document.symbols().values().filter_map(|symbol| {
                if expects_label || symbol.kind == SymbolKind::Define {
                    Some(simple_completion(
                        symbol.name.clone(),
                        match symbol.kind {
                            SymbolKind::Label => CompletionItemKind::REFERENCE,
                            SymbolKind::Define => CompletionItemKind::CONSTANT,
                            SymbolKind::Alias => CompletionItemKind::VARIABLE,
                        },
                        "Document symbol",
                    ))
                } else {
                    None
                }
            }));
        }
        items
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(value) = params.initialization_options
            && let Ok(options) = serde_json::from_value::<InitializationOptions>(value)
        {
            *self.asset_uri.write().await = options.asset_uri;
        }
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "Stationeers IC10 Language Server".to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![" ".to_owned(), "\"".to_owned()]),
                    ..CompletionOptions::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec![" ".to_owned()]),
                    retrigger_characters: Some(vec![" ".to_owned()]),
                    ..SignatureHelpOptions::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "IC10 language data loaded for Stationeers {}",
                    self.knowledge.language.game_version
                ),
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.update_document(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.update_document(params.text_document.uri, change.text)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let Some(document) = self.document(uri).await else {
            return Ok(None);
        };
        let offset = position_to_offset(document.source(), params.text_document_position.position);
        let Some(line) = document.line_at_offset(offset) else {
            return Ok(None);
        };
        let items = match &line.kind {
            LineKind::Empty | LineKind::Label { .. } => self.command_completions(),
            LineKind::Instruction { mnemonic, operands } => {
                if offset <= mnemonic.span.end {
                    self.command_completions()
                } else if let Some(instruction) = self.knowledge.instruction(&mnemonic.text) {
                    let active = active_operand(operands, offset);
                    instruction
                        .operands
                        .get(active)
                        .map_or_else(Vec::new, |operand| {
                            self.operand_completions(
                                &document,
                                &mnemonic.text,
                                &operand.operand_type,
                            )
                        })
                } else {
                    self.command_completions()
                }
            }
        };
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = &params.text_document_position_params.text_document.uri;
        let Some(document) = self.document(uri).await else {
            return Ok(None);
        };
        let offset = position_to_offset(document.source(), position);
        let Some(token) = document.token_at_offset(offset) else {
            return Ok(None);
        };
        let value = if let Some(instruction) = self.knowledge.instruction(&token.text) {
            Some(instruction_markdown(&token.text, instruction))
        } else if let Some(device) = device_from_token(&token.text, &self.knowledge) {
            let asset_uri = self.asset_uri.read().await;
            Some(device_markdown(device, asset_uri.as_deref()))
        } else if let Some(constant) = self.knowledge.language.constants.get(&token.text) {
            Some(format!(
                "### `{}`\n\n**Value:** `{}`\n\n{}",
                token.text, constant.value, constant.description
            ))
        } else if let Some((enum_name, value)) = self.knowledge.enum_value(&token.text) {
            Some(format!(
                "### `{}`\n\n**{} value:** `{}`\n\n{}",
                token.text, enum_name, value.value, value.description
            ))
        } else {
            document.symbol(&token.text).map(|symbol| {
                let kind = match symbol.kind {
                    SymbolKind::Label => "Label",
                    SymbolKind::Define => "Define",
                    SymbolKind::Alias => "Alias",
                };
                let value = symbol
                    .value
                    .as_deref()
                    .map_or(String::new(), |value| format!("\n\n**Value:** `{value}`"));
                format!("### `{}`\n\n**{kind}**{value}", symbol.name)
            })
        };
        Ok(value.map(|value| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(span_to_range(document.source(), token.span)),
        }))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let position = params.text_document_position_params.position;
        let uri = &params.text_document_position_params.text_document.uri;
        let Some(document) = self.document(uri).await else {
            return Ok(None);
        };
        let offset = position_to_offset(document.source(), position);
        let Some((mnemonic, operands)) = document.instruction_at_offset(offset) else {
            return Ok(None);
        };
        let Some(instruction) = self.knowledge.instruction(&mnemonic.text) else {
            return Ok(None);
        };
        let active = active_operand(operands, offset);
        Ok(Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label: instruction.syntax.clone(),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: instruction.description.clone(),
                })),
                parameters: Some(
                    instruction
                        .operands
                        .iter()
                        .map(|operand| ParameterInformation {
                            label: ParameterLabel::Simple(operand.display.clone()),
                            documentation: Some(Documentation::String(format!(
                                "{}: {}",
                                operand.label, operand.operand_type
                            ))),
                        })
                        .collect(),
                ),
                active_parameter: None,
            }],
            active_signature: Some(0),
            active_parameter: (!instruction.operands.is_empty())
                .then_some(active.min(instruction.operands.len() - 1) as u32),
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let Some(document) = self.document(uri).await else {
            return Ok(None);
        };
        let offset = position_to_offset(
            document.source(),
            params.text_document_position_params.position,
        );
        let Some(token) = document.token_at_offset(offset) else {
            return Ok(None);
        };
        let Some(symbol) = document.symbol(&token.text) else {
            return Ok(None);
        };
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: span_to_range(document.source(), symbol.span),
        })))
    }

    #[allow(deprecated)]
    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(document) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        let symbols = document
            .symbols()
            .values()
            .map(|symbol| {
                let range = span_to_range(document.source(), symbol.span);
                DocumentSymbol {
                    name: symbol.name.clone(),
                    detail: symbol.value.clone(),
                    kind: match symbol.kind {
                        SymbolKind::Label => tower_lsp::lsp_types::SymbolKind::KEY,
                        SymbolKind::Define => tower_lsp::lsp_types::SymbolKind::CONSTANT,
                        SymbolKind::Alias => tower_lsp::lsp_types::SymbolKind::VARIABLE,
                    },
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                }
            })
            .collect();
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }
}

fn simple_completion(label: String, kind: CompletionItemKind, detail: &str) -> CompletionItem {
    CompletionItem {
        label,
        kind: Some(kind),
        detail: Some(detail.to_owned()),
        ..CompletionItem::default()
    }
}

fn instruction_snippet(name: &str, instruction: &Instruction) -> String {
    let mut snippet = name.to_owned();
    for (index, operand) in instruction.operands.iter().enumerate() {
        snippet.push_str(&format!(
            " ${{{}:{}}}",
            index + 1,
            operand.label.replace(['$', '}'], "")
        ));
    }
    snippet
}

fn active_operand(operands: &[ic10_core::Token], offset: usize) -> usize {
    operands
        .iter()
        .position(|operand| offset <= operand.span.end)
        .unwrap_or(operands.len())
}

fn instruction_markdown(name: &str, instruction: &Instruction) -> String {
    let deprecated = if instruction.deprecated {
        "\n\n> Deprecated"
    } else {
        ""
    };
    format!(
        "### `{name}`\n\n```ic10\n{}\n```\n\n{}{}",
        instruction.syntax, instruction.description, deprecated
    )
}

fn device_from_token<'a>(token: &str, knowledge: &'a KnowledgeBase) -> Option<&'a Device> {
    if let Some(name) = token
        .strip_prefix("HASH(\"")
        .and_then(|value| value.strip_suffix("\")"))
    {
        return knowledge.device_by_name(name);
    }
    if let Some(device) = knowledge.device_by_name(token) {
        return Some(device);
    }
    let normalized = token.replace('_', "");
    let hash = normalized.parse::<i32>().ok()?;
    knowledge.device_by_hash(hash)
}

fn device_markdown(device: &Device, asset_uri: Option<&str>) -> String {
    let image = match (asset_uri, device.image.as_deref()) {
        (Some(base), Some(image)) => format!(
            "\n\n![{}]({}/{})",
            markdown_escape(&device.display_name),
            base.trim_end_matches('/'),
            image
        ),
        _ => String::new(),
    };
    let description = if device.description.is_empty() {
        String::new()
    } else {
        format!("\n\n{}", markdown_escape(&device.description))
    };
    let mut logic: Vec<_> = device.logic_types.iter().collect();
    logic.sort_by_key(|(name, _)| *name);
    let shown = logic
        .iter()
        .take(24)
        .map(|(name, access)| {
            let mode = match (access.read, access.write) {
                (true, true) => "read/write",
                (true, false) => "read",
                (false, true) => "write",
                (false, false) => "none",
            };
            format!("`{name}` ({mode})")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let remaining = logic.len().saturating_sub(24);
    let suffix = if remaining > 0 {
        format!(", … and {remaining} more")
    } else {
        String::new()
    };
    format!(
        "### {}\n\n`{}` · PrefabHash `{}`{}{}\n\n**Logic:** {}{}",
        markdown_escape(&device.display_name),
        device.prefab_name,
        device.prefab_hash,
        image,
        description,
        shown,
        suffix
    )
}

fn markdown_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn is_absolute_branch(mnemonic: &str) -> bool {
    matches!(mnemonic, "j" | "jal")
        || (mnemonic.starts_with('b')
            && !mnemonic.starts_with("br")
            && !matches!(mnemonic, "bdse" | "bdns" | "bdnvl" | "bdnvs"))
}

fn position_to_offset(source: &str, position: Position) -> usize {
    let mut line_start = 0;
    for _ in 0..position.line {
        let Some(relative) = source[line_start..].find('\n') else {
            return source.len();
        };
        line_start += relative + 1;
    }
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |relative| line_start + relative);
    let line = &source[line_start..line_end];
    let mut utf16_units = 0_u32;
    for (byte, character) in line.char_indices() {
        let width = character.len_utf16() as u32;
        if utf16_units + width > position.character {
            return line_start + byte;
        }
        utf16_units += width;
        if utf16_units == position.character {
            return line_start + byte + character.len_utf8();
        }
    }
    line_end
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let before = &source[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let character = source[line_start..offset].encode_utf16().count() as u32;
    Position::new(line, character)
}

fn span_to_range(source: &str, span: Span) -> Range {
    Range::new(
        offset_to_position(source, span.start),
        offset_to_position(source, span.end),
    )
}

#[tokio::main]
async fn main() {
    let knowledge =
        Arc::new(KnowledgeBase::load_embedded().expect("generated IC10 data must be valid"));
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) =
        LspService::new(move |client| Backend::new(client, Arc::clone(&knowledge)));
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use ic10_data::KnowledgeBase;
    use tower_lsp::lsp_types::Position;

    use super::{device_from_token, offset_to_position, position_to_offset};

    #[test]
    fn converts_utf16_positions() {
        let source = "move r0 1 # °\nnext";
        let offset = source.find('°').expect("test character");
        let position = offset_to_position(source, offset);

        assert_eq!(position_to_offset(source, position), offset);
        assert_eq!(position, Position::new(0, 12));
    }

    #[test]
    fn finds_devices_from_hash_macro_and_integer() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data");

        assert_eq!(
            device_from_token("HASH(\"StructureAccessBridge\")", &knowledge)
                .map(|device| device.prefab_hash),
            Some(1_298_920_475)
        );
        assert_eq!(
            device_from_token("1298920475", &knowledge).map(|device| device.prefab_name.as_str()),
            Some("StructureAccessBridge")
        );
    }
}
