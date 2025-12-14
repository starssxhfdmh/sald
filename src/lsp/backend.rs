// Sald Language Server Backend
// Implements tower_lsp::LanguageServer trait with full LSP support

use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use super::analyzer::SemanticAnalyzer;
use super::completion::{get_builtin_symbols, get_keyword_completions};
use super::symbols::{span_to_range, Symbol, SymbolKind, SymbolTable};
use crate::lexer::Scanner;
use crate::parser::Parser;
use crate::ast::{Stmt, Expr, FunctionDef, ClassDef};
use crate::compiler::Compiler;

/// Sald Language Server
pub struct SaldLanguageServer {
    client: Client,
    symbols: Arc<SymbolTable>,
}

impl SaldLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            symbols: Arc::new(SymbolTable::new()),
        }
    }

    /// Analyze a document and publish diagnostics
    async fn analyze_document(&self, uri: Url, text: String) {
        let mut diagnostics = Vec::new();
        let mut symbols = Vec::new();

        let file_name = uri.path().to_string();

        // Tokenize
        let mut scanner = Scanner::new(&text, &file_name);
        match scanner.scan_tokens() {
            Ok(tokens) => {
                // Parse
                let mut parser = Parser::new(tokens, &file_name, &text);
                match parser.parse() {
                    Ok(program) => {
                        // Extract symbols from AST (including nested ones)
                        for stmt in &program.statements {
                            self.extract_symbols_recursive(stmt, &mut symbols);
                        }

                        // Run semantic analysis
                        let mut analyzer = SemanticAnalyzer::new();
                        let semantic_diagnostics = analyzer.analyze(&program);
                        diagnostics.extend(semantic_diagnostics);

                        // Also run compiler to catch additional errors
                        let mut compiler = Compiler::new(&file_name, &text);
                        if let Err(e) = compiler.compile(&program) {
                            diagnostics.push(Diagnostic {
                                range: span_to_range(&e.span),
                                severity: Some(DiagnosticSeverity::ERROR),
                                source: Some("sald".to_string()),
                                message: e.message.clone(),
                                ..Default::default()
                            });
                        }
                    }
                    Err(e) => {
                        diagnostics.push(Diagnostic {
                            range: span_to_range(&e.span),
                            severity: Some(DiagnosticSeverity::ERROR),
                            source: Some("sald".to_string()),
                            message: e.message.clone(),
                            ..Default::default()
                        });
                    }
                }
            }
            Err(e) => {
                diagnostics.push(Diagnostic {
                    range: span_to_range(&e.span),
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("sald".to_string()),
                    message: e.message.clone(),
                    ..Default::default()
                });
            }
        }

        // Update symbol table
        self.symbols.update_document(uri.clone(), text, symbols);

        // Publish diagnostics
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
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
                    type_hint,
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
                });
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
                    type_hint: None,
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
                }).collect();
                symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::Enum,
                    range: span_to_range(span),
                    selection_range: span_to_range(span),
                    detail: Some(format!("enum {} ({} variants)", name, variants.len())),
                    documentation: None,
                    children,
                    type_hint: None,
                });
            }
            Stmt::Block { statements, .. } => {
                for s in statements {
                    self.extract_symbols_recursive(s, symbols);
                }
            }
            _ => {}
        }
    }

    /// Infer type from expression (simple heuristic)
    fn infer_type(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call { callee, .. } => {
                // If calling a class constructor, the type is that class
                if let Expr::Identifier { name, .. } = callee.as_ref() {
                    // Check if first letter is uppercase (class naming convention)
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
            type_hint: None,
        }
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
                    type_hint: None,
                }
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
            type_hint: None,
        }
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
                            type_hint,
                        });
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
                        // Get methods from that class (check both doc symbols and builtins)
                        if let Some(class_sym) = self.find_symbol_by_name(&doc.symbols, &type_name)
                            .or_else(|| self.find_symbol_by_name(&builtin_symbols, &type_name)) {
                            if class_sym.kind == SymbolKind::Class {
                                for child in &class_sym.children {
                                    items.push(CompletionItem {
                                        label: child.name.clone(),
                                        kind: Some(child.kind.to_completion_kind()),
                                        detail: child.detail.clone(),
                                        insert_text: Some(format!("{}($0)", child.name)),
                                        insert_text_format: Some(InsertTextFormat::SNIPPET),
                                        ..Default::default()
                                    });
                                }
                                if !items.is_empty() {
                                    return items;
                                }
                            }
                        }
                    }

                    // Check if var_name itself is a class/namespace in symbols (doc or builtin)
                    if let Some(sym) = self.find_symbol_by_name(&doc.symbols, var_name)
                        .or_else(|| self.find_symbol_by_name(&builtin_symbols, var_name)) {
                        match sym.kind {
                            SymbolKind::Class | SymbolKind::Namespace | SymbolKind::Enum => {
                                for child in &sym.children {
                                    items.push(CompletionItem {
                                        label: child.name.clone(),
                                        kind: Some(child.kind.to_completion_kind()),
                                        detail: child.detail.clone(),
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

        // Find word at position
        let char_pos = position.character as usize;
        if char_pos > line.len() {
            return None;
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

        // Search for symbol definition (including methods)
        if let Some(sym) = self.find_symbol_deep(&doc.symbols, word) {
            return Some(Location {
                uri: uri.clone(),
                range: sym.selection_range,
            });
        }

        None
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

        // Check builtin symbols first
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

        // Check keywords
        let keywords = ["let", "const", "fun", "class", "if", "else", "while", "for", 
                       "in", "return", "break", "continue", "import", "namespace", 
                       "enum", "try", "catch", "throw", "switch", "default", "async", "await"];
        if keywords.contains(&word) {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("**{}** (keyword)", word),
                }),
                range: Some(word_range),
            });
        }

        // Check document symbols (including nested methods)
        if let Some(sym) = self.find_symbol_deep(&doc.symbols, word) {
            let kind_str = format!("{:?}", sym.kind).to_lowercase();
            let type_info = sym.type_hint.as_ref()
                .map(|t| format!(" → {}", t))
                .unwrap_or_default();
            
            let content = format!(
                "**{}** ({}){}\n\n```sald\n{}\n```",
                sym.name,
                kind_str,
                type_info,
                sym.detail.as_deref().unwrap_or(&sym.name)
            );
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: Some(word_range),
            });
        }

        // Fallback: show the word itself with inferred info
        let fallback = format!("**{}** (identifier)", word);
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: fallback,
            }),
            range: Some(word_range),
        })
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SaldLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
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
                version: Some("0.2.0".to_string()),
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
