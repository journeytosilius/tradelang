use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lsp_types::{Diagnostic, Uri};
use palmscript::{
    analyze_document, load_project_config, CompileEnvironment, CompileError, SemanticDocument,
};
use serde::Deserialize;

use crate::convert::from_trade_diagnostic;
use crate::convert::uri_to_path;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct InitializationOptions {
    #[serde(default)]
    pub project_config_path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct OpenDocument {
    pub text: String,
    pub version: i32,
    pub semantic: Option<SemanticDocument>,
}

#[derive(Default)]
pub struct Session {
    workspace_roots: Vec<PathBuf>,
    project_config_override: Option<String>,
    documents: HashMap<Uri, OpenDocument>,
}

impl Session {
    pub fn new(workspace_roots: Vec<PathBuf>, options: InitializationOptions) -> Self {
        Self {
            workspace_roots,
            project_config_override: options.project_config_path,
            documents: HashMap::new(),
        }
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

    fn analyze_uri(&self, uri: &Uri, text: &str) -> (Option<SemanticDocument>, Vec<Diagnostic>) {
        let env = self.compile_environment_for(uri);
        match analyze_document(text, &env) {
            Ok(semantic) => (Some(semantic), Vec::new()),
            Err(err) => (None, compile_diagnostics(&err)),
        }
    }

    fn compile_environment_for(&self, uri: &Uri) -> CompileEnvironment {
        let Some(path) = uri_to_path(uri) else {
            return CompileEnvironment::default();
        };
        let Some(root) = self.workspace_root_for(&path) else {
            return CompileEnvironment::default();
        };
        let config_path = self.project_config_path(&root);
        if !config_path.exists() {
            return CompileEnvironment::default();
        }
        let Ok(config) = load_project_config(&config_path) else {
            return CompileEnvironment::default();
        };
        config.compile_environment_for_document(&root, &path)
    }

    fn workspace_root_for(&self, path: &Path) -> Option<PathBuf> {
        self.workspace_roots
            .iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.as_os_str().len())
            .cloned()
    }

    fn project_config_path(&self, root: &Path) -> PathBuf {
        match &self.project_config_override {
            Some(path) => {
                let override_path = PathBuf::from(path);
                if override_path.is_absolute() {
                    override_path
                } else {
                    root.join(override_path)
                }
            }
            None => root.join(".palmscript.json"),
        }
    }
}

fn compile_diagnostics(err: &CompileError) -> Vec<Diagnostic> {
    err.diagnostics.iter().map(from_trade_diagnostic).collect()
}
