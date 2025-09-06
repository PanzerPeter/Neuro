//! Shared types used across NEURO compiler slices
//! 
//! This crate provides common types and data structures that are used
//! throughout the NEURO compiler architecture while maintaining VSA principles.

pub mod span;
pub mod literal;
pub mod identifier;
pub mod attributes;
pub mod ast;
pub mod value;

pub use span::*;
pub use literal::*;
pub use identifier::*;
pub use attributes::*;
pub use ast::*;
pub use value::*;

use serde::{Deserialize, Serialize};
use std::fmt;

/// Token types for the lexical analyzer
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TokenType {
    // Literals
    Integer(String),
    Float(String),
    String(String),
    Boolean(bool),
    
    // Identifiers and keywords
    Identifier(String),
    Keyword(Keyword),
    
    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Assign,       // =
    Equal,        // ==
    NotEqual,     // !=
    Less,         // <
    LessEqual,    // <=
    Greater,      // >
    GreaterEqual, // >=
    
    // Punctuation
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Semicolon,
    Comma,
    Dot,
    Colon,        // :
    DoubleColon,  // ::
    Arrow,        // ->
    
    // Special
    Newline,
    EndOfFile,
    Unknown(char),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Keyword {
    // Control flow
    If,
    Else,
    While,
    For,
    Break,
    Continue,
    Return,
    
    // Functions and types
    Fn,
    Let,
    Mut,
    Const,
    Type,
    Struct,
    Enum,
    
    // AI/ML specific
    Tensor,
    Grad,
    Kernel,
    Gpu,
    
    // Modules
    Import,
    Export,
    Module,
    
    // Memory management
    Arc,
    Pool,
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Keyword::If => "if",
            Keyword::Else => "else", 
            Keyword::While => "while",
            Keyword::For => "for",
            Keyword::Break => "break",
            Keyword::Continue => "continue",
            Keyword::Return => "return",
            Keyword::Fn => "fn",
            Keyword::Let => "let",
            Keyword::Mut => "mut",
            Keyword::Const => "const",
            Keyword::Type => "type",
            Keyword::Struct => "struct",
            Keyword::Enum => "enum",
            Keyword::Tensor => "Tensor",
            Keyword::Grad => "grad",
            Keyword::Kernel => "kernel",
            Keyword::Gpu => "gpu",
            Keyword::Import => "import",
            Keyword::Export => "export",
            Keyword::Module => "module",
            Keyword::Arc => "Arc",
            Keyword::Pool => "Pool",
        };
        write!(f, "{}", s)
    }
}

/// A token with location information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub token_type: TokenType,
    pub span: Span,
}

impl Token {
    pub fn new(token_type: TokenType, span: Span) -> Self {
        Self { token_type, span }
    }
}

/// Basic types in the NEURO type system
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Type {
    /// Primitive types
    Int,
    Float,
    Bool,
    String,
    
    /// Tensor type with optional shape information
    Tensor {
        element_type: Box<Type>,
        shape: Option<Vec<usize>>,
    },
    
    /// Function type
    Function {
        params: Vec<Type>,
        return_type: Box<Type>,
    },
    
    /// Generic type parameter
    Generic(String),
    
    /// Unknown/inferred type
    Unknown,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "string"),
            Type::Tensor { element_type, shape } => {
                if let Some(shape) = shape {
                    write!(f, "Tensor<{}, {:?}>", element_type, shape)
                } else {
                    write!(f, "Tensor<{}>", element_type)
                }
            }
            Type::Function { params, return_type } => {
                write!(f, "fn(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", param)?;
                }
                write!(f, ") -> {}", return_type)
            }
            Type::Generic(name) => write!(f, "{}", name),
            Type::Unknown => write!(f, "?"),
        }
    }
}