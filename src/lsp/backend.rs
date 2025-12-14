// Sald Language Server Backend
// Implements tower_lsp::LanguageServer trait with full LSP support

use std::sync::Arc;
use std::path::PathBuf;
use parking_lot::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use super::analyzer::SemanticAnalyzer;
use super::completion::{get_builtin_symbols, get_keyword_completions};
use super::import_resolver::ImportResolver;
use super::symbols::{span_to_range, Symbol, SymbolKind, SymbolTable, WorkspaceIndex};
use crate::lexer::Scanner;
use crate::parser::Parser;
use crate::ast::{Stmt, Expr, FunctionDef, ClassDef};

/// Sald Language Server
pub struct SaldLanguageServer {
    client: Client,
    symbols: Arc<SymbolTable>,
    import_resolver: Arc<RwLock<ImportResolver>>,
    workspace_index: Arc<WorkspaceIndex>,
}

impl SaldLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            symbols: Arc::new(SymbolTable::new()),
            import_resolver: Arc::new(RwLock::new(ImportResolver::new())),
            workspace_index: Arc::new(WorkspaceIndex::new()),
        }
    }

    /// Convert URL to file path
    fn url_to_path(uri: &Url) -> Option<PathBuf> {
        uri.to_file_path().ok()
    }

    /// Analyze a document and publish diagnostics
    /// Uses Scanner/Parser for syntax errors and SemanticAnalyzer for semantic errors
    async fn analyze_document(&self, uri: Url, text: String) {
        let mut diagnostics = Vec::new();
        let mut symbols = Vec::new();

        let file_name = uri.path().to_string();
        let file_path = Self::url_to_path(&uri);

        // Step 1: Tokenize
        let mut scanner = Scanner::new(&text, &file_name);
        let tokens = match scanner.scan_tokens() {
            Ok(t) => t,
            Err(e) => {
                // Lexer error - publish and return early
                diagnostics.push(self.error_to_diagnostic(&e));
                self.client.publish_diagnostics(uri, diagnostics, None).await;
                return;
            }
        };

        // Step 2: Parse
        let mut parser = Parser::new(tokens, &file_name, &text);
        let program = match parser.parse() {
            Ok(p) => p,
            Err(e) => {
                // Parser error - publish and return early
                diagnostics.push(self.error_to_diagnostic(&e));
                self.client.publish_diagnostics(uri, diagnostics, None).await;
                return;
            }
        };

        // Step 3: Extract symbols from AST
        for stmt in &program.statements {
            self.extract_symbols_recursive(stmt, &mut symbols);
        }

        // Step 4: Resolve imports and add imported symbols
        if let Some(ref path) = file_path {
            let resolver = self.import_resolver.read();
            let all_symbols = resolver.get_all_symbols_for_document(path, &program, &symbols);
            symbols = all_symbols;
            
            // Re-index this file for workspace tracking
            self.workspace_index.clear_file_references(path);
            self.index_file(path);
        }

        // Step 5: Semantic analysis - catches undefined variables, const assignments, etc.
        let mut analyzer = SemanticAnalyzer::new();
        // Register imported symbols so they don't trigger undefined errors
        analyzer.add_imported_symbols(&symbols);
        
        // Pass workspace info for cross-file usage tracking
        if let Some(ref path) = file_path {
            let exported_names = self.workspace_index.get_exported_names(path);
            let externally_used: std::collections::HashSet<String> = exported_names
                .iter()
                .filter(|name| self.workspace_index.is_symbol_used_externally(name, path))
                .cloned()
                .collect();
            analyzer.set_externally_used_symbols(externally_used);
        }
        
        let semantic_diagnostics = analyzer.analyze(&program);
        diagnostics.extend(semantic_diagnostics);

        // Update symbol table
        self.symbols.update_document(uri.clone(), text, symbols);

        // Invalidate import cache for this file
        if let Some(path) = file_path {
            self.import_resolver.read().invalidate_cache(&path);
        }

        // Publish all diagnostics
        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }

    /// Convert SaldError to LSP Diagnostic
    fn error_to_diagnostic(&self, e: &crate::error::SaldError) -> Diagnostic {
        Diagnostic {
            range: span_to_range(&e.span),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("sald".to_string()),
            message: e.message.to_string(),
            ..Default::default()
        }
    }

    /// Extract symbols recursively from statements
    fn extract_symbols_recursive(&self, stmt: &Stmt, symbols: &mut Vec<Symbol>) {
        match stmt {
            Stmt::Function { def } => symbols.push(self.function_to_symbol(def)),
            Stmt::Class { def } => symbols.push(self.class_to_symbol(def)),
            Stmt::Let { name, initializer, span } => {
                // Try to infer type from initializer
                let type_hint = initializer.as_ref().and_then(|e| self.infer_type(e));
                symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Variable,
                    range: span_to_range(span),
                    selection_range: span_to_range(span),
                    detail: Some(format!("let {}", name)),
                    documentation: None,
                    children: Vec::new(),
                    type_hint, source_uri: None });
                // Also extract symbols from initializer (lambdas, etc.)
                if let Some(init) = initializer {
                    self.extract_expr_symbols(init, symbols);
                }
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
                    type_hint, source_uri: None });
                // Also extract symbols from value
                self.extract_expr_symbols(value, symbols);
            }
            Stmt::Namespace { name, body, span } => {
                let mut children = Vec::new();
                for s in body {
                    self.extract_symbols_recursive(s, &mut children);
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
                    detail: Some(format!("enum {} ({} variants)", name, variants.len())),
                    documentation: None,
                    children,
                    type_hint: None, source_uri: None });
            }
            Stmt::Block { statements, .. } => {
                for s in statements {
                    self.extract_symbols_recursive(s, symbols);
                }
            }
            Stmt::Expression { expr, .. } => {
                // Extract symbols from expression (like method calls with lambda args)
                self.extract_expr_symbols(expr, symbols);
            }
            _ => {}
        }
    }
    
    /// Extract symbols from expressions (mainly for lambda parameters)
    fn extract_expr_symbols(&self, expr: &Expr, symbols: &mut Vec<Symbol>) {
        match expr {
            Expr::Lambda { params, body, is_async: _, span } => {
                // Lambda parameters are local symbols
                for param in params {
                    symbols.push(Symbol {
                        name: param.name.clone(),
                        kind: SymbolKind::Parameter,
                        range: span_to_range(&param.span),
                        selection_range: span_to_range(&param.span),
                        detail: Some(format!("param {}", param.name)),
                        documentation: None,
                        children: Vec::new(),
                        type_hint: None,
                        source_uri: None,
                    });
                }
                // Also extract from lambda body
                match body {
                    crate::ast::LambdaBody::Expr(e) => self.extract_expr_symbols(e, symbols),
                    crate::ast::LambdaBody::Block(stmts) => {
                        for stmt in stmts {
                            self.extract_symbols_recursive(stmt, symbols);
                        }
                    }
                }
            }
            Expr::Call { callee, args, .. } => {
                self.extract_expr_symbols(callee, symbols);
                for arg in args {
                    self.extract_expr_symbols(&arg.value, symbols);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.extract_expr_symbols(left, symbols);
                self.extract_expr_symbols(right, symbols);
            }
            Expr::Unary { operand, .. } => {
                self.extract_expr_symbols(operand, symbols);
            }
            Expr::Get { object, .. } => {
                self.extract_expr_symbols(object, symbols);
            }
            Expr::Set { object, value, .. } => {
                self.extract_expr_symbols(object, symbols);
                self.extract_expr_symbols(value, symbols);
            }
            Expr::Index { object, index, .. } => {
                self.extract_expr_symbols(object, symbols);
                self.extract_expr_symbols(index, symbols);
            }
            Expr::Array { elements, .. } => {
                for el in elements {
                    self.extract_expr_symbols(el, symbols);
                }
            }
            Expr::Dictionary { entries, .. } => {
                for (k, v) in entries {
                    self.extract_expr_symbols(k, symbols);
                    self.extract_expr_symbols(v, symbols);
                }
            }
            Expr::Ternary { condition, then_expr, else_expr, .. } => {
                self.extract_expr_symbols(condition, symbols);
                self.extract_expr_symbols(then_expr, symbols);
                self.extract_expr_symbols(else_expr, symbols);
            }
            Expr::Await { expr: e, .. } | Expr::Grouping { expr: e, .. } => {
                self.extract_expr_symbols(e, symbols);
            }
            _ => {}
        }
    }

    /// Infer type from expression (heuristic-based type inference)
    fn infer_type(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call { callee, .. } => {
                match callee.as_ref() {
                    // Direct class constructor: ClassName()
                    Expr::Identifier { name, .. } => {
                        if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                            return Some(name.clone());
                        }
                    }
                    // Namespace/Class method call: Namespace.method() or Class.staticMethod()
                    // e.g., Spark.createApp() → type is "Spark.App"
                    Expr::Get { object, property, .. } => {
                        // Get full path like "Spark" or "Spark.Response"
                        if let Some(base_path) = self.get_expr_full_path(object) {
                            // Check for factory pattern: createXxx -> Xxx
                            if property.starts_with("create") && property.len() > 6 {
                                // createApp -> App, createRouter -> Router
                                let class_name = &property[6..];
                                // Return full path like "Spark.App"
                                return Some(format!("{}.{}", base_path, class_name));
                            }
                            // For other methods, just return the base namespace
                            if base_path.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                                return Some(base_path);
                            }
                        }
                    }
                    _ => {}
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
            Expr::Lambda { .. } => Some("Function".to_string()),
            _ => None
        }
    }

    /// Get full path from expression chain (e.g., Spark.Response -> "Spark.Response")
    fn get_expr_full_path(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Identifier { name, .. } => Some(name.clone()),
            Expr::Get { object, property, .. } => {
                if let Some(base) = self.get_expr_full_path(object) {
                    Some(format!("{}.{}", base, property))
                } else {
                    Some(property.clone())
                }
            }
            _ => None
        }
    }


    fn function_to_symbol(&self, def: &FunctionDef) -> Symbol {
        let params: Vec<String> = def.params.iter().map(|p| p.name.clone()).collect();
        let detail = format!("fun {}({})", def.name, params.join(", "));

        Symbol {
            name: def.name.clone(),
            kind: SymbolKind::Function,
            range: span_to_range(&def.span),
            selection_range: span_to_range(&def.span),
            detail: Some(detail),
            documentation: None,
            children: Vec::new(),
            type_hint: None, source_uri: None }
    }

    fn class_to_symbol(&self, def: &ClassDef) -> Symbol {
        // Extract methods
        let mut children: Vec<Symbol> = def
            .methods
            .iter()
            .map(|m| {
                let params: Vec<String> = m.params.iter().map(|p| p.name.clone()).collect();
                let detail = if m.is_static {
                    format!("static fun {}({})", m.name, params.join(", "))
                } else {
                    format!("fun {}({})", m.name, params.join(", "))
                };

                Symbol {
                    name: m.name.clone(),
                    kind: SymbolKind::Method,
                    range: span_to_range(&m.span),
                    selection_range: span_to_range(&m.span),
                    detail: Some(detail),
                    documentation: None,
                    children: Vec::new(),
                    type_hint: None, source_uri: None }
            })
            .collect();
        
        // Extract properties from method bodies (self.xxx = yyy patterns)
        let mut seen_props = std::collections::HashSet::new();
        for method in &def.methods {
            self.extract_properties_from_stmts(&method.body, &mut children, &mut seen_props);
        }

        let detail = if let Some(ref superclass) = def.superclass {
            format!("class {} extends {}", def.name, superclass)
        } else {
            format!("class {}", def.name)
        };

        Symbol {
            name: def.name.clone(),
            kind: SymbolKind::Class,
            range: span_to_range(&def.span),
            selection_range: span_to_range(&def.span),
            detail: Some(detail),
            documentation: None,
            children,
            type_hint: None, source_uri: None }
    }
    
    /// Extract self.xxx = yyy property assignments from statements
    fn extract_properties_from_stmts(&self, stmts: &[Stmt], children: &mut Vec<Symbol>, seen: &mut std::collections::HashSet<String>) {
        for stmt in stmts {
            self.extract_properties_from_stmt(stmt, children, seen);
        }
    }
    
    fn extract_properties_from_stmt(&self, stmt: &Stmt, children: &mut Vec<Symbol>, seen: &mut std::collections::HashSet<String>) {
        match stmt {
            Stmt::Expression { expr, .. } => {
                self.extract_properties_from_expr(expr, children, seen);
            }
            Stmt::Block { statements, .. } => {
                self.extract_properties_from_stmts(statements, children, seen);
            }
            Stmt::If { then_branch, else_branch, .. } => {
                self.extract_properties_from_stmt(then_branch, children, seen);
                if let Some(el) = else_branch {
                    self.extract_properties_from_stmt(el, children, seen);
                }
            }
            Stmt::While { body, .. } | Stmt::DoWhile { body, .. } => {
                self.extract_properties_from_stmt(body, children, seen);
            }
            Stmt::For { body, .. } => {
                self.extract_properties_from_stmt(body, children, seen);
            }
            Stmt::TryCatch { try_body, catch_body, .. } => {
                self.extract_properties_from_stmt(try_body, children, seen);
                self.extract_properties_from_stmt(catch_body, children, seen);
            }
            _ => {}
        }
    }
    
    fn extract_properties_from_expr(&self, expr: &Expr, children: &mut Vec<Symbol>, seen: &mut std::collections::HashSet<String>) {
        match expr {
            Expr::Set { object, property, value, span } => {
                // Check if object is 'self'
                if matches!(object.as_ref(), Expr::SelfExpr { .. }) {
                    if !seen.contains(property) {
                        seen.insert(property.clone());
                        // Infer type from value
                        let type_hint = self.infer_type(value);
                        children.push(Symbol {
                            name: property.clone(),
                            kind: SymbolKind::Variable, // Properties shown as variables
                            range: span_to_range(span),
                            selection_range: span_to_range(span),
                            detail: Some(format!("self.{}", property)),
                            documentation: None,
                            children: Vec::new(),
                            type_hint, source_uri: None });
                    }
                }
                self.extract_properties_from_expr(value, children, seen);
            }
            Expr::Assignment { value, .. } => {
                self.extract_properties_from_expr(value, children, seen);
            }
            Expr::Call { callee, args, .. } => {
                self.extract_properties_from_expr(callee, children, seen);
                for arg in args {
                    self.extract_properties_from_expr(&arg.value, children, seen);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.extract_properties_from_expr(left, children, seen);
                self.extract_properties_from_expr(right, children, seen);
            }
            _ => {}
        }
    }

    /// Get completions based on context
    fn get_completions(&self, uri: &Url, position: Position) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let builtin_symbols = get_builtin_symbols();

        if let Some(doc) = self.symbols.get_document(uri) {
            let lines: Vec<&str> = doc.content.lines().collect();
            if let Some(line) = lines.get(position.line as usize) {
                let char_pos = position.character as usize;
                if char_pos > line.len() {
                    return items;
                }
                let prefix = &line[..char_pos];

                // Check if after a dot (method/property completion)
                if let Some(dot_pos) = prefix.rfind('.') {
                    let before_dot = prefix[..dot_pos].trim_end();
                    let word_start = before_dot.rfind(|c: char| !c.is_alphanumeric() && c != '_')
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    let var_name = &before_dot[word_start..];

                    // Special handling for 'self' - find enclosing class
                    if var_name == "self" {
                        if let Some(class_sym) = self.find_class_at_position(&doc.symbols, position) {
                            for child in &class_sym.children {
                                let insert = if child.kind == SymbolKind::Method {
                                    Some(format!("{}($0)", child.name))
                                } else {
                                    None
                                };
                                items.push(CompletionItem {
                                    label: child.name.clone(),
                                    kind: Some(child.kind.to_completion_kind()),
                                    detail: child.detail.clone(),
                                    insert_text: insert,
                                    insert_text_format: if child.kind == SymbolKind::Method {
                                        Some(InsertTextFormat::SNIPPET)
                                    } else { None },
                                    ..Default::default()
                                });
                            }
                            return items;
                        }
                    }

                    // Check if it's a variable and get its type
                    if let Some(type_name) = self.find_variable_type(&doc.symbols, var_name) {
                        // Resolve type path (handles nested paths like "Spark.App")
                        if let Some(type_sym) = self.resolve_type_path(&doc.symbols, &type_name)
                            .or_else(|| self.resolve_type_path(&builtin_symbols, &type_name)) {
                            // For both Class and Namespace, show children (methods/functions)
                            if matches!(type_sym.kind, SymbolKind::Class | SymbolKind::Namespace) {
                                for child in &type_sym.children {
                                    let insert = if matches!(child.kind, SymbolKind::Method | SymbolKind::Function) {
                                        Some(format!("{}($0)", child.name))
                                    } else {
                                        None
                                    };
                                    items.push(CompletionItem {
                                        label: child.name.clone(),
                                        kind: Some(child.kind.to_completion_kind()),
                                        detail: child.detail.clone(),
                                        insert_text: insert,
                                        insert_text_format: if matches!(child.kind, SymbolKind::Method | SymbolKind::Function) {
                                            Some(InsertTextFormat::SNIPPET)
                                        } else { None },
                                        ..Default::default()
                                    });
                                }
                                if !items.is_empty() {
                                    return items;
                                }
                            }
                        }
                    }

                    // Check if var_name itself is a class/namespace/enum in symbols (doc or builtin)
                    if let Some(sym) = self.find_symbol_by_name(&doc.symbols, var_name)
                        .or_else(|| self.find_symbol_by_name(&builtin_symbols, var_name)) {
                        match sym.kind {
                            SymbolKind::Class | SymbolKind::Namespace | SymbolKind::Enum => {
                                for child in &sym.children {
                                    let is_callable = matches!(child.kind, SymbolKind::Method | SymbolKind::Function);
                                    items.push(CompletionItem {
                                        label: child.name.clone(),
                                        kind: Some(child.kind.to_completion_kind()),
                                        detail: child.detail.clone(),
                                        insert_text: if is_callable { Some(format!("{}($0)", child.name)) } else { None },
                                        insert_text_format: if is_callable { Some(InsertTextFormat::SNIPPET) } else { None },
                                        ..Default::default()
                                    });
                                }
                                if !items.is_empty() {
                                    return items;
                                }
                            }
                            _ => {}
                        }
                    }

                    // No completions found for this dot context
                    return items;
                }
            }
        }

        // Default completions: keywords + all symbols
        items.extend(get_keyword_completions());

        // Add builtin symbols (classes)
        self.add_symbols_to_completions(&builtin_symbols, &mut items);

        // Add all document symbols
        if let Some(doc) = self.symbols.get_document(uri) {
            self.add_symbols_to_completions(&doc.symbols, &mut items);
        }

        items
    }

    fn add_symbols_to_completions(&self, symbols: &[Symbol], items: &mut Vec<CompletionItem>) {
        for sym in symbols {
            // Don't add methods to top-level completions (they're only accessible via instance)
            if sym.kind == SymbolKind::Method {
                continue;
            }
            
            items.push(CompletionItem {
                label: sym.name.clone(),
                kind: Some(sym.kind.to_completion_kind()),
                detail: sym.detail.clone(),
                ..Default::default()
            });
            
            // Add children for namespaces/enums (but not for classes - their methods are accessed via instance)
            if matches!(sym.kind, SymbolKind::Namespace | SymbolKind::Enum) {
                self.add_symbols_to_completions(&sym.children, items);
            }
        }
    }

    fn find_variable_type(&self, symbols: &[Symbol], name: &str) -> Option<String> {
        for sym in symbols {
            if sym.name == name && matches!(sym.kind, SymbolKind::Variable | SymbolKind::Constant) {
                return sym.type_hint.clone();
            }
            if let Some(t) = self.find_variable_type(&sym.children, name) {
                return Some(t);
            }
        }
        None
    }

    /// Resolve a type path like "Spark.App" by traversing namespace children
    fn resolve_type_path<'a>(&self, symbols: &'a [Symbol], type_path: &str) -> Option<&'a Symbol> {
        let parts: Vec<&str> = type_path.split('.').collect();
        if parts.is_empty() {
            return None;
        }

        // Find the root symbol
        let root = self.find_symbol_by_name(symbols, parts[0])?;
        
        if parts.len() == 1 {
            return Some(root);
        }

        // Traverse children for remaining parts
        let mut current = root;
        for part in &parts[1..] {
            let found = current.children.iter().find(|c| c.name == *part)?;
            current = found;
        }
        
        Some(current)
    }

    
    /// Find the class that contains the given position (for self. completion)
    fn find_class_at_position<'a>(&self, symbols: &'a [Symbol], position: Position) -> Option<&'a Symbol> {
        for sym in symbols {
            if sym.kind == SymbolKind::Class {
                // Check if position is within class range
                if position.line >= sym.range.start.line 
                    && position.line <= sym.range.end.line {
                    return Some(sym);
                }
            }
            // Also check nested symbols (like in namespaces)
            if let Some(found) = self.find_class_at_position(&sym.children, position) {
                return Some(found);
            }
        }
        None
    }

    fn find_symbol_by_name<'a>(&self, symbols: &'a [Symbol], name: &str) -> Option<&'a Symbol> {
        for sym in symbols {
            if sym.name == name {
                return Some(sym);
            }
            // For classes, also check if name matches a method (but don't return the method directly)
            // For namespaces/enums, recurse into children
            if matches!(sym.kind, SymbolKind::Namespace | SymbolKind::Enum) {
                if let Some(found) = self.find_symbol_by_name(&sym.children, name) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Find symbol including methods (for hover on method calls)
    fn find_symbol_deep<'a>(&self, symbols: &'a [Symbol], name: &str) -> Option<&'a Symbol> {
        for sym in symbols {
            if sym.name == name {
                return Some(sym);
            }
            // Check all children including class methods
            if let Some(found) = self.find_symbol_deep(&sym.children, name) {
                return Some(found);
            }
        }
        None
    }

    /// Find symbol at position for go-to-definition
    fn find_definition(&self, uri: &Url, position: Position) -> Option<Location> {
        let doc = self.symbols.get_document(uri)?;
        let lines: Vec<&str> = doc.content.lines().collect();
        let line = lines.get(position.line as usize)?;

        let char_pos = position.character as usize;
        if char_pos > line.len() {
            return None;
        }
        
        // Get the full qualified name (e.g., "Spark.Response.json")
        let full_expr = self.get_full_qualified_name(line, char_pos);
        let parts: Vec<&str> = full_expr.split('.').collect();
        
        // Simple word extraction for the current identifier
        let start = line[..char_pos]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = line[char_pos..]
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| char_pos + i)
            .unwrap_or(line.len());

        if start >= end {
            return None;
        }

        let word = &line[start..end];

        // Helper to create Location from symbol
        let make_location = |sym: &Symbol, fallback_uri: &Url| -> Option<Location> {
            // If symbol has source_uri (which is a file path), convert to URL
            if let Some(ref source_path) = sym.source_uri {
                if !source_path.is_empty() {
                    // Convert file path to URL using from_file_path for correct handling on all platforms
                    let path = std::path::Path::new(source_path);
                    if let Ok(file_uri) = Url::from_file_path(path) {
                        // Only use if range seems valid (not all zeros)
                        if sym.selection_range.end.line > 0 || sym.selection_range.end.character > 0 
                            || sym.selection_range.start.character > 0 {
                            return Some(Location {
                                uri: file_uri,
                                range: sym.selection_range,
                            });
                        }
                    }
                }
            }
            // Use fallback URI - only if range is valid
            if sym.selection_range.end.line > 0 || sym.selection_range.end.character > 0 
                || sym.selection_range.start.character > 0 
                || sym.range.start.line > 0 {
                return Some(Location {
                    uri: fallback_uri.clone(),
                    range: sym.selection_range,
                });
            }
            None
        };

        // First try: Handle qualified names like Spark.Response.json
        if parts.len() > 1 {
            // Resolve the full path using our resolve_type_path helper
            if let Some(sym) = self.resolve_type_path(&doc.symbols, &full_expr) {
                if let Some(loc) = make_location(sym, uri) {
                    return Some(loc);
                }
            }
            
            // If full path didn't work, try to find the specific part we're hovering
            // Find the part before the current word
            let word_index = parts.iter().position(|&p| p == word);
            if let Some(idx) = word_index {
                // Build path up to and including current word
                let path_to_word = parts[..=idx].join(".");
                if let Some(sym) = self.resolve_type_path(&doc.symbols, &path_to_word) {
                    if let Some(loc) = make_location(sym, uri) {
                        return Some(loc);
                    }
                }
            }
        }

        // Second try: Search in document symbols (local definitions)
        if let Some(sym) = self.find_symbol_deep(&doc.symbols, word) {
            if let Some(loc) = make_location(&sym, uri) {
                return Some(loc);
            }
        }

        // Third try: builtin symbols (no location available for builtins)
        // Builtins don't have real file locations
        None
    }

    /// Get the full qualified name around position (e.g., "Spark.Response.json")
    fn get_full_qualified_name(&self, line: &str, char_pos: usize) -> String {
        let chars: Vec<char> = line.chars().collect();
        
        // Find start - go backwards
        let mut start = char_pos;
        while start > 0 {
            let c = chars[start - 1];
            if c.is_alphanumeric() || c == '_' || c == '.' {
                start -= 1;
            } else {
                break;
            }
        }
        
        // Find end - go forwards
        let mut end = char_pos;
        while end < chars.len() {
            let c = chars[end];
            if c.is_alphanumeric() || c == '_' {
                end += 1;
            } else {
                break;
            }
        }
        
        chars[start..end].iter().collect()
    }


    /// Find if cursor is inside a string literal and return (content, start, end)
    fn find_string_at_position(&self, line: &str, char_pos: usize) -> Option<(String, usize, usize)> {
        // Find all string literals in the line
        let mut in_string = false;
        let mut string_start = 0;
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        
        while i < chars.len() {
            if chars[i] == '"' && (i == 0 || chars[i-1] != '\\') {
                if !in_string {
                    in_string = true;
                    string_start = i;
                } else {
                    // End of string
                    let string_end = i + 1;
                    // Check if cursor is inside this string
                    if char_pos >= string_start && char_pos < string_end {
                        let content: String = chars[string_start+1..i].iter().collect();
                        return Some((content, string_start, string_end));
                    }
                    in_string = false;
                }
            }
            i += 1;
        }
        
        None
    }

    /// Check if cursor position is inside a comment
    fn is_in_comment(&self, line: &str, char_pos: usize) -> bool {
        // Check for single-line comment //
        if let Some(comment_start) = line.find("//") {
            if char_pos >= comment_start {
                return true;
            }
        }
        
        // For simplicity, we don't handle multi-line comments here
        // as they would require cross-line analysis
        false
    }

    /// Get hover info with fallback
    fn get_hover_info(&self, uri: &Url, position: Position) -> Option<Hover> {
        let doc = self.symbols.get_document(uri)?;
        let lines: Vec<&str> = doc.content.lines().collect();
        let line = lines.get(position.line as usize)?;

        let char_pos = position.character as usize;
        if char_pos > line.len() {
            return None;
        }

        // Check if cursor is inside a string literal
        if let Some((string_content, string_start, string_end)) = self.find_string_at_position(line, char_pos) {
            let word_range = Range {
                start: Position { line: position.line, character: string_start as u32 },
                end: Position { line: position.line, character: string_end as u32 },
            };
            
            // Check if this looks like an import path
            if string_content.ends_with(".sald") || !string_content.contains(' ') {
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("**\"{}\"** (string literal)", string_content),
                    }),
                    range: Some(word_range),
                });
            }
            
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("**String literal**\n\n`\"{}\"`", string_content),
                }),
                range: Some(word_range),
            });
        }

        // Check if cursor is inside a comment
        if self.is_in_comment(line, char_pos) {
            return None; // Don't show hover for comments
        }

        let start = line[..char_pos]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = line[char_pos..]
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| char_pos + i)
            .unwrap_or(line.len());

        if start >= end {
            return None;
        }

        let word = &line[start..end];
        let word_range = Range {
            start: Position { line: position.line, character: start as u32 },
            end: Position { line: position.line, character: end as u32 },
        };

        // Check keywords first (including self, super, true, false, null)
        let keywords = [
            ("let", "Variable declaration"),
            ("const", "Constant declaration"),
            ("fun", "Function declaration"),
            ("class", "Class declaration"),
            ("if", "Conditional statement"),
            ("else", "Else branch"),
            ("while", "While loop"),
            ("for", "For-in loop"),
            ("in", "In keyword for loops"),
            ("do", "Do-while loop"),
            ("return", "Return from function"),
            ("break", "Break from loop"),
            ("continue", "Continue to next iteration"),
            ("import", "Import module"),
            ("as", "Import alias"),
            ("namespace", "Namespace declaration"),
            ("enum", "Enum declaration"),
            ("try", "Try block"),
            ("catch", "Catch block"),
            ("throw", "Throw exception"),
            ("switch", "Switch expression"),
            ("default", "Default case"),
            ("async", "Async function modifier"),
            ("await", "Await async expression"),
            ("self", "Reference to current instance"),
            ("super", "Reference to parent class"),
            ("extends", "Class inheritance"),
            ("static", "Static method/property"),
            ("true", "Boolean true"),
            ("false", "Boolean false"),
            ("null", "Null value"),
        ];
        
        for (kw, doc) in keywords {
            if word == kw {
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("**{}** (keyword)\n\n{}", kw, doc),
                    }),
                    range: Some(word_range),
                });
            }
        }

        // Check builtin symbols
        let builtin_symbols = get_builtin_symbols();
        if let Some(sym) = self.find_symbol_deep(&builtin_symbols, word) {
            let kind_str = format!("{:?}", sym.kind).to_lowercase();
            let content = format!(
                "**{}** ({})",
                sym.name,
                kind_str,
            );
            let doc_content = sym.documentation.as_ref()
                .or(sym.detail.as_ref())
                .map(|d| format!("\n\n{}", d))
                .unwrap_or_default();
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("{}{}", content, doc_content),
                }),
                range: Some(word_range),
            });
        }

        // Check document symbols (including nested methods)
        if let Some(sym) = self.find_symbol_deep(&doc.symbols, word) {
            let kind_str = format!("{:?}", sym.kind).to_lowercase();
            
            // Build type info string
            let type_info = sym.type_hint.as_ref()
                .map(|t| format!(": {}", t))
                .unwrap_or_default();
            
            // Build hover content based on symbol kind
            let content = match sym.kind {
                SymbolKind::Variable | SymbolKind::Constant => {
                    let decl_keyword = if sym.kind == SymbolKind::Constant { "const" } else { "let" };
                    format!(
                        "```sald\n{} {}{}\n```\n\n_{}_",
                        decl_keyword,
                        sym.name,
                        type_info,
                        kind_str
                    )
                }
                SymbolKind::Function | SymbolKind::Method => {
                    format!(
                        "```sald\n{}\n```\n\n_{}_",
                        sym.detail.as_deref().unwrap_or(&sym.name),
                        kind_str
                    )
                }
                SymbolKind::Class => {
                    format!(
                        "```sald\nclass {}\n```\n\n_{}_",
                        sym.name,
                        kind_str
                    )
                }
                SymbolKind::Namespace => {
                    format!(
                        "```sald\nnamespace {}\n```\n\n_{}_",
                        sym.name,
                        kind_str
                    )
                }
                _ => {
                    format!(
                        "**{}**{}\n\n_{}_",
                        sym.name,
                        type_info,
                        kind_str
                    )
                }
            };
            
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: Some(word_range),
            });
        }

        // No symbol found - don't show unhelpful fallback
        None
    }
    
    /// Index the entire workspace for cross-file symbol tracking
    async fn index_workspace(&self) {
        use super::symbols::WorkspaceIndex;
        
        let files = self.workspace_index.scan_workspace_files();
        
        self.client
            .log_message(MessageType::INFO, format!("Indexing {} workspace files...", files.len()))
            .await;
        
        // First pass: index all files for exports/references
        for file_path in &files {
            self.index_file(file_path);
        }
        
        self.client
            .log_message(MessageType::INFO, "Workspace indexing complete")
            .await;
        
        // Count non-module files
        let analyze_files: Vec<_> = files.iter()
            .filter(|p| !WorkspaceIndex::is_sald_modules_path(p))
            .collect();
        
        self.client
            .log_message(MessageType::INFO, format!("Analyzing {} project files for diagnostics...", analyze_files.len()))
            .await;
        
        // Second pass: run diagnostics for non-sald_modules files
        for file_path in analyze_files {
            // Read and analyze file
            if let Ok(content) = std::fs::read_to_string(file_path) {
                if let Ok(uri) = Url::from_file_path(file_path) {
                    self.client
                        .log_message(MessageType::LOG, format!("Analyzing: {}", file_path.display()))
                        .await;
                    self.analyze_document(uri, content).await;
                }
            }
        }
        
        self.client
            .log_message(MessageType::INFO, "Workspace analysis complete")
            .await;
    }
    
    /// Index a single file for exports and references
    fn index_file(&self, file_path: &std::path::Path) {
        // Read file content
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => return,
        };
        
        let file_name = file_path.to_string_lossy().to_string();
        
        // Tokenize
        let mut scanner = Scanner::new(&content, &file_name);
        let tokens = match scanner.scan_tokens() {
            Ok(t) => t,
            Err(_) => return,
        };
        
        // Parse
        let mut parser = Parser::new(tokens, &file_name, &content);
        let program = match parser.parse() {
            Ok(p) => p,
            Err(_) => return,
        };
        
        // Extract exported symbols (top-level)
        let mut exports = Vec::new();
        for stmt in &program.statements {
            self.extract_export_symbol(stmt, &mut exports);
        }
        
        // Register exports
        self.workspace_index.register_exports(file_path.to_path_buf(), exports);
        
        // Collect referenced symbols (identifiers used in this file)
        let mut references = Vec::new();
        for stmt in &program.statements {
            self.collect_references_from_stmt(stmt, &mut references);
        }
        
        // Register references
        self.workspace_index.register_references(&file_path.to_path_buf(), references);
    }
    
    /// Extract top-level exported symbols from a statement
    fn extract_export_symbol(&self, stmt: &Stmt, exports: &mut Vec<Symbol>) {
        match stmt {
            Stmt::Function { def } => exports.push(self.function_to_symbol(def)),
            Stmt::Class { def } => exports.push(self.class_to_symbol(def)),
            Stmt::Let { name, span, .. } => {
                exports.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Variable,
                    range: span_to_range(span),
                    selection_range: span_to_range(span),
                    detail: Some(format!("let {}", name)),
                    documentation: None,
                    children: Vec::new(),
                    type_hint: None,
                    source_uri: None,
                });
            }
            Stmt::Const { name, span, .. } => {
                exports.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Constant,
                    range: span_to_range(span),
                    selection_range: span_to_range(span),
                    detail: Some(format!("const {}", name)),
                    documentation: None,
                    children: Vec::new(),
                    type_hint: None,
                    source_uri: None,
                });
            }
            Stmt::Namespace { name, span, .. } => {
                exports.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Namespace,
                    range: span_to_range(span),
                    selection_range: span_to_range(span),
                    detail: Some(format!("namespace {}", name)),
                    documentation: None,
                    children: Vec::new(),
                    type_hint: None,
                    source_uri: None,
                });
            }
            Stmt::Enum { name, span, .. } => {
                exports.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Enum,
                    range: span_to_range(span),
                    selection_range: span_to_range(span),
                    detail: Some(format!("enum {}", name)),
                    documentation: None,
                    children: Vec::new(),
                    type_hint: None,
                    source_uri: None,
                });
            }
            _ => {}
        }
    }
    
    /// Collect referenced symbol names from a statement
    fn collect_references_from_stmt(&self, stmt: &Stmt, refs: &mut Vec<String>) {
        match stmt {
            Stmt::Expression { expr, .. } => self.collect_references_from_expr(expr, refs),
            Stmt::Let { initializer, .. } => {
                if let Some(init) = initializer {
                    self.collect_references_from_expr(init, refs);
                }
            }
            Stmt::Const { value, .. } => self.collect_references_from_expr(value, refs),
            Stmt::Block { statements, .. } => {
                for s in statements {
                    self.collect_references_from_stmt(s, refs);
                }
            }
            Stmt::If { condition, then_branch, else_branch, .. } => {
                self.collect_references_from_expr(condition, refs);
                self.collect_references_from_stmt(then_branch, refs);
                if let Some(eb) = else_branch {
                    self.collect_references_from_stmt(eb, refs);
                }
            }
            Stmt::While { condition, body, .. } => {
                self.collect_references_from_expr(condition, refs);
                self.collect_references_from_stmt(body, refs);
            }
            Stmt::For { iterable, body, .. } => {
                self.collect_references_from_expr(iterable, refs);
                self.collect_references_from_stmt(body, refs);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.collect_references_from_expr(v, refs);
                }
            }
            Stmt::Function { def } => {
                for s in &def.body {
                    self.collect_references_from_stmt(s, refs);
                }
            }
            Stmt::Class { def } => {
                for method in &def.methods {
                    for s in &method.body {
                        self.collect_references_from_stmt(s, refs);
                    }
                }
            }
            _ => {}
        }
    }
    
    /// Collect referenced symbol names from an expression
    fn collect_references_from_expr(&self, expr: &Expr, refs: &mut Vec<String>) {
        match expr {
            Expr::Identifier { name, .. } => {
                refs.push(name.clone());
            }
            Expr::Call { callee, args, .. } => {
                self.collect_references_from_expr(callee, refs);
                for arg in args {
                    self.collect_references_from_expr(&arg.value, refs);
                }
            }
            Expr::Get { object, .. } => {
                self.collect_references_from_expr(object, refs);
            }
            Expr::Binary { left, right, .. } => {
                self.collect_references_from_expr(left, refs);
                self.collect_references_from_expr(right, refs);
            }
            Expr::Unary { operand, .. } => {
                self.collect_references_from_expr(operand, refs);
            }
            Expr::Lambda { body, .. } => {
                match body {
                    crate::ast::LambdaBody::Expr(e) => self.collect_references_from_expr(e, refs),
                    crate::ast::LambdaBody::Block(stmts) => {
                        for s in stmts {
                            self.collect_references_from_stmt(s, refs);
                        }
                    }
                }
            }
            Expr::Array { elements, .. } => {
                for el in elements {
                    self.collect_references_from_expr(el, refs);
                }
            }
            Expr::Dictionary { entries, .. } => {
                for (k, v) in entries {
                    self.collect_references_from_expr(k, refs);
                    self.collect_references_from_expr(v, refs);
                }
            }
            Expr::Assignment { value, .. } | Expr::Set { value, .. } => {
                self.collect_references_from_expr(value, refs);
            }
            Expr::Index { object, index, .. } => {
                self.collect_references_from_expr(object, refs);
                self.collect_references_from_expr(index, refs);
            }
            Expr::Ternary { condition, then_expr, else_expr, .. } => {
                self.collect_references_from_expr(condition, refs);
                self.collect_references_from_expr(then_expr, refs);
                self.collect_references_from_expr(else_expr, refs);
            }
            Expr::Await { expr: e, .. } | Expr::Grouping { expr: e, .. } => {
                self.collect_references_from_expr(e, refs);
            }
            _ => {}
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SaldLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Set workspace root for import resolution
        if let Some(root_uri) = params.root_uri {
            if let Ok(path) = root_uri.to_file_path() {
                self.import_resolver.write().set_workspace_root(path.clone());
                self.workspace_index.set_workspace_root(path.clone());
                crate::set_project_root(&path);
                // Initial workspace indexing
                self.index_workspace().await;
            }
        } else if let Some(folders) = params.workspace_folders {
            if let Some(first) = folders.first() {
                if let Ok(path) = first.uri.to_file_path() {
                    self.import_resolver.write().set_workspace_root(path.clone());
                    self.workspace_index.set_workspace_root(path.clone());
                    crate::set_project_root(&path);
                    // Initial workspace indexing
                    self.index_workspace().await;
                }
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "sald-lsp".to_string(),
                version: Some("0.3.0".to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Sald LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.analyze_document(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            self.analyze_document(params.text_document.uri, change.text)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.symbols.remove_document(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let items = self.get_completions(&uri, position);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        Ok(self.get_hover_info(&uri, position))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        
        if let Some(location) = self.find_definition(&uri, position) {
            Ok(Some(GotoDefinitionResponse::Scalar(location)))
        } else {
            Ok(None)
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        if let Some(doc) = self.symbols.get_document(&uri) {
            let symbols: Vec<DocumentSymbol> = doc
                .symbols
                .iter()
                .map(|s| symbol_to_document_symbol(s))
                .collect();

            return Ok(Some(DocumentSymbolResponse::Nested(symbols)));
        }

        Ok(None)
    }
}

/// Convert our Symbol to LSP DocumentSymbol
#[allow(deprecated)]
fn symbol_to_document_symbol(sym: &Symbol) -> DocumentSymbol {
    let children = if sym.children.is_empty() {
        None
    } else {
        Some(sym.children.iter().map(symbol_to_document_symbol).collect())
    };

    DocumentSymbol {
        name: sym.name.clone(),
        detail: sym.detail.clone(),
        kind: sym.kind.to_lsp(),
        tags: None,
        deprecated: None,
        range: sym.range,
        selection_range: sym.selection_range,
        children,
    }
}


