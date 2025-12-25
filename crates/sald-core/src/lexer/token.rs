

use crate::error::Span;
use std::fmt;


#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    
    Number(f64),
    String(String),
    
    FormatStringStart(String), 
    FormatStringPart(String), 
    FormatStringEnd(String),  
    
    RawString(String),
    True,
    False,
    Null,

    
    Identifier(String),

    
    Let,
    If,
    Else,
    While,
    Do,
    Fun,
    Return,
    Class,
    For,
    In,
    SelfKeyword, 
    Break,
    Continue,
    Extends,
    Super,
    Import,
    As,
    Try,
    Catch,
    Throw,
    Switch,
    Default,
    Async,
    Await,
    Namespace,  
    Const,      
    Enum,       
    Interface,  
    Implements, 

    
    ThinArrow, 
    Plus,      
    Minus,     
    Star,      
    Slash,     
    Percent,   

    
    Equal,        
    EqualEqual,   
    Bang,         
    BangEqual,    
    Less,         
    LessEqual,    
    Greater,      
    GreaterEqual, 

    
    And, 
    Or,  

    
    PlusEqual,    
    MinusEqual,   
    StarEqual,    
    SlashEqual,   
    PercentEqual, 

    
    LeftParen,        
    RightParen,       
    LeftBrace,        
    RightBrace,       
    LeftBracket,      
    RightBracket,     
    Comma,            
    Dot,              
    DotDot,           
    DotDotLess,       
    DotDotDot,        
    Semicolon,        
    Colon,            
    Question,         
    QuestionQuestion, 
    QuestionDot,      
    Pipe,             
    Arrow,            

    
    Ampersand,      
    Caret,          
    Tilde,          
    LessLess,       
    GreaterGreater, 

    
    At, 
    Newline,
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Number(n) => write!(f, "{}", n),
            TokenKind::String(s) => write!(f, "\"{}\"", s),
            TokenKind::FormatStringStart(s) => write!(f, "$\"{{{}", s),
            TokenKind::FormatStringPart(s) => write!(f, "}}{{{}", s),
            TokenKind::FormatStringEnd(s) => write!(f, "}}{}\"", s),
            TokenKind::RawString(s) => write!(f, "r\"{}\"", s),
            TokenKind::True => write!(f, "true"),
            TokenKind::False => write!(f, "false"),
            TokenKind::Null => write!(f, "null"),
            TokenKind::Identifier(s) => write!(f, "{}", s),
            TokenKind::Let => write!(f, "let"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::While => write!(f, "while"),
            TokenKind::Do => write!(f, "do"),
            TokenKind::Fun => write!(f, "fun"),
            TokenKind::Return => write!(f, "return"),
            TokenKind::Class => write!(f, "class"),
            TokenKind::For => write!(f, "for"),
            TokenKind::In => write!(f, "in"),
            TokenKind::SelfKeyword => write!(f, "self"),
            TokenKind::Break => write!(f, "break"),
            TokenKind::Continue => write!(f, "continue"),
            TokenKind::Extends => write!(f, "extends"),
            TokenKind::Super => write!(f, "super"),
            TokenKind::Import => write!(f, "import"),
            TokenKind::As => write!(f, "as"),
            TokenKind::Try => write!(f, "try"),
            TokenKind::Catch => write!(f, "catch"),
            TokenKind::Throw => write!(f, "throw"),
            TokenKind::Switch => write!(f, "switch"),
            TokenKind::Default => write!(f, "default"),
            TokenKind::Async => write!(f, "async"),
            TokenKind::Await => write!(f, "await"),
            TokenKind::Namespace => write!(f, "namespace"),
            TokenKind::Const => write!(f, "const"),
            TokenKind::Enum => write!(f, "enum"),
            TokenKind::Interface => write!(f, "interface"),
            TokenKind::Implements => write!(f, "implements"),
            TokenKind::ThinArrow => write!(f, "->"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::Equal => write!(f, "="),
            TokenKind::EqualEqual => write!(f, "=="),
            TokenKind::Bang => write!(f, "!"),
            TokenKind::BangEqual => write!(f, "!="),
            TokenKind::Less => write!(f, "<"),
            TokenKind::LessEqual => write!(f, "<="),
            TokenKind::Greater => write!(f, ">"),
            TokenKind::GreaterEqual => write!(f, ">="),
            TokenKind::And => write!(f, "&&"),
            TokenKind::Or => write!(f, "||"),
            TokenKind::PlusEqual => write!(f, "+="),
            TokenKind::MinusEqual => write!(f, "-="),
            TokenKind::StarEqual => write!(f, "*="),
            TokenKind::SlashEqual => write!(f, "/="),
            TokenKind::PercentEqual => write!(f, "%="),
            TokenKind::LeftParen => write!(f, "("),
            TokenKind::RightParen => write!(f, ")"),
            TokenKind::LeftBrace => write!(f, "{{"),
            TokenKind::RightBrace => write!(f, "}}"),
            TokenKind::LeftBracket => write!(f, "["),
            TokenKind::RightBracket => write!(f, "]"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Dot => write!(f, "."),
            TokenKind::DotDot => write!(f, ".."),
            TokenKind::DotDotLess => write!(f, "..<"),
            TokenKind::DotDotDot => write!(f, "..."),
            TokenKind::Semicolon => write!(f, ";"),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::QuestionQuestion => write!(f, "??"),
            TokenKind::QuestionDot => write!(f, "?."),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Arrow => write!(f, "=>"),
            TokenKind::Ampersand => write!(f, "&"),
            TokenKind::Caret => write!(f, "^"),
            TokenKind::Tilde => write!(f, "~"),
            TokenKind::LessLess => write!(f, "<<"),
            TokenKind::GreaterGreater => write!(f, ">>"),
            TokenKind::At => write!(f, "@"),
            TokenKind::Newline => write!(f, "\\n"),
            TokenKind::Eof => write!(f, "EOF"),
        }
    }
}


#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, lexeme: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            lexeme: lexeme.into(),
            span,
        }
    }

    pub fn is_eof(&self) -> bool {
        matches!(self.kind, TokenKind::Eof)
    }

    pub fn is_keyword(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::Let
                | TokenKind::If
                | TokenKind::Else
                | TokenKind::While
                | TokenKind::Do
                | TokenKind::Fun
                | TokenKind::Return
                | TokenKind::Class
                | TokenKind::For
                | TokenKind::In
                | TokenKind::SelfKeyword
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Null
        )
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}
