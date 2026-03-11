//! Transport-agnostic PalmScript language-server session handling.
//!
//! This module owns the request, notification, and diagnostics flow shared by
//! both the stdio `palmscript-lsp` binary and the hosted browser IDE websocket
//! transport.

use std::collections::HashMap;

use lsp_server::{Message, Notification, Request, Response, ResponseError};
use lsp_types::{
    notification::Notification as LspNotification,
    notification::{
        DidChangeConfiguration, DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
        DidSaveTextDocument, PublishDiagnostics,
    },
    request::{
        Completion, DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest,
        Request as LspRequest,
    },
    CompletionOptions, CompletionParams, Diagnostic, DiagnosticSeverity, DocumentFormattingParams,
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverContents, HoverParams, InitializeResult, MarkupContent,
    MarkupKind, OneOf, Position as LspPosition, Range, SemanticToken, SemanticTokenModifier,
    SemanticTokenType, SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensResult, SemanticTokensServerCapabilities,
    ServerCapabilities, SymbolKind as LspSymbolKind, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Uri, WorkDoneProgressOptions,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::ide::{
    analyze_document, classify_highlight, format_document, CompletionEntry, CompletionKind,
    DefinitionTarget, DocumentSymbolInfo, HighlightKind, HoverInfo, SemanticDocument,
};
use crate::lexer;
use crate::token::Token;
use crate::{CompileError, Diagnostic as PalmDiagnostic, Position, Span, SymbolKind};

const SEMANTIC_TOKEN_TYPES: [SemanticTokenType; 8] = [
    SemanticTokenType::KEYWORD,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::TYPE,
];

#[derive(Clone, Debug)]
pub struct OpenDocument {
    pub text: String,
    pub version: i32,
    pub semantic: Option<SemanticDocument>,
}

#[derive(Default)]
struct DocumentSession {
    documents: HashMap<Uri, OpenDocument>,
}

impl DocumentSession {
    fn open(&mut self, uri: Uri, version: i32, text: String) -> Vec<Diagnostic> {
        let (semantic, diagnostics) = self.analyze_uri(&uri, &text);
        self.documents.insert(
            uri,
            OpenDocument {
                text,
                version,
                semantic,
            },
        );
        diagnostics
    }

    fn change(&mut self, uri: &Uri, version: i32, text: String) -> Vec<Diagnostic> {
        let (semantic, diagnostics) = self.analyze_uri(uri, &text);
        self.documents.insert(
            uri.clone(),
            OpenDocument {
                text,
                version,
                semantic,
            },
        );
        diagnostics
    }

    fn close(&mut self, uri: &Uri) {
        self.documents.remove(uri);
    }

    fn revalidate_all(&mut self) -> Vec<(Uri, Vec<Diagnostic>)> {
        let uris: Vec<Uri> = self.documents.keys().cloned().collect();
        let mut diagnostics = Vec::with_capacity(uris.len());
        for uri in uris {
            if let Some(document) = self.documents.get(&uri).cloned() {
                let (semantic, current) = self.analyze_uri(&uri, &document.text);
                self.documents.insert(
                    uri.clone(),
                    OpenDocument {
                        text: document.text,
                        version: document.version,
                        semantic,
                    },
                );
                diagnostics.push((uri, current));
            }
        }
        diagnostics
    }

    fn semantic(&self, uri: &Uri) -> Option<&SemanticDocument> {
        self.documents
            .get(uri)
            .and_then(|document| document.semantic.as_ref())
    }

    fn document(&self, uri: &Uri) -> Option<&OpenDocument> {
        self.documents.get(uri)
    }

    fn analyze_uri(&self, _uri: &Uri, text: &str) -> (Option<SemanticDocument>, Vec<Diagnostic>) {
        match analyze_document(text) {
            Ok(semantic) => (Some(semantic), Vec::new()),
            Err(err) => (None, compile_diagnostics(&err)),
        }
    }
}

#[derive(Default)]
pub struct IdeLspSession {
    documents: DocumentSession,
    shutdown_requested: bool,
    exit_received: bool,
}

impl IdeLspSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_message(&mut self, message: Message) -> Vec<Message> {
        match message {
            Message::Request(request) => self.handle_request(request),
            Message::Notification(notification) => self.handle_notification(notification),
            Message::Response(_) => Vec::new(),
        }
    }

    pub fn should_exit(&self) -> bool {
        self.exit_received
    }

    fn handle_request(&mut self, request: Request) -> Vec<Message> {
        match request.method.as_str() {
            <lsp_types::request::Initialize as LspRequest>::METHOD => {
                vec![send_response(
                    request.id,
                    json!(InitializeResult {
                        capabilities: server_capabilities(),
                        server_info: None,
                    }),
                )]
            }
            "shutdown" => {
                self.shutdown_requested = true;
                vec![send_response(request.id, Value::Null)]
            }
            <HoverRequest as LspRequest>::METHOD => {
                let Ok(params) =
                    parse_params::<HoverParams>(&request.method, request.params.clone())
                else {
                    return vec![invalid_params_response(request)];
                };
                let uri = params.text_document_position_params.text_document.uri;
                let Some(document) = self.documents.document(&uri) else {
                    return vec![send_response(request.id, Value::Null)];
                };
                let offset = offset_from_position(
                    &document.text,
                    params.text_document_position_params.position,
                );
                let result = document
                    .semantic
                    .as_ref()
                    .and_then(|semantic| semantic.hover_at(offset))
                    .map(hover_from_info);
                vec![send_optional_response(request.id, result)]
            }
            <Completion as LspRequest>::METHOD => {
                let Ok(params) =
                    parse_params::<CompletionParams>(&request.method, request.params.clone())
                else {
                    return vec![invalid_params_response(request)];
                };
                let uri = params.text_document_position.text_document.uri;
                let Some(document) = self.documents.document(&uri) else {
                    return vec![send_response(request.id, Value::Null)];
                };
                let offset =
                    offset_from_position(&document.text, params.text_document_position.position);
                let items = document
                    .semantic
                    .as_ref()
                    .map(|semantic| semantic.completions_at(offset))
                    .unwrap_or_default()
                    .into_iter()
                    .map(completion_item)
                    .collect::<Vec<_>>();
                vec![send_response(request.id, json!(items))]
            }
            <GotoDefinition as LspRequest>::METHOD => {
                let Ok(params) =
                    parse_params::<GotoDefinitionParams>(&request.method, request.params.clone())
                else {
                    return vec![invalid_params_response(request)];
                };
                let uri = params.text_document_position_params.text_document.uri;
                let Some(document) = self.documents.document(&uri) else {
                    return vec![send_response(request.id, Value::Null)];
                };
                let offset = offset_from_position(
                    &document.text,
                    params.text_document_position_params.position,
                );
                let result = document
                    .semantic
                    .as_ref()
                    .and_then(|semantic| semantic.definition_at(offset))
                    .and_then(|definition| definition_response(&uri, definition));
                vec![send_optional_response(request.id, result)]
            }
            <DocumentSymbolRequest as LspRequest>::METHOD => {
                let Ok(params) =
                    parse_params::<DocumentSymbolParams>(&request.method, request.params.clone())
                else {
                    return vec![invalid_params_response(request)];
                };
                let result = self
                    .documents
                    .semantic(&params.text_document.uri)
                    .map(|semantic| {
                        DocumentSymbolResponse::Nested(
                            semantic
                                .document_symbols()
                                .iter()
                                .cloned()
                                .map(document_symbol)
                                .collect(),
                        )
                    });
                vec![send_optional_response(request.id, result)]
            }
            <Formatting as LspRequest>::METHOD => {
                let Ok(params) = parse_params::<DocumentFormattingParams>(
                    &request.method,
                    request.params.clone(),
                ) else {
                    return vec![invalid_params_response(request)];
                };
                let uri = params.text_document.uri;
                let Some(document) = self.documents.document(&uri) else {
                    return vec![send_response(request.id, Value::Null)];
                };
                match format_document(&document.text) {
                    Ok(formatted) => {
                        let edits = vec![format_edit(&document.text, formatted)];
                        vec![send_response(request.id, json!(edits))]
                    }
                    Err(err) => vec![error_response(
                        request.id,
                        lsp_server::ErrorCode::InternalError as i32,
                        err.to_string(),
                    )],
                }
            }
            <lsp_types::request::SemanticTokensFullRequest as LspRequest>::METHOD => {
                let Ok(params) = parse_params::<lsp_types::SemanticTokensParams>(
                    &request.method,
                    request.params.clone(),
                ) else {
                    return vec![invalid_params_response(request)];
                };
                let result = self
                    .documents
                    .document(&params.text_document.uri)
                    .and_then(build_semantic_tokens)
                    .map(SemanticTokensResult::Tokens);
                vec![send_optional_response(request.id, result)]
            }
            _ => vec![error_response(
                request.id,
                lsp_server::ErrorCode::MethodNotFound as i32,
                format!("unknown request `{}`", request.method),
            )],
        }
    }

    fn handle_notification(&mut self, notification: Notification) -> Vec<Message> {
        match notification.method.as_str() {
            <lsp_types::notification::Initialized as LspNotification>::METHOD => Vec::new(),
            "exit" => {
                self.exit_received = true;
                Vec::new()
            }
            <DidOpenTextDocument as LspNotification>::METHOD => {
                let Ok(params) = parse_params::<lsp_types::DidOpenTextDocumentParams>(
                    &notification.method,
                    notification.params,
                ) else {
                    return Vec::new();
                };
                let diagnostics = self.documents.open(
                    params.text_document.uri.clone(),
                    params.text_document.version,
                    params.text_document.text,
                );
                vec![publish_diagnostics(params.text_document.uri, diagnostics)]
            }
            <DidChangeTextDocument as LspNotification>::METHOD => {
                let Ok(params) = parse_params::<lsp_types::DidChangeTextDocumentParams>(
                    &notification.method,
                    notification.params,
                ) else {
                    return Vec::new();
                };
                let text = params
                    .content_changes
                    .into_iter()
                    .last()
                    .map(|change| change.text)
                    .unwrap_or_default();
                let diagnostics = self.documents.change(
                    &params.text_document.uri,
                    params.text_document.version,
                    text,
                );
                vec![publish_diagnostics(params.text_document.uri, diagnostics)]
            }
            <DidSaveTextDocument as LspNotification>::METHOD
            | <DidChangeConfiguration as LspNotification>::METHOD => self
                .documents
                .revalidate_all()
                .into_iter()
                .map(|(uri, diagnostics)| publish_diagnostics(uri, diagnostics))
                .collect(),
            <DidCloseTextDocument as LspNotification>::METHOD => {
                let Ok(params) = parse_params::<lsp_types::DidCloseTextDocumentParams>(
                    &notification.method,
                    notification.params,
                ) else {
                    return Vec::new();
                };
                self.documents.close(&params.text_document.uri);
                vec![publish_diagnostics(
                    params.text_document.uri,
                    Vec::<Diagnostic>::new(),
                )]
            }
            _ => Vec::new(),
        }
    }
}

pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions::default()),
        document_formatting_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: None,
                },
                legend: SemanticTokensLegend {
                    token_types: SEMANTIC_TOKEN_TYPES.to_vec(),
                    token_modifiers: Vec::<SemanticTokenModifier>::new(),
                },
                range: None,
                full: Some(SemanticTokensFullOptions::Bool(true)),
            },
        )),
        ..ServerCapabilities::default()
    }
}

fn compile_diagnostics(err: &CompileError) -> Vec<Diagnostic> {
    err.diagnostics.iter().map(from_palm_diagnostic).collect()
}

fn from_palm_diagnostic(diagnostic: &PalmDiagnostic) -> Diagnostic {
    Diagnostic {
        range: range_from_span(diagnostic.span),
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("palmscript".to_string()),
        message: diagnostic.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    }
}

fn range_from_span(span: Span) -> Range {
    Range {
        start: position_from_palm(span.start),
        end: position_from_palm(span.end),
    }
}

fn position_from_palm(position: Position) -> LspPosition {
    LspPosition::new(
        position.line.saturating_sub(1) as u32,
        position.column.saturating_sub(1) as u32,
    )
}

pub fn offset_from_position(text: &str, position: LspPosition) -> usize {
    let mut offset = 0usize;
    let mut line = 0u32;
    let mut column = 0u32;
    for ch in text.chars() {
        if line == position.line && column == position.character {
            return offset;
        }
        offset += ch.len_utf8();
        if ch == '\n' {
            line += 1;
            column = 0;
            if line > position.line {
                return offset;
            }
        } else if line == position.line {
            column += 1;
        }
    }
    text.len()
}

fn hover_from_info(info: HoverInfo) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: info.contents,
        }),
        range: Some(range_from_span(info.span)),
    }
}

fn definition_response(uri: &Uri, definition: DefinitionTarget) -> Option<GotoDefinitionResponse> {
    if !definition.navigable {
        return None;
    }
    Some(GotoDefinitionResponse::Scalar(lsp_types::Location {
        uri: uri.clone(),
        range: range_from_span(definition.selection_span),
    }))
}

fn completion_item(entry: CompletionEntry) -> lsp_types::CompletionItem {
    lsp_types::CompletionItem {
        label: entry.label,
        kind: Some(match entry.kind {
            CompletionKind::Keyword | CompletionKind::Interval | CompletionKind::Field => {
                lsp_types::CompletionItemKind::KEYWORD
            }
            CompletionKind::Builtin | CompletionKind::Function => {
                lsp_types::CompletionItemKind::FUNCTION
            }
            CompletionKind::Series | CompletionKind::Variable | CompletionKind::Source => {
                lsp_types::CompletionItemKind::VARIABLE
            }
        }),
        detail: entry.detail,
        ..lsp_types::CompletionItem::default()
    }
}

#[allow(deprecated)]
fn document_symbol(symbol: DocumentSymbolInfo) -> DocumentSymbol {
    DocumentSymbol {
        name: symbol.name,
        detail: symbol.detail,
        kind: match symbol.kind {
            SymbolKind::Interval | SymbolKind::Source | SymbolKind::UseInterval => {
                LspSymbolKind::NAMESPACE
            }
            SymbolKind::Function => LspSymbolKind::FUNCTION,
            SymbolKind::Parameter | SymbolKind::Let | SymbolKind::Export | SymbolKind::Trigger => {
                LspSymbolKind::VARIABLE
            }
        },
        tags: None,
        deprecated: None,
        range: range_from_span(symbol.span),
        selection_range: range_from_span(symbol.selection_span),
        children: if symbol.children.is_empty() {
            None
        } else {
            Some(symbol.children.into_iter().map(document_symbol).collect())
        },
    }
}

fn format_edit(old_text: &str, new_text: String) -> TextEdit {
    TextEdit {
        range: Range {
            start: LspPosition::new(0, 0),
            end: LspPosition::new(old_text.lines().count() as u32 + 1, 0),
        },
        new_text,
    }
}

fn parse_params<T: DeserializeOwned>(method: &str, value: Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|err| format!("invalid params for `{method}`: {err}"))
}

fn invalid_params_response(request: Request) -> Message {
    error_response(
        request.id,
        lsp_server::ErrorCode::InvalidParams as i32,
        format!("invalid params for `{}`", request.method),
    )
}

fn send_response(id: lsp_server::RequestId, result: Value) -> Message {
    Message::Response(Response {
        id,
        result: Some(result),
        error: None,
    })
}

fn send_optional_response<T: serde::Serialize>(
    id: lsp_server::RequestId,
    result: Option<T>,
) -> Message {
    match result {
        Some(result) => Message::Response(Response {
            id,
            result: Some(serde_json::to_value(result).expect("LSP result should serialize")),
            error: None,
        }),
        None => send_response(id, Value::Null),
    }
}

fn error_response(id: lsp_server::RequestId, code: i32, message: String) -> Message {
    Message::Response(Response {
        id,
        result: None,
        error: Some(ResponseError {
            code,
            message,
            data: None,
        }),
    })
}

fn publish_diagnostics(uri: Uri, diagnostics: Vec<Diagnostic>) -> Message {
    Message::Notification(Notification::new(
        <PublishDiagnostics as LspNotification>::METHOD.to_string(),
        lsp_types::PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: None,
        },
    ))
}

fn build_semantic_tokens(document: &OpenDocument) -> Option<SemanticTokens> {
    let semantic = document.semantic.as_ref()?;
    let tokens = lexer::lex(&document.text).ok()?;
    let mut encoded = Vec::new();
    let mut previous_line = 0u32;
    let mut previous_start = 0u32;
    let mut first = true;

    for token in tokens {
        let Some(token_type) = semantic_token_type(&token, semantic) else {
            continue;
        };
        if token.span.start.line == 0 || token.span.end.offset < token.span.start.offset {
            continue;
        }
        let length = document.text[token.span.start.offset..token.span.end.offset]
            .chars()
            .count() as u32;
        if length == 0 {
            continue;
        }
        let line = token.span.start.line.saturating_sub(1) as u32;
        let start = token.span.start.column.saturating_sub(1) as u32;
        let (delta_line, delta_start) = if first {
            first = false;
            (line, start)
        } else if line == previous_line {
            (0, start.saturating_sub(previous_start))
        } else {
            (line.saturating_sub(previous_line), start)
        };
        encoded.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });
        previous_line = line;
        previous_start = start;
    }

    Some(SemanticTokens {
        result_id: None,
        data: encoded,
    })
}

fn semantic_token_type(token: &Token, semantic: &SemanticDocument) -> Option<u32> {
    classify_highlight(token, Some(semantic)).map(|kind| match kind {
        HighlightKind::Keyword => 0,
        HighlightKind::String => 1,
        HighlightKind::Number => 2,
        HighlightKind::Function => 3,
        HighlightKind::Variable => 4,
        HighlightKind::Parameter => 5,
        HighlightKind::Namespace => 6,
        HighlightKind::Type => 7,
    })
}

#[cfg(test)]
mod tests {
    use super::IdeLspSession;
    use lsp_server::{Message, Notification, Request};
    use serde_json::json;

    #[test]
    fn initialize_request_returns_capabilities() {
        let mut session = IdeLspSession::new();
        let messages = session.handle_message(Message::Request(Request {
            id: 1.into(),
            method: "initialize".to_string(),
            params: json!({}),
        }));
        assert_eq!(messages.len(), 1);
        let Message::Response(response) = &messages[0] else {
            panic!("expected response");
        };
        assert!(response.error.is_none());
        assert!(response.result.is_some());
    }

    #[test]
    fn opening_document_publishes_compile_diagnostics() {
        let mut session = IdeLspSession::new();
        let messages = session.handle_message(Message::Notification(Notification::new(
            "textDocument/didOpen".to_string(),
            json!({
                "textDocument": {
                    "uri": "inmemory:///strategy.palm",
                    "languageId": "palmscript",
                    "version": 1,
                    "text": "interval"
                }
            }),
        )));
        assert_eq!(messages.len(), 1);
        let Message::Notification(notification) = &messages[0] else {
            panic!("expected diagnostics notification");
        };
        assert_eq!(notification.method, "textDocument/publishDiagnostics");
        let params = notification
            .params
            .as_object()
            .expect("diagnostics params should be an object");
        assert!(!params["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .is_empty());
    }

    #[test]
    fn semantic_tokens_request_returns_token_data() {
        let mut session = IdeLspSession::new();
        session.handle_message(Message::Notification(Notification::new(
            "textDocument/didOpen".to_string(),
            json!({
                "textDocument": {
                    "uri": "inmemory:///strategy.palm",
                    "languageId": "palmscript",
                    "version": 1,
                    "text": "interval 4h\nsource spot = binance.spot(\"BTCUSDT\")\nlet fast = ema(spot.close, 13)\n"
                }
            }),
        )));
        let messages = session.handle_message(Message::Request(Request {
            id: 2.into(),
            method: "textDocument/semanticTokens/full".to_string(),
            params: json!({
                "textDocument": { "uri": "inmemory:///strategy.palm" }
            }),
        }));
        let Message::Response(response) = &messages[0] else {
            panic!("expected response");
        };
        let result = response.result.as_ref().expect("semantic tokens result");
        let data = result["data"].as_array().expect("semantic token data");
        assert!(!data.is_empty());
    }
}
