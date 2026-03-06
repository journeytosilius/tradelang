use std::path::PathBuf;

use lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, DocumentSymbol,
    GotoDefinitionResponse, Hover, HoverContents, Location, MarkupContent, MarkupKind,
    Position as LspPosition, Range, SymbolKind as LspSymbolKind, TextEdit, Uri,
};
use palmscript::{
    CompletionEntry, CompletionKind, DefinitionTarget, Diagnostic as TradeDiagnostic, HoverInfo,
    Position, Span, SymbolKind,
};

pub fn from_trade_diagnostic(diagnostic: &TradeDiagnostic) -> Diagnostic {
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

pub fn range_from_span(span: Span) -> Range {
    Range {
        start: position_from_trade(span.start),
        end: position_from_trade(span.end),
    }
}

pub fn position_from_trade(position: Position) -> LspPosition {
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

pub fn hover_from_info(info: HoverInfo) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: info.contents,
        }),
        range: Some(range_from_span(info.span)),
    }
}

pub fn definition_response(
    uri: &Uri,
    definition: DefinitionTarget,
) -> Option<GotoDefinitionResponse> {
    if !definition.navigable {
        return None;
    }
    Some(GotoDefinitionResponse::Scalar(Location {
        uri: uri.clone(),
        range: range_from_span(definition.selection_span),
    }))
}

pub fn completion_item(entry: CompletionEntry) -> CompletionItem {
    CompletionItem {
        label: entry.label,
        kind: Some(match entry.kind {
            CompletionKind::Keyword | CompletionKind::Interval | CompletionKind::Field => {
                CompletionItemKind::KEYWORD
            }
            CompletionKind::Builtin | CompletionKind::Function => CompletionItemKind::FUNCTION,
            CompletionKind::Series | CompletionKind::Variable => CompletionItemKind::VARIABLE,
        }),
        detail: entry.detail,
        ..CompletionItem::default()
    }
}

#[allow(deprecated)]
pub fn document_symbol(symbol: palmscript::DocumentSymbolInfo) -> DocumentSymbol {
    DocumentSymbol {
        name: symbol.name,
        detail: symbol.detail,
        kind: symbol_kind(symbol.kind),
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

fn symbol_kind(kind: SymbolKind) -> LspSymbolKind {
    match kind {
        SymbolKind::Interval | SymbolKind::UseInterval => LspSymbolKind::NAMESPACE,
        SymbolKind::Function => LspSymbolKind::FUNCTION,
        SymbolKind::Parameter => LspSymbolKind::VARIABLE,
        SymbolKind::Let | SymbolKind::Export | SymbolKind::Trigger => LspSymbolKind::VARIABLE,
    }
}

pub fn format_edit(old_text: &str, new_text: String) -> TextEdit {
    TextEdit {
        range: Range {
            start: LspPosition::new(0, 0),
            end: LspPosition::new(old_text.lines().count() as u32 + 1, 0),
        },
        new_text,
    }
}

pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    url::Url::parse(uri.as_str()).ok()?.to_file_path().ok()
}
