//! Abstract Syntax Tree definitions for NEURO
//! 
//! This module defines the AST node types that represent the structure
//! of parsed NEURO source code.

use crate::span::Span;
use crate::Type;
use serde::{Deserialize, Serialize};

/// A complete NEURO program
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub items: Vec<Item>,
    pub span: Span,
}

/// Top-level items in a NEURO program
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Item {
    Function(Function),
    Struct(Struct),
    Import(Import),
    // TODO: Add more item types as needed
}

/// Function definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Block,
    pub span: Span,
}

/// Function parameter
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub param_type: Type,
    pub span: Span,
}

/// Struct definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Struct {
    pub name: String,
    pub fields: Vec<StructField>,
    pub span: Span,
}

/// Struct field
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructField {
    pub name: String,
    pub field_type: Type,
    pub span: Span,
}

/// Import statement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Import {
    pub path: String,
    pub span: Span,
}

/// Block of statements
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
}

/// Statements in NEURO
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Expression(Expression),
    Let(LetStatement),
    Assignment(AssignmentStatement),
    Return(ReturnStatement),
    If(IfStatement),
    While(WhileStatement),
    For(ForStatement),
    Break(BreakStatement),
    Continue(ContinueStatement),
    Block(Block),
}

/// Let statement for variable declarations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LetStatement {
    pub name: String,
    pub var_type: Option<Type>,
    pub initializer: Option<Expression>,
    pub mutable: bool,
    pub span: Span,
}

/// Assignment statement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentStatement {
    pub target: String,
    pub value: Expression,
    pub span: Span,
}

/// Return statement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnStatement {
    pub value: Option<Expression>,
    pub span: Span,
}

/// If statement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IfStatement {
    pub condition: Expression,
    pub then_branch: Block,
    pub else_branch: Option<Block>,
    pub span: Span,
}

/// While loop
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhileStatement {
    pub condition: Expression,
    pub body: Block,
    pub span: Span,
}

/// For loop
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForStatement {
    pub variable: String,
    pub iterable: Expression,
    pub body: Block,
    pub span: Span,
}

/// Break statement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BreakStatement {
    pub span: Span,
}

/// Continue statement
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContinueStatement {
    pub span: Span,
}

/// Expressions in NEURO
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    Literal(Literal),
    Identifier(Identifier),
    Binary(BinaryExpression),
    Unary(UnaryExpression),
    Call(CallExpression),
    Index(IndexExpression),
    Member(MemberExpression),
    TensorLiteral(TensorLiteral),
}

/// Literal values
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Integer(i64, Span),
    Float(f64, Span),
    String(String, Span),
    Boolean(bool, Span),
}

/// Identifier
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Identifier {
    pub name: String,
    pub span: Span,
}

/// Binary expression (e.g., a + b)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BinaryExpression {
    pub left: Box<Expression>,
    pub operator: BinaryOperator,
    pub right: Box<Expression>,
    pub span: Span,
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

/// Unary expression (e.g., -x, !x)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnaryExpression {
    pub operator: UnaryOperator,
    pub operand: Box<Expression>,
    pub span: Span,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Negate,
    Not,
}

/// Function call expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallExpression {
    pub function: Box<Expression>,
    pub arguments: Vec<Expression>,
    pub span: Span,
}

/// Index expression (e.g., array[index])
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexExpression {
    pub object: Box<Expression>,
    pub index: Box<Expression>,
    pub span: Span,
}

/// Member access expression (e.g., obj.field)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberExpression {
    pub object: Box<Expression>,
    pub member: String,
    pub span: Span,
}

/// Tensor literal expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TensorLiteral {
    pub elements: Vec<Expression>,
    pub dimensions: Option<Vec<usize>>,
    pub span: Span,
}

impl Expression {
    /// Get the span of this expression
    pub fn span(&self) -> Span {
        match self {
            Expression::Literal(lit) => match lit {
                Literal::Integer(_, span) => *span,
                Literal::Float(_, span) => *span,
                Literal::String(_, span) => *span,
                Literal::Boolean(_, span) => *span,
            },
            Expression::Identifier(id) => id.span,
            Expression::Binary(bin) => bin.span,
            Expression::Unary(un) => un.span,
            Expression::Call(call) => call.span,
            Expression::Index(idx) => idx.span,
            Expression::Member(mem) => mem.span,
            Expression::TensorLiteral(tensor) => tensor.span,
        }
    }
}

impl Statement {
    /// Get the span of this statement
    pub fn span(&self) -> Span {
        match self {
            Statement::Expression(expr) => expr.span(),
            Statement::Let(let_stmt) => let_stmt.span,
            Statement::Assignment(assign_stmt) => assign_stmt.span,
            Statement::Return(ret) => ret.span,
            Statement::If(if_stmt) => if_stmt.span,
            Statement::While(while_stmt) => while_stmt.span,
            Statement::For(for_stmt) => for_stmt.span,
            Statement::Break(break_stmt) => break_stmt.span,
            Statement::Continue(continue_stmt) => continue_stmt.span,
            Statement::Block(block) => block.span,
        }
    }
}