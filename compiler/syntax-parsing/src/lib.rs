// NEURO Programming Language - Syntax Parsing
// Feature slice for AST generation and syntax analysis

use lexical_analysis::{tokenize, LexError, Token, TokenKind};
use shared_types::{Identifier, Literal, Span};
use thiserror::Error;

/// Abstract Syntax Tree node for expressions
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal, Span),
    Identifier(Identifier),
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    Paren(Box<Expr>, Span),
}

impl Expr {
    /// Get the span of this expression
    pub fn span(&self) -> Span {
        match self {
            Expr::Literal(_, span) => *span,
            Expr::Identifier(ident) => ident.span,
            Expr::Binary { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::Unary { span, .. } => *span,
            Expr::Paren(_, span) => *span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
}

/// Statement AST nodes
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VarDecl {
        name: Identifier,
        ty: Option<Type>,
        init: Option<Expr>,
        mutable: bool,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    If {
        condition: Expr,
        then_block: Vec<Stmt>,
        else_if_blocks: Vec<(Expr, Vec<Stmt>)>,
        else_block: Option<Vec<Stmt>>,
        span: Span,
    },
    Expr(Expr),
}

/// Type AST nodes
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Named(Identifier),
    Tensor {
        element_type: Box<Type>,
        shape: Vec<usize>,
        span: Span,
    },
}

/// Function definition
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    pub name: Identifier,
    pub params: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: Identifier,
    pub ty: Type,
    pub span: Span,
}

/// Top-level AST
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(FunctionDef),
}

/// Parse errors
#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("unexpected token {found:?}, expected {expected}")]
    UnexpectedToken {
        found: TokenKind,
        expected: String,
        span: Span,
    },

    #[error("unexpected end of file, expected {expected}")]
    UnexpectedEof { expected: String },

    #[error("lexical error: {0}")]
    LexError(#[from] LexError),
}

pub type ParseResult<T> = Result<T, ParseError>;

/// Operator precedence for Pratt parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Lowest,
    LogicalOr,  // ||
    LogicalAnd, // &&
    Equality,   // == !=
    Comparison, // < > <= >=
    Sum,        // + -
    Product,    // * / %
    Unary,      // - !
    Call,       // function calls
}

/// Parser for NEURO source code
pub(crate) struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    /// Create a new parser from a token stream
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    /// Get the current token without consuming it
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.current)
    }

    /// Get the current token kind
    fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|t| &t.kind)
    }

    /// Check if we're at the end of the token stream
    fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Eof) | None)
    }

    /// Consume and return the current token
    fn advance(&mut self) -> Option<Token> {
        if !self.is_at_end() {
            let token = self.tokens.get(self.current).cloned();
            self.current += 1;
            token
        } else {
            None
        }
    }

    /// Check if the current token matches the given kind
    fn check(&self, kind: &TokenKind) -> bool {
        if let Some(current_kind) = self.peek_kind() {
            std::mem::discriminant(current_kind) == std::mem::discriminant(kind)
        } else {
            false
        }
    }

    /// Consume the current token if it matches the expected kind
    fn consume(&mut self, expected: TokenKind, message: &str) -> ParseResult<Token> {
        if self.check(&expected) {
            self.advance().ok_or_else(|| ParseError::UnexpectedEof {
                expected: message.to_string(),
            })
        } else if let Some(token) = self.peek() {
            Err(ParseError::UnexpectedToken {
                found: token.kind.clone(),
                expected: message.to_string(),
                span: token.span,
            })
        } else {
            Err(ParseError::UnexpectedEof {
                expected: message.to_string(),
            })
        }
    }

    /// Skip newline tokens (whitespace handling)
    fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), Some(TokenKind::Newline)) {
            self.advance();
        }
    }

    /// Parse an expression with the given precedence
    pub fn parse_expr(&mut self, precedence: Precedence) -> ParseResult<Expr> {
        self.skip_newlines();

        // Parse prefix expression
        let mut left = self.parse_prefix()?;

        // Parse infix expressions
        while !self.is_at_end() {
            self.skip_newlines();

            if let Some(token) = self.peek() {
                let token_precedence = self.get_precedence(&token.kind);
                if precedence >= token_precedence {
                    break;
                }

                left = self.parse_infix(left)?;
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse a prefix expression (literals, identifiers, unary operators, parentheses)
    fn parse_prefix(&mut self) -> ParseResult<Expr> {
        let token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "expression".to_string(),
        })?;

        match token.kind {
            // Literals
            TokenKind::Integer(n) => Ok(Expr::Literal(Literal::Integer(n), token.span)),
            TokenKind::Float(f) => Ok(Expr::Literal(Literal::Float(f), token.span)),
            TokenKind::String(s) => Ok(Expr::Literal(Literal::String(s), token.span)),
            TokenKind::True => Ok(Expr::Literal(Literal::Boolean(true), token.span)),
            TokenKind::False => Ok(Expr::Literal(Literal::Boolean(false), token.span)),

            // Identifiers
            TokenKind::Identifier(name) => Ok(Expr::Identifier(Identifier {
                name,
                span: token.span,
            })),

            // Unary operators
            TokenKind::Minus => {
                let operand = self.parse_expr(Precedence::Unary)?;
                let span = token.span.merge(operand.span());
                Ok(Expr::Unary {
                    op: UnaryOp::Negate,
                    operand: Box::new(operand),
                    span,
                })
            }
            TokenKind::Bang => {
                let operand = self.parse_expr(Precedence::Unary)?;
                let span = token.span.merge(operand.span());
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                    span,
                })
            }

            // Parenthesized expressions
            TokenKind::LeftParen => {
                let expr = self.parse_expr(Precedence::Lowest)?;
                let close = self.consume(TokenKind::RightParen, "')'")?;
                let span = token.span.merge(close.span);
                Ok(Expr::Paren(Box::new(expr), span))
            }

            _ => Err(ParseError::UnexpectedToken {
                found: token.kind,
                expected: "expression".to_string(),
                span: token.span,
            }),
        }
    }

    /// Parse an infix expression (binary operators, function calls)
    fn parse_infix(&mut self, left: Expr) -> ParseResult<Expr> {
        let token = self.peek().ok_or(ParseError::UnexpectedEof {
            expected: "operator or '('".to_string(),
        })?;

        match &token.kind {
            // Function calls
            TokenKind::LeftParen => {
                self.advance(); // consume '('
                let mut args = Vec::new();

                self.skip_newlines();
                if !self.check(&TokenKind::RightParen) {
                    loop {
                        args.push(self.parse_expr(Precedence::Lowest)?);
                        self.skip_newlines();

                        if !self.check(&TokenKind::Comma) {
                            break;
                        }
                        self.advance(); // consume ','
                        self.skip_newlines();
                    }
                }

                let close = self.consume(TokenKind::RightParen, "')'")?;
                let span = left.span().merge(close.span);

                Ok(Expr::Call {
                    func: Box::new(left),
                    args,
                    span,
                })
            }

            // Binary operators
            kind if self.is_binary_op(kind) => {
                let op_token = self.advance().ok_or(ParseError::UnexpectedEof {
                    expected: "operator".to_string(),
                })?;
                let op = self.token_to_binary_op(&op_token.kind)?;
                let precedence = self.get_precedence(&op_token.kind);
                let right = self.parse_expr(precedence)?;
                let span = left.span().merge(right.span());

                Ok(Expr::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                    span,
                })
            }

            _ => Err(ParseError::UnexpectedToken {
                found: token.kind.clone(),
                expected: "operator or '('".to_string(),
                span: token.span,
            }),
        }
    }

    /// Check if a token kind is a binary operator
    fn is_binary_op(&self, kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
                | TokenKind::EqualEqual
                | TokenKind::NotEqual
                | TokenKind::Less
                | TokenKind::Greater
                | TokenKind::LessEqual
                | TokenKind::GreaterEqual
                | TokenKind::AmpAmp
                | TokenKind::PipePipe
        )
    }

    /// Convert a token kind to a binary operator
    fn token_to_binary_op(&self, kind: &TokenKind) -> ParseResult<BinaryOp> {
        match kind {
            TokenKind::Plus => Ok(BinaryOp::Add),
            TokenKind::Minus => Ok(BinaryOp::Subtract),
            TokenKind::Star => Ok(BinaryOp::Multiply),
            TokenKind::Slash => Ok(BinaryOp::Divide),
            TokenKind::Percent => Ok(BinaryOp::Modulo),
            TokenKind::EqualEqual => Ok(BinaryOp::Equal),
            TokenKind::NotEqual => Ok(BinaryOp::NotEqual),
            TokenKind::Less => Ok(BinaryOp::Less),
            TokenKind::Greater => Ok(BinaryOp::Greater),
            TokenKind::LessEqual => Ok(BinaryOp::LessEqual),
            TokenKind::GreaterEqual => Ok(BinaryOp::GreaterEqual),
            TokenKind::AmpAmp => Ok(BinaryOp::And),
            TokenKind::PipePipe => Ok(BinaryOp::Or),
            _ => Err(ParseError::UnexpectedToken {
                found: kind.clone(),
                expected: "binary operator".to_string(),
                span: Span::new(0, 0),
            }),
        }
    }

    /// Get the precedence of an operator token
    fn get_precedence(&self, kind: &TokenKind) -> Precedence {
        match kind {
            TokenKind::PipePipe => Precedence::LogicalOr,
            TokenKind::AmpAmp => Precedence::LogicalAnd,
            TokenKind::EqualEqual | TokenKind::NotEqual => Precedence::Equality,
            TokenKind::Less
            | TokenKind::Greater
            | TokenKind::LessEqual
            | TokenKind::GreaterEqual => Precedence::Comparison,
            TokenKind::Plus | TokenKind::Minus => Precedence::Sum,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Precedence::Product,
            TokenKind::LeftParen => Precedence::Call,
            _ => Precedence::Lowest,
        }
    }

    /// Parse a type annotation
    pub(crate) fn parse_type(&mut self) -> ParseResult<Type> {
        let token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "type".to_string(),
        })?;

        match token.kind {
            TokenKind::Identifier(name) => {
                let span = token.span;
                Ok(Type::Named(Identifier { name, span }))
            }
            _ => Err(ParseError::UnexpectedToken {
                found: token.kind,
                expected: "type name".to_string(),
                span: token.span,
            }),
        }
    }

    /// Parse a variable declaration statement (val/mut)
    pub(crate) fn parse_var_decl(&mut self, mutable: bool, start_span: Span) -> ParseResult<Stmt> {
        // Expect identifier
        let name_token = self.consume(TokenKind::Identifier(String::new()), "variable name")?;

        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "identifier".to_string(),
                span: name_token.span,
            });
        };

        // Optional type annotation
        self.skip_newlines();
        let ty = if self.check(&TokenKind::Colon) {
            self.advance(); // consume ':'
            self.skip_newlines();
            Some(self.parse_type()?)
        } else {
            None
        };

        // Optional initialization
        self.skip_newlines();
        let init = if self.check(&TokenKind::Equal) {
            self.advance(); // consume '='
            self.skip_newlines();
            Some(self.parse_expr(Precedence::Lowest)?)
        } else {
            None
        };

        let span = start_span.merge(
            init.as_ref()
                .map(|e| e.span())
                .or_else(|| {
                    ty.as_ref().map(|t| match t {
                        Type::Named(ident) => ident.span,
                        Type::Tensor { span, .. } => *span,
                    })
                })
                .unwrap_or(name.span),
        );

        Ok(Stmt::VarDecl {
            name,
            ty,
            init,
            mutable,
            span,
        })
    }

    /// Parse a return statement
    pub(crate) fn parse_return_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        self.skip_newlines();

        // Check if there's a return value
        let value = if self.is_at_end()
            || matches!(
                self.peek_kind(),
                Some(TokenKind::Newline) | Some(TokenKind::RightBrace)
            ) {
            None
        } else {
            Some(self.parse_expr(Precedence::Lowest)?)
        };

        let span = value
            .as_ref()
            .map(|e| start_span.merge(e.span()))
            .unwrap_or(start_span);

        Ok(Stmt::Return { value, span })
    }

    /// Parse an if/else statement
    pub(crate) fn parse_if_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        self.skip_newlines();

        // Parse condition expression
        let condition = self.parse_expr(Precedence::Lowest)?;
        self.skip_newlines();

        // Parse then block
        let then_block = self.parse_block()?;
        self.skip_newlines();

        // Parse optional else-if and else clauses
        let mut else_if_blocks = Vec::new();
        let mut else_block = None;

        while self.check(&TokenKind::Else) {
            self.advance(); // consume 'else'
            self.skip_newlines();

            // Check if this is an else-if
            if self.check(&TokenKind::If) {
                self.advance(); // consume 'if'
                self.skip_newlines();

                let else_if_condition = self.parse_expr(Precedence::Lowest)?;
                self.skip_newlines();

                let else_if_block = self.parse_block()?;
                else_if_blocks.push((else_if_condition, else_if_block));
                self.skip_newlines();
            } else {
                // This is a final else block
                else_block = Some(self.parse_block()?);
                break; // No more clauses after else
            }
        }

        // Calculate final span
        let end_span = else_block
            .as_ref()
            .and_then(|stmts| stmts.last())
            .or_else(|| else_if_blocks.last().and_then(|(_, stmts)| stmts.last()))
            .or_else(|| then_block.last())
            .map(|stmt| match stmt {
                Stmt::VarDecl { span, .. } => *span,
                Stmt::Return { span, .. } => *span,
                Stmt::If { span, .. } => *span,
                Stmt::Expr(e) => e.span(),
            })
            .unwrap_or(start_span);

        Ok(Stmt::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
            span: start_span.merge(end_span),
        })
    }

    /// Parse a single statement
    pub(crate) fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        self.skip_newlines();

        let token = self.peek().ok_or(ParseError::UnexpectedEof {
            expected: "statement".to_string(),
        })?;

        match &token.kind {
            TokenKind::Val => {
                let start_span = token.span;
                self.advance(); // consume 'val'
                self.skip_newlines();
                self.parse_var_decl(false, start_span)
            }
            TokenKind::Mut => {
                let start_span = token.span;
                self.advance(); // consume 'mut'
                self.skip_newlines();
                self.parse_var_decl(true, start_span)
            }
            TokenKind::Return => {
                let start_span = token.span;
                self.advance(); // consume 'return'
                self.parse_return_stmt(start_span)
            }
            TokenKind::If => {
                let start_span = token.span;
                self.advance(); // consume 'if'
                self.parse_if_stmt(start_span)
            }
            _ => {
                // Expression statement
                let expr = self.parse_expr(Precedence::Lowest)?;
                Ok(Stmt::Expr(expr))
            }
        }
    }

    /// Parse a block of statements (within braces)
    pub(crate) fn parse_block(&mut self) -> ParseResult<Vec<Stmt>> {
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut statements = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            statements.push(self.parse_stmt()?);
            self.skip_newlines();
        }

        self.consume(TokenKind::RightBrace, "'}'")?;

        Ok(statements)
    }

    /// Parse a function definition
    pub(crate) fn parse_function(&mut self) -> ParseResult<FunctionDef> {
        let start = self.consume(TokenKind::Func, "'func'")?;
        self.skip_newlines();

        // Function name
        let name_token = self.consume(TokenKind::Identifier(String::new()), "function name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "function name".to_string(),
                span: name_token.span,
            });
        };

        // Parameters
        self.consume(TokenKind::LeftParen, "'('")?;
        self.skip_newlines();

        let mut params = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                let param_start = self
                    .peek()
                    .ok_or(ParseError::UnexpectedEof {
                        expected: "parameter".to_string(),
                    })?
                    .span;

                let param_name_token =
                    self.consume(TokenKind::Identifier(String::new()), "parameter name")?;
                let param_name = if let TokenKind::Identifier(n) = param_name_token.kind {
                    Identifier {
                        name: n,
                        span: param_name_token.span,
                    }
                } else {
                    return Err(ParseError::UnexpectedToken {
                        found: param_name_token.kind,
                        expected: "parameter name".to_string(),
                        span: param_name_token.span,
                    });
                };

                self.skip_newlines();
                self.consume(TokenKind::Colon, "':'")?;
                self.skip_newlines();

                let param_ty = self.parse_type()?;
                let param_span = param_start.merge(match &param_ty {
                    Type::Named(ident) => ident.span,
                    Type::Tensor { span, .. } => *span,
                });

                params.push(Parameter {
                    name: param_name,
                    ty: param_ty,
                    span: param_span,
                });

                self.skip_newlines();
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance(); // consume ','
                self.skip_newlines();
            }
        }

        self.consume(TokenKind::RightParen, "')'")?;
        self.skip_newlines();

        // Return type (optional)
        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance(); // consume '->'
            self.skip_newlines();
            Some(self.parse_type()?)
        } else {
            None
        };

        self.skip_newlines();

        // Function body
        let body = self.parse_block()?;

        let end_span = body
            .last()
            .map(|s| match s {
                Stmt::VarDecl { span, .. } => *span,
                Stmt::Return { span, .. } => *span,
                Stmt::If { span, .. } => *span,
                Stmt::Expr(e) => e.span(),
            })
            .unwrap_or(start.span);

        Ok(FunctionDef {
            name,
            params,
            return_type,
            body,
            span: start.span.merge(end_span),
        })
    }
}

/// Parse NEURO source code into AST
pub fn parse(source: &str) -> ParseResult<Vec<Item>> {
    // Tokenize the source code
    let tokens = tokenize(source)?;

    // Create parser
    let mut parser = Parser::new(tokens);

    // Parse top-level items (functions for Phase 1)
    let mut items = Vec::new();

    parser.skip_newlines();
    while !parser.is_at_end() {
        // For Phase 1, we only support function definitions
        if parser.check(&TokenKind::Func) {
            let func = parser.parse_function()?;
            items.push(Item::Function(func));
        } else {
            let token = parser.peek().ok_or(ParseError::UnexpectedEof {
                expected: "function definition".to_string(),
            })?;
            return Err(ParseError::UnexpectedToken {
                found: token.kind.clone(),
                expected: "function definition".to_string(),
                span: token.span,
            });
        }
        parser.skip_newlines();
    }

    Ok(items)
}

/// Parse a NEURO expression from source code
pub fn parse_expr(source: &str) -> ParseResult<Expr> {
    let tokens = tokenize(source)?;
    let mut parser = Parser::new(tokens);
    parser.parse_expr(Precedence::Lowest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_source() {
        let result = parse("").unwrap();
        assert_eq!(result.len(), 0);
    }

    // Literal parsing tests
    #[test]
    fn parse_integer_literal() {
        let result = parse_expr("42").unwrap();
        assert!(matches!(result, Expr::Literal(Literal::Integer(42), _)));
    }

    #[test]
    fn parse_float_literal() {
        let result = parse_expr("2.5").unwrap();
        match result {
            Expr::Literal(Literal::Float(f), _) => assert!((f - 2.5).abs() < 1e-10),
            _ => panic!("Expected float literal"),
        }
    }

    #[test]
    fn parse_string_literal() {
        let result = parse_expr(r#""hello""#).unwrap();
        match result {
            Expr::Literal(Literal::String(s), _) => assert_eq!(s, "hello"),
            _ => panic!("Expected string literal"),
        }
    }

    #[test]
    fn parse_boolean_literals() {
        let true_expr = parse_expr("true").unwrap();
        assert!(matches!(
            true_expr,
            Expr::Literal(Literal::Boolean(true), _)
        ));

        let false_expr = parse_expr("false").unwrap();
        assert!(matches!(
            false_expr,
            Expr::Literal(Literal::Boolean(false), _)
        ));
    }

    #[test]
    fn parse_identifier() {
        let result = parse_expr("foo").unwrap();
        match result {
            Expr::Identifier(ident) => assert_eq!(ident.name, "foo"),
            _ => panic!("Expected identifier"),
        }
    }

    // Binary operator tests
    #[test]
    fn parse_binary_addition() {
        let result = parse_expr("1 + 2").unwrap();
        match result {
            Expr::Binary {
                left,
                op: BinaryOp::Add,
                right,
                ..
            } => {
                assert!(matches!(*left, Expr::Literal(Literal::Integer(1), _)));
                assert!(matches!(*right, Expr::Literal(Literal::Integer(2), _)));
            }
            _ => panic!("Expected binary addition"),
        }
    }

    #[test]
    fn parse_binary_subtraction() {
        let result = parse_expr("10 - 5").unwrap();
        match result {
            Expr::Binary {
                op: BinaryOp::Subtract,
                ..
            } => {}
            _ => panic!("Expected binary subtraction"),
        }
    }

    #[test]
    fn parse_binary_multiplication() {
        let result = parse_expr("3 * 4").unwrap();
        match result {
            Expr::Binary {
                op: BinaryOp::Multiply,
                ..
            } => {}
            _ => panic!("Expected binary multiplication"),
        }
    }

    #[test]
    fn parse_binary_division() {
        let result = parse_expr("20 / 4").unwrap();
        match result {
            Expr::Binary {
                op: BinaryOp::Divide,
                ..
            } => {}
            _ => panic!("Expected binary division"),
        }
    }

    // Operator precedence tests
    #[test]
    fn parse_operator_precedence_mul_before_add() {
        let result = parse_expr("1 + 2 * 3").unwrap();
        match result {
            Expr::Binary {
                left,
                op: BinaryOp::Add,
                right,
                ..
            } => {
                assert!(matches!(*left, Expr::Literal(Literal::Integer(1), _)));
                assert!(matches!(
                    *right,
                    Expr::Binary {
                        op: BinaryOp::Multiply,
                        ..
                    }
                ));
            }
            _ => panic!("Expected addition with multiplication on right"),
        }
    }

    #[test]
    fn parse_operator_precedence_left_associative() {
        let result = parse_expr("10 - 5 - 2").unwrap();
        match result {
            Expr::Binary {
                left,
                op: BinaryOp::Subtract,
                right,
                ..
            } => {
                assert!(matches!(
                    *left,
                    Expr::Binary {
                        op: BinaryOp::Subtract,
                        ..
                    }
                ));
                assert!(matches!(*right, Expr::Literal(Literal::Integer(2), _)));
            }
            _ => panic!("Expected left-associative subtraction"),
        }
    }

    #[test]
    fn parse_comparison_operators() {
        let less = parse_expr("a < b").unwrap();
        assert!(matches!(
            less,
            Expr::Binary {
                op: BinaryOp::Less,
                ..
            }
        ));

        let greater = parse_expr("a > b").unwrap();
        assert!(matches!(
            greater,
            Expr::Binary {
                op: BinaryOp::Greater,
                ..
            }
        ));

        let equal = parse_expr("a == b").unwrap();
        assert!(matches!(
            equal,
            Expr::Binary {
                op: BinaryOp::Equal,
                ..
            }
        ));
    }

    // Parenthesized expressions
    #[test]
    fn parse_parenthesized_expression() {
        let result = parse_expr("(1 + 2)").unwrap();
        match result {
            Expr::Paren(inner, _) => {
                assert!(matches!(
                    *inner,
                    Expr::Binary {
                        op: BinaryOp::Add,
                        ..
                    }
                ));
            }
            _ => panic!("Expected parenthesized expression"),
        }
    }

    #[test]
    fn parse_parentheses_override_precedence() {
        let result = parse_expr("(1 + 2) * 3").unwrap();
        match result {
            Expr::Binary {
                left,
                op: BinaryOp::Multiply,
                right,
                ..
            } => {
                assert!(matches!(*left, Expr::Paren(_, _)));
                assert!(matches!(*right, Expr::Literal(Literal::Integer(3), _)));
            }
            _ => panic!("Expected multiplication with parenthesized left"),
        }
    }

    // Unary operators
    #[test]
    fn parse_unary_negation() {
        let result = parse_expr("-42").unwrap();
        match result {
            Expr::Unary {
                op: UnaryOp::Negate,
                operand,
                ..
            } => {
                assert!(matches!(*operand, Expr::Literal(Literal::Integer(42), _)));
            }
            _ => panic!("Expected unary negation"),
        }
    }

    #[test]
    fn parse_unary_not() {
        let result = parse_expr("!true").unwrap();
        match result {
            Expr::Unary {
                op: UnaryOp::Not,
                operand,
                ..
            } => {
                assert!(matches!(*operand, Expr::Literal(Literal::Boolean(true), _)));
            }
            _ => panic!("Expected unary not"),
        }
    }

    // Function call tests
    #[test]
    fn parse_function_call_no_args() {
        let result = parse_expr("foo()").unwrap();
        match result {
            Expr::Call { func, args, .. } => {
                assert!(matches!(*func, Expr::Identifier(_)));
                assert_eq!(args.len(), 0);
            }
            _ => panic!("Expected function call"),
        }
    }

    #[test]
    fn parse_function_call_one_arg() {
        let result = parse_expr("foo(42)").unwrap();
        match result {
            Expr::Call { func, args, .. } => {
                assert!(matches!(*func, Expr::Identifier(_)));
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0], Expr::Literal(Literal::Integer(42), _)));
            }
            _ => panic!("Expected function call with one argument"),
        }
    }

    #[test]
    fn parse_function_call_multiple_args() {
        let result = parse_expr("add(1, 2, 3)").unwrap();
        match result {
            Expr::Call { func, args, .. } => {
                assert!(matches!(*func, Expr::Identifier(_)));
                assert_eq!(args.len(), 3);
            }
            _ => panic!("Expected function call with multiple arguments"),
        }
    }

    #[test]
    fn parse_nested_function_calls() {
        let result = parse_expr("foo(bar(42))").unwrap();
        match result {
            Expr::Call { func, args, .. } => {
                assert!(matches!(*func, Expr::Identifier(_)));
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0], Expr::Call { .. }));
            }
            _ => panic!("Expected nested function calls"),
        }
    }

    // Complex expression tests
    #[test]
    fn parse_complex_expression() {
        let result = parse_expr("(a + b) * c - d / e").unwrap();
        assert!(matches!(result, Expr::Binary { .. }));
    }

    #[test]
    fn parse_logical_operators() {
        let and_expr = parse_expr("a && b").unwrap();
        assert!(matches!(
            and_expr,
            Expr::Binary {
                op: BinaryOp::And,
                ..
            }
        ));

        let or_expr = parse_expr("a || b").unwrap();
        assert!(matches!(
            or_expr,
            Expr::Binary {
                op: BinaryOp::Or,
                ..
            }
        ));
    }

    // Error cases
    #[test]
    fn error_on_unexpected_token() {
        let result = parse_expr("+");
        assert!(result.is_err());
    }

    #[test]
    fn error_on_unclosed_paren() {
        let result = parse_expr("(1 + 2");
        assert!(result.is_err());
    }

    #[test]
    fn error_on_incomplete_binary_expr() {
        let result = parse_expr("1 +");
        assert!(result.is_err());
    }

    // Type parsing tests
    #[test]
    fn parse_type_i32() {
        let tokens = tokenize("i32").unwrap();
        let mut parser = Parser::new(tokens);
        let ty = parser.parse_type().unwrap();
        match ty {
            Type::Named(ident) => assert_eq!(ident.name, "i32"),
            _ => panic!("Expected named type"),
        }
    }

    #[test]
    fn parse_type_f64() {
        let tokens = tokenize("f64").unwrap();
        let mut parser = Parser::new(tokens);
        let ty = parser.parse_type().unwrap();
        match ty {
            Type::Named(ident) => assert_eq!(ident.name, "f64"),
            _ => panic!("Expected named type"),
        }
    }

    #[test]
    fn parse_type_bool() {
        let tokens = tokenize("bool").unwrap();
        let mut parser = Parser::new(tokens);
        let ty = parser.parse_type().unwrap();
        match ty {
            Type::Named(ident) => assert_eq!(ident.name, "bool"),
            _ => panic!("Expected named type"),
        }
    }

    #[test]
    fn parse_type_string() {
        let tokens = tokenize("string").unwrap();
        let mut parser = Parser::new(tokens);
        let ty = parser.parse_type().unwrap();
        match ty {
            Type::Named(ident) => assert_eq!(ident.name, "string"),
            _ => panic!("Expected named type"),
        }
    }

    // Variable declaration tests
    #[test]
    fn parse_val_declaration_with_type_and_init() {
        let tokens = tokenize("val x: i32 = 42").unwrap();
        let mut parser = Parser::new(tokens);
        let stmt = parser.parse_stmt().unwrap();
        match stmt {
            Stmt::VarDecl {
                name,
                ty,
                init,
                mutable,
                ..
            } => {
                assert_eq!(name.name, "x");
                assert!(!mutable);
                assert!(ty.is_some());
                assert!(init.is_some());
            }
            _ => panic!("Expected variable declaration"),
        }
    }

    #[test]
    fn parse_mut_declaration() {
        let tokens = tokenize("mut counter: i32 = 0").unwrap();
        let mut parser = Parser::new(tokens);
        let stmt = parser.parse_stmt().unwrap();
        match stmt {
            Stmt::VarDecl { name, mutable, .. } => {
                assert_eq!(name.name, "counter");
                assert!(mutable);
            }
            _ => panic!("Expected mutable variable declaration"),
        }
    }

    #[test]
    fn parse_val_declaration_without_type() {
        let tokens = tokenize("val x = 42").unwrap();
        let mut parser = Parser::new(tokens);
        let stmt = parser.parse_stmt().unwrap();
        match stmt {
            Stmt::VarDecl { name, ty, init, .. } => {
                assert_eq!(name.name, "x");
                assert!(ty.is_none());
                assert!(init.is_some());
            }
            _ => panic!("Expected variable declaration"),
        }
    }

    #[test]
    fn parse_val_declaration_without_init() {
        let tokens = tokenize("val x: i32").unwrap();
        let mut parser = Parser::new(tokens);
        let stmt = parser.parse_stmt().unwrap();
        match stmt {
            Stmt::VarDecl { name, ty, init, .. } => {
                assert_eq!(name.name, "x");
                assert!(ty.is_some());
                assert!(init.is_none());
            }
            _ => panic!("Expected variable declaration"),
        }
    }

    #[test]
    fn parse_val_with_expression_init() {
        let tokens = tokenize("val result: i32 = a + b").unwrap();
        let mut parser = Parser::new(tokens);
        let stmt = parser.parse_stmt().unwrap();
        match stmt {
            Stmt::VarDecl { name, init, .. } => {
                assert_eq!(name.name, "result");
                assert!(matches!(init, Some(Expr::Binary { .. })));
            }
            _ => panic!("Expected variable declaration"),
        }
    }

    // Return statement tests
    #[test]
    fn parse_return_with_value() {
        let tokens = tokenize("return 42").unwrap();
        let mut parser = Parser::new(tokens);
        let stmt = parser.parse_stmt().unwrap();
        match stmt {
            Stmt::Return { value, .. } => {
                assert!(value.is_some());
                assert!(matches!(
                    value,
                    Some(Expr::Literal(Literal::Integer(42), _))
                ));
            }
            _ => panic!("Expected return statement"),
        }
    }

    #[test]
    fn parse_return_without_value() {
        let tokens = tokenize("return").unwrap();
        let mut parser = Parser::new(tokens);
        let stmt = parser.parse_stmt().unwrap();
        match stmt {
            Stmt::Return { value, .. } => {
                assert!(value.is_none());
            }
            _ => panic!("Expected return statement"),
        }
    }

    #[test]
    fn parse_return_with_expression() {
        let tokens = tokenize("return a + b").unwrap();
        let mut parser = Parser::new(tokens);
        let stmt = parser.parse_stmt().unwrap();
        match stmt {
            Stmt::Return { value, .. } => {
                assert!(matches!(value, Some(Expr::Binary { .. })));
            }
            _ => panic!("Expected return statement"),
        }
    }

    // Expression statement tests
    #[test]
    fn parse_expression_statement() {
        let tokens = tokenize("foo()").unwrap();
        let mut parser = Parser::new(tokens);
        let stmt = parser.parse_stmt().unwrap();
        match stmt {
            Stmt::Expr(expr) => {
                assert!(matches!(expr, Expr::Call { .. }));
            }
            _ => panic!("Expected expression statement"),
        }
    }

    // Block parsing tests
    #[test]
    fn parse_empty_block() {
        let tokens = tokenize("{}").unwrap();
        let mut parser = Parser::new(tokens);
        let block = parser.parse_block().unwrap();
        assert_eq!(block.len(), 0);
    }

    #[test]
    fn parse_block_with_single_statement() {
        let tokens = tokenize("{ return 42 }").unwrap();
        let mut parser = Parser::new(tokens);
        let block = parser.parse_block().unwrap();
        assert_eq!(block.len(), 1);
        assert!(matches!(block[0], Stmt::Return { .. }));
    }

    #[test]
    fn parse_block_with_multiple_statements() {
        let source = r#"{
            val x: i32 = 5
            val y: i32 = 10
            return x + y
        }"#;
        let tokens = tokenize(source).unwrap();
        let mut parser = Parser::new(tokens);
        let block = parser.parse_block().unwrap();
        assert_eq!(block.len(), 3);
        assert!(matches!(block[0], Stmt::VarDecl { .. }));
        assert!(matches!(block[1], Stmt::VarDecl { .. }));
        assert!(matches!(block[2], Stmt::Return { .. }));
    }

    // Function parsing tests
    #[test]
    fn parse_function_no_params_no_return() {
        let source = "func foo() {}";
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.name.name, "foo");
                assert_eq!(func.params.len(), 0);
                assert!(func.return_type.is_none());
                assert_eq!(func.body.len(), 0);
            }
        }
    }

    #[test]
    fn parse_function_with_params_no_return() {
        let source = "func add(a: i32, b: i32) {}";
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.name.name, "add");
                assert_eq!(func.params.len(), 2);
                assert_eq!(func.params[0].name.name, "a");
                assert_eq!(func.params[1].name.name, "b");
                assert!(func.return_type.is_none());
            }
        }
    }

    #[test]
    fn parse_function_with_return_type() {
        let source = "func get_value() -> i32 {}";
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.name.name, "get_value");
                assert!(func.return_type.is_some());
            }
        }
    }

    #[test]
    fn parse_function_with_body() {
        let source = r#"func add(a: i32, b: i32) -> i32 {
            val result: i32 = a + b
            return result
        }"#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.name.name, "add");
                assert_eq!(func.params.len(), 2);
                assert!(func.return_type.is_some());
                assert_eq!(func.body.len(), 2);
            }
        }
    }

    #[test]
    fn parse_function_simple_return() {
        let source = r#"func add(a: i32, b: i32) -> i32 {
            return a + b
        }"#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.name.name, "add");
                assert_eq!(func.body.len(), 1);
                assert!(matches!(func.body[0], Stmt::Return { .. }));
            }
        }
    }

    // Complete program tests
    #[test]
    fn parse_complete_program() {
        let source = r#"
            func add(a: i32, b: i32) -> i32 {
                return a + b
            }

            func main() -> i32 {
                val result: i32 = add(5, 3)
                return result
            }
        "#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 2);
        match &result[0] {
            Item::Function(func) => assert_eq!(func.name.name, "add"),
        }
        match &result[1] {
            Item::Function(func) => assert_eq!(func.name.name, "main"),
        }
    }

    #[test]
    fn parse_program_with_multiple_variables() {
        let source = r#"
            func calculate(x: i32) -> i32 {
                val doubled: i32 = x * 2
                val added: i32 = doubled + 10
                mut result: i32 = added
                return result
            }
        "#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.body.len(), 4);
                assert!(matches!(func.body[0], Stmt::VarDecl { mutable: false, .. }));
                assert!(matches!(func.body[1], Stmt::VarDecl { mutable: false, .. }));
                assert!(matches!(func.body[2], Stmt::VarDecl { mutable: true, .. }));
                assert!(matches!(func.body[3], Stmt::Return { .. }));
            }
        }
    }

    // If/else statement tests
    #[test]
    fn parse_simple_if_statement() {
        let source = r#"func test() {
            if x > 0 {
                return 1
            }
        }"#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.body.len(), 1);
                match &func.body[0] {
                    Stmt::If {
                        condition,
                        then_block,
                        else_if_blocks,
                        else_block,
                        ..
                    } => {
                        assert!(matches!(condition, Expr::Binary { .. }));
                        assert_eq!(then_block.len(), 1);
                        assert!(matches!(then_block[0], Stmt::Return { .. }));
                        assert_eq!(else_if_blocks.len(), 0);
                        assert!(else_block.is_none());
                    }
                    _ => panic!("Expected if statement"),
                }
            }
        }
    }

    #[test]
    fn parse_if_else_statement() {
        let source = r#"func test() {
            if x > 0 {
                return 1
            } else {
                return -1
            }
        }"#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.body.len(), 1);
                match &func.body[0] {
                    Stmt::If {
                        then_block,
                        else_if_blocks,
                        else_block,
                        ..
                    } => {
                        assert_eq!(then_block.len(), 1);
                        assert_eq!(else_if_blocks.len(), 0);
                        assert!(else_block.is_some());
                        let else_stmts = else_block.as_ref().unwrap();
                        assert_eq!(else_stmts.len(), 1);
                        assert!(matches!(else_stmts[0], Stmt::Return { .. }));
                    }
                    _ => panic!("Expected if statement"),
                }
            }
        }
    }

    #[test]
    fn parse_if_else_if_statement() {
        let source = r#"func test() {
            if x > 0 {
                return 1
            } else if x < 0 {
                return -1
            }
        }"#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.body.len(), 1);
                match &func.body[0] {
                    Stmt::If {
                        then_block,
                        else_if_blocks,
                        else_block,
                        ..
                    } => {
                        assert_eq!(then_block.len(), 1);
                        assert_eq!(else_if_blocks.len(), 1);
                        assert!(else_block.is_none());

                        let (else_if_cond, else_if_stmts) = &else_if_blocks[0];
                        assert!(matches!(else_if_cond, Expr::Binary { .. }));
                        assert_eq!(else_if_stmts.len(), 1);
                        assert!(matches!(else_if_stmts[0], Stmt::Return { .. }));
                    }
                    _ => panic!("Expected if statement"),
                }
            }
        }
    }

    #[test]
    fn parse_if_else_if_else_chain() {
        let source = r#"func test() {
            if x > 0 {
                return 1
            } else if x < 0 {
                return -1
            } else {
                return 0
            }
        }"#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.body.len(), 1);
                match &func.body[0] {
                    Stmt::If {
                        then_block,
                        else_if_blocks,
                        else_block,
                        ..
                    } => {
                        assert_eq!(then_block.len(), 1);
                        assert_eq!(else_if_blocks.len(), 1);
                        assert!(else_block.is_some());

                        let else_stmts = else_block.as_ref().unwrap();
                        assert_eq!(else_stmts.len(), 1);
                        assert!(matches!(else_stmts[0], Stmt::Return { .. }));
                    }
                    _ => panic!("Expected if statement"),
                }
            }
        }
    }

    #[test]
    fn parse_multiple_else_if_clauses() {
        let source = r#"func test() {
            if x > 10 {
                return 1
            } else if x > 5 {
                return 2
            } else if x > 0 {
                return 3
            } else {
                return 4
            }
        }"#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.body.len(), 1);
                match &func.body[0] {
                    Stmt::If {
                        then_block,
                        else_if_blocks,
                        else_block,
                        ..
                    } => {
                        assert_eq!(then_block.len(), 1);
                        assert_eq!(else_if_blocks.len(), 2);
                        assert!(else_block.is_some());
                    }
                    _ => panic!("Expected if statement"),
                }
            }
        }
    }

    #[test]
    fn parse_nested_if_statements() {
        let source = r#"func test() {
            if x > 0 {
                if y > 0 {
                    return 1
                }
            }
        }"#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.body.len(), 1);
                match &func.body[0] {
                    Stmt::If { then_block, .. } => {
                        assert_eq!(then_block.len(), 1);
                        assert!(matches!(then_block[0], Stmt::If { .. }));
                    }
                    _ => panic!("Expected if statement"),
                }
            }
        }
    }

    #[test]
    fn parse_if_with_complex_condition() {
        let source = r#"func test() {
            if (a && b) || c {
                return 1
            }
        }"#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.body.len(), 1);
                match &func.body[0] {
                    Stmt::If { condition, .. } => {
                        assert!(matches!(condition, Expr::Binary { .. }));
                    }
                    _ => panic!("Expected if statement"),
                }
            }
        }
    }

    #[test]
    fn parse_if_with_multiple_statements_in_blocks() {
        let source = r#"func test() {
            if x > 0 {
                val a: i32 = 1
                val b: i32 = 2
                return a + b
            } else {
                val c: i32 = 3
                return c
            }
        }"#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.body.len(), 1);
                match &func.body[0] {
                    Stmt::If {
                        then_block,
                        else_block,
                        ..
                    } => {
                        assert_eq!(then_block.len(), 3);
                        let else_stmts = else_block.as_ref().unwrap();
                        assert_eq!(else_stmts.len(), 2);
                    }
                    _ => panic!("Expected if statement"),
                }
            }
        }
    }

    #[test]
    fn parse_if_with_empty_blocks() {
        let source = r#"func test() {
            if x > 0 {
            }
        }"#;
        let result = parse(source).unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            Item::Function(func) => {
                assert_eq!(func.body.len(), 1);
                match &func.body[0] {
                    Stmt::If { then_block, .. } => {
                        assert_eq!(then_block.len(), 0);
                    }
                    _ => panic!("Expected if statement"),
                }
            }
        }
    }

    #[test]
    fn error_on_if_without_condition() {
        let source = r#"func test() {
            if {
                return 1
            }
        }"#;
        let result = parse(source);
        assert!(result.is_err());
    }

    #[test]
    fn error_on_if_without_braces() {
        let source = "func test() { if x > 0 return 1 }";
        let result = parse(source);
        assert!(result.is_err());
    }

    #[test]
    fn error_on_else_without_if() {
        let source = r#"func test() {
            else {
                return 1
            }
        }"#;
        let result = parse(source);
        assert!(result.is_err());
    }

    // Error case tests
    #[test]
    fn error_on_missing_function_name() {
        let source = "func () {}";
        let result = parse(source);
        assert!(result.is_err());
    }

    #[test]
    fn error_on_missing_parameter_type() {
        let source = "func foo(x) {}";
        let result = parse(source);
        assert!(result.is_err());
    }

    #[test]
    fn error_on_unclosed_function_body() {
        let source = "func foo() {";
        let result = parse(source);
        assert!(result.is_err());
    }

    #[test]
    fn error_on_invalid_statement() {
        let source = "func foo() { val }";
        let result = parse(source);
        assert!(result.is_err());
    }
}
