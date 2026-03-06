use std::collections::HashMap;

use lsp_types::{Diagnostic, Uri};
use palmscript::{analyze_document, CompileError, SemanticDocument};

use crate::convert::from_trade_diagnostic;

#[derive(Clone, Debug)]
pub struct OpenDocument {
    pub text: String,
    pub version: i32,
    pub semantic: Option<SemanticDocument>,
}

#[derive(Default)]
pub struct Session {
    documents: HashMap<Uri, OpenDocument>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self, uri: Uri, version: i32, text: String) -> Vec<Diagnostic> {
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

    pub fn change(&mut self, uri: &Uri, version: i32, text: String) -> Vec<Diagnostic> {
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

    pub fn close(&mut self, uri: &Uri) {
        self.documents.remove(uri);
    }

    pub fn revalidate_all(&mut self) -> Vec<(Uri, Vec<Diagnostic>)> {
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

    pub fn semantic(&self, uri: &Uri) -> Option<&SemanticDocument> {
        self.documents
            .get(uri)
            .and_then(|document| document.semantic.as_ref())
    }

    pub fn document(&self, uri: &Uri) -> Option<&OpenDocument> {
        self.documents.get(uri)
    }

    fn analyze_uri(&self, _uri: &Uri, text: &str) -> (Option<SemanticDocument>, Vec<Diagnostic>) {
        match analyze_document(text) {
            Ok(semantic) => (Some(semantic), Vec::new()),
            Err(err) => (None, compile_diagnostics(&err)),
        }
    }
}

fn compile_diagnostics(err: &CompileError) -> Vec<Diagnostic> {
    err.diagnostics.iter().map(from_trade_diagnostic).collect()
}
