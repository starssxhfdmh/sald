// Sald Statement AST Nodes

use super::expr::Expr;
use crate::error::Span;

/// Function parameter
#[derive(Debug, Clone)]
pub struct FunctionParam {
    pub name: String,
    pub is_variadic: bool, // true for ...args style parameters
    pub default_value: Option<Expr>, // None = required, Some = optional with default
    pub span: Span,
}

/// Function definition (used in class methods and standalone functions)
#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<FunctionParam>,
    pub body: Vec<Stmt>,
    pub is_static: bool,
    pub is_async: bool,
    pub span: Span,
}

/// Class definition
#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,
    pub superclass: Option<String>,
    pub methods: Vec<FunctionDef>,
    pub span: Span,
}

/// Statement nodes
#[derive(Debug, Clone)]
pub enum Stmt {
    /// Variable declaration: let x = 5
    Let {
        name: String,
        initializer: Option<Expr>,
        span: Span,
    },

    /// Expression statement: foo()
    Expression { expr: Expr, span: Span },

    /// Block: { statements }
    Block { statements: Vec<Stmt>, span: Span },

    /// If statement: if cond { } else { }
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
        span: Span,
    },

    /// While loop: while cond { }
    While {
        condition: Expr,
        body: Box<Stmt>,
        span: Span,
    },

    /// Do-while loop: do { } while cond
    DoWhile {
        body: Box<Stmt>,
        condition: Expr,
        span: Span,
    },

    /// Function declaration: fun name(params) { }
    Function { def: FunctionDef },

    /// Return statement: return value
    Return { value: Option<Expr>, span: Span },

    /// Class declaration: class Name { }
    Class { def: ClassDef },

    /// For-in loop: for item in iterable { }
    For {
        variable: String,
        iterable: Expr,
        body: Box<Stmt>,
        span: Span,
    },

    /// Break statement: break
    Break { span: Span },

    /// Continue statement: continue
    Continue { span: Span },

    /// Import statement: import "file.sald" as Alias
    Import {
        path: String,
        alias: Option<String>,
        span: Span,
    },

    /// Try-catch statement: try { } catch (e) { }
    TryCatch {
        try_body: Box<Stmt>,
        catch_var: String,
        catch_body: Box<Stmt>,
        span: Span,
    },

    /// Throw statement: throw value
    Throw { value: Expr, span: Span },

    /// Namespace declaration: namespace Name { ... }
    Namespace {
        name: String,
        body: Vec<Stmt>,
        span: Span,
    },

    /// Const declaration: const NAME = value
    Const {
        name: String,
        value: Expr,
        span: Span,
    },

    /// Enum declaration: enum Name { Variant1, Variant2 }
    Enum {
        name: String,
        variants: Vec<String>,
        span: Span,
    },
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let { span, .. } => *span,
            Stmt::Expression { span, .. } => *span,
            Stmt::Block { span, .. } => *span,
            Stmt::If { span, .. } => *span,
            Stmt::While { span, .. } => *span,
            Stmt::DoWhile { span, .. } => *span,
            Stmt::Function { def } => def.span,
            Stmt::Return { span, .. } => *span,
            Stmt::Class { def } => def.span,
            Stmt::For { span, .. } => *span,
            Stmt::Break { span } => *span,
            Stmt::Continue { span } => *span,
            Stmt::Import { span, .. } => *span,
            Stmt::TryCatch { span, .. } => *span,
            Stmt::Throw { span, .. } => *span,
            Stmt::Namespace { span, .. } => *span,
            Stmt::Const { span, .. } => *span,
            Stmt::Enum { span, .. } => *span,
        }
    }
}

/// Program is a list of statements
#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

impl Program {
    pub fn new(statements: Vec<Stmt>) -> Self {
        Self { statements }
    }
}
