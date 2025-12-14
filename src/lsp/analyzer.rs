// Semantic Analyzer for Sald LSP
// Detects runtime-like errors statically: undefined variables, type mismatches, etc.

use std::collections::{HashMap, HashSet};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::ast::{Expr, Program, Stmt, FunctionDef, LambdaBody};
use crate::error::Span;
use super::symbols::span_to_range;

/// Scope for tracking local variables
#[derive(Debug, Clone)]
struct Scope {
    variables: HashMap<String, VarInfo>,
}

#[derive(Debug, Clone)]
#[allow(unused)]
struct VarInfo {
    span: Span,
    is_const: bool,
    is_used: bool,
}

/// Semantic Analyzer
pub struct SemanticAnalyzer {
    scopes: Vec<Scope>,
    diagnostics: Vec<Diagnostic>,
    defined_classes: HashSet<String>,
    defined_functions: HashSet<String>,
    has_imports: bool, // If file has imports, be lenient with undefined checks
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        let mut defined_classes = HashSet::new();
        // Built-in classes
        for cls in &[
            "Console", "Math", "File", "Timer", "Date", "Json", "Path",
            "Process", "Http", "Type", "System", "Array", "Dict", "String", "Ffi",
            "Number", "Boolean", // Type conversion classes
        ] {
            defined_classes.insert(cls.to_string());
        }

        Self {
            scopes: vec![Scope {
                variables: HashMap::new(),
            }],
            diagnostics: Vec::new(),
            defined_classes,
            defined_functions: HashSet::new(),
            has_imports: false,
        }
    }

    pub fn analyze(&mut self, program: &Program) -> Vec<Diagnostic> {
        self.diagnostics.clear();
        self.has_imports = false;

        // First pass: collect top-level declarations and check for imports
        for stmt in &program.statements {
            self.collect_declaration(stmt);
        }

        // Second pass: analyze
        for stmt in &program.statements {
            self.analyze_stmt(stmt);
        }

        // Don't check for unused variables to reduce noise
        // self.check_unused_warnings();

        std::mem::take(&mut self.diagnostics)
    }

    fn collect_declaration(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Function { def } => {
                self.defined_functions.insert(def.name.clone());
            }
            Stmt::Class { def } => {
                self.defined_classes.insert(def.name.clone());
            }
            Stmt::Namespace { name, body, .. } => {
                // Namespace itself is a valid identifier
                self.defined_classes.insert(name.clone());
                for s in body {
                    self.collect_declaration(s);
                }
            }
            Stmt::Enum { name, .. } => {
                self.defined_classes.insert(name.clone());
            }
            Stmt::Import { .. } => {
                // Mark that this file has imports - be lenient with undefined checks
                self.has_imports = true;
            }
            _ => {}
        }
    }

    fn current_scope_mut(&mut self) -> &mut Scope {
        self.scopes.last_mut().unwrap()
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope {
            variables: HashMap::new(),
        });
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        // Don't warn about unused variables - too noisy
    }

    fn define_var(&mut self, name: &str, span: &Span, is_const: bool) {
        // Allow variable redefinition in Sald (unlike JS let)
        self.current_scope_mut().variables.insert(
            name.to_string(),
            VarInfo {
                span: *span,
                is_const,
                is_used: false,
            },
        );
    }

    fn resolve_var(&mut self, name: &str) -> Option<bool> {
        // Check built-in classes first
        if self.defined_classes.contains(name) {
            return Some(false);
        }
        // Check defined functions
        if self.defined_functions.contains(name) {
            return Some(false);
        }
        // Check built-in constants and special variables
        if matches!(name, "true" | "false" | "null" | "self" | "super") {
            return Some(false);
        }

        // Search scopes from innermost to outermost
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.variables.get_mut(name) {
                info.is_used = true;
                return Some(info.is_const);
            }
        }
        None
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, initializer, span } => {
                // Analyze value first
                if let Some(val) = initializer {
                    self.analyze_expr(val);
                }
                self.define_var(name, span, false);
            }
            Stmt::Const { name, value, span } => {
                self.analyze_expr(value);
                self.define_var(name, span, true);
            }
            Stmt::Expression { expr, .. } => {
                self.analyze_expr(expr);
            }
            Stmt::Block { statements, .. } => {
                self.push_scope();
                for s in statements {
                    self.analyze_stmt(s);
                }
                self.pop_scope();
            }
            Stmt::If { condition, then_branch, else_branch, .. } => {
                self.analyze_expr(condition);
                self.analyze_stmt(then_branch);
                if let Some(eb) = else_branch {
                    self.analyze_stmt(eb);
                }
            }
            Stmt::While { condition, body, .. } => {
                self.analyze_expr(condition);
                self.analyze_stmt(body);
            }
            Stmt::DoWhile { condition, body, .. } => {
                self.analyze_stmt(body);
                self.analyze_expr(condition);
            }
            Stmt::For { variable, iterable, body, span } => {
                self.analyze_expr(iterable);
                self.push_scope();
                self.define_var(variable, span, false);
                self.analyze_stmt(body);
                self.pop_scope();
            }
            Stmt::Function { def } => {
                self.analyze_function(def);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.analyze_expr(v);
                }
            }
            Stmt::Class { def } => {
                for method in &def.methods {
                    self.analyze_function(method);
                }
            }
            Stmt::TryCatch { try_body, catch_var, catch_body, span } => {
                self.analyze_stmt(try_body);
                self.push_scope();
                self.define_var(catch_var, span, false);
                self.analyze_stmt(catch_body);
                self.pop_scope();
            }
            Stmt::Throw { value, .. } => {
                self.analyze_expr(value);
            }
            Stmt::Namespace { body, .. } => {
                self.push_scope();
                for s in body {
                    self.analyze_stmt(s);
                }
                self.pop_scope();
            }
            Stmt::Import { .. } => {
                // Import statements introduce globals from external files
                // We can't fully analyze without loading them
            }
            Stmt::Break { .. } | Stmt::Continue { .. } | Stmt::Enum { .. } => {}
        }
    }

    fn analyze_function(&mut self, def: &FunctionDef) {
        self.push_scope();
        
        // Define parameters (including 'self' implicitly for methods)
        for param in &def.params {
            self.define_var(&param.name, &param.span, false);
        }

        // Analyze body
        for stmt in &def.body {
            self.analyze_stmt(stmt);
        }

        self.pop_scope();
    }

    fn analyze_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Identifier { name, span } => {
                // Skip undefined check if file has imports (can't track imported symbols)
                if self.has_imports {
                    // Just mark as used if it exists
                    self.resolve_var(name);
                    return;
                }
                
                if self.resolve_var(name).is_none() {
                    self.diagnostics.push(Diagnostic {
                        range: span_to_range(span),
                        severity: Some(DiagnosticSeverity::ERROR),
                        source: Some("sald".to_string()),
                        message: format!("Undefined variable '{}'", name),
                        ..Default::default()
                    });
                }
            }
            Expr::Assignment { target, value, span, .. } => {
                // Check if assigning to const
                if let Expr::Identifier { name, .. } = target.as_ref() {
                    if let Some(is_const) = self.resolve_var(name) {
                        if is_const {
                            self.diagnostics.push(Diagnostic {
                                range: span_to_range(span),
                                severity: Some(DiagnosticSeverity::ERROR),
                                source: Some("sald".to_string()),
                                message: format!("Cannot assign to constant '{}'", name),
                                ..Default::default()
                            });
                        }
                    }
                }
                self.analyze_expr(target);
                self.analyze_expr(value);
            }
            Expr::Binary { left, right, .. } => {
                self.analyze_expr(left);
                self.analyze_expr(right);
            }
            Expr::Unary { operand, .. } => {
                self.analyze_expr(operand);
            }
            Expr::Grouping { expr, .. } => {
                self.analyze_expr(expr);
            }
            Expr::Call { callee, args, .. } => {
                self.analyze_expr(callee);
                for arg in args {
                    self.analyze_expr(&arg.value);
                }
            }
            Expr::Get { object, .. } => {
                self.analyze_expr(object);
            }
            Expr::Set { object, value, .. } => {
                self.analyze_expr(object);
                self.analyze_expr(value);
            }
            Expr::Index { object, index, .. } => {
                self.analyze_expr(object);
                self.analyze_expr(index);
            }
            Expr::IndexSet { object, index, value, .. } => {
                self.analyze_expr(object);
                self.analyze_expr(index);
                self.analyze_expr(value);
            }
            Expr::Array { elements, .. } => {
                for el in elements {
                    self.analyze_expr(el);
                }
            }
            Expr::Dictionary { entries, .. } => {
                for (k, v) in entries {
                    self.analyze_expr(k);
                    self.analyze_expr(v);
                }
            }
            Expr::Lambda { params, body, span, .. } => {
                self.push_scope();
                for param in params {
                    self.define_var(&param.name, span, false);
                }
                match body {
                    LambdaBody::Expr(expr) => self.analyze_expr(expr),
                    LambdaBody::Block(stmts) => {
                        for stmt in stmts {
                            self.analyze_stmt(stmt);
                        }
                    }
                }
                self.pop_scope();
            }
            Expr::Ternary { condition, then_expr, else_expr, .. } => {
                self.analyze_expr(condition);
                self.analyze_expr(then_expr);
                self.analyze_expr(else_expr);
            }
            Expr::Await { expr, .. } => {
                self.analyze_expr(expr);
            }
            Expr::Switch { value, arms, default, .. } => {
                self.analyze_expr(value);
                for arm in arms {
                    for pattern in &arm.patterns {
                        self.analyze_expr(pattern);
                    }
                    self.analyze_expr(&arm.body);
                }
                if let Some(def) = default {
                    self.analyze_expr(def);
                }
            }
            Expr::Block { statements, expr, .. } => {
                self.push_scope();
                for stmt in statements {
                    self.analyze_stmt(stmt);
                }
                if let Some(e) = expr {
                    self.analyze_expr(e);
                }
                self.pop_scope();
            }
            Expr::Return { value, .. } => {
                if let Some(v) = value {
                    self.analyze_expr(v);
                }
            }
            Expr::Throw { value, .. } => {
                self.analyze_expr(value);
            }
            // Literals and spread don't need analysis
            Expr::Literal { .. } | Expr::SelfExpr { .. } | Expr::Super { .. } |
            Expr::Break { .. } | Expr::Continue { .. } => {}
            Expr::Spread { expr, .. } => {
                self.analyze_expr(expr);
            }
        }
    }
}
