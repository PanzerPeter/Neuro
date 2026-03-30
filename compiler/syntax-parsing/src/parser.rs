// NEURO Programming Language - Syntax Parsing
// Parser implementation using Pratt parsing for expressions

use lexical_analysis::{Token, TokenKind};
use shared_types::{Identifier, Literal, Span};

use crate::ast::{
    BinaryOp, Expr, FieldDef, FieldInit, FunctionDef, ImplDef, Item, MethodDef, Parameter,
    SelfParam, Stmt, StructDef, Type, UnaryOp,
};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

/// Maximum expression nesting depth to prevent stack overflow
const MAX_EXPR_DEPTH: usize = 256;

/// Parser for NEURO source code
pub(crate) struct Parser {
    tokens: Vec<Token>,
    current: usize,
    expr_depth: usize,
    /// When true, an identifier followed by `{` is NOT parsed as a struct literal.
    /// Set to true inside if/while/for conditions to prevent consuming the block's `{`.
    no_struct_lit: bool,
}

impl Parser {
    /// Create a new parser from a token stream
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            current: 0,
            expr_depth: 0,
            no_struct_lit: false,
        }
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
        // Check recursion depth to prevent stack overflow
        if self.expr_depth >= MAX_EXPR_DEPTH {
            return Err(ParseError::MaxDepthExceeded(MAX_EXPR_DEPTH));
        }

        self.expr_depth += 1;
        let result = self.parse_expr_inner(precedence);
        self.expr_depth -= 1;

        result
    }

    /// Inner expression parsing implementation
    fn parse_expr_inner(&mut self, precedence: Precedence) -> ParseResult<Expr> {
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

            // Identifiers — path expressions (`Type::member`), struct literals, or plain idents
            TokenKind::Identifier(name) => {
                let ident = Identifier {
                    name,
                    span: token.span,
                };
                if self.check(&TokenKind::ColonColon) {
                    self.advance(); // consume '::'
                    let member_token = self.consume(
                        TokenKind::Identifier(String::new()),
                        "member name after '::'",
                    )?;
                    let member = if let TokenKind::Identifier(n) = member_token.kind {
                        Identifier {
                            name: n,
                            span: member_token.span,
                        }
                    } else {
                        return Err(ParseError::UnexpectedToken {
                            found: member_token.kind,
                            expected: "member name".to_string(),
                            span: member_token.span,
                        });
                    };
                    let span = ident.span.merge(member.span);
                    Ok(Expr::Path {
                        type_name: ident,
                        member,
                        span,
                    })
                } else if !self.no_struct_lit && self.check(&TokenKind::LeftBrace) {
                    self.parse_struct_literal(ident)
                } else {
                    Ok(Expr::Identifier(ident))
                }
            }

            // `self` keyword used as expression inside method bodies
            TokenKind::SelfLower => Ok(Expr::Identifier(Identifier {
                name: "self".to_string(),
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

            // Field access: `expr.field`
            TokenKind::Dot => {
                self.advance(); // consume '.'
                let field_token =
                    self.consume(TokenKind::Identifier(String::new()), "field name")?;
                let field = if let TokenKind::Identifier(name) = field_token.kind {
                    Identifier {
                        name,
                        span: field_token.span,
                    }
                } else {
                    return Err(ParseError::UnexpectedToken {
                        found: field_token.kind,
                        expected: "field name".to_string(),
                        span: field_token.span,
                    });
                };
                let span = left.span().merge(field.span);
                Ok(Expr::FieldAccess {
                    object: Box::new(left),
                    field,
                    span,
                })
            }

            // Binary operators
            kind if self.is_binary_op(kind) => {
                let op_token = self.advance().ok_or(ParseError::UnexpectedEof {
                    expected: "operator".to_string(),
                })?;
                let op = self.token_to_binary_op(&op_token)?;
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

    /// Convert a token to a binary operator
    fn token_to_binary_op(&self, token: &Token) -> ParseResult<BinaryOp> {
        match &token.kind {
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
                found: token.kind.clone(),
                expected: "binary operator".to_string(),
                span: token.span,
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
            TokenKind::Dot => Precedence::FieldAccess,
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

    /// Parse a compound assignment statement, desugaring into a plain assignment.
    /// `target OP= rhs` → `target = target OP rhs` — no new AST nodes required.
    pub(crate) fn parse_compound_assignment_stmt(&mut self) -> ParseResult<Stmt> {
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

        let op_token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "compound assignment operator".to_string(),
        })?;

        let binary_op = match op_token.kind {
            TokenKind::PlusEqual => BinaryOp::Add,
            TokenKind::MinusEqual => BinaryOp::Subtract,
            TokenKind::StarEqual => BinaryOp::Multiply,
            TokenKind::SlashEqual => BinaryOp::Divide,
            TokenKind::PercentEqual => BinaryOp::Modulo,
            _ => {
                return Err(ParseError::UnexpectedToken {
                    found: op_token.kind,
                    expected: "compound assignment operator".to_string(),
                    span: op_token.span,
                })
            }
        };

        self.skip_newlines();

        let rhs = self.parse_expr(Precedence::Lowest)?;

        let target_expr = Expr::Identifier(target.clone());
        let binary_span = target.span.merge(rhs.span());
        let value = Expr::Binary {
            left: Box::new(target_expr),
            op: binary_op,
            right: Box::new(rhs),
            span: binary_span,
        };
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

        self.no_struct_lit = true;
        let condition = self.parse_expr(Precedence::Lowest)?;
        self.no_struct_lit = false;
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
                Stmt::While { span, .. } => *span,
                Stmt::ForRange { span, .. } => *span,
                Stmt::Break { span } => *span,
                Stmt::Continue { span } => *span,
                Stmt::FieldAssignment { span, .. } => *span,
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

    /// Parse a while statement
    pub(crate) fn parse_while_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        self.skip_newlines();

        self.no_struct_lit = true;
        let condition = self.parse_expr(Precedence::Lowest)?;
        self.no_struct_lit = false;
        self.skip_newlines();

        let body = self.parse_block()?;

        let end_span = body
            .last()
            .map(|stmt| match stmt {
                Stmt::VarDecl { span, .. } => *span,
                Stmt::Assignment { span, .. } => *span,
                Stmt::Return { span, .. } => *span,
                Stmt::If { span, .. } => *span,
                Stmt::While { span, .. } => *span,
                Stmt::ForRange { span, .. } => *span,
                Stmt::Break { span } => *span,
                Stmt::Continue { span } => *span,
                Stmt::FieldAssignment { span, .. } => *span,
                Stmt::Expr(e) => e.span(),
            })
            .unwrap_or(condition.span());

        Ok(Stmt::While {
            condition,
            body,
            span: start_span.merge(end_span),
        })
    }

    /// Parse a for-range statement: `for <ident> in <expr>..<expr> { ... }`
    pub(crate) fn parse_for_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        self.skip_newlines();

        let iterator_token = self.consume(TokenKind::Identifier(String::new()), "loop variable")?;
        let iterator = if let TokenKind::Identifier(name) = iterator_token.kind {
            Identifier {
                name,
                span: iterator_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: iterator_token.kind,
                expected: "loop variable".to_string(),
                span: iterator_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::In, "'in'")?;
        self.skip_newlines();

        // Range expressions must not be struct literals or `{` would be consumed
        self.no_struct_lit = true;
        let start = self.parse_expr(Precedence::Lowest)?;
        self.skip_newlines();

        if self.check(&TokenKind::DotDotEqual) {
            let token = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'..'".to_string(),
            })?;
            return Err(ParseError::UnexpectedToken {
                found: token.kind,
                expected: "'..' (exclusive range; '..=' is not yet supported)".to_string(),
                span: token.span,
            });
        }

        self.consume(TokenKind::DotDot, "'..'")?;
        self.skip_newlines();

        let end = self.parse_expr(Precedence::Lowest)?;
        self.no_struct_lit = false;
        self.skip_newlines();

        let body = self.parse_block()?;

        let end_span = body
            .last()
            .map(|stmt| match stmt {
                Stmt::VarDecl { span, .. } => *span,
                Stmt::Assignment { span, .. } => *span,
                Stmt::Return { span, .. } => *span,
                Stmt::If { span, .. } => *span,
                Stmt::While { span, .. } => *span,
                Stmt::ForRange { span, .. } => *span,
                Stmt::Break { span } => *span,
                Stmt::Continue { span } => *span,
                Stmt::FieldAssignment { span, .. } => *span,
                Stmt::Expr(e) => e.span(),
            })
            .unwrap_or(end.span());

        Ok(Stmt::ForRange {
            iterator,
            start,
            end,
            body,
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
            TokenKind::While => {
                let start_span = token.span;
                self.advance(); // consume 'while'
                self.parse_while_stmt(start_span)
            }
            TokenKind::For => {
                let start_span = token.span;
                self.advance(); // consume 'for'
                self.parse_for_stmt(start_span)
            }
            TokenKind::Break => {
                let span = token.span;
                self.advance(); // consume 'break'
                Ok(Stmt::Break { span })
            }
            TokenKind::Continue => {
                let span = token.span;
                self.advance(); // consume 'continue'
                Ok(Stmt::Continue { span })
            }
            TokenKind::Identifier(_) => {
                // Lookahead to distinguish:
                //   ident = expr          → assignment
                //   ident OP= expr        → compound assignment (desugared)
                //   ident.field = expr    → field assignment
                //   anything else         → expression statement
                if self.current + 1 < self.tokens.len() {
                    if let Some(next_token) = self.tokens.get(self.current + 1) {
                        if matches!(next_token.kind, TokenKind::Equal) {
                            return self.parse_assignment_stmt();
                        }
                        if matches!(
                            next_token.kind,
                            TokenKind::PlusEqual
                                | TokenKind::MinusEqual
                                | TokenKind::StarEqual
                                | TokenKind::SlashEqual
                                | TokenKind::PercentEqual
                        ) {
                            return self.parse_compound_assignment_stmt();
                        }
                        if matches!(next_token.kind, TokenKind::Dot) {
                            // Check for ident.field = expr (two more tokens ahead)
                            if let (Some(field_tok), Some(eq_tok)) = (
                                self.tokens.get(self.current + 2),
                                self.tokens.get(self.current + 3),
                            ) {
                                if matches!(field_tok.kind, TokenKind::Identifier(_))
                                    && matches!(eq_tok.kind, TokenKind::Equal)
                                {
                                    return self.parse_field_assignment_stmt();
                                }
                            }
                        }
                    }
                }

                let expr = self.parse_expr(Precedence::Lowest)?;
                Ok(Stmt::Expr(expr))
            }
            // `self` keyword as statement — detect `self.field = expr` field assignments
            TokenKind::SelfLower => {
                if self.current + 1 < self.tokens.len() {
                    if let Some(next_token) = self.tokens.get(self.current + 1) {
                        if matches!(next_token.kind, TokenKind::Dot) {
                            if let (Some(field_tok), Some(eq_tok)) = (
                                self.tokens.get(self.current + 2),
                                self.tokens.get(self.current + 3),
                            ) {
                                if matches!(field_tok.kind, TokenKind::Identifier(_))
                                    && matches!(eq_tok.kind, TokenKind::Equal)
                                {
                                    return self.parse_self_field_assignment_stmt();
                                }
                            }
                        }
                    }
                }
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

        let mut params: Vec<Parameter> = Vec::new();
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

                // Check for duplicate parameter name
                for existing_param in &params {
                    if existing_param.name.name == param_name.name {
                        return Err(ParseError::DuplicateParameter {
                            name: param_name.name.clone(),
                            span: param_name.span,
                        });
                    }
                }

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
                Stmt::While { span, .. } => *span,
                Stmt::ForRange { span, .. } => *span,
                Stmt::Break { span } => *span,
                Stmt::Continue { span } => *span,
                Stmt::FieldAssignment { span, .. } => *span,
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

    /// Parse top-level items: function, struct, or impl definitions
    pub(crate) fn parse_program(&mut self) -> ParseResult<Vec<Item>> {
        let mut items = Vec::new();

        self.skip_newlines();
        while !self.is_at_end() {
            if self.check(&TokenKind::Func) {
                let func = self.parse_function()?;
                items.push(Item::Function(func));
            } else if self.check(&TokenKind::Struct) {
                let s = self.parse_struct_def()?;
                items.push(Item::Struct(s));
            } else if self.check(&TokenKind::Impl) {
                let impl_def = self.parse_impl_def()?;
                items.push(Item::Impl(impl_def));
            } else {
                let token = self.peek().ok_or(ParseError::UnexpectedEof {
                    expected: "function, struct, or impl definition".to_string(),
                })?;
                return Err(ParseError::UnexpectedToken {
                    found: token.kind.clone(),
                    expected: "function, struct, or impl definition".to_string(),
                    span: token.span,
                });
            }
            self.skip_newlines();
        }

        Ok(items)
    }

    /// Parse a struct definition: `struct Name { field: Type, ... }`
    pub(crate) fn parse_struct_def(&mut self) -> ParseResult<StructDef> {
        let start = self.consume(TokenKind::Struct, "'struct'")?;
        self.skip_newlines();

        let name_token = self.consume(TokenKind::Identifier(String::new()), "struct name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "struct name".to_string(),
                span: name_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut fields: Vec<FieldDef> = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let field_name_token =
                self.consume(TokenKind::Identifier(String::new()), "field name")?;
            let field_name = if let TokenKind::Identifier(n) = field_name_token.kind {
                Identifier {
                    name: n,
                    span: field_name_token.span,
                }
            } else {
                return Err(ParseError::UnexpectedToken {
                    found: field_name_token.kind,
                    expected: "field name".to_string(),
                    span: field_name_token.span,
                });
            };

            self.skip_newlines();
            self.consume(TokenKind::Colon, "':'")?;
            self.skip_newlines();

            let field_ty = self.parse_type()?;
            let field_span = field_name.span.merge(match &field_ty {
                Type::Named(ident) => ident.span,
                Type::Tensor { span, .. } => *span,
            });

            fields.push(FieldDef {
                name: field_name,
                ty: field_ty,
                span: field_span,
            });

            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance(); // consume ','
                self.skip_newlines();
            } else {
                break;
            }
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;

        Ok(StructDef {
            name,
            fields,
            span: start.span.merge(close.span),
        })
    }

    /// Parse a struct literal expression: `TypeName { field: expr, ... }`
    ///
    /// The `name` identifier has already been consumed by `parse_prefix`.
    pub(crate) fn parse_struct_literal(&mut self, name: Identifier) -> ParseResult<Expr> {
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut fields: Vec<FieldInit> = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let field_name_token =
                self.consume(TokenKind::Identifier(String::new()), "field name")?;
            let field_name = if let TokenKind::Identifier(n) = field_name_token.kind {
                Identifier {
                    name: n,
                    span: field_name_token.span,
                }
            } else {
                return Err(ParseError::UnexpectedToken {
                    found: field_name_token.kind,
                    expected: "field name".to_string(),
                    span: field_name_token.span,
                });
            };

            self.skip_newlines();
            self.consume(TokenKind::Colon, "':'")?;
            self.skip_newlines();

            let value = self.parse_expr(Precedence::Lowest)?;
            let field_span = field_name.span.merge(value.span());

            fields.push(FieldInit {
                name: field_name,
                value: Box::new(value),
                span: field_span,
            });

            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance(); // consume ','
                self.skip_newlines();
            } else {
                break;
            }
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;
        let span = name.span.merge(close.span);

        Ok(Expr::StructLiteral { name, fields, span })
    }

    /// Parse an `impl TypeName { … }` block
    pub(crate) fn parse_impl_def(&mut self) -> ParseResult<ImplDef> {
        let start = self.consume(TokenKind::Impl, "'impl'")?;
        self.skip_newlines();

        let name_token = self.consume(TokenKind::Identifier(String::new()), "struct name")?;
        let type_name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "struct name".to_string(),
                span: name_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut methods = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            methods.push(self.parse_method_def()?);
            self.skip_newlines();
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;

        Ok(ImplDef {
            type_name,
            methods,
            span: start.span.merge(close.span),
        })
    }

    /// Parse a single method definition inside an `impl` block.
    ///
    /// Handles three self-parameter forms:
    ///   `&self`     — immutable borrow (SelfParam::Ref)
    ///   `&mut self` — mutable borrow   (SelfParam::RefMut)
    ///   `self`      — owned/consuming  (SelfParam::Owned)
    ///
    /// Associated functions have no self parameter and use the same syntax as
    /// free functions. The distinction is detected by checking the first parameter.
    pub(crate) fn parse_method_def(&mut self) -> ParseResult<MethodDef> {
        let start = self.consume(TokenKind::Func, "'func'")?;
        self.skip_newlines();

        let name_token = self.consume(TokenKind::Identifier(String::new()), "method name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "method name".to_string(),
                span: name_token.span,
            });
        };

        self.consume(TokenKind::LeftParen, "'('")?;
        self.skip_newlines();

        // Detect and consume the optional self parameter before parsing regular params.
        let self_param = self.try_parse_self_param()?;

        // If there was a self param and more params follow, consume the comma separator.
        if self_param.is_some() {
            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance(); // consume ','
                self.skip_newlines();
            }
        }

        // Parse remaining regular parameters (may be empty)
        let mut params: Vec<Parameter> = Vec::new();
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

        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance(); // consume '->'
            self.skip_newlines();
            Some(self.parse_type()?)
        } else {
            None
        };

        self.skip_newlines();
        let body = self.parse_block()?;

        let end_span = body
            .last()
            .map(|s| match s {
                Stmt::VarDecl { span, .. } => *span,
                Stmt::Assignment { span, .. } => *span,
                Stmt::Return { span, .. } => *span,
                Stmt::If { span, .. } => *span,
                Stmt::While { span, .. } => *span,
                Stmt::ForRange { span, .. } => *span,
                Stmt::Break { span } => *span,
                Stmt::Continue { span } => *span,
                Stmt::FieldAssignment { span, .. } => *span,
                Stmt::Expr(e) => e.span(),
            })
            .unwrap_or(start.span);

        Ok(MethodDef {
            name,
            self_param,
            params,
            return_type,
            body,
            span: start.span.merge(end_span),
        })
    }

    /// Attempt to parse a self parameter (`self`, `&self`, `&mut self`) at the
    /// current token position. Returns `None` without consuming tokens if no
    /// self parameter is present.
    fn try_parse_self_param(&mut self) -> ParseResult<Option<SelfParam>> {
        match self.peek_kind() {
            // `self` — consuming self
            Some(TokenKind::SelfLower) => {
                self.advance(); // consume 'self'
                Ok(Some(SelfParam::Owned))
            }
            // `&self` or `&mut self`
            Some(TokenKind::Amp) => {
                // Peek ahead to confirm this is a self/mut-self param, not a
                // regular reference type (reference types are not yet in the grammar,
                // but we guard against future ambiguity).
                let next = self.tokens.get(self.current + 1).map(|t| &t.kind);
                match next {
                    Some(TokenKind::SelfLower) => {
                        self.advance(); // consume '&'
                        self.advance(); // consume 'self'
                        Ok(Some(SelfParam::Ref))
                    }
                    Some(TokenKind::Mut) => {
                        // Must be followed by 'self'
                        let after_mut = self.tokens.get(self.current + 2).map(|t| &t.kind);
                        if matches!(after_mut, Some(TokenKind::SelfLower)) {
                            self.advance(); // consume '&'
                            self.advance(); // consume 'mut'
                            self.advance(); // consume 'self'
                            Ok(Some(SelfParam::RefMut))
                        } else {
                            Ok(None)
                        }
                    }
                    _ => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }

    /// Parse a field assignment statement: `object.field = value`
    pub(crate) fn parse_field_assignment_stmt(&mut self) -> ParseResult<Stmt> {
        let object_token = self.consume(TokenKind::Identifier(String::new()), "variable name")?;
        let object = if let TokenKind::Identifier(n) = object_token.kind {
            Identifier {
                name: n,
                span: object_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: object_token.kind,
                expected: "variable name".to_string(),
                span: object_token.span,
            });
        };

        self.consume(TokenKind::Dot, "'.'")?;

        let field_token = self.consume(TokenKind::Identifier(String::new()), "field name")?;
        let field = if let TokenKind::Identifier(n) = field_token.kind {
            Identifier {
                name: n,
                span: field_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: field_token.kind,
                expected: "field name".to_string(),
                span: field_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let value = self.parse_expr(Precedence::Lowest)?;
        let span = object.span.merge(value.span());

        Ok(Stmt::FieldAssignment {
            object,
            field,
            value,
            span,
        })
    }

    /// Parse `self.field = value` inside a method body.
    pub(crate) fn parse_self_field_assignment_stmt(&mut self) -> ParseResult<Stmt> {
        let self_token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "self".to_string(),
        })?;
        let object = Identifier {
            name: "self".to_string(),
            span: self_token.span,
        };

        self.consume(TokenKind::Dot, "'.'")?;

        let field_token = self.consume(TokenKind::Identifier(String::new()), "field name")?;
        let field = if let TokenKind::Identifier(n) = field_token.kind {
            Identifier {
                name: n,
                span: field_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: field_token.kind,
                expected: "field name".to_string(),
                span: field_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let value = self.parse_expr(Precedence::Lowest)?;
        let span = object.span.merge(value.span());

        Ok(Stmt::FieldAssignment {
            object,
            field,
            value,
            span,
        })
    }
}
