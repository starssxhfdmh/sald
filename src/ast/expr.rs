// Sald Expression AST Nodes

use crate::error::Span;
use crate::lexer::TokenKind;

/// Binary operators
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    // Comparison
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    // Logical
    And,
    Or,
    // Null coalescing
    NullCoalesce,
    // Bitwise
    BitAnd,     // &
    BitOr,      // |
    BitXor,     // ^
    LeftShift,  // <<
    RightShift, // >>
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
            // Bitwise
            TokenKind::Ampersand => Some(BinaryOp::BitAnd),
            TokenKind::Pipe => Some(BinaryOp::BitOr),
            TokenKind::Caret => Some(BinaryOp::BitXor),
            TokenKind::LessLess => Some(BinaryOp::LeftShift),
            TokenKind::GreaterGreater => Some(BinaryOp::RightShift),
            _ => None,
        }
    }
}

/// Unary operators
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Negate, // -
    Not,    // !
    BitNot, // ~ (bitwise NOT)
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

/// Assignment operators (for compound assignment)
#[derive(Debug, Clone, PartialEq)]
pub enum AssignOp {
    Assign,    // =
    AddAssign, // +=
    SubAssign, // -=
    MulAssign, // *=
    DivAssign, // /=
    ModAssign, // %=
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

/// Literal values
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
}

/// Function call argument (positional or named)
#[derive(Debug, Clone)]
pub struct CallArg {
    /// Parameter name (Some for named args like `name: value`, None for positional)
    pub name: Option<String>,
    /// The argument value expression
    pub value: Expr,
    pub span: Span,
}

/// Expression nodes
#[derive(Debug, Clone)]
pub enum Expr {
    /// Literal value: 42, "hello", true, null
    Literal { value: Literal, span: Span },

    /// Variable reference: x, myVar
    Identifier { name: String, span: Span },

    /// Binary operation: a + b, x == y
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },

    /// Unary operation: -x, !done
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },

    /// Grouping: (expr)
    Grouping { expr: Box<Expr>, span: Span },

    /// Assignment: x = 5, y += 1
    Assignment {
        target: Box<Expr>,
        op: AssignOp,
        value: Box<Expr>,
        span: Span,
    },

    /// Function call: foo(a, b) or foo(name: value)
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
        is_optional: bool,  // true for ?.() optional call
        span: Span,
    },

    /// Property access: obj.property
    Get {
        object: Box<Expr>,
        property: String,
        is_optional: bool,  // true for ?. optional access
        span: Span,
    },

    /// Property assignment: obj.property = value
    Set {
        object: Box<Expr>,
        property: String,
        value: Box<Expr>,
        span: Span,
    },

    /// Self reference: self
    SelfExpr { span: Span },

    /// Array literal: [1, 2, 3]
    Array { elements: Vec<Expr>, span: Span },

    /// Index access: arr[0], "hello"[1]
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        is_optional: bool,  // true for ?[] optional access
        span: Span,
    },

    /// Index assignment: arr[0] = value
    IndexSet {
        object: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },

    /// Ternary operator: condition ? then_expr : else_expr
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
        span: Span,
    },

    /// Lambda/anonymous function: |params| body or async |params| body
    Lambda {
        params: Vec<super::FunctionParam>,
        body: LambdaBody,
        is_async: bool,
        span: Span,
    },

    /// Super method call: super.method()
    Super { method: String, span: Span },

    /// Switch expression: switch value { patterns -> expr, default -> expr }
    Switch {
        value: Box<Expr>,
        arms: Vec<SwitchArm>,
        default: Option<Box<Expr>>,
        span: Span,
    },

    /// Block expression: { statements; last_expr }
    /// The value of the block is the value of the last expression
    Block {
        statements: Vec<super::Stmt>,
        /// The final expression that provides the block's value (optional)
        expr: Option<Box<Expr>>,
        span: Span,
    },

    /// Dictionary literal: {"key": value, "key2": value2}
    Dictionary {
        entries: Vec<(Expr, Expr)>,
        span: Span,
    },

    /// Await expression: await promise
    Await { expr: Box<Expr>, span: Span },

    /// Return expression: return value (for use in switch arms, etc.)
    Return {
        value: Option<Box<Expr>>,
        span: Span,
    },

    /// Throw expression: throw value (for use in switch arms, etc.)
    Throw {
        value: Box<Expr>,
        span: Span,
    },

    /// Break expression (for use in switch arms, etc.)
    Break { span: Span },

    /// Continue expression (for use in switch arms, etc.)
    Continue { span: Span },

    /// Spread expression: ...array (used in function calls)
    Spread { expr: Box<Expr>, span: Span },
}

/// Pattern for switch expression matching
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Literal value pattern: 1, "hello", true
    Literal { value: Literal, span: Span },
    
    /// Variable binding with optional guard: n, n if n > 0
    Binding {
        name: String,
        guard: Option<Box<Expr>>,
        span: Span,
    },
    
    /// Array destructuring pattern: [], [a], [a, b], [head, ...tail]
    Array {
        elements: Vec<SwitchArrayElement>,
        span: Span,
    },
    
    /// Dictionary destructuring pattern: {"key": binding, "key2": binding2}
    Dict {
        entries: Vec<(String, Pattern)>,
        span: Span,
    },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Literal { span, .. } => *span,
            Pattern::Binding { span, .. } => *span,
            Pattern::Array { span, .. } => *span,
            Pattern::Dict { span, .. } => *span,
        }
    }
}

/// Element in a switch array pattern (different from ArrayPatternElement in stmt)
#[derive(Debug, Clone)]
pub enum SwitchArrayElement {
    /// Single pattern element
    Single(Pattern),
    /// Rest pattern: ...name
    Rest { name: String, span: Span },
}

/// A single arm of a switch expression
#[derive(Debug, Clone)]
pub struct SwitchArm {
    /// Patterns to match (can be multiple: 1, 2, 3 -> expr)
    pub patterns: Vec<Pattern>,
    /// Body expression
    pub body: Expr,
    pub span: Span,
}

/// Lambda body can be a block or a single expression
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
        }
    }

    /// Check if this expression is a valid assignment target
    pub fn is_lvalue(&self) -> bool {
        matches!(
            self,
            Expr::Identifier { .. } | Expr::Get { .. } | Expr::Index { .. }
        )
    }
}
