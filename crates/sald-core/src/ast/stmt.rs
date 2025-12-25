use super::expr::Expr;
use crate::error::Span;

#[derive(Debug, Clone)]
pub struct FunctionParam {
    pub name: String,
    pub is_variadic: bool,
    pub default_value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Decorator {
    pub name: String,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<FunctionParam>,
    pub body: Vec<Stmt>,
    pub is_static: bool,
    pub is_async: bool,
    pub decorators: Vec<Decorator>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,
    pub superclass: Option<String>,
    pub implements: Vec<String>,
    pub methods: Vec<FunctionDef>,
    pub decorators: Vec<Decorator>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InterfaceMethodDef {
    pub name: String,
    pub params: Vec<FunctionParam>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InterfaceDef {
    pub name: String,
    pub methods: Vec<InterfaceMethodDef>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ArrayPatternElement {
    Variable { name: String, span: Span },

    Rest { name: String, span: Span },

    Hole,
}

#[derive(Debug, Clone)]
pub struct ArrayPattern {
    pub elements: Vec<ArrayPatternElement>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let {
        name: String,
        name_span: Span,
        initializer: Option<Expr>,
        span: Span,
    },

    LetDestructure {
        pattern: ArrayPattern,
        initializer: Expr,
        span: Span,
    },

    Expression {
        expr: Expr,
        span: Span,
    },

    Block {
        statements: Vec<Stmt>,
        span: Span,
    },

    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
        span: Span,
    },

    While {
        condition: Expr,
        body: Box<Stmt>,
        span: Span,
    },

    DoWhile {
        body: Box<Stmt>,
        condition: Expr,
        span: Span,
    },

    Function {
        def: FunctionDef,
    },

    Return {
        value: Option<Expr>,
        span: Span,
    },

    Class {
        def: ClassDef,
    },

    For {
        variable: String,
        iterable: Expr,
        body: Box<Stmt>,
        span: Span,
    },

    Break {
        span: Span,
    },

    Continue {
        span: Span,
    },

    Import {
        path: String,
        alias: Option<String>,
        span: Span,
    },

    TryCatch {
        try_body: Box<Stmt>,
        catch_var: String,
        catch_body: Box<Stmt>,
        span: Span,
    },

    Throw {
        value: Expr,
        span: Span,
    },

    Namespace {
        name: String,
        body: Vec<Stmt>,
        span: Span,
    },

    Const {
        name: String,
        value: Expr,
        span: Span,
    },

    Enum {
        name: String,
        variants: Vec<String>,
        span: Span,
    },

    Interface {
        def: InterfaceDef,
    },
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Let { span, .. } => *span,
            Stmt::LetDestructure { span, .. } => *span,
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
            Stmt::Interface { def } => def.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

impl Program {
    pub fn new(statements: Vec<Stmt>) -> Self {
        Self { statements }
    }
}
