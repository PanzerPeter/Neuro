use lexical_analysis::TokenKind;
use shared_types::{Identifier, Span};

use crate::ast::{BinaryOp, Expr, Stmt};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;

impl Parser {
    /// Parse a const declaration statement: `const NAME: Type = expr`
    pub(crate) fn parse_const_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        let name_token = self.consume(TokenKind::Identifier(String::new()), "constant name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "constant name".to_string(),
                span: name_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::Colon, "':'")?;
        self.skip_newlines();

        let ty = self.parse_type()?;

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let value = self.parse_expr(Precedence::Lowest)?;
        let span = start_span.merge(value.span());

        Ok(Stmt::Const {
            name,
            ty,
            value,
            span,
        })
    }
}

impl Parser {
    /// Parse a variable declaration statement (val/mut)
    pub(crate) fn parse_var_decl(&mut self, mutable: bool, start_span: Span) -> ParseResult<Stmt> {
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

        self.skip_newlines();
        let ty = if self.check(&TokenKind::Colon) {
            self.advance(); // consume ':'
            self.skip_newlines();
            Some(self.parse_type()?)
        } else {
            None
        };

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
                .or_else(|| ty.as_ref().map(|t| t.span()))
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
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

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

        let then_block = self.parse_block()?;
        self.skip_newlines();

        let mut else_if_blocks = Vec::new();
        let mut else_block = None;

        while self.check(&TokenKind::Else) {
            self.advance(); // consume 'else'
            self.skip_newlines();

            if self.check(&TokenKind::If) {
                self.advance(); // consume 'if'
                self.skip_newlines();

                self.no_struct_lit = true;
                let else_if_condition = self.parse_expr(Precedence::Lowest)?;
                self.no_struct_lit = false;
                self.skip_newlines();

                let else_if_block = self.parse_block()?;
                else_if_blocks.push((else_if_condition, else_if_block));
                self.skip_newlines();
            } else {
                else_block = Some(self.parse_block()?);
                break;
            }
        }

        let end_span = else_block
            .as_ref()
            .and_then(|stmts| stmts.last())
            .or_else(|| else_if_blocks.last().and_then(|(_, stmts)| stmts.last()))
            .or_else(|| then_block.last())
            .map(stmt_span)
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

        let end_span = body.last().map(stmt_span).unwrap_or(condition.span());

        Ok(Stmt::While {
            condition,
            body,
            span: start_span.merge(end_span),
        })
    }

    /// Parse a loop statement: `loop { ... }` (§3.7).
    pub(crate) fn parse_loop_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        self.skip_newlines();

        let body = self.parse_block()?;

        let end_span = body.last().map(stmt_span).unwrap_or(start_span);

        Ok(Stmt::Loop {
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

        let mut inclusive = false;
        if self.check(&TokenKind::DotDotEqual) {
            self.advance();
            inclusive = true;
        } else {
            self.consume(TokenKind::DotDot, "'..' or '..='")?;
        }

        self.skip_newlines();

        let end = self.parse_expr(Precedence::Lowest)?;
        self.no_struct_lit = false;
        self.skip_newlines();

        let body = self.parse_block()?;

        let end_span = body.last().map(stmt_span).unwrap_or(end.span());

        Ok(Stmt::ForRange {
            iterator,
            start,
            end,
            inclusive,
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
            TokenKind::Const => {
                let start_span = token.span;
                self.advance(); // consume 'const'
                self.skip_newlines();
                self.parse_const_stmt(start_span)
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
            TokenKind::Loop => {
                let start_span = token.span;
                self.advance(); // consume 'loop'
                self.parse_loop_stmt(start_span)
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
            // A leading `*` is a dereference: either an assignment through a
            // mutable reference (`*r = value`, §2.5) or a deref expression statement.
            TokenKind::Star => {
                let start_span = token.span;
                let expr = self.parse_expr(Precedence::Lowest)?;
                if self.check(&TokenKind::Equal) {
                    self.advance(); // consume '='
                    let value = self.parse_expr(Precedence::Lowest)?;
                    let span = start_span.merge(value.span());
                    let pointer = match expr {
                        Expr::Deref { operand, .. } => *operand,
                        // The `*` prefix always parses to a Deref, so this is unreachable
                        // in practice; fall back to the parsed expression defensively.
                        other => other,
                    };
                    return Ok(Stmt::DerefAssignment {
                        pointer,
                        value,
                        span,
                    });
                }
                Ok(Stmt::Expr(expr))
            }
            _ => {
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

/// Extract the span from a statement — shared by span-calculation in block-ending logic.
pub(crate) fn stmt_span(stmt: &Stmt) -> shared_types::Span {
    match stmt {
        Stmt::VarDecl { span, .. } => *span,
        Stmt::Const { span, .. } => *span,
        Stmt::Assignment { span, .. } => *span,
        Stmt::Return { span, .. } => *span,
        Stmt::If { span, .. } => *span,
        Stmt::While { span, .. } => *span,
        Stmt::Loop { span, .. } => *span,
        Stmt::ForRange { span, .. } => *span,
        Stmt::Break { span } => *span,
        Stmt::Continue { span } => *span,
        Stmt::FieldAssignment { span, .. } => *span,
        Stmt::DerefAssignment { span, .. } => *span,
        Stmt::Expr(e) => e.span(),
    }
}
