

use crate::error::Span;
use crate::lexer::TokenKind;


#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    
    And,
    Or,
    
    NullCoalesce,
    
    BitAnd,     
    BitOr,      
    BitXor,     
    LeftShift,  
    RightShift, 
}

impl BinaryOp {
    pub fn from_token(kind: &TokenKind) -> Option<Self> {
        match kind {
            TokenKind::Plus => Some(BinaryOp::Add),
            TokenKind::Minus => Some(BinaryOp::Sub),
            TokenKind::Star => Some(BinaryOp::Mul),
            TokenKind::Slash => Some(BinaryOp::Div),
            TokenKind::Percent => Some(BinaryOp::Mod),
            TokenKind::EqualEqual => Some(BinaryOp::Equal),
            TokenKind::BangEqual => Some(BinaryOp::NotEqual),
            TokenKind::Less => Some(BinaryOp::Less),
            TokenKind::LessEqual => Some(BinaryOp::LessEqual),
            TokenKind::Greater => Some(BinaryOp::Greater),
            TokenKind::GreaterEqual => Some(BinaryOp::GreaterEqual),
            TokenKind::And => Some(BinaryOp::And),
            TokenKind::Or => Some(BinaryOp::Or),
            TokenKind::QuestionQuestion => Some(BinaryOp::NullCoalesce),
            
            TokenKind::Ampersand => Some(BinaryOp::BitAnd),
            TokenKind::Pipe => Some(BinaryOp::BitOr),
            TokenKind::Caret => Some(BinaryOp::BitXor),
            TokenKind::LessLess => Some(BinaryOp::LeftShift),
            TokenKind::GreaterGreater => Some(BinaryOp::RightShift),
            _ => None,
        }
    }
}


#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Negate, 
    Not,    
    BitNot, 
}

impl UnaryOp {
    pub fn from_token(kind: &TokenKind) -> Option<Self> {
        match kind {
            TokenKind::Minus => Some(UnaryOp::Negate),
            TokenKind::Bang => Some(UnaryOp::Not),
            TokenKind::Tilde => Some(UnaryOp::BitNot),
            _ => None,
        }
    }
}


#[derive(Debug, Clone, PartialEq)]
pub enum AssignOp {
    Assign,    
    AddAssign, 
    SubAssign, 
    MulAssign, 
    DivAssign, 
    ModAssign, 
}

impl AssignOp {
    pub fn from_token(kind: &TokenKind) -> Option<Self> {
        match kind {
            TokenKind::Equal => Some(AssignOp::Assign),
            TokenKind::PlusEqual => Some(AssignOp::AddAssign),
            TokenKind::MinusEqual => Some(AssignOp::SubAssign),
            TokenKind::StarEqual => Some(AssignOp::MulAssign),
            TokenKind::SlashEqual => Some(AssignOp::DivAssign),
            TokenKind::PercentEqual => Some(AssignOp::ModAssign),
            _ => None,
        }
    }

    pub fn is_compound(&self) -> bool {
        !matches!(self, AssignOp::Assign)
    }
}


#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
}


#[derive(Debug, Clone)]
pub struct CallArg {
    
    pub name: Option<String>,
    
    pub value: Expr,
    pub span: Span,
}


#[derive(Debug, Clone)]
pub enum Expr {
    
    Literal { value: Literal, span: Span },

    
    Identifier { name: String, span: Span },

    
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },

    
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },

    
    Grouping { expr: Box<Expr>, span: Span },

    
    Assignment {
        target: Box<Expr>,
        op: AssignOp,
        value: Box<Expr>,
        span: Span,
    },

    
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
        is_optional: bool, 
        span: Span,
    },

    
    Get {
        object: Box<Expr>,
        property: String,
        is_optional: bool, 
        span: Span,
    },

    
    Set {
        object: Box<Expr>,
        property: String,
        value: Box<Expr>,
        span: Span,
    },

    
    SelfExpr { span: Span },

    
    Array { elements: Vec<Expr>, span: Span },

    
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        is_optional: bool, 
        span: Span,
    },

    
    IndexSet {
        object: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },

    
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
        span: Span,
    },

    
    Lambda {
        params: Vec<super::FunctionParam>,
        body: LambdaBody,
        is_async: bool,
        span: Span,
    },

    
    Super { method: String, span: Span },

    
    Switch {
        value: Box<Expr>,
        arms: Vec<SwitchArm>,
        default: Option<Box<Expr>>,
        span: Span,
    },

    
    
    Block {
        statements: Vec<super::Stmt>,
        
        expr: Option<Box<Expr>>,
        span: Span,
    },

    
    Dictionary {
        entries: Vec<(Expr, Expr)>,
        span: Span,
    },

    
    Await { expr: Box<Expr>, span: Span },

    
    Return {
        value: Option<Box<Expr>>,
        span: Span,
    },

    
    Throw { value: Box<Expr>, span: Span },

    
    Break { span: Span },

    
    Continue { span: Span },

    
    Spread { expr: Box<Expr>, span: Span },

    
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool, 
        span: Span,
    },
}


#[derive(Debug, Clone)]
pub enum Pattern {
    
    Literal { value: Literal, span: Span },

    
    Binding {
        name: String,
        guard: Option<Box<Expr>>,
        span: Span,
    },

    
    Array {
        elements: Vec<SwitchArrayElement>,
        span: Span,
    },

    
    Dict {
        entries: Vec<(String, Pattern)>,
        span: Span,
    },

    
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
        span: Span,
    },

    
    
    Expression { expr: Box<Expr>, span: Span },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Literal { span, .. } => *span,
            Pattern::Binding { span, .. } => *span,
            Pattern::Array { span, .. } => *span,
            Pattern::Dict { span, .. } => *span,
            Pattern::Range { span, .. } => *span,
            Pattern::Expression { span, .. } => *span,
        }
    }
}


#[derive(Debug, Clone)]
pub enum SwitchArrayElement {
    
    Single(Pattern),
    
    Rest { name: String, span: Span },
}


#[derive(Debug, Clone)]
pub struct SwitchArm {
    
    pub patterns: Vec<Pattern>,
    
    pub body: Expr,
    pub span: Span,
}


#[derive(Debug, Clone)]
pub enum LambdaBody {
    Block(Vec<super::Stmt>),
    Expr(Box<Expr>),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Literal { span, .. } => *span,
            Expr::Identifier { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Unary { span, .. } => *span,
            Expr::Grouping { span, .. } => *span,
            Expr::Assignment { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::Get { span, .. } => *span,
            Expr::Set { span, .. } => *span,
            Expr::SelfExpr { span } => *span,
            Expr::Array { span, .. } => *span,
            Expr::Index { span, .. } => *span,
            Expr::IndexSet { span, .. } => *span,
            Expr::Ternary { span, .. } => *span,
            Expr::Lambda { span, .. } => *span,
            Expr::Super { span, .. } => *span,
            Expr::Switch { span, .. } => *span,
            Expr::Block { span, .. } => *span,
            Expr::Dictionary { span, .. } => *span,
            Expr::Await { span, .. } => *span,
            Expr::Return { span, .. } => *span,
            Expr::Throw { span, .. } => *span,
            Expr::Break { span } => *span,
            Expr::Continue { span } => *span,
            Expr::Spread { span, .. } => *span,
            Expr::Range { span, .. } => *span,
        }
    }

    
    pub fn is_lvalue(&self) -> bool {
        matches!(
            self,
            Expr::Identifier { .. } | Expr::Get { .. } | Expr::Index { .. }
        )
    }
}
