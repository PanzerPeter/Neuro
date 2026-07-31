use lexical_analysis::TokenKind;
use shared_types::{Identifier, Span};

use crate::ast::{Expr, Stmt};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;

/// A binding pattern on the left of a destructuring `val`/`mut`. Lives only
/// during parsing — it is expanded to ordinary variable declarations and never
/// reaches the AST.
pub(super) enum DestructurePattern {
    /// `_` — matches and discards the value, binding nothing.
    Wildcard,
    /// A binding name.
    Bind(Identifier),
    /// A nested tuple pattern `(a, b, ...)`.
    Tuple(Vec<DestructurePattern>),
    /// A struct pattern `Name { field, field, ... }` binding each field by its name.
    /// The type name is syntax-only — field access in the desugar resolves against
    /// the scrutinee's own type — so it is not retained.
    Struct { fields: Vec<Identifier> },
    /// An array pattern `[p0, p1, ..rest]` binding elements positionally with an
    /// optional trailing rest.
    Array(Vec<ArrayPatternElem>),
}

/// One element of an array destructuring pattern.
pub(super) enum ArrayPatternElem {
    /// A positional sub-pattern.
    Pattern(DestructurePattern),
    /// A trailing rest `..` (discarded) or `..name` (binds the remainder).
    Rest(Option<Identifier>),
}

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
}

impl Parser {
    /// Parse a single statement
    pub(crate) fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        self.skip_newlines();

        let token = self.peek().ok_or(ParseError::UnexpectedEof {
            expected: "statement".to_string(),
        })?;

        match &token.kind {
            TokenKind::Val => {
                let start_span = token.span;
                let val_else = self.starts_val_else();
                self.advance(); // consume 'val'
                self.skip_newlines();
                if val_else {
                    return self.parse_val_else(start_span);
                }
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
                self.parse_while_stmt(start_span, None)
            }
            TokenKind::Loop => {
                let start_span = token.span;
                self.advance(); // consume 'loop'
                self.parse_loop_stmt(start_span, None)
            }
            TokenKind::For => {
                let start_span = token.span;
                self.advance(); // consume 'for'
                self.parse_for_stmt(start_span, None)
            }
            TokenKind::Break => {
                let span = token.span;
                self.advance(); // consume 'break'
                self.parse_break_stmt(span)
            }
            TokenKind::Continue => {
                let span = token.span;
                self.advance(); // consume 'continue'
                let label = self.parse_optional_loop_label();
                Ok(Stmt::Continue { label, span })
            }
            TokenKind::Identifier(_) => {
                // A loop label (`outer: for ...`) is the only statement form
                // where an identifier is immediately followed by a colon, so it is
                // unambiguous to dispatch on `ident : <loop-keyword>` here.
                if let Some(stmt) = self.try_parse_labeled_loop()? {
                    return Ok(stmt);
                }

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
                // Array element assignment `arr[i] = v`: the parsed expression
                // is an index whose object is a bare binding, followed by `=`.
                if let Expr::Index { object, index, .. } = &expr {
                    if matches!(object.as_ref(), Expr::Identifier(_))
                        && self.check(&TokenKind::Equal)
                    {
                        let Expr::Identifier(target) = object.as_ref().clone() else {
                            unreachable!("guarded by the matches! above")
                        };
                        let index = (**index).clone();
                        self.advance(); // consume '='
                        self.skip_newlines();
                        let value = self.parse_expr(Precedence::Lowest)?;
                        let span = target.span.merge(value.span());
                        return Ok(Stmt::IndexAssignment {
                            target,
                            index,
                            value,
                            span,
                        });
                    }
                }
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
            // mutable reference (`*r = value`) or a deref expression statement.
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

    /// Parse one source statement and append the resulting AST statement(s) to
    /// `out`. Most statements append exactly one node, but a tuple-destructuring
    /// bind `val (a, b) = e` desugars to several — a temp binding plus one
    /// projection per leaf — so it is spliced in here rather than forcing the
    /// single-`Stmt` shape of [`Parser::parse_stmt`].
    pub(crate) fn parse_stmt_into(&mut self, out: &mut Vec<Stmt>) -> ParseResult<()> {
        self.skip_newlines();
        if matches!(self.peek_kind(), Some(TokenKind::Val | TokenKind::Mut)) {
            let mutable = matches!(self.peek_kind(), Some(TokenKind::Mut));
            // A tuple `(`, array `[`, or struct `Name {` pattern after the keyword is a
            // destructuring bind; anything else is an ordinary variable
            // declaration (`val name`, `val name: T`).
            if self.starts_destructure_pattern() {
                let kw = self.advance().ok_or(ParseError::UnexpectedEof {
                    expected: "'val' or 'mut'".to_string(),
                })?;
                let start_span = kw.span;
                self.skip_newlines();
                return self.parse_destructure_bind(mutable, start_span, out);
            }
        }
        out.push(self.parse_stmt()?);
        Ok(())
    }

    /// Parse a block of statements (within braces)
    pub(crate) fn parse_block(&mut self) -> ParseResult<Vec<Stmt>> {
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut statements = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            self.parse_stmt_into(&mut statements)?;
            self.skip_newlines();
        }

        self.consume(TokenKind::RightBrace, "'}'")?;

        Ok(statements)
    }
}

/// Extract the span from a statement — shared by span-calculation in block-ending logic.
pub(crate) fn stmt_span(stmt: &Stmt) -> shared_types::Span {
    match stmt {
        Stmt::VarDecl { span, .. } => *span,
        Stmt::ValElse { span, .. } => *span,
        Stmt::Const { span, .. } => *span,
        Stmt::Assignment { span, .. } => *span,
        Stmt::Return { span, .. } => *span,
        Stmt::If { span, .. } => *span,
        Stmt::While { span, .. } => *span,
        Stmt::ForRange { span, .. } => *span,
        Stmt::ForEach { span, .. } => *span,
        Stmt::Break { span, .. } => *span,
        Stmt::Continue { span, .. } => *span,
        Stmt::FieldAssignment { span, .. } => *span,
        Stmt::IndexAssignment { span, .. } => *span,
        Stmt::DerefAssignment { span, .. } => *span,
        Stmt::Expr(e) => e.span(),
    }
}
