use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::symbols::{span_to_range, Symbol, SymbolKind};
use sald_core::ast::{Expr, Program, Stmt};
use sald_core::lexer::Scanner;
use sald_core::parser::Parser;

#[derive(Debug, Clone)]
pub struct FileExports {
    pub symbols: Vec<Symbol>,
    pub last_modified: std::time::SystemTime,
}

pub struct ImportResolver {
    cache: Arc<RwLock<FxHashMap<PathBuf, FileExports>>>,

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

    fn get_workspace_root(&self) -> PathBuf {
        self.workspace_root
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn resolve_import_path(&self, from_file: &Path, import_path: &str) -> Option<PathBuf> {
        let clean_path = import_path.trim_matches('"').trim_matches('\'');

        let workspace = self.get_workspace_root();

        let is_relative = clean_path.starts_with('.')
            || clean_path.starts_with('/')
            || clean_path.starts_with('\\')
            || clean_path.ends_with(".sald");

        if !is_relative {
            let from_dir = from_file.parent().unwrap_or(Path::new("."));
            let local_modules = from_dir.join("sald_modules").join(clean_path);

            if let Some(path) = self.resolve_module_dir(&local_modules) {
                return Some(path);
            }

            let workspace_modules = workspace.join("sald_modules").join(clean_path);
            if let Some(path) = self.resolve_module_dir(&workspace_modules) {
                return Some(path);
            }

            let direct_file = workspace
                .join("sald_modules")
                .join(format!("{}.sald", clean_path));
            if direct_file.exists() {
                return direct_file.canonicalize().ok();
            }
        }

        let from_dir = from_file.parent().unwrap_or(Path::new("."));
        let resolved = from_dir.join(clean_path);

        if resolved.exists() {
            return resolved.canonicalize().ok();
        }

        if !clean_path.ends_with(".sald") {
            let with_ext = from_dir.join(format!("{}.sald", clean_path));
            if with_ext.exists() {
                return with_ext.canonicalize().ok();
            }
        }

        None
    }

    fn resolve_module_dir(&self, module_dir: &Path) -> Option<PathBuf> {
        if !module_dir.exists() || !module_dir.is_dir() {
            return None;
        }

        let config_path = module_dir.join("salad.json");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
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

        let module_name = module_dir.file_name()?.to_str()?;
        let default_entry = module_dir.join(format!("{}.sald", module_name));
        if default_entry.exists() {
            return default_entry.canonicalize().ok();
        }

        let main_entry = module_dir.join("main.sald");
        if main_entry.exists() {
            return main_entry.canonicalize().ok();
        }

        None
    }

    pub fn get_exports(&self, file_path: &Path) -> Option<Vec<Symbol>> {
        {
            let cache = self.cache.read();
            if let Some(exports) = cache.get(file_path) {
                if let Ok(metadata) = std::fs::metadata(file_path) {
                    if let Ok(modified) = metadata.modified() {
                        if modified <= exports.last_modified {
                            return Some(exports.symbols.clone());
                        }
                    }
                }
            }
        }

        let content = std::fs::read_to_string(file_path).ok()?;
        let file_name = file_path.to_string_lossy().to_string();

        let mut scanner = Scanner::new(&content, &file_name);
        let tokens = scanner.scan_tokens().ok()?;

        let mut parser = Parser::new(tokens, &file_name, &content);
        let program = parser.parse().ok()?;

        let mut symbols = self.extract_exports(&program);

        let file_path_str = file_path.to_string_lossy().to_string();
        self.set_source_uri_recursive(&mut symbols, &file_path_str);

        {
            let mut cache = self.cache.write();
            let last_modified = std::fs::metadata(file_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or_else(std::time::SystemTime::now);

            cache.insert(
                file_path.to_path_buf(),
                FileExports {
                    symbols: symbols.clone(),
                    last_modified,
                },
            );
        }

        Some(symbols)
    }

    fn set_source_uri_recursive(&self, symbols: &mut [Symbol], uri: &str) {
        for sym in symbols.iter_mut() {
            sym.source_uri = Some(uri.to_string());
            self.set_source_uri_recursive(&mut sym.children, uri);
        }
    }

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
                        type_hint: None,
                        source_uri: None,
                    });
                }
                Stmt::Class { def } => {
                    let children: Vec<Symbol> = def
                        .methods
                        .iter()
                        .map(|m| {
                            let params: Vec<String> =
                                m.params.iter().map(|p| p.name.clone()).collect();
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
                        })
                        .collect();

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
                        type_hint: None,
                        source_uri: None,
                    });
                }
                Stmt::Enum {
                    name,
                    variants,
                    span,
                } => {
                    let children: Vec<Symbol> = variants
                        .iter()
                        .map(|v| Symbol {
                            name: v.clone(),
                            kind: SymbolKind::Constant,
                            range: span_to_range(span),
                            selection_range: span_to_range(span),
                            detail: Some(format!("{}.{}", name, v)),
                            documentation: None,
                            children: Vec::new(),
                            type_hint: None,
                            source_uri: None,
                        })
                        .collect();

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

                Stmt::Let {
                    name,
                    name_span: _,
                    initializer,
                    span,
                } => {
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
                let children: Vec<Symbol> = def
                    .methods
                    .iter()
                    .map(|m| {
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
                    })
                    .collect();

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
            Stmt::Let {
                name,
                name_span: _,
                initializer,
                span,
            } => {
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
            Stmt::Enum {
                name,
                variants,
                span,
            } => {
                let children: Vec<Symbol> = variants
                    .iter()
                    .map(|v| Symbol {
                        name: v.clone(),
                        kind: SymbolKind::Constant,
                        range: span_to_range(span),
                        selection_range: span_to_range(span),
                        detail: Some(format!("{}.{}", name, v)),
                        documentation: None,
                        children: Vec::new(),
                        type_hint: None,
                        source_uri: None,
                    })
                    .collect();

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

    fn infer_type(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call { callee, .. } => {
                if let Expr::Identifier { name, .. } = callee.as_ref() {
                    if name
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                    {
                        return Some(name.clone());
                    }
                }
                None
            }
            Expr::Array { .. } => Some("Array".to_string()),
            Expr::Dictionary { .. } => Some("Dict".to_string()),
            Expr::Literal { value, .. } => match value {
                sald_core::ast::Literal::Number(_) => Some("Number".to_string()),
                sald_core::ast::Literal::String(_) => Some("String".to_string()),
                sald_core::ast::Literal::Boolean(_) => Some("Boolean".to_string()),
                sald_core::ast::Literal::Null => None,
            },
            _ => None,
        }
    }

    pub fn resolve_imports_for_document(
        &self,
        file_path: &Path,
        program: &Program,
    ) -> (Vec<Symbol>, FxHashMap<String, Vec<Symbol>>) {
        let mut global_symbols = Vec::new();
        let mut aliased_symbols = FxHashMap::default();

        for stmt in &program.statements {
            if let Stmt::Import { path, alias, .. } = stmt {
                if let Some(resolved_path) = self.resolve_import_path(file_path, path) {
                    if let Some(symbols) = self.get_exports(&resolved_path) {
                        if let Some(alias_name) = alias {
                            aliased_symbols.insert(alias_name.clone(), symbols);
                        } else {
                            global_symbols.extend(symbols);
                        }
                    }
                }
            }
        }

        (global_symbols, aliased_symbols)
    }

    pub fn get_all_symbols_for_document(
        &self,
        file_path: &Path,
        program: &Program,
        doc_symbols: &[Symbol],
    ) -> Vec<Symbol> {
        let mut all_symbols = doc_symbols.to_vec();

        let (global_symbols, aliased_symbols) =
            self.resolve_imports_for_document(file_path, program);

        all_symbols.extend(global_symbols);

        for (alias, symbols) in aliased_symbols {
            all_symbols.push(Symbol {
                name: alias.clone(),
                kind: SymbolKind::Namespace,
                range: Default::default(),
                selection_range: Default::default(),
                detail: Some(format!("import ... as {}", alias)),
                documentation: None,
                children: symbols,
                type_hint: None,
                source_uri: None,
            });
        }

        all_symbols
    }

    pub fn invalidate_cache(&self, file_path: &Path) {
        self.cache.write().remove(file_path);
    }
}

impl Default for ImportResolver {
    fn default() -> Self {
        Self::new()
    }
}
