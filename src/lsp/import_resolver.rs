// Import Resolver for Sald LSP
// Resolves import paths and extracts exported symbols from imported files

use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::RwLock;

use crate::ast::{Program, Stmt, Expr};
use crate::lexer::Scanner;
use crate::parser::Parser;
use super::symbols::{Symbol, SymbolKind, span_to_range};

/// Cached file info for import resolution
#[derive(Debug, Clone)]
pub struct FileExports {
    pub symbols: Vec<Symbol>,
    pub last_modified: std::time::SystemTime,
}

/// Import resolver that handles cross-file symbol resolution
pub struct ImportResolver {
    /// Cache of parsed exports per file
    cache: Arc<RwLock<FxHashMap<PathBuf, FileExports>>>,
    /// Workspace root for resolving relative paths
    workspace_root: Option<PathBuf>,
}

impl ImportResolver {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(FxHashMap::default())),
            workspace_root: None,
        }
    }

    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.workspace_root = Some(root);
    }

    /// Get workspace root, falling back to CWD
    fn get_workspace_root(&self) -> PathBuf {
        self.workspace_root.clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Resolve an import path to an absolute file path
    pub fn resolve_import_path(&self, from_file: &Path, import_path: &str) -> Option<PathBuf> {
        // Remove quotes if present
        let clean_path = import_path.trim_matches('"').trim_matches('\'');
        
        // Get the workspace root (for module imports)
        let workspace = self.get_workspace_root();
        
        // Check if it's a relative import (starts with . or /)
        let is_relative = clean_path.starts_with('.') 
            || clean_path.starts_with('/') 
            || clean_path.starts_with('\\')
            || clean_path.ends_with(".sald");
        
        if !is_relative {
            // Module import: look in sald_modules/<module>/
            // First check in the same directory as from_file
            let from_dir = from_file.parent().unwrap_or(Path::new("."));
            let local_modules = from_dir.join("sald_modules").join(clean_path);
            
            if let Some(path) = self.resolve_module_dir(&local_modules) {
                return Some(path);
            }
            
            // Then check in workspace root
            let workspace_modules = workspace.join("sald_modules").join(clean_path);
            if let Some(path) = self.resolve_module_dir(&workspace_modules) {
                return Some(path);
            }
            
            // Try as direct file in sald_modules
            let direct_file = workspace.join("sald_modules").join(format!("{}.sald", clean_path));
            if direct_file.exists() {
                return direct_file.canonicalize().ok();
            }
        }
        
        // Relative import: resolve from the importing file's directory
        let from_dir = from_file.parent().unwrap_or(Path::new("."));
        let resolved = from_dir.join(clean_path);
        
        // If path exists, return it
        if resolved.exists() {
            return resolved.canonicalize().ok();
        }
        
        // Try with .sald extension if not present
        if !clean_path.ends_with(".sald") {
            let with_ext = from_dir.join(format!("{}.sald", clean_path));
            if with_ext.exists() { 
                return with_ext.canonicalize().ok();
            }
        }
        
        None
    }
    
    /// Resolve a module directory by reading salad.json
    fn resolve_module_dir(&self, module_dir: &Path) -> Option<PathBuf> {
        if !module_dir.exists() || !module_dir.is_dir() {
            return None;
        }
        
        // Check for salad.json
        let config_path = module_dir.join("salad.json");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                // Parse JSON to get "main" field
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(main) = json.get("main").and_then(|v| v.as_str()) {
                        let entry = module_dir.join(main);
                        if entry.exists() {
                            return entry.canonicalize().ok();
                        }
                    }
                }
            }
        }
        
        // Fallback: look for <module_name>.sald
        let module_name = module_dir.file_name()?.to_str()?;
        let default_entry = module_dir.join(format!("{}.sald", module_name));
        if default_entry.exists() {
            return default_entry.canonicalize().ok();
        }
        
        // Last fallback: main.sald
        let main_entry = module_dir.join("main.sald");
        if main_entry.exists() {
            return main_entry.canonicalize().ok();
        }
        
        None
    }

    /// Parse a file and extract its exported symbols
    pub fn get_exports(&self, file_path: &Path) -> Option<Vec<Symbol>> {
        // Check cache first
        {
            let cache = self.cache.read();
            if let Some(exports) = cache.get(file_path) {
                // Check if file hasn't been modified
                if let Ok(metadata) = std::fs::metadata(file_path) {
                    if let Ok(modified) = metadata.modified() {
                        if modified <= exports.last_modified {
                            return Some(exports.symbols.clone());
                        }
                    }
                }
            }
        }
        
        // Parse the file
        let content = std::fs::read_to_string(file_path).ok()?;
        let file_name = file_path.to_string_lossy().to_string();
        
        let mut scanner = Scanner::new(&content, &file_name);
        let tokens = scanner.scan_tokens().ok()?;
        
        let mut parser = Parser::new(tokens, &file_name, &content);
        let program = parser.parse().ok()?;
        
        // Extract exported symbols
        let mut symbols = self.extract_exports(&program);
        
        // Set source_uri on all symbols for cross-file go-to-definition
        // Store the absolute file path (not URI) - it will be converted to URL in find_definition
        let file_path_str = file_path.to_string_lossy().to_string();
        self.set_source_uri_recursive(&mut symbols, &file_path_str);
        
        // Cache the result
        {
            let mut cache = self.cache.write();
            let last_modified = std::fs::metadata(file_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or_else(std::time::SystemTime::now);
            
            cache.insert(file_path.to_path_buf(), FileExports {
                symbols: symbols.clone(),
                last_modified,
            });
        }
        
        Some(symbols)
    }

    /// Recursively set source_uri on symbols and their children
    fn set_source_uri_recursive(&self, symbols: &mut [Symbol], uri: &str) {
        for sym in symbols.iter_mut() {
            sym.source_uri = Some(uri.to_string());
            self.set_source_uri_recursive(&mut sym.children, uri);
        }
    }

    /// Extract top-level exported symbols from a program
    fn extract_exports(&self, program: &Program) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        
        for stmt in &program.statements {
            match stmt {
                Stmt::Function { def } => {
                    let params: Vec<String> = def.params.iter().map(|p| p.name.clone()).collect();
                    symbols.push(Symbol {
                        name: def.name.clone(),
                        kind: SymbolKind::Function,
                        range: span_to_range(&def.span),
                        selection_range: span_to_range(&def.span),
                        detail: Some(format!("fun {}({})", def.name, params.join(", "))),
                        documentation: None,
                        children: Vec::new(),
                        type_hint: None, source_uri: None });
                }
                Stmt::Class { def } => {
                    let children: Vec<Symbol> = def.methods.iter().map(|m| {
                        let params: Vec<String> = m.params.iter().map(|p| p.name.clone()).collect();
                        Symbol {
                            name: m.name.clone(),
                            kind: SymbolKind::Method,
                            range: span_to_range(&m.span),
                            selection_range: span_to_range(&m.span),
                            detail: Some(format!("fun {}({})", m.name, params.join(", "))),
                            documentation: None,
                            children: Vec::new(),
                            type_hint: None, source_uri: None }
                    }).collect();
                    
                    symbols.push(Symbol {
                        name: def.name.clone(),
                        kind: SymbolKind::Class,
                        range: span_to_range(&def.span),
                        selection_range: span_to_range(&def.span),
                        detail: Some(format!("class {}", def.name)),
                        documentation: None,
                        children,
                        type_hint: None, source_uri: None });
                }
                Stmt::Const { name, value, span } => {
                    let type_hint = self.infer_type(value);
                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Constant,
                        range: span_to_range(span),
                        selection_range: span_to_range(span),
                        detail: Some(format!("const {}", name)),
                        documentation: None,
                        children: Vec::new(),
                        type_hint,
                        ..Default::default()
                    });
                }
                Stmt::Namespace { name, body, span } => {
                    let mut children = Vec::new();
                    for s in body {
                        self.extract_stmt_symbol(s, &mut children);
                    }
                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Namespace,
                        range: span_to_range(span),
                        selection_range: span_to_range(span),
                        detail: Some(format!("namespace {}", name)),
                        documentation: None,
                        children,
                        type_hint: None, source_uri: None });
                }
                Stmt::Enum { name, variants, span } => {
                    let children: Vec<Symbol> = variants.iter().map(|v| Symbol {
                        name: v.clone(),
                        kind: SymbolKind::Constant,
                        range: span_to_range(span),
                        selection_range: span_to_range(span),
                        detail: Some(format!("{}.{}", name, v)),
                        documentation: None,
                        children: Vec::new(),
                        type_hint: None, source_uri: None }).collect();
                    
                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Enum,
                        range: span_to_range(span),
                        selection_range: span_to_range(span),
                        detail: Some(format!("enum {}", name)),
                        documentation: None,
                        children,
                        type_hint: None, source_uri: None });
                }
                // Let statements at top-level are also exports
                Stmt::Let { name, name_span: _, initializer, span } => {
                    let type_hint = initializer.as_ref().and_then(|e| self.infer_type(e));
                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Variable,
                        range: span_to_range(span),
                        selection_range: span_to_range(span),
                        detail: Some(format!("let {}", name)),
                        documentation: None,
                        children: Vec::new(),
                        type_hint,
                        ..Default::default()
                    });
                }
                _ => {}
            }
        }
        
        symbols
    }

    /// Extract symbol from a statement (for nested contexts - handles recursion for nested namespaces)
    fn extract_stmt_symbol(&self, stmt: &Stmt, symbols: &mut Vec<Symbol>) {
        match stmt {
            Stmt::Function { def } => {
                let params: Vec<String> = def.params.iter().map(|p| p.name.clone()).collect();
                symbols.push(Symbol {
                    name: def.name.clone(),
                    kind: SymbolKind::Function,
                    range: span_to_range(&def.span),
                    selection_range: span_to_range(&def.span),
                    detail: Some(format!("fun {}({})", def.name, params.join(", "))),
                    documentation: None,
                    children: Vec::new(),
                    type_hint: None,
                    source_uri: None,
                });
            }
            Stmt::Class { def } => {
                let children: Vec<Symbol> = def.methods.iter().map(|m| {
                    let params: Vec<String> = m.params.iter().map(|p| p.name.clone()).collect();
                    Symbol {
                        name: m.name.clone(),
                        kind: SymbolKind::Method,
                        range: span_to_range(&m.span),
                        selection_range: span_to_range(&m.span),
                        detail: Some(format!("fun {}({})", m.name, params.join(", "))),
                        documentation: None,
                        children: Vec::new(),
                        type_hint: None,
                        source_uri: None,
                    }
                }).collect();
                
                symbols.push(Symbol {
                    name: def.name.clone(),
                    kind: SymbolKind::Class,
                    range: span_to_range(&def.span),
                    selection_range: span_to_range(&def.span),
                    detail: Some(format!("class {}", def.name)),
                    documentation: None,
                    children,
                    type_hint: None,
                    source_uri: None,
                });
            }
            Stmt::Namespace { name, body, span } => {
                // RECURSIVELY extract nested symbols
                let mut children = Vec::new();
                for s in body {
                    self.extract_stmt_symbol(s, &mut children);
                }
                symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Namespace,
                    range: span_to_range(span),
                    selection_range: span_to_range(span),
                    detail: Some(format!("namespace {}", name)),
                    documentation: None,
                    children,
                    type_hint: None,
                    source_uri: None,
                });
            }
            Stmt::Const { name, value, span } => {
                let type_hint = self.infer_type(value);
                symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Constant,
                    range: span_to_range(span),
                    selection_range: span_to_range(span),
                    detail: Some(format!("const {}", name)),
                    documentation: None,
                    children: Vec::new(),
                    type_hint,
                    source_uri: None,
                });
            }
            Stmt::Let { name, name_span: _, initializer, span } => {
                let type_hint = initializer.as_ref().and_then(|e| self.infer_type(e));
                symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Variable,
                    range: span_to_range(span),
                    selection_range: span_to_range(span),
                    detail: Some(format!("let {}", name)),
                    documentation: None,
                    children: Vec::new(),
                    type_hint,
                    source_uri: None,
                });
            }
            Stmt::Enum { name, variants, span } => {
                let children: Vec<Symbol> = variants.iter().map(|v| Symbol {
                    name: v.clone(),
                    kind: SymbolKind::Constant,
                    range: span_to_range(span),
                    selection_range: span_to_range(span),
                    detail: Some(format!("{}.{}", name, v)),
                    documentation: None,
                    children: Vec::new(),
                    type_hint: None,
                    source_uri: None,
                }).collect();
                
                symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Enum,
                    range: span_to_range(span),
                    selection_range: span_to_range(span),
                    detail: Some(format!("enum {}", name)),
                    documentation: None,
                    children,
                    type_hint: None,
                    source_uri: None,
                });
            }
            _ => {}
        }
    }

    /// Simple type inference from expression
    fn infer_type(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call { callee, .. } => {
                if let Expr::Identifier { name, .. } = callee.as_ref() {
                    if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                        return Some(name.clone());
                    }
                }
                None
            }
            Expr::Array { .. } => Some("Array".to_string()),
            Expr::Dictionary { .. } => Some("Dict".to_string()),
            Expr::Literal { value, .. } => {
                match value {
                    crate::ast::Literal::Number(_) => Some("Number".to_string()),
                    crate::ast::Literal::String(_) => Some("String".to_string()),
                    crate::ast::Literal::Boolean(_) => Some("Boolean".to_string()),
                    crate::ast::Literal::Null => None,
                }
            }
            _ => None
        }
    }

    /// Resolve imports for a document and return all imported symbols
    /// Returns: (global_symbols, aliased_symbols)
    /// - global_symbols: symbols from `import "file.sald"` (no alias) - added directly to scope
    /// - aliased_symbols: FxHashMap<alias, symbols> from `import "file.sald" as Alias`
    pub fn resolve_imports_for_document(
        &self,
        file_path: &Path,
        program: &Program
    ) -> (Vec<Symbol>, FxHashMap<String, Vec<Symbol>>) {
        let mut global_symbols = Vec::new();
        let mut aliased_symbols = FxHashMap::default();
        
        for stmt in &program.statements {
            if let Stmt::Import { path, alias, .. } = stmt {
                if let Some(resolved_path) = self.resolve_import_path(file_path, path) {
                    if let Some(symbols) = self.get_exports(&resolved_path) {
                        if let Some(alias_name) = alias {
                            // Import with alias: import "file.sald" as Module
                            aliased_symbols.insert(alias_name.clone(), symbols);
                        } else {
                            // Global import: import "file.sald" - add symbols directly
                            global_symbols.extend(symbols);
                        }
                    }
                }
            }
        }
        
        (global_symbols, aliased_symbols)
    }

    /// Get all symbols available in a document (including imports)
    pub fn get_all_symbols_for_document(
        &self,
        file_path: &Path,
        program: &Program,
        doc_symbols: &[Symbol]
    ) -> Vec<Symbol> {
        let mut all_symbols = doc_symbols.to_vec();
        
        // Add imported symbols
        let (global_symbols, aliased_symbols) = self.resolve_imports_for_document(file_path, program);
        
        // Add global imports directly to scope
        all_symbols.extend(global_symbols);
        
        // Add aliased imports as namespace-like symbols
        for (alias, symbols) in aliased_symbols {
            all_symbols.push(Symbol {
                name: alias.clone(),
                kind: SymbolKind::Namespace,
                range: Default::default(),
                selection_range: Default::default(),
                detail: Some(format!("import ... as {}", alias)),
                documentation: None,
                children: symbols,
                type_hint: None, source_uri: None });
        }
        
        all_symbols
    }

    /// Clear the cache (useful when files change)
    pub fn invalidate_cache(&self, file_path: &Path) {
        self.cache.write().remove(file_path);
    }
}

impl Default for ImportResolver {
    fn default() -> Self {
        Self::new()
    }
}

