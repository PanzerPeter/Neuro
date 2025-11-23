// NEURO Programming Language - Syntax Parsing
// Parser implementation using Pratt parsing for expressions

use lexical_analysis::{Token, TokenKind};
use shared_types::{Identifier, Literal, Span};

use crate::ast::{BinaryOp, Expr, FunctionDef, Item, Parameter, Stmt, Type, UnaryOp};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

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

    /// Parse an assignment statement (identifier = expression)
    pub(crate) fn parse_assignment_stmt(&mut self) -> ParseResult<Stmt> {
        // Parse target identifier
        let target_token = self.consume(TokenKind::Identifier(String::new()), "identifier")?;
        let target = if let TokenKind::Identifier(name) = target_token.kind {
            Identifier {
                name,
                span: target_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: target_token.kind,
                expected: "identifier".to_string(),
                span: target_token.span,
            });
        };

        self.skip_newlines();

        // Consume '='
        self.consume(TokenKind::Equal, "'='")?;

        self.skip_newlines();

        // Parse value expression
        let value = self.parse_expr(Precedence::Lowest)?;

        let span = target.span.merge(value.span());

        Ok(Stmt::Assignment {
            target,
            value,
            span,
        })
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
                Stmt::Assignment { span, .. } => *span,
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
            TokenKind::Identifier(_) => {
                // Check if this is an assignment (identifier = expr) or expression statement
                // Lookahead to see if next token is '='
                if self.current + 1 < self.tokens.len() {
                    if let Some(next_token) = self.tokens.get(self.current + 1) {
                        if matches!(next_token.kind, TokenKind::Equal) {
                            // This is an assignment statement
                            return self.parse_assignment_stmt();
                        }
                    }
                }

                // Otherwise, parse as expression statement
                let expr = self.parse_expr(Precedence::Lowest)?;
                Ok(Stmt::Expr(expr))
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
                Stmt::Assignment { span, .. } => *span,
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

    /// Parse top-level items (currently only functions in Phase 1)
    pub(crate) fn parse_program(&mut self) -> ParseResult<Vec<Item>> {
        let mut items = Vec::new();

        self.skip_newlines();
        while !self.is_at_end() {
            // For Phase 1, we only support function definitions
            if self.check(&TokenKind::Func) {
                let func = self.parse_function()?;
                items.push(Item::Function(func));
            } else {
                let token = self.peek().ok_or(ParseError::UnexpectedEof {
                    expected: "function definition".to_string(),
                })?;
                return Err(ParseError::UnexpectedToken {
                    found: token.kind.clone(),
                    expected: "function definition".to_string(),
                    span: token.span,
                });
            }
            self.skip_newlines();
        }

        Ok(items)
    }
}
