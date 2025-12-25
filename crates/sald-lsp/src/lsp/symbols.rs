use dashmap::DashMap;
use tower_lsp::lsp_types::{Position, Range, Url};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Variable,
    Function,
    Class,
    Method,
    Parameter,
    Namespace,
    Enum,
    Constant,
}

impl SymbolKind {
    pub fn to_lsp(&self) -> tower_lsp::lsp_types::SymbolKind {
        match self {
            SymbolKind::Variable => tower_lsp::lsp_types::SymbolKind::VARIABLE,
            SymbolKind::Function => tower_lsp::lsp_types::SymbolKind::FUNCTION,
            SymbolKind::Class => tower_lsp::lsp_types::SymbolKind::CLASS,
            SymbolKind::Method => tower_lsp::lsp_types::SymbolKind::METHOD,
            SymbolKind::Parameter => tower_lsp::lsp_types::SymbolKind::VARIABLE,
            SymbolKind::Namespace => tower_lsp::lsp_types::SymbolKind::NAMESPACE,
            SymbolKind::Enum => tower_lsp::lsp_types::SymbolKind::ENUM,
            SymbolKind::Constant => tower_lsp::lsp_types::SymbolKind::CONSTANT,
        }
    }

    pub fn to_completion_kind(&self) -> tower_lsp::lsp_types::CompletionItemKind {
        match self {
            SymbolKind::Variable => tower_lsp::lsp_types::CompletionItemKind::VARIABLE,
            SymbolKind::Function => tower_lsp::lsp_types::CompletionItemKind::FUNCTION,
            SymbolKind::Class => tower_lsp::lsp_types::CompletionItemKind::CLASS,
            SymbolKind::Method => tower_lsp::lsp_types::CompletionItemKind::METHOD,
            SymbolKind::Parameter => tower_lsp::lsp_types::CompletionItemKind::VARIABLE,
            SymbolKind::Namespace => tower_lsp::lsp_types::CompletionItemKind::MODULE,
            SymbolKind::Enum => tower_lsp::lsp_types::CompletionItemKind::ENUM,
            SymbolKind::Constant => tower_lsp::lsp_types::CompletionItemKind::CONSTANT,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    pub selection_range: Range,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub children: Vec<Symbol>,
    pub type_hint: Option<String>,
    pub source_uri: Option<String>,
}

impl Default for SymbolKind {
    fn default() -> Self {
        SymbolKind::Variable
    }
}

#[derive(Debug, Default)]
pub struct DocumentInfo {
    pub symbols: Vec<Symbol>,
    pub content: String,
}

#[derive(Debug, Default)]
pub struct SymbolTable {
    pub documents: DashMap<Url, DocumentInfo>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            documents: DashMap::new(),
        }
    }

    pub fn update_document(&self, uri: Url, content: String, symbols: Vec<Symbol>) {
        self.documents
            .insert(uri, DocumentInfo { symbols, content });
    }

    pub fn get_document(
        &self,
        uri: &Url,
    ) -> Option<dashmap::mapref::one::Ref<'_, Url, DocumentInfo>> {
        self.documents.get(uri)
    }

    pub fn remove_document(&self, uri: &Url) {
        self.documents.remove(uri);
    }
}

use rustc_hash::FxHashSet;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct WorkspaceIndex {
    exports: DashMap<PathBuf, Vec<Symbol>>,

    references: DashMap<String, FxHashSet<PathBuf>>,

    workspace_root: parking_lot::RwLock<Option<PathBuf>>,
}

impl WorkspaceIndex {
    pub fn new() -> Self {
        Self {
            exports: DashMap::new(),
            references: DashMap::new(),
            workspace_root: parking_lot::RwLock::new(None),
        }
    }

    pub fn set_workspace_root(&self, root: PathBuf) {
        *self.workspace_root.write() = Some(root);
    }

    pub fn get_workspace_root(&self) -> Option<PathBuf> {
        self.workspace_root.read().clone()
    }

    pub fn register_exports(&self, file_path: PathBuf, symbols: Vec<Symbol>) {
        self.exports.insert(file_path, symbols);
    }

    pub fn register_references(&self, file_path: &PathBuf, symbol_names: Vec<String>) {
        for name in symbol_names {
            self.references
                .entry(name)
                .or_insert_with(FxHashSet::default)
                .insert(file_path.clone());
        }
    }

    pub fn is_symbol_used_externally(&self, symbol_name: &str, defining_file: &PathBuf) -> bool {
        if let Some(refs) = self.references.get(symbol_name) {
            refs.iter().any(|f| f != defining_file)
        } else {
            false
        }
    }

    pub fn get_exported_names(&self, file_path: &PathBuf) -> FxHashSet<String> {
        if let Some(exports) = self.exports.get(file_path) {
            exports.iter().map(|s| s.name.clone()).collect()
        } else {
            FxHashSet::default()
        }
    }

    pub fn clear_file_references(&self, file_path: &PathBuf) {
        for mut entry in self.references.iter_mut() {
            entry.value_mut().remove(file_path);
        }
    }

    pub fn is_sald_modules_path(path: &std::path::Path) -> bool {
        path.components().any(|c| c.as_os_str() == "sald_modules")
    }

    pub fn scan_workspace_files(&self) -> Vec<PathBuf> {
        let root = match self.get_workspace_root() {
            Some(r) => r,
            None => return Vec::new(),
        };

        let mut files = Vec::new();
        Self::scan_directory(&root, &mut files);
        files
    }

    fn scan_directory(dir: &PathBuf, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Self::scan_directory(&path, files);
                } else if path.extension().map_or(false, |e| e == "sald") {
                    files.push(path);
                }
            }
        }
    }
}

pub fn span_to_range(span: &sald_core::error::Span) -> Range {
    Range {
        start: Position {
            line: span.start.line.saturating_sub(1) as u32,
            character: span.start.column.saturating_sub(1) as u32,
        },
        end: Position {
            line: span.end.line.saturating_sub(1) as u32,

            character: span.end.column as u32,
        },
    }
}
