// Symbol Table for LSP
// Tracks definitions, references, and type information

use dashmap::DashMap;
use tower_lsp::lsp_types::{Position, Range, Url};

/// Symbol kind for LSP
#[allow(unused)]
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

/// A symbol definition
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    pub selection_range: Range,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub children: Vec<Symbol>,
    pub type_hint: Option<String>, // Inferred type for variables
}

/// Document symbols and diagnostics
#[derive(Debug, Default)]
pub struct DocumentInfo {
    pub symbols: Vec<Symbol>,
    pub content: String,
}

/// Global symbol table across all documents
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
        self.documents.insert(uri, DocumentInfo { symbols, content });
    }

    pub fn get_document(&self, uri: &Url) -> Option<dashmap::mapref::one::Ref<'_, Url, DocumentInfo>> {
        self.documents.get(uri)
    }

    pub fn remove_document(&self, uri: &Url) {
        self.documents.remove(uri);
    }
}

/// Convert Sald Span to LSP Range
pub fn span_to_range(span: &crate::error::Span) -> Range {
    Range {
        start: Position {
            line: span.start.line.saturating_sub(1) as u32,
            character: span.start.column.saturating_sub(1) as u32,
        },
        end: Position {
            line: span.end.line.saturating_sub(1) as u32,
            character: span.end.column.saturating_sub(1) as u32,
        },
    }
}
