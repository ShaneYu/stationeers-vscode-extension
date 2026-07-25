use std::collections::HashMap;
use std::sync::Arc;

use ic10_core::{
    AnalysisOptions, Document, FormatOptions, LineKind, LiteralMacro, LiteralMacroKind,
    PackedStringError, Severity, Span, Symbol, SymbolKind, UnusedDiagnosticLevel,
    pack_stationeers_string, parse_literal_macro, parse_numeric_literal, stationeers_crc32,
};
use ic10_data::{Device, Instruction, KnowledgeBase, Reagent, Resource};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializationOptions {
    asset_uri: Option<String>,
    unused_diagnostics: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LanguageSettings {
    diagnostics: Option<DiagnosticSettings>,
}

#[derive(Debug, Default, Deserialize)]
struct DiagnosticSettings {
    unused: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgramBudgetParams {
    uri: Url,
    physical_lines: usize,
    program_lines: usize,
    maximum_program_lines: usize,
    estimated_operations_per_tick: Option<u32>,
    maximum_operations_per_tick: u32,
}

enum ProgramBudgetNotification {}

impl notification::Notification for ProgramBudgetNotification {
    type Params = ProgramBudgetParams;
    const METHOD: &'static str = "ic10/programBudget";
}

struct Backend {
    client: Client,
    knowledge: Arc<KnowledgeBase>,
    documents: RwLock<HashMap<Url, Arc<Document>>>,
    asset_uri: RwLock<Option<String>>,
    analysis_options: RwLock<AnalysisOptions>,
}

impl Backend {
    fn new(client: Client, knowledge: Arc<KnowledgeBase>) -> Self {
        Self {
            client,
            knowledge,
            documents: RwLock::new(HashMap::new()),
            asset_uri: RwLock::new(None),
            analysis_options: RwLock::new(AnalysisOptions::default()),
        }
    }

    async fn document(&self, uri: &Url) -> Option<Arc<Document>> {
        self.documents.read().await.get(uri).cloned()
    }

    async fn update_document(&self, uri: Url, text: String) {
        let options = *self.analysis_options.read().await;
        let document = Arc::new(Document::parse_with_options(text, &self.knowledge, options));
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
                tags: diagnostic
                    .unnecessary
                    .then_some(vec![DiagnosticTag::UNNECESSARY]),
                ..Diagnostic::default()
            })
            .collect();
        let budget = document.budget();
        self.documents.write().await.insert(uri.clone(), document);
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
        self.client
            .send_notification::<ProgramBudgetNotification>(ProgramBudgetParams {
                uri,
                physical_lines: budget.physical_lines,
                program_lines: budget.program_lines,
                maximum_program_lines: budget.maximum_program_lines,
                estimated_operations_per_tick: budget.estimated_operations_per_tick,
                maximum_operations_per_tick: budget.maximum_operations_per_tick,
            })
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

        let accepts_number = operand_type.contains("num") || operand_type.contains("int");
        let accepts_hash = operand_type.contains("Hash") || operand_type.contains("hash");
        if accepts_number || accepts_hash {
            items.push(literal_macro_completion(
                "HASH",
                "CRC-32 string hash",
                "Computes the Stationeers CRC-32 checksum for a prefab or user-defined name.",
            ));
        }
        if accepts_number {
            items.push(literal_macro_completion(
                "STR",
                "Packed display string",
                "Packs up to six ASCII characters into a numeric value for a display in String mode.",
            ));
        }

        if accepts_number {
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
            if let Some(unused) = options.unused_diagnostics {
                self.analysis_options.write().await.unused = parse_unused_level(&unused);
            }
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
                references_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions::default(),
                            legend: semantic_token_legend(),
                            range: Some(true),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
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

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        let settings =
            serde_json::from_value::<LanguageSettings>(params.settings.clone()).or_else(|_| {
                params
                    .settings
                    .get("ic10")
                    .cloned()
                    .ok_or_else(|| serde_json::Error::io(std::io::Error::other("missing ic10")))
                    .and_then(serde_json::from_value::<LanguageSettings>)
            });
        let Ok(settings) = settings else {
            return;
        };
        if let Some(unused) = settings
            .diagnostics
            .and_then(|diagnostics| diagnostics.unused)
        {
            self.analysis_options.write().await.unused = parse_unused_level(&unused);
            let sources = self
                .documents
                .read()
                .await
                .iter()
                .map(|(uri, document)| (uri.clone(), document.source().to_owned()))
                .collect::<Vec<_>>();
            for (uri, source) in sources {
                self.update_document(uri, source).await;
            }
        }
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
        let literal_macro = parse_literal_macro(&token.text);
        let show_crc = literal_macro
            .as_ref()
            .is_some_and(|literal| literal.kind == LiteralMacroKind::Hash);
        let value = if let Some(instruction) = self.knowledge.instruction(&token.text) {
            Some(instruction_markdown(
                &token.text,
                instruction,
                &self.knowledge,
            ))
        } else if let Some(register) = register_markdown(&token.text, &self.knowledge) {
            Some(register)
        } else if let Some(device_reference) =
            device_reference_markdown(&token.text, &self.knowledge)
        {
            Some(device_reference)
        } else if let Some(device) = device_from_token(&token.text, &self.knowledge) {
            let asset_uri = self.asset_uri.read().await;
            Some(device_markdown(
                device,
                &self.knowledge,
                asset_uri.as_deref(),
                show_crc,
            ))
        } else if let Some(resource) = resource_from_token(&token.text, &self.knowledge) {
            let asset_uri = self.asset_uri.read().await;
            Some(resource_markdown(
                resource,
                &self.knowledge,
                asset_uri.as_deref(),
                show_crc,
            ))
        } else if let Some(reagent) = reagent_from_token(&token.text, &self.knowledge) {
            let asset_uri = self.asset_uri.read().await;
            Some(reagent_markdown(
                reagent,
                &self.knowledge,
                asset_uri.as_deref(),
                show_crc,
            ))
        } else if let Some(literal) = literal_macro.as_ref() {
            Some(literal_macro_markdown(literal))
        } else if let Some(constant) = self.knowledge.language.constants.get(&token.text) {
            Some(format!(
                "### `{}`\n\n**Value:** `{}`\n\n{}",
                token.text,
                constant.value,
                description_markdown(&constant.description, &self.knowledge, false)
            ))
        } else if let Some((enum_name, value)) = self.knowledge.enum_value(&token.text) {
            Some(format!(
                "### `{}`\n\n**{} value:** `{}`\n\n{}",
                token.text,
                enum_name,
                value.value,
                description_markdown(&value.description, &self.knowledge, token.text == "Color")
            ))
        } else if let Some(symbol) = document.symbol(&token.text) {
            let asset_uri = self.asset_uri.read().await;
            Some(symbol_markdown(
                symbol,
                &self.knowledge,
                asset_uri.as_deref(),
            ))
        } else {
            None
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
                    value: description_markdown(&instruction.description, &self.knowledge, false),
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

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let Some(document) = self.document(uri).await else {
            return Ok(None);
        };
        let offset = position_to_offset(document.source(), params.text_document_position.position);
        let Some(occurrence) = document.symbol_occurrence_at_offset(offset) else {
            return Ok(None);
        };
        let locations = document
            .occurrences_for(&occurrence.name)
            .filter(|item| params.context.include_declaration || !item.declaration)
            .map(|item| Location {
                uri: uri.clone(),
                range: span_to_range(document.source(), item.span),
            })
            .collect();
        Ok(Some(locations))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let Some(document) = self.document(uri).await else {
            return Ok(None);
        };
        let offset = position_to_offset(
            document.source(),
            params.text_document_position_params.position,
        );
        let Some(occurrence) = document.symbol_occurrence_at_offset(offset) else {
            return Ok(None);
        };
        Ok(Some(
            document
                .occurrences_for(&occurrence.name)
                .map(|item| DocumentHighlight {
                    range: span_to_range(document.source(), item.span),
                    kind: Some(if item.declaration {
                        DocumentHighlightKind::WRITE
                    } else {
                        DocumentHighlightKind::READ
                    }),
                })
                .collect(),
        ))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let Some(document) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        let offset = position_to_offset(document.source(), params.position);
        let Some((token, symbol)) = rename_target(&document, offset) else {
            return Ok(None);
        };
        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: span_to_range(document.source(), token.span),
            placeholder: symbol.name.clone(),
        }))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let Some(document) = self.document(uri).await else {
            return Ok(None);
        };
        let offset = position_to_offset(document.source(), params.text_document_position.position);
        let edits = rename_symbol_edits(&document, offset, &params.new_name)
            .map_err(Error::invalid_params)?;
        let Some(edits) = edits else {
            return Ok(None);
        };

        Ok(Some(WorkspaceEdit {
            changes: Some(HashMap::from([(uri.clone(), edits)])),
            ..WorkspaceEdit::default()
        }))
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

    #[allow(deprecated)]
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query.to_ascii_lowercase();
        let documents = self.documents.read().await;
        let mut result = Vec::new();
        for (uri, document) in documents.iter() {
            for symbol in document.symbols().values() {
                if !query.is_empty() && !symbol.name.to_ascii_lowercase().contains(&query) {
                    continue;
                }
                result.push(SymbolInformation {
                    name: symbol.name.clone(),
                    kind: lsp_symbol_kind(symbol.kind),
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: uri.clone(),
                        range: span_to_range(document.source(), symbol.span),
                    },
                    container_name: None,
                });
            }
        }
        Ok(Some(result))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let Some(document) = self.document(uri).await else {
            return Ok(None);
        };
        Ok(Some(code_actions(
            &document,
            uri,
            params.range,
            &self.knowledge,
        )))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Some(document) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: semantic_tokens(&document, None, &self.knowledge),
        })))
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let Some(document) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data: semantic_tokens(&document, Some(params.range), &self.knowledge),
        })))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let Some(document) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        let labels = document
            .lines()
            .iter()
            .enumerate()
            .filter(|(_, line)| matches!(line.kind, LineKind::Label { .. }))
            .collect::<Vec<_>>();
        let mut ranges = Vec::new();
        for (position, (index, _)) in labels.iter().enumerate() {
            let end = labels
                .get(position + 1)
                .map_or(document.lines().len(), |(next, _)| *next)
                .saturating_sub(1);
            if end > *index {
                ranges.push(FoldingRange {
                    start_line: *index as u32,
                    start_character: None,
                    end_line: end as u32,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Region),
                    collapsed_text: None,
                });
            }
        }
        Ok(Some(ranges))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let Some(document) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        let mut hints = Vec::new();
        for occurrence in document
            .occurrences()
            .iter()
            .filter(|occurrence| !occurrence.declaration)
        {
            let position = offset_to_position(document.source(), occurrence.span.end);
            if position < params.range.start || position > params.range.end {
                continue;
            }
            let Some(symbol) = document.symbol(&occurrence.name) else {
                continue;
            };
            let Some(value) = symbol.value.as_deref() else {
                continue;
            };
            hints.push(InlayHint {
                position,
                label: InlayHintLabel::String(format!(" = {value}")),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: Some(InlayHintTooltip::String(format!(
                    "Resolved {} `{}`",
                    match symbol.kind {
                        SymbolKind::Alias => "alias",
                        SymbolKind::Define => "define",
                        SymbolKind::Label => "label",
                    },
                    symbol.name
                ))),
                padding_left: Some(true),
                padding_right: Some(false),
                data: None,
            });
        }
        Ok(Some(hints))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let Some(document) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        let options = FormatOptions {
            indent_size: params.options.tab_size as usize,
            insert_spaces: params.options.insert_spaces,
            directives_at_column_zero: true,
        };
        let formatted = document.format(options);
        if formatted == document.source() {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(vec![TextEdit {
            range: Range::new(
                Position::new(0, 0),
                offset_to_position(document.source(), document.source().len()),
            ),
            new_text: formatted,
        }]))
    }
}

fn rename_target(document: &Document, offset: usize) -> Option<(&ic10_core::Token, &Symbol)> {
    let token = document.token_at_offset(offset)?;
    let occurrence = document.symbol_occurrence_at_offset(offset)?;
    let symbol = document.symbol(&occurrence.name)?;
    Some((token, symbol))
}

fn rename_symbol_edits(
    document: &Document,
    offset: usize,
    new_name: &str,
) -> std::result::Result<Option<Vec<TextEdit>>, String> {
    let Some((_, symbol)) = rename_target(document, offset) else {
        return Ok(None);
    };
    if !is_identifier(new_name) {
        return Err(format!(
            "`{new_name}` is not a valid IC10 symbol name. Use letters, numbers, and underscores, starting with a letter or underscore."
        ));
    }
    if new_name != symbol.name
        && let Some(existing) = document.symbol(new_name)
    {
        let (article, kind) = match existing.kind {
            SymbolKind::Alias => ("an", "alias"),
            SymbolKind::Define => ("a", "define"),
            SymbolKind::Label => ("a", "label"),
        };
        return Err(format!(
            "Cannot rename `{}` to `{new_name}` because `{new_name}` is already declared as {article} {kind}.",
            symbol.name
        ));
    }

    let edits = document
        .occurrences_for(&symbol.name)
        .map(|occurrence| TextEdit {
            range: span_to_range(document.source(), occurrence.span),
            new_text: new_name.to_owned(),
        })
        .collect();
    Ok(Some(edits))
}

fn is_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn parse_unused_level(value: &str) -> UnusedDiagnosticLevel {
    match value {
        "off" => UnusedDiagnosticLevel::Off,
        "warning" => UnusedDiagnosticLevel::Warning,
        _ => UnusedDiagnosticLevel::Hint,
    }
}

fn lsp_symbol_kind(kind: SymbolKind) -> tower_lsp::lsp_types::SymbolKind {
    match kind {
        SymbolKind::Label => tower_lsp::lsp_types::SymbolKind::KEY,
        SymbolKind::Define => tower_lsp::lsp_types::SymbolKind::CONSTANT,
        SymbolKind::Alias => tower_lsp::lsp_types::SymbolKind::VARIABLE,
    }
}

fn semantic_token_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::KEYWORD,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::MACRO,
            SemanticTokenType::NUMBER,
            SemanticTokenType::ENUM_MEMBER,
            SemanticTokenType::FUNCTION,
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::READONLY,
            SemanticTokenModifier::DEPRECATED,
            SemanticTokenModifier::MODIFICATION,
        ],
    }
}

fn semantic_tokens(
    document: &Document,
    requested_range: Option<Range>,
    knowledge: &KnowledgeBase,
) -> Vec<SemanticToken> {
    let mut absolute = Vec::<(Position, u32, u32, u32)>::new();
    for line in document.lines() {
        match &line.kind {
            LineKind::Empty => {}
            LineKind::Label { name } => {
                absolute.push(semantic_token(document, name.span, 6, 1));
            }
            LineKind::Instruction { mnemonic, operands } => {
                let modifiers = if knowledge
                    .instruction(&mnemonic.text)
                    .is_some_and(|instruction| instruction.deprecated)
                {
                    1 << 2
                } else {
                    0
                };
                absolute.push(semantic_token(document, mnemonic.span, 0, modifiers));
                let instruction = knowledge.instruction(&mnemonic.text);
                for (index, operand) in operands.iter().enumerate() {
                    let occurrence = document
                        .occurrences()
                        .iter()
                        .find(|occurrence| occurrence.span == operand.span);
                    let (token_type, modifiers) = if let Some(occurrence) = occurrence {
                        let token_type = match occurrence.kind {
                            SymbolKind::Label => 6,
                            SymbolKind::Define => 4,
                            SymbolKind::Alias => 1,
                        };
                        (
                            token_type,
                            if occurrence.declaration {
                                1 | if occurrence.kind == SymbolKind::Define {
                                    1 << 1
                                } else {
                                    0
                                }
                            } else {
                                0
                            },
                        )
                    } else if parse_literal_macro(&operand.text).is_some() {
                        (3, 1 << 1)
                    } else if parse_numeric_literal(&operand.text).is_some() {
                        (4, 1 << 1)
                    } else if knowledge.enum_value(&operand.text).is_some() {
                        (5, 1 << 1)
                    } else if register_markdown(&operand.text, knowledge).is_some() {
                        let write = instruction
                            .and_then(|value| value.operands.get(index))
                            .is_some_and(|value| value.operand_type == "r?");
                        (1, if write { 1 << 3 } else { 0 })
                    } else if device_reference_markdown(&operand.text, knowledge).is_some() {
                        (2, 0)
                    } else {
                        continue;
                    };
                    absolute.push(semantic_token(
                        document,
                        operand.span,
                        token_type,
                        modifiers,
                    ));
                }
            }
        }
    }
    absolute.retain(|(position, length, _, _)| {
        requested_range.is_none_or(|range| {
            *position >= range.start
                && Position::new(position.line, position.character + *length) <= range.end
        })
    });
    absolute.sort_by_key(|(position, _, _, _)| (position.line, position.character));
    let mut previous = Position::new(0, 0);
    absolute
        .into_iter()
        .map(|(position, length, token_type, token_modifiers_bitset)| {
            let delta_line = position.line - previous.line;
            let delta_start = if delta_line == 0 {
                position.character - previous.character
            } else {
                position.character
            };
            previous = position;
            SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset,
            }
        })
        .collect()
}

fn semantic_token(
    document: &Document,
    span: Span,
    token_type: u32,
    modifiers: u32,
) -> (Position, u32, u32, u32) {
    (
        offset_to_position(document.source(), span.start),
        document.source()[span.start..span.end]
            .encode_utf16()
            .count() as u32,
        token_type,
        modifiers,
    )
}

fn code_actions(
    document: &Document,
    uri: &Url,
    requested_range: Range,
    knowledge: &KnowledgeBase,
) -> CodeActionResponse {
    let mut actions = Vec::new();
    for diagnostic in document.diagnostics() {
        let diagnostic_range = span_to_range(document.source(), diagnostic.span);
        if !ranges_overlap(diagnostic_range, requested_range) {
            continue;
        }
        if matches!(
            diagnostic.code,
            "unused-label" | "unused-alias" | "unused-define" | "unreachable-code"
        ) {
            let Some(line) = document.line_at_offset(diagnostic.span.start) else {
                continue;
            };
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: "Remove unused code (preserve line numbering)".to_owned(),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: None,
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(
                        uri.clone(),
                        vec![TextEdit {
                            range: span_to_range(document.source(), line.code_span),
                            new_text: String::new(),
                        }],
                    )])),
                    ..WorkspaceEdit::default()
                }),
                is_preferred: Some(true),
                ..CodeAction::default()
            }));
        } else if diagnostic.code == "unknown-instruction"
            && let Some(token) = document.token_at_offset(diagnostic.span.start)
            && let Some(replacement) = closest_instruction(&token.text, knowledge)
        {
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Change to `{replacement}`"),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: None,
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from([(
                        uri.clone(),
                        vec![TextEdit {
                            range: diagnostic_range,
                            new_text: replacement.to_owned(),
                        }],
                    )])),
                    ..WorkspaceEdit::default()
                }),
                is_preferred: Some(true),
                ..CodeAction::default()
            }));
        }
    }
    actions
}

fn ranges_overlap(left: Range, right: Range) -> bool {
    left.start <= right.end && right.start <= left.end
}

fn closest_instruction<'a>(value: &str, knowledge: &'a KnowledgeBase) -> Option<&'a str> {
    knowledge
        .language
        .instructions
        .keys()
        .map(String::as_str)
        .filter_map(|candidate| {
            let distance = edit_distance(value, candidate);
            (distance <= 2).then_some((distance, candidate))
        })
        .min_by_key(|(distance, candidate)| (*distance, *candidate))
        .map(|(_, candidate)| candidate)
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    for (left_index, left_byte) in left.bytes().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_byte) in right.bytes().enumerate() {
            current.push(
                (previous[right_index + 1] + 1)
                    .min(current[right_index] + 1)
                    .min(previous[right_index] + usize::from(left_byte != right_byte)),
            );
        }
        previous = current;
    }
    previous[right.len()]
}

fn simple_completion(label: String, kind: CompletionItemKind, detail: &str) -> CompletionItem {
    CompletionItem {
        label,
        kind: Some(kind),
        detail: Some(detail.to_owned()),
        ..CompletionItem::default()
    }
}

fn literal_macro_completion(name: &str, detail: &str, documentation: &str) -> CompletionItem {
    CompletionItem {
        label: format!("{name}(\"…\")"),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some(detail.to_owned()),
        documentation: Some(Documentation::String(documentation.to_owned())),
        insert_text: Some(format!("{name}(\"${{1:text}}\")")),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        filter_text: Some(name.to_owned()),
        sort_text: Some(format!("1-{name}")),
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

fn instruction_markdown(
    name: &str,
    instruction: &Instruction,
    knowledge: &KnowledgeBase,
) -> String {
    let deprecated = if instruction.deprecated {
        "\n\n> Deprecated"
    } else {
        ""
    };
    let description = description_markdown(&instruction.description, knowledge, false);
    format!(
        "### `{name}`\n\n```ic10\n{}\n```\n\n{}{}",
        instruction.syntax, description, deprecated
    )
}

fn register_markdown(token: &str, knowledge: &KnowledgeBase) -> Option<String> {
    let architecture = &knowledge.language.architecture;
    if let Some(number) = token
        .strip_prefix('r')
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|number| *number <= 15)
    {
        return Some(format!(
            "### `{token}`\n\n**General-purpose register {number}**\n\n\
             Stores one {} numeric value. `{token}` is one of the {} general-purpose \
             registers and can be given a descriptive name with `alias`.",
            architecture.numeric_storage, architecture.general_registers
        ));
    }
    if token == architecture.stack_pointer_register {
        return Some(format!(
            "### `{token}`\n\n**Stack pointer register**\n\n\
             Tracks the current position in the IC housing's {}-value stack. `push` \
             writes at the stack pointer and increments it; `pop` reads the top value \
             and decrements it; `peek` reads the top value without changing `{token}`. \
             The pointer can also be read or written like a register.",
            architecture.stack_size
        ));
    }
    if token == architecture.return_address_register {
        return Some(format!(
            "### `{token}`\n\n**Return-address register**\n\n\
             `jal` and branch-and-link instructions store the next program line in \
             `{token}`. Jumping with `j {token}` returns execution to that saved line. \
             It can also be read or written like a register."
        ));
    }
    None
}

fn device_reference_markdown(token: &str, knowledge: &KnowledgeBase) -> Option<String> {
    let architecture = &knowledge.language.architecture;
    if token == architecture.base_device {
        return Some(format!(
            "### `{token}`\n\n**Base-device reference**\n\n\
             Refers to the device that is executing the IC10 program—normally the IC \
             Housing containing the chip. It can be used wherever an instruction accepts \
             a device reference and can be given a descriptive name with `alias`."
        ));
    }
    if let Some(number) = token
        .strip_prefix('d')
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|number| *number <= 5)
    {
        return Some(format!(
            "### `{token}`\n\n**Device pin {number}**\n\n\
             Refers to the device assigned to the `{token}` connection on the IC Housing. \
             `{token}` is one of the {} device references and can be given a descriptive \
             name with `alias`.",
            architecture.device_pins
        ));
    }
    None
}

fn device_from_token<'a>(token: &str, knowledge: &'a KnowledgeBase) -> Option<&'a Device> {
    if let Some(name) = hash_macro_name(token) {
        return knowledge.device_by_name(name);
    }
    if let Some(device) = knowledge.device_by_name(token) {
        return Some(device);
    }
    let normalized = token.replace('_', "");
    let hash = normalized.parse::<i32>().ok()?;
    knowledge.device_by_hash(hash)
}

fn resource_from_token<'a>(token: &str, knowledge: &'a KnowledgeBase) -> Option<&'a Resource> {
    if let Some(name) = hash_macro_name(token) {
        return knowledge.resource_by_name(name);
    }
    if let Some(resource) = knowledge.resource_by_name(token) {
        return Some(resource);
    }
    let normalized = token.replace('_', "");
    let hash = normalized.parse::<i32>().ok()?;
    knowledge.resource_by_hash(hash)
}

fn reagent_from_token<'a>(token: &str, knowledge: &'a KnowledgeBase) -> Option<&'a Reagent> {
    if let Some(name) = hash_macro_name(token) {
        return knowledge.reagent_by_name(name);
    }
    if let Some(reagent) = knowledge.reagent_by_name(token) {
        return Some(reagent);
    }
    let normalized = token.replace('_', "");
    let hash = normalized.parse::<i32>().ok()?;
    knowledge.reagent_by_hash(hash)
}

fn hash_macro_name(token: &str) -> Option<&str> {
    token
        .strip_prefix("HASH(\"")
        .and_then(|value| value.strip_suffix("\")"))
}

fn symbol_markdown(symbol: &Symbol, knowledge: &KnowledgeBase, asset_uri: Option<&str>) -> String {
    let kind = match symbol.kind {
        SymbolKind::Label => "Label",
        SymbolKind::Define => "Define",
        SymbolKind::Alias => "Alias",
    };
    let value = symbol.value.as_deref().map_or(String::new(), |value| {
        let hash = parse_literal_macro(value)
            .filter(|literal| literal.kind == LiteralMacroKind::Hash)
            .map_or(String::new(), |literal| {
                format!(
                    "\n\n**Hash:** `{}`",
                    stationeers_crc32(&literal.value) as i32
                )
            });
        format!("\n\n**Value:** `{value}`{hash}")
    });
    let known_prefab = (symbol.kind == SymbolKind::Define)
        .then_some(symbol.value.as_deref())
        .flatten()
        .and_then(|value| {
            device_from_token(value, knowledge)
                .map(|device| {
                    (
                        device.display_name.as_str(),
                        device.prefab_name.as_str(),
                        device.image.as_deref(),
                    )
                })
                .or_else(|| {
                    resource_from_token(value, knowledge).map(|resource| {
                        (
                            resource.display_name.as_str(),
                            resource.prefab_name.as_str(),
                            resource.image.as_deref(),
                        )
                    })
                })
        });
    let (image, resolved) = known_prefab.map_or_else(
        || (String::new(), String::new()),
        |(display_name, prefab_name, image)| {
            (
                image_markdown(display_name, image, asset_uri),
                format!(
                    "\n\n**Friendly name:** {}\n\n**Prefab name:** `{}`",
                    markdown_escape(display_name),
                    markdown_escape(prefab_name)
                ),
            )
        },
    );
    format!(
        "{image}### `{}`\n\n**{kind}**{value}{resolved}",
        symbol.name
    )
}

fn literal_macro_markdown(literal: &LiteralMacro) -> String {
    match literal.kind {
        LiteralMacroKind::Hash => format!(
            "### <code>HASH(&quot;{}&quot;)</code>\n\n\
             **CRC-32 hash literal**\n\n{}\n\n\
             `HASH` computes the CRC-32 checksum of the string's UTF-8 bytes. \
             Signed and unsigned decimal are two representations of the same 32 bits.",
            html_escape(&literal.value),
            crc32_summary(&literal.value)
        ),
        LiteralMacroKind::String => {
            let heading = format!(
                "### <code>STR(&quot;{}&quot;)</code>",
                html_escape(&literal.value)
            );
            match pack_stationeers_string(&literal.value) {
                Ok(packed) => {
                    let width = (literal.value.len() * 2).max(2);
                    format!(
                        "{heading}\n\n**Packed display string**\n\n\
                         | Characters | Packed decimal | IC10 hex |\n\
                         |--:|--:|:--|\n\
                         | `{}` / `6` | `{packed}` | `${packed:0width$X}` |\n\n\
                         `STR` packs ASCII text from left to right using eight bits per \
                         character. Write the resulting number to a compatible display \
                         while that display is in String mode.",
                        literal.value.len()
                    )
                }
                Err(PackedStringError::NonAscii) => format!(
                    "{heading}\n\n> **Invalid `STR` literal:** only ASCII characters are \
                     supported because every character must fit in one byte."
                ),
                Err(PackedStringError::TooLong { length }) => format!(
                    "{heading}\n\n> **Invalid `STR` literal:** at most six characters are \
                     supported; this literal contains {length}."
                ),
            }
        }
    }
}

fn crc32_summary(value: &str) -> String {
    let unsigned = stationeers_crc32(value);
    let signed = unsigned as i32;
    format!(
        "**Computed CRC-32:** signed `{signed}` · unsigned `{unsigned}` · \
         IC10 hex `${unsigned:08X}`"
    )
}

fn device_markdown(
    device: &Device,
    knowledge: &KnowledgeBase,
    asset_uri: Option<&str>,
    show_crc: bool,
) -> String {
    let image = image_markdown(&device.display_name, device.image.as_deref(), asset_uri);
    let crc = if show_crc {
        format!("\n\n{}", crc32_summary(&device.prefab_name))
    } else {
        String::new()
    };
    let description = if device.description.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n{}",
            description_markdown(&device.description, knowledge, false)
        )
    };
    let mut logic: Vec<_> = device.logic_types.iter().collect();
    logic.sort_by(|(left_name, left_access), (right_name, right_access)| {
        logic_access_rank(left_access.read, left_access.write)
            .cmp(&logic_access_rank(right_access.read, right_access.write))
            .then_with(|| left_name.cmp(right_name))
    });
    let rows = logic
        .iter()
        .take(24)
        .map(|(name, access)| {
            let access = match (access.read, access.write) {
                (true, true) => "**R / W**",
                (true, false) => "**R**",
                (false, true) => "**W**",
                (false, false) => "—",
            };
            let definition = knowledge
                .language
                .enums
                .get("LogicType")
                .and_then(|listing| listing.values.get(*name));
            let id = definition
                .map(|value| format!("`{}`", value.value))
                .unwrap_or_else(|| "—".to_owned());
            let mut details = definition.map_or_else(
                || "—".to_owned(),
                |value| {
                    if value.description.is_empty() {
                        "—".to_owned()
                    } else {
                        description_markdown(&value.description, knowledge, *name == "Color")
                    }
                },
            );
            if *name == "Mode" && !device.modes.is_empty() {
                details.push_str("<br>**Values:** ");
                details.push_str(&device_mode_values_markdown(device));
            }
            format!(
                "| `{}`&nbsp;&nbsp;&nbsp; | {id}&nbsp;&nbsp;&nbsp; | \
                 {access}&nbsp;&nbsp;&nbsp; | {} |",
                markdown_escape(name),
                markdown_table_cell(&details)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let remaining = logic.len().saturating_sub(24);
    let suffix = if remaining > 0 {
        format!(
            "\n\n_{remaining} additional parameters are omitted to keep this hover manageable._"
        )
    } else {
        String::new()
    };
    format!(
        "{image}### {}\n\n`{}` · **PrefabHash** `{}`{crc}{description}\n\n\
         **Logic parameters ({})** · values use {} storage\n\n\
         **R** = read · **W** = write\n\n\
         | Parameter&nbsp;&nbsp;&nbsp; | Logic&nbsp;ID&nbsp;&nbsp;&nbsp; | \
         Access&nbsp;&nbsp;&nbsp; | Description |\n\
         |:--|--:|:--|:--|\n\
         {rows}{suffix}",
        markdown_escape(&device.display_name),
        device.prefab_name,
        device.prefab_hash,
        logic.len(),
        knowledge.language.architecture.numeric_storage,
    )
}

fn logic_access_rank(read: bool, write: bool) -> u8 {
    match (read, write) {
        (true, true) => 0,
        (false, true) => 1,
        (true, false) => 2,
        (false, false) => 3,
    }
}

fn device_mode_values_markdown(device: &Device) -> String {
    let mut modes: Vec<_> = device.modes.iter().collect();
    modes.sort_by(|(left_name, left_value), (right_name, right_value)| {
        json_scalar_sort_key(left_value)
            .cmp(&json_scalar_sort_key(right_value))
            .then_with(|| left_name.cmp(right_name))
    });
    modes
        .into_iter()
        .map(|(name, value)| {
            format!(
                "`{}` = `{}`",
                markdown_escape(&json_scalar_display(value)),
                markdown_escape(name)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn json_scalar_display(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn json_scalar_sort_key(value: &serde_json::Value) -> (u8, String) {
    match value {
        serde_json::Value::Number(number) => (
            0,
            format!("{:020.8}", number.as_f64().unwrap_or(f64::INFINITY)),
        ),
        serde_json::Value::String(value) => (1, value.clone()),
        other => (2, other.to_string()),
    }
}

fn resource_markdown(
    resource: &Resource,
    knowledge: &KnowledgeBase,
    asset_uri: Option<&str>,
    show_crc: bool,
) -> String {
    let image = image_markdown(&resource.display_name, resource.image.as_deref(), asset_uri);
    let crc = if show_crc {
        format!("\n\n{}", crc32_summary(&resource.prefab_name))
    } else {
        String::new()
    };
    let description = if resource.description.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n{}",
            description_markdown(&resource.description, knowledge, false)
        )
    };
    let category = match resource.kind.as_str() {
        "ingot" => "Ingot",
        "ice" => "Ice",
        other => other,
    };
    let maximum = resource.max_quantity.map_or(String::new(), |quantity| {
        format!(" · Maximum stack `{quantity}`")
    });
    let composition = if resource.reagents.is_empty() {
        String::new()
    } else {
        let values = resource
            .reagents
            .iter()
            .map(|(name, quantity)| {
                let unit = knowledge
                    .reagent_by_name(name)
                    .map_or("", |reagent| reagent.unit.as_str());
                format!("`{name}` {quantity}{unit}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("\n\n**Composition:** {values}")
    };
    let gases = if resource.gases.is_empty() {
        String::new()
    } else {
        let values = resource
            .gases
            .iter()
            .map(|gas| {
                format!(
                    "`{}` quantity {}, {} K",
                    gas.gas_type, gas.quantity, gas.temperature
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        format!("\n\n**Gas contents:** {values}")
    };
    format!(
        "{image}### {}\n\n**{category}** · `{}` · **PrefabHash** `{}`{maximum}{}{}{}{}",
        markdown_escape(&resource.display_name),
        resource.prefab_name,
        resource.prefab_hash,
        crc,
        description,
        composition,
        gases
    )
}

fn reagent_markdown(
    reagent: &Reagent,
    knowledge: &KnowledgeBase,
    asset_uri: Option<&str>,
    show_crc: bool,
) -> String {
    let pictured_source = reagent
        .sources
        .keys()
        .find_map(|name| knowledge.resource_by_name(name));
    let image = pictured_source.map_or(String::new(), |resource| {
        image_markdown(&resource.display_name, resource.image.as_deref(), asset_uri)
    });
    let sources = reagent
        .sources
        .iter()
        .map(|(name, quantity)| {
            knowledge.resource_by_name(name).map_or_else(
                || format!("`{name}` × {quantity}"),
                |resource| {
                    format!(
                        "{} (`{name}`) × {quantity}",
                        markdown_escape(&resource.display_name)
                    )
                },
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let crc = if show_crc {
        format!("\n\n{}", crc32_summary(&reagent.name))
    } else {
        String::new()
    };
    format!(
        "{image}### `{}`\n\n**Reagent** · `HASH(\"{}\")` = `{}` · **ID** `{}` · **Unit** `{}`\
         {}\n\n**Sources:** {}",
        reagent.name, reagent.name, reagent.hash, reagent.id, reagent.unit, crc, sources
    )
}

fn image_markdown(display_name: &str, image: Option<&str>, asset_uri: Option<&str>) -> String {
    match (asset_uri, image) {
        (Some(base), Some(image)) => format!(
            "<img src=\"{}/{}\" alt=\"{}\" width=\"96\" align=\"right\">\n\n",
            html_escape(base.trim_end_matches('/')),
            html_escape(image),
            html_escape(display_name)
        ),
        _ => String::new(),
    }
}

fn description_markdown(value: &str, knowledge: &KnowledgeBase, colorize: bool) -> String {
    let mut result = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < value.len() {
        let character = value[cursor..]
            .chars()
            .next()
            .expect("cursor should be at a character boundary");
        if is_description_word_character(character) {
            let start = cursor;
            cursor += character.len_utf8();
            while cursor < value.len() {
                let next = value[cursor..]
                    .chars()
                    .next()
                    .expect("cursor should be at a character boundary");
                if !is_description_word_character(next) {
                    break;
                }
                cursor += next.len_utf8();
            }
            let word = &value[start..cursor];
            if colorize && let Some(color) = knowledge.hover.colors.get(word) {
                result.push_str(&format!(
                    "<span style=\"color:{};background-color:{};border-radius:3px;\">{}</span>",
                    html_escape(&color.foreground),
                    html_escape(&color.background),
                    html_escape(word)
                ));
            } else if is_hover_keyword(word, knowledge) {
                result.push('`');
                result.push_str(word);
                result.push('`');
            } else {
                result.push_str(&markdown_escape(word));
            }
        } else {
            let end = cursor + character.len_utf8();
            result.push_str(&markdown_escape(&value[cursor..end]));
            cursor = end;
        }
    }
    result
}

fn is_description_word_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '?')
}

fn is_hover_keyword(word: &str, knowledge: &KnowledgeBase) -> bool {
    if knowledge
        .hover
        .keywords
        .iter()
        .any(|keyword| keyword == word)
    {
        return true;
    }
    let is_register = word
        .strip_prefix('r')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| number <= 15);
    let is_device = word
        .strip_prefix('d')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| number <= 5);
    let is_unambiguous_instruction = word.len() >= 3
        && !matches!(word, "add" | "and" | "get" | "move" | "not")
        && knowledge.instruction(word).is_some();
    is_register || is_device || is_unambiguous_instruction
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn markdown_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn markdown_table_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\r', '\n'], "<br>")
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
    use ic10_core::Document;
    use ic10_data::KnowledgeBase;
    use tower_lsp::lsp_types::{Position, Range, TextEdit, Url};

    use super::{
        code_actions, description_markdown, device_from_token, device_markdown,
        device_reference_markdown, literal_macro_markdown, offset_to_position, parse_literal_macro,
        position_to_offset, reagent_from_token, reagent_markdown, register_markdown,
        rename_symbol_edits, resource_from_token, resource_markdown, semantic_tokens,
        symbol_markdown,
    };

    fn knowledge() -> KnowledgeBase {
        KnowledgeBase::load_embedded().expect("embedded data")
    }

    #[test]
    fn converts_utf16_positions() {
        let source = "move r0 1 # °\nnext";
        let offset = source.find('°').expect("test character");
        let position = offset_to_position(source, offset);

        assert_eq!(position_to_offset(source, position), offset);
        assert_eq!(position, Position::new(0, 12));
    }

    #[test]
    fn semantic_tokens_and_safe_actions_tolerate_utf16_and_incomplete_lines() {
        let knowledge = knowledge();
        let source = "# ° unicode\nunused:\nadd r0 r\n";
        let document = Document::parse(source, &knowledge);
        let tokens = semantic_tokens(&document, None, &knowledge);
        assert!(!tokens.is_empty());
        assert!(
            !document
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "invalid-operand")
        );

        let uri = Url::parse("file:///program.ic10").expect("URI");
        let actions = code_actions(
            &document,
            &uri,
            Range::new(Position::new(1, 0), Position::new(1, 8)),
            &knowledge,
        );
        let edit = match &actions[0] {
            tower_lsp::lsp_types::CodeActionOrCommand::CodeAction(action) => action
                .edit
                .as_ref()
                .and_then(|edit| edit.changes.as_ref())
                .and_then(|changes| changes.get(&uri))
                .and_then(|edits| edits.first())
                .expect("safe removal edit"),
            tower_lsp::lsp_types::CodeActionOrCommand::Command(_) => panic!("expected code action"),
        };
        let updated = apply_text_edits(source, std::slice::from_ref(edit));
        assert_eq!(updated.lines().count(), source.lines().count());
        assert_eq!(updated, "# ° unicode\n\nadd r0 r\n");
    }

    #[test]
    fn renames_define_references_without_touching_partial_or_literal_matches() {
        let source = "define DOOR -111111111\n\
                      define DOORWAY -222222222\n\
                      push DOOR\n\
                      push DOORWAY\n\
                      push HASH(\"DOOR\")\n\
                      # DOOR\n";
        let document = Document::parse(source, &knowledge());
        let usage = source.find("push DOOR\n").expect("DOOR usage") + 7;
        let edits = rename_symbol_edits(&document, usage, "GATE")
            .expect("valid rename")
            .expect("renameable define");

        assert_eq!(edits.len(), 2);
        assert_eq!(
            apply_text_edits(source, &edits),
            "define GATE -111111111\n\
             define DOORWAY -222222222\n\
             push GATE\n\
             push DOORWAY\n\
             push HASH(\"DOOR\")\n\
             # DOOR\n"
        );
    }

    #[test]
    fn renames_alias_declarations_and_references() {
        let source = "alias SENSOR d0\nl r0 SENSOR Setting\n";
        let document = Document::parse(source, &knowledge());
        let declaration = source.find("SENSOR").expect("SENSOR declaration") + 2;
        let edits = rename_symbol_edits(&document, declaration, "PRESSURE_SENSOR")
            .expect("valid rename")
            .expect("renameable alias");

        assert_eq!(edits.len(), 2);
        assert_eq!(
            apply_text_edits(source, &edits),
            "alias PRESSURE_SENSOR d0\nl r0 PRESSURE_SENSOR Setting\n"
        );
    }

    #[test]
    fn rejects_rename_collisions_and_invalid_names() {
        let source = "define DOOR -111111111\nalias GATE d0\nexisting:\n";
        let document = Document::parse(source, &knowledge());
        let door = source.find("DOOR").expect("DOOR declaration");

        let alias_collision =
            rename_symbol_edits(&document, door, "GATE").expect_err("alias collision");
        assert!(alias_collision.contains("already declared as an alias"));

        let label_collision =
            rename_symbol_edits(&document, door, "existing").expect_err("label collision");
        assert!(label_collision.contains("already declared as a label"));

        for invalid in ["", "1DOOR", "TWO WORDS", "DOOR-2"] {
            let error =
                rename_symbol_edits(&document, door, invalid).expect_err("invalid symbol name");
            assert!(error.contains("not a valid IC10 symbol name"));
        }
    }

    #[test]
    fn renames_label_declarations_without_replacing_the_colon() {
        let source = "mainLoop:\nbeqz r0 mainLoop\nj mainLoop\n# mainLoop\n";
        let document = Document::parse(source, &knowledge());
        let label = source.find("mainLoop").expect("label declaration") + 3;
        let edits = rename_symbol_edits(&document, label, "main")
            .expect("valid rename")
            .expect("renameable label");

        assert_eq!(edits.len(), 3);
        assert_eq!(
            apply_text_edits(source, &edits),
            "main:\nbeqz r0 main\nj main\n# mainLoop\n"
        );
    }

    fn apply_text_edits(source: &str, edits: &[TextEdit]) -> String {
        let mut replacements = edits
            .iter()
            .map(|edit| {
                (
                    position_to_offset(source, edit.range.start),
                    position_to_offset(source, edit.range.end),
                    edit.new_text.as_str(),
                )
            })
            .collect::<Vec<_>>();
        replacements.sort_unstable_by_key(|(start, _, _)| std::cmp::Reverse(*start));

        let mut result = source.to_owned();
        for (start, end, new_text) in replacements {
            result.replace_range(start..end, new_text);
        }
        result
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

    #[test]
    fn presents_device_logic_as_a_documented_access_table() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data");
        let device = knowledge
            .device_by_name("StructurePoweredVentLarge")
            .expect("large powered vent");
        let rendered = device_markdown(device, &knowledge, Some("file:///assets"), false);

        assert!(rendered.contains(
            "| Parameter&nbsp;&nbsp;&nbsp; | Logic&nbsp;ID&nbsp;&nbsp;&nbsp; | \
             Access&nbsp;&nbsp;&nbsp; | Description |"
        ));
        assert!(rendered.contains("**R** = read · **W** = write"));
        assert!(rendered.contains(
            "| `Mode`&nbsp;&nbsp;&nbsp; | `3`&nbsp;&nbsp;&nbsp; | \
             **R / W**&nbsp;&nbsp;&nbsp; |"
        ));
        assert!(rendered.contains("`0` = `Outward`; `1` = `Inward`"));
        assert!(rendered.contains(
            "| `Power`&nbsp;&nbsp;&nbsp; | `1`&nbsp;&nbsp;&nbsp; | \
             **R**&nbsp;&nbsp;&nbsp; |"
        ));
        assert!(rendered.contains("correctly powered"));
        assert!(
            rendered.find("| `Mode`").expect("writable Mode row")
                < rendered.find("| `Power`").expect("read-only Power row")
        );
        assert!(rendered.contains("18 additional parameters are omitted"));
        assert!(rendered.contains("IEEE 754 double storage"));
    }

    #[test]
    fn describes_hash_and_packed_string_literals() {
        let hash = parse_literal_macro("HASH(\"Iron\")").expect("HASH literal");
        let hash_hover = literal_macro_markdown(&hash);
        assert!(hash_hover.contains("CRC-32 hash literal"));
        assert!(hash_hover.contains("signed `-666742878`"));
        assert!(hash_hover.contains("unsigned `3628224418`"));
        assert!(hash_hover.contains("$D8424FA2"));

        let string = parse_literal_macro("STR(\"Hello!\")").expect("STR literal");
        let string_hover = literal_macro_markdown(&string);
        assert!(string_hover.contains("Packed display string"));
        assert!(string_hover.contains("`79600447942433`"));
        assert!(string_hover.contains("$48656C6C6F21"));

        let too_long = parse_literal_macro("STR(\"1234567\")").expect("long STR literal");
        assert!(literal_macro_markdown(&too_long).contains("at most six characters"));
    }

    #[test]
    fn shows_computed_hash_for_hash_backed_defines() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data");
        let document = ic10_core::Document::parse(
            "define ANAME HASH(\"CONTROL ROOM\")\n\
             define LEVEL 2\n\
             define DOOR -793837322\n",
            &knowledge,
        );

        let rendered = symbol_markdown(
            document.symbol("ANAME").expect("ANAME define"),
            &knowledge,
            Some("file:///assets"),
        );
        assert!(rendered.contains("**Value:** `HASH(\"CONTROL ROOM\")`"));
        assert!(rendered.contains("**Hash:** `-1358946780`"));

        let ordinary = symbol_markdown(
            document.symbol("LEVEL").expect("LEVEL define"),
            &knowledge,
            Some("file:///assets"),
        );
        assert!(ordinary.contains("**Value:** `2`"));
        assert!(!ordinary.contains("**Hash:**"));

        let door = symbol_markdown(
            document.symbol("DOOR").expect("DOOR define"),
            &knowledge,
            Some("file:///assets"),
        );
        assert!(door.contains("**Friendly name:** Composite Door"));
        assert!(door.contains("**Prefab name:** `StructureCompositeDoor`"));
        assert!(door.contains(
            "<img src=\"file:///assets/StructureCompositeDoor.png\" \
             alt=\"Composite Door\" width=\"96\" align=\"right\">"
        ));
        assert!(!door.contains("Logic parameters"));
        assert!(!door.contains("steep pressure differentials"));
    }

    #[test]
    fn describes_general_and_special_registers_from_architecture_data() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data");

        assert!(register_markdown("r0", &knowledge).is_some_and(|text| {
            text.contains("General-purpose register 0") && text.contains("IEEE 754 double")
        }));
        assert!(register_markdown("r15", &knowledge).is_some());
        assert!(register_markdown("r16", &knowledge).is_none());
        assert!(
            register_markdown("sp", &knowledge)
                .is_some_and(|text| text.contains("512-value stack") && text.contains("`push`"))
        );
        assert!(
            register_markdown("ra", &knowledge)
                .is_some_and(|text| text.contains("Return-address") && text.contains("`jal`"))
        );
    }

    #[test]
    fn describes_base_device_and_device_pin_references() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data");

        assert!(
            device_reference_markdown("db", &knowledge)
                .is_some_and(|text| text.contains("Base-device") && text.contains("IC Housing"))
        );
        assert!(
            device_reference_markdown("d0", &knowledge)
                .is_some_and(|text| text.contains("Device pin 0") && text.contains("d0-d5"))
        );
        assert!(device_reference_markdown("d5", &knowledge).is_some());
        assert!(device_reference_markdown("d6", &knowledge).is_none());
    }

    #[test]
    fn distinguishes_resource_prefab_hashes_from_reagent_hashes() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data");
        let ingot = resource_from_token("-1301215609", &knowledge)
            .expect("iron ingot prefab hash should resolve");
        let reagent =
            reagent_from_token("HASH(\"Iron\")", &knowledge).expect("iron reagent should resolve");

        assert_eq!(ingot.prefab_name, "ItemIronIngot");
        assert_eq!(reagent.name, "Iron");
        assert_ne!(ingot.prefab_hash, reagent.hash);
        assert!(
            resource_markdown(ingot, &knowledge, Some("file:///assets"), false).contains(
                "<img src=\"file:///assets/ItemIronIngot.png\" alt=\"Ingot (Iron)\" width=\"96\" align=\"right\">"
            )
        );
        assert!(
            reagent_markdown(reagent, &knowledge, Some("file:///assets"), true)
                .contains("ItemIronIngot.png")
        );
        assert!(reagent_markdown(reagent, &knowledge, None, true).contains("IC10 hex `$D8424FA2`"));
    }

    #[test]
    fn resolves_ice_prefab_names_and_hashes() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data");

        assert_eq!(
            resource_from_token("HASH(\"ItemOxite\")", &knowledge)
                .map(|resource| resource.prefab_hash),
            Some(-1_805_394_113)
        );
        assert_eq!(
            resource_from_token("1217489948", &knowledge)
                .map(|resource| resource.prefab_name.as_str()),
            Some("ItemIce")
        );
    }

    #[test]
    fn colorizes_every_named_color_and_formats_language_keywords() {
        let knowledge = KnowledgeBase::load_embedded().expect("embedded data");
        let color = knowledge
            .enum_value("Color")
            .map(|(_, value)| value)
            .expect("Color LogicType should exist");
        let rendered = description_markdown(&color.description, &knowledge, true);

        for name in knowledge.hover.colors.keys() {
            assert!(
                rendered.contains(&format!(">{name}</span>")),
                "{name} should be rendered as a color swatch"
            );
        }
        assert_eq!(rendered.matches(">Blue</span>").count(), 2);
        assert!(rendered.contains("background-color:#2563EB80"));

        let keywords =
            description_markdown("Use db, jal, ra, and a hash value.", &knowledge, false);
        assert!(keywords.contains("`db`"));
        assert!(keywords.contains("`jal`"));
        assert!(keywords.contains("`ra`"));
        assert!(keywords.contains("`hash`"));
        assert!(!keywords.contains("`and`"));
    }
}
