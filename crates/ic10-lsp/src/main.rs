use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use ic10_build::{BuildOptions, BuildOutput, build};
use ic10_core::{
    AnalysisOptions, Document, FormatOptions, LineKind, LiteralMacro, LiteralMacroKind,
    PackedStringError, Severity, Span, Symbol, SymbolKind, UnusedDiagnosticLevel,
    pack_stationeers_string, parse_literal_macro, parse_numeric_literal, stationeers_crc32,
};
use ic10_data::{Device, Instruction, KnowledgeBase, Reagent, Resource};
use ic10_sim::{
    AnalysisContext, EnvironmentTarget, ProgramUri, Scenario, ScenarioIndex,
    context_device_markdown, valid_logic_fields, validate_context,
};
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioChangedParams {
    scenario_uri: Url,
    version: i64,
    source: Option<String>,
    #[serde(default)]
    resolved_programs: BTreeMap<String, Url>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContextSelectionParams {
    program_uri: Url,
    scenario_uri: Option<Url>,
    ic_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextStatus {
    uri: Url,
    contexts: Vec<ContextStatusItem>,
    active: Option<ContextStatusItem>,
    ambiguous: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextStatusItem {
    scenario_uri: Url,
    ic_id: String,
    label: String,
}

enum ContextStatusNotification {}

impl notification::Notification for ContextStatusNotification {
    type Params = ContextStatus;
    const METHOD: &'static str = "ic10/contextStatus";
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildParams {
    uri: Url,
    #[serde(default)]
    options: BuildOptions,
}

struct Backend {
    client: Client,
    knowledge: Arc<KnowledgeBase>,
    documents: RwLock<HashMap<Url, Arc<Document>>>,
    asset_uri: RwLock<Option<String>>,
    analysis_options: RwLock<AnalysisOptions>,
    scenarios: RwLock<ScenarioIndex>,
    context_selections: RwLock<HashMap<Url, (Url, String)>>,
}

impl Backend {
    fn new(client: Client, knowledge: Arc<KnowledgeBase>) -> Self {
        Self {
            client,
            knowledge,
            documents: RwLock::new(HashMap::new()),
            asset_uri: RwLock::new(None),
            analysis_options: RwLock::new(AnalysisOptions::default()),
            scenarios: RwLock::new(ScenarioIndex::default()),
            context_selections: RwLock::new(HashMap::new()),
        }
    }

    async fn document(&self, uri: &Url) -> Option<Arc<Document>> {
        self.documents.read().await.get(uri).cloned()
    }

    async fn build_deployment(&self, params: BuildParams) -> Result<BuildOutput> {
        let document = self
            .document(&params.uri)
            .await
            .ok_or_else(|| Error::invalid_params("The IC10 document is not open."))?;
        build(document.source(), &params.options, &self.knowledge).map_err(|build_error| {
            Error::invalid_params(
                serde_json::to_string(&build_error.diagnostics)
                    .unwrap_or_else(|_| build_error.to_string()),
            )
        })
    }

    async fn update_document(&self, uri: Url, text: String) {
        let options = *self.analysis_options.read().await;
        let document = Arc::new(Document::parse_with_options(text, &self.knowledge, options));
        let mut diagnostics = document
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
            .collect::<Vec<_>>();
        if let Some(context) = self.active_context(&uri).await {
            diagnostics.extend(
                validate_context(&document, &context, &self.knowledge)
                    .into_iter()
                    .map(|diagnostic| Diagnostic {
                        range: span_to_range(document.source(), diagnostic.span),
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String(diagnostic.code.to_owned())),
                        source: Some("ic10 environment".to_owned()),
                        message: diagnostic.message,
                        data: diagnostic
                            .target
                            .and_then(|target| serde_json::to_value(target_payload(target)).ok()),
                        ..Diagnostic::default()
                    }),
            );
        }
        let budget = document.budget();
        self.documents.write().await.insert(uri.clone(), document);
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, None)
            .await;
        self.client
            .send_notification::<ProgramBudgetNotification>(ProgramBudgetParams {
                uri: uri.clone(),
                physical_lines: budget.physical_lines,
                program_lines: budget.program_lines,
                maximum_program_lines: budget.maximum_program_lines,
                estimated_operations_per_tick: budget.estimated_operations_per_tick,
                maximum_operations_per_tick: budget.maximum_operations_per_tick,
            })
            .await;
        self.publish_context_status(&uri).await;
    }

    async fn active_context(&self, uri: &Url) -> Option<AnalysisContext> {
        let contexts = self
            .scenarios
            .read()
            .await
            .contexts(&ProgramUri(uri.to_string()))
            .to_vec();
        if contexts.len() == 1 {
            return contexts.into_iter().next();
        }
        let selected = self.context_selections.read().await.get(uri).cloned()?;
        contexts.into_iter().find(|context| {
            context.scenario_uri == selected.0.as_str() && context.ic_id == selected.1
        })
    }

    async fn publish_context_status(&self, uri: &Url) {
        let contexts = self
            .scenarios
            .read()
            .await
            .contexts(&ProgramUri(uri.to_string()))
            .to_vec();
        let active = self.active_context(uri).await;
        let convert = |context: &AnalysisContext| ContextStatusItem {
            scenario_uri: Url::parse(&context.scenario_uri).expect("client supplied valid URI"),
            ic_id: context.ic_id.clone(),
            label: context.label(),
        };
        self.client
            .send_notification::<ContextStatusNotification>(ContextStatus {
                uri: uri.clone(),
                contexts: contexts.iter().map(convert).collect(),
                active: active.as_ref().map(convert),
                ambiguous: contexts.len() > 1 && active.is_none(),
            })
            .await;
    }

    async fn refresh_open_documents(&self) {
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

    async fn scenario_changed(&self, params: ScenarioChangedParams) {
        if let Some(source) = params.source {
            match serde_json::from_str::<Scenario>(&source) {
                Ok(scenario) => {
                    self.scenarios.write().await.update(
                        params.scenario_uri.to_string(),
                        params.version,
                        scenario,
                        |program| {
                            params.resolved_programs.get(program).map_or_else(
                                || ProgramUri(program.to_owned()),
                                |uri| ProgramUri(uri.to_string()),
                            )
                        },
                        &self.knowledge,
                    );
                }
                Err(error) => {
                    self.scenarios
                        .write()
                        .await
                        .remove(params.scenario_uri.as_str());
                    self.client
                        .log_message(
                            MessageType::WARNING,
                            format!("Ignoring invalid scenario {}: {error}", params.scenario_uri),
                        )
                        .await;
                }
            }
        } else {
            self.scenarios
                .write()
                .await
                .remove(params.scenario_uri.as_str());
        }
        self.refresh_open_documents().await;
    }

    async fn select_context(&self, params: ContextSelectionParams) {
        match (params.scenario_uri, params.ic_id) {
            (Some(scenario), Some(ic_id)) => {
                self.context_selections
                    .write()
                    .await
                    .insert(params.program_uri.clone(), (scenario, ic_id));
            }
            _ => {
                self.context_selections
                    .write()
                    .await
                    .remove(&params.program_uri);
            }
        }
        if let Some(document) = self.document(&params.program_uri).await {
            self.update_document(params.program_uri, document.source().to_owned())
                .await;
        }
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
                        detail: Some(reference_value(&constant.value)),
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentTargetPayload {
    scenario_uri: String,
    ic_id: String,
    device_id: Option<String>,
    property: Option<String>,
}

fn target_payload(target: EnvironmentTarget) -> EnvironmentTargetPayload {
    EnvironmentTargetPayload {
        scenario_uri: target.scenario_uri,
        ic_id: target.ic_id,
        device_id: target.device_id,
        property: target.property,
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
                    trigger_characters: Some(vec![" ".to_owned(), "\"".to_owned(), "-".to_owned()]),
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
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
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
        let context = self.active_context(uri).await;
        let items = match &line.kind {
            LineKind::Empty | LineKind::Label { .. } => self.command_completions(),
            LineKind::Instruction { mnemonic, operands } => {
                if offset <= mnemonic.span.end {
                    self.command_completions()
                } else if let Some(instruction) = self.knowledge.instruction(&mnemonic.text) {
                    let active = active_operand(operands, offset);
                    let mut items =
                        instruction
                            .operands
                            .get(active)
                            .map_or_else(Vec::new, |operand| {
                                self.operand_completions(
                                    &document,
                                    &mnemonic.text,
                                    &operand.operand_type,
                                )
                            });
                    if mnemonic.text == "define" && active == 1 {
                        items.extend(prefab_hash_completions(&self.knowledge));
                    }
                    if instruction
                        .operands
                        .get(active)
                        .is_some_and(|operand| operand.operand_type.contains("logicType"))
                    {
                        let write = matches!(mnemonic.text.as_str(), "s" | "sb" | "sbn" | "sbs");
                        let direct_fields = context.as_ref().and_then(|context| {
                            direct_device_operand_index(&mnemonic.text)
                                .and_then(|device_index| operands.get(device_index))
                                .and_then(|device| {
                                    valid_logic_fields(
                                        context,
                                        &document,
                                        &device.text,
                                        write,
                                        &self.knowledge,
                                    )
                                })
                        });
                        if let Some(fields) = direct_fields.or_else(|| {
                            known_batch_logic_fields(
                                &mnemonic.text,
                                operands,
                                &document,
                                &self.knowledge,
                            )
                        }) {
                            items.retain(|item| {
                                item.kind != Some(CompletionItemKind::ENUM_MEMBER)
                                    || fields.contains(&item.label.as_str())
                            });
                        }
                    }
                    items
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
        if let Some(context) = self.active_context(uri).await
            && let Some(device) = context.device_for_reference(&token.text, &document)
            && let Some(metadata) = self.knowledge.device_by_name(&device.prefab)
        {
            let asset_uri = self.asset_uri.read().await;
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: context_device_markdown(
                        &context,
                        device,
                        metadata,
                        asset_uri.as_deref(),
                    ),
                }),
                range: Some(span_to_range(document.source(), token.span)),
            }));
        }
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
                reference_value(&constant.value),
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
        if let Some(context) = self.active_context(uri).await
            && let Some(_device) = context.device_for_reference(&token.text, &document)
            && let Ok(scenario_uri) = Url::parse(&context.scenario_uri)
        {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: scenario_uri,
                range: Range::default(),
            })));
        }
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
        let mut actions = code_actions(&document, uri, params.range, &self.knowledge);
        for diagnostic in &params.context.diagnostics {
            if diagnostic.source.as_deref() != Some("ic10 environment")
                || !ranges_overlap(diagnostic.range, params.range)
            {
                continue;
            }
            let Some(data) = diagnostic.data.clone() else {
                continue;
            };
            actions.push(CodeActionOrCommand::Command(Command {
                title: "Open simulation environment at this object".to_owned(),
                command: "ic10.openEnvironmentTarget".to_owned(),
                arguments: Some(vec![data]),
            }));
        }
        Ok(Some(actions))
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
        if let Some(context) = self.active_context(&params.text_document.uri).await {
            for line in document.lines() {
                let LineKind::Instruction { operands, .. } = &line.kind else {
                    continue;
                };
                for operand in operands {
                    let Some(device) = context.device_for_reference(&operand.text, &document)
                    else {
                        continue;
                    };
                    let position = offset_to_position(document.source(), operand.span.end);
                    if position < params.range.start || position > params.range.end {
                        continue;
                    }
                    let name = if device.name.is_empty() {
                        device.id.as_str()
                    } else {
                        device.name.as_str()
                    };
                    hints.push(InlayHint {
                        position,
                        label: InlayHintLabel::String(format!(" → {name}")),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: Some(InlayHintTooltip::String(format!(
                            "{} in {}",
                            device.prefab,
                            context.label()
                        ))),
                        padding_left: Some(true),
                        padding_right: Some(false),
                        data: None,
                    });
                }
            }
        }
        Ok(Some(hints))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let contexts = self
            .scenarios
            .read()
            .await
            .contexts(&ProgramUri(params.text_document.uri.to_string()))
            .to_vec();
        Ok(Some(
            contexts
                .into_iter()
                .filter_map(|context| {
                    let label = context.label();
                    let target = serde_json::to_value(EnvironmentTargetPayload {
                        scenario_uri: context.scenario_uri,
                        ic_id: context.ic_id,
                        device_id: Some(context.housing.id),
                        property: Some("ic.program".to_owned()),
                    })
                    .ok()?;
                    Some(CodeLens {
                        range: Range::default(),
                        command: Some(Command {
                            title: format!("Used by {label}"),
                            command: "ic10.openEnvironmentTarget".to_owned(),
                            arguments: Some(vec![target]),
                        }),
                        data: None,
                    })
                })
                .collect(),
        ))
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
                    } else if knowledge.language.constants.contains_key(&operand.text)
                        || parse_numeric_literal(&operand.text).is_some()
                    {
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

fn reference_value(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

fn prefab_hash_completions(knowledge: &KnowledgeBase) -> Vec<CompletionItem> {
    knowledge
        .all_devices()
        .map(|device| {
            (
                device.prefab_name.as_str(),
                device.display_name.as_str(),
                device.prefab_hash,
                "Device prefab",
            )
        })
        .chain(knowledge.resources.resources.values().map(|resource| {
            (
                resource.prefab_name.as_str(),
                resource.display_name.as_str(),
                resource.prefab_hash,
                "Item prefab",
            )
        }))
        .map(
            |(prefab_name, display_name, prefab_hash, kind)| CompletionItem {
                label: prefab_name.to_owned(),
                label_details: Some(CompletionItemLabelDetails {
                    detail: Some(format!(" ({prefab_hash})")),
                    description: Some(display_name.to_owned()),
                }),
                kind: Some(CompletionItemKind::CONSTANT),
                detail: Some(format!(
                    "{kind} · {display_name} · PrefabHash {prefab_hash}"
                )),
                documentation: Some(Documentation::String(format!(
                    "Insert the numeric PrefabHash for `{prefab_name}`."
                ))),
                insert_text: Some(prefab_hash.to_string()),
                filter_text: Some(format!("{prefab_name} {display_name} {prefab_hash}")),
                sort_text: Some(format!("0-{display_name}-{prefab_name}")),
                ..CompletionItem::default()
            },
        )
        .collect()
}

fn known_batch_logic_fields<'a>(
    mnemonic: &str,
    operands: &[ic10_core::Token],
    document: &Document,
    knowledge: &'a KnowledgeBase,
) -> Option<Vec<&'a str>> {
    let (hash_index, slot_index, write) = match mnemonic {
        "lb" | "lbn" => (1, None, false),
        "lbs" => (1, Some(2), false),
        "lbns" => (1, Some(3), false),
        "sb" | "sbn" => (0, None, true),
        "sbs" => (0, Some(1), true),
        _ => return None,
    };
    let device = operands
        .get(hash_index)
        .and_then(|token| resolved_prefab_device(&token.text, document, knowledge))?;
    let fields = if let Some(slot_index) = slot_index {
        let slot = operands
            .get(slot_index)
            .and_then(|token| resolved_integer(&token.text, document, knowledge))?;
        &device.slots.get(&slot.to_string())?.logic_types
    } else {
        &device.logic_types
    };
    Some(
        fields
            .iter()
            .filter(|(_, access)| if write { access.write } else { access.read })
            .map(|(name, _)| name.as_str())
            .collect(),
    )
}

fn resolved_prefab_device<'a>(
    token: &str,
    document: &Document,
    knowledge: &'a KnowledgeBase,
) -> Option<&'a Device> {
    let mut value = token;
    for _ in 0..32 {
        let Some(symbol) = document.symbol(value) else {
            break;
        };
        if symbol.kind != SymbolKind::Define {
            break;
        }
        value = symbol.value.as_deref()?;
    }
    device_from_token(value, knowledge)
}

fn resolved_integer(token: &str, document: &Document, knowledge: &KnowledgeBase) -> Option<i64> {
    let mut value = token;
    for _ in 0..32 {
        let Some(symbol) = document.symbol(value) else {
            break;
        };
        if symbol.kind != SymbolKind::Define {
            break;
        }
        value = symbol.value.as_deref()?;
    }
    parse_numeric_literal(value)
        .or_else(|| {
            knowledge
                .language
                .constants
                .get(value)
                .and_then(|constant| constant.value.as_f64())
        })
        .filter(|value| value.is_finite() && value.fract() == 0.0)
        .map(|value| value as i64)
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

fn direct_device_operand_index(mnemonic: &str) -> Option<usize> {
    match mnemonic {
        "l" | "ls" | "lr" | "get" => Some(1),
        "s" | "put" => Some(0),
        _ => None,
    }
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
    if let Some((base, connection)) = token.split_once(':')
        && connection.parse::<usize>().is_ok()
        && (base == architecture.base_device
            || base
                .strip_prefix('d')
                .and_then(|value| value.parse::<u8>().ok())
                .is_some_and(|number| number <= 5))
    {
        let target = if base == architecture.base_device {
            "the device executing this program"
        } else {
            "the device assigned to this housing pin"
        };
        return Some(format!(
            "### `{token}`\n\n**Device connection {connection}**\n\n\
             Selects connection `{connection}` on `{base}` ({target}). Use a numbered \
             connection with `Channel0`–`Channel7` to read or write values carried by \
             the attached cable network. The selected simulation environment can verify \
             that this connection exists and is attached to a compatible network."
        ));
    }
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
        LspService::build(move |client| Backend::new(client, Arc::clone(&knowledge)))
            .custom_method("ic10/scenarioChanged", Backend::scenario_changed)
            .custom_method("ic10/selectContext", Backend::select_context)
            .custom_method("ic10/build", Backend::build_deployment)
            .finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use ic10_core::{Document, LineKind};
    use ic10_data::KnowledgeBase;
    use tower_lsp::lsp_types::{Position, Range, TextEdit, Url};

    use super::{
        code_actions, description_markdown, device_from_token, device_markdown,
        device_reference_markdown, instruction_markdown, known_batch_logic_fields,
        literal_macro_markdown, offset_to_position, parse_literal_macro, position_to_offset,
        prefab_hash_completions, reagent_from_token, reagent_markdown, register_markdown,
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
        assert!(
            device_reference_markdown("db:1", &knowledge).is_some_and(|text| {
                text.contains("Device connection 1")
                    && text.contains("Channel0")
                    && text.contains("attached cable network")
            })
        );
        assert!(device_reference_markdown("d0:0", &knowledge).is_some());
    }

    #[test]
    fn completes_define_values_with_searchable_prefab_hashes() {
        let items = prefab_hash_completions(&knowledge());
        let diode = items
            .iter()
            .find(|item| item.label == "StructureDiode")
            .expect("StructureDiode prefab completion");

        assert_eq!(diode.insert_text.as_deref(), Some("1944485013"));
        assert!(
            diode
                .filter_text
                .as_deref()
                .is_some_and(|text| text.contains("LED") && text.contains("1944485013"))
        );
    }

    #[test]
    fn filters_batch_logic_completions_by_defined_prefab_hash() {
        let knowledge = knowledge();
        let document = Document::parse(
            "define LED 1944485013\nsb LED RatioCarbonDioxideInput2 0.34\n",
            &knowledge,
        );
        let LineKind::Instruction { mnemonic, operands } = &document.lines()[1].kind else {
            panic!("batch instruction");
        };
        let fields = known_batch_logic_fields(&mnemonic.text, operands, &document, &knowledge)
            .expect("known prefab fields");

        assert!(fields.contains(&"On"));
        assert!(fields.contains(&"Color"));
        assert!(!fields.contains(&"Power"));
        assert!(!fields.contains(&"RatioCarbonDioxideInput2"));
    }

    #[test]
    fn semantically_colorizes_all_special_numeric_constants() {
        let knowledge = knowledge();
        for constant in ["nan", "pinf", "ninf"] {
            let document = Document::parse(format!("move r0 {constant}\n"), &knowledge);
            assert_eq!(
                semantic_tokens(&document, None, &knowledge).len(),
                3,
                "{constant} should receive a semantic token"
            );
            assert!(knowledge.language.constants.contains_key(constant));
        }
    }

    #[test]
    fn exposes_stationpedia_operator_signatures_and_descriptions() {
        let knowledge = knowledge();
        for (name, syntax, description) in [
            ("sgn", "sgn r? a(r?|num)", "Stores the sign"),
            (
                "clamp",
                "clamp r? a(r?|num) min(r?|num) max(r?|num)",
                "inclusive range",
            ),
            (
                "ror",
                "ror r? a(r?|num) b(r?|num)",
                "bitwise right rotation",
            ),
            ("rol", "rol r? a(r?|num) b(r?|num)", "bitwise left rotation"),
        ] {
            let instruction = knowledge
                .instruction(name)
                .expect("Stationpedia instruction");
            let hover = instruction_markdown(name, instruction, &knowledge);
            assert!(hover.contains(syntax));
            assert!(hover.contains(description));
        }
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
