use std::error::Error;
use std::path::PathBuf;

use lsp_server::{Connection, Message, Notification, Request, Response, ResponseError};
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
    CompletionOptions, CompletionParams, DocumentFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, GotoDefinitionParams, HoverParams, InitializeParams, OneOf,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};
use palmscript::format_document;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::convert::{
    completion_item, definition_response, document_symbol, format_edit, hover_from_info,
    offset_from_position, uri_to_path,
};
use crate::session::Session;

pub fn run() -> Result<(), Box<dyn Error>> {
    let (connection, io_threads) = Connection::stdio();
    let initialization_params =
        connection.initialize(serde_json::to_value(server_capabilities())?)?;
    let init_params: InitializeParams = serde_json::from_value(initialization_params)?;
    let _workspace_roots = workspace_roots(&init_params);
    let mut session = Session::new();

    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
                handle_request(&connection, &session, request.clone())?;
            }
            Message::Notification(notification) => {
                handle_notification(&connection, &mut session, &notification)?;
            }
            Message::Response(_) => {}
        }
    }

    io_threads.join()?;
    Ok(())
}

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions::default()),
        document_formatting_provider: Some(OneOf::Left(true)),
        ..ServerCapabilities::default()
    }
}

#[allow(deprecated)]
fn workspace_roots(params: &InitializeParams) -> Vec<PathBuf> {
    if let Some(folders) = &params.workspace_folders {
        let roots: Vec<PathBuf> = folders
            .iter()
            .filter_map(|folder| uri_to_path(&folder.uri))
            .collect();
        if !roots.is_empty() {
            return roots;
        }
    }

    params
        .root_uri
        .as_ref()
        .and_then(uri_to_path)
        .into_iter()
        .collect()
}

fn handle_request(
    connection: &Connection,
    session: &Session,
    request: Request,
) -> Result<(), Box<dyn Error>> {
    match request.method.as_str() {
        <HoverRequest as LspRequest>::METHOD => {
            let params: HoverParams = parse_params(request.params)?;
            let uri = params.text_document_position_params.text_document.uri;
            let Some(document) = session.document(&uri) else {
                return send_response(connection, request.id, None::<Value>);
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
            send_response(connection, request.id, result)?;
        }
        <Completion as LspRequest>::METHOD => {
            let params: CompletionParams = parse_params(request.params)?;
            let uri = params.text_document_position.text_document.uri;
            let Some(document) = session.document(&uri) else {
                return send_response(connection, request.id, None::<Value>);
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
            send_response(connection, request.id, Some(serde_json::to_value(items)?))?;
        }
        <GotoDefinition as LspRequest>::METHOD => {
            let params: GotoDefinitionParams = parse_params(request.params)?;
            let uri = params.text_document_position_params.text_document.uri;
            let Some(document) = session.document(&uri) else {
                return send_response(connection, request.id, None::<Value>);
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
            send_response(connection, request.id, result)?;
        }
        <DocumentSymbolRequest as LspRequest>::METHOD => {
            let params: DocumentSymbolParams = parse_params(request.params)?;
            let result = session.semantic(&params.text_document.uri).map(|semantic| {
                DocumentSymbolResponse::Nested(
                    semantic
                        .document_symbols()
                        .iter()
                        .cloned()
                        .map(document_symbol)
                        .collect(),
                )
            });
            send_response(connection, request.id, result)?;
        }
        <Formatting as LspRequest>::METHOD => {
            let params: DocumentFormattingParams = parse_params(request.params)?;
            let uri = params.text_document.uri;
            let Some(document) = session.document(&uri) else {
                return send_response(connection, request.id, None::<Value>);
            };
            let formatted = format_document(&document.text)?;
            let edits = vec![format_edit(&document.text, formatted)];
            send_response(connection, request.id, Some(serde_json::to_value(edits)?))?;
        }
        _ => {
            let response = Response {
                id: request.id,
                result: None,
                error: Some(ResponseError {
                    code: lsp_server::ErrorCode::MethodNotFound as i32,
                    message: format!("unknown request `{}`", request.method),
                    data: None,
                }),
            };
            connection.sender.send(Message::Response(response))?;
        }
    }

    Ok(())
}

fn handle_notification(
    connection: &Connection,
    session: &mut Session,
    notification: &Notification,
) -> Result<(), Box<dyn Error>> {
    match notification.method.as_str() {
        <DidOpenTextDocument as LspNotification>::METHOD => {
            let params: lsp_types::DidOpenTextDocumentParams =
                parse_params(notification.params.clone())?;
            let diagnostics = session.open(
                params.text_document.uri.clone(),
                params.text_document.version,
                params.text_document.text,
            );
            publish_diagnostics(connection, params.text_document.uri, diagnostics)?;
        }
        <DidChangeTextDocument as LspNotification>::METHOD => {
            let params: lsp_types::DidChangeTextDocumentParams =
                parse_params(notification.params.clone())?;
            let text = params
                .content_changes
                .into_iter()
                .last()
                .map(|change| change.text)
                .unwrap_or_default();
            let diagnostics = session.change(
                &params.text_document.uri,
                params.text_document.version,
                text,
            );
            publish_diagnostics(connection, params.text_document.uri, diagnostics)?;
        }
        <DidSaveTextDocument as LspNotification>::METHOD => {
            for (uri, diagnostics) in session.revalidate_all() {
                publish_diagnostics(connection, uri, diagnostics)?;
            }
        }
        <DidChangeConfiguration as LspNotification>::METHOD => {
            for (uri, diagnostics) in session.revalidate_all() {
                publish_diagnostics(connection, uri, diagnostics)?;
            }
        }
        <DidCloseTextDocument as LspNotification>::METHOD => {
            let params: lsp_types::DidCloseTextDocumentParams =
                parse_params(notification.params.clone())?;
            session.close(&params.text_document.uri);
            publish_diagnostics(connection, params.text_document.uri, Vec::new())?;
        }
        _ => {}
    }
    Ok(())
}

fn publish_diagnostics(
    connection: &Connection,
    uri: lsp_types::Uri,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> Result<(), Box<dyn Error>> {
    let notification = Notification::new(
        <PublishDiagnostics as LspNotification>::METHOD.to_string(),
        lsp_types::PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: None,
        },
    );
    connection
        .sender
        .send(Message::Notification(notification))?;
    Ok(())
}

fn parse_params<T: DeserializeOwned>(value: Value) -> Result<T, Box<dyn Error>> {
    Ok(serde_json::from_value(value)?)
}

fn send_response<T: serde::Serialize>(
    connection: &Connection,
    id: lsp_server::RequestId,
    result: T,
) -> Result<(), Box<dyn Error>> {
    let response = Response {
        id,
        result: Some(serde_json::to_value(result)?),
        error: None,
    };
    connection.sender.send(Message::Response(response))?;
    Ok(())
}
