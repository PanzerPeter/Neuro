// Destructuring `val`/`mut` binds: pattern parsing and the expansion to plain
// variable declarations that reaches the AST.
//
// One of the statement-shape parsers; each adds methods to the same
// `impl Parser` block.

use lexical_analysis::TokenKind;
use shared_types::{Identifier, Literal, Span};

use crate::ast::{Expr, Stmt};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::statements::{ArrayPatternElem, DestructurePattern};
use super::Parser;

impl Parser {
    /// Whether the tokens after the current `val`/`mut` keyword open a destructuring
    /// pattern: `(` (tuple), `[` (array), or `Name {` (struct). A bare name followed
    /// by `:` or `=` is an ordinary variable declaration.
    pub(super) fn starts_destructure_pattern(&self) -> bool {
        let (first, second) = self.peek_two_after_keyword();
        match first {
            Some(TokenKind::LeftParen | TokenKind::LeftBracket) => true,
            Some(TokenKind::Identifier(_)) => matches!(second, Some(TokenKind::LeftBrace)),
            _ => false,
        }
    }

    /// The first two non-newline token kinds *after* the current `val`/`mut` keyword,
    /// used to detect a destructuring pattern without consuming input.
    pub(super) fn peek_two_after_keyword(&self) -> (Option<TokenKind>, Option<TokenKind>) {
        let mut i = self.current + 1;
        let next_non_newline = |start: usize| {
            let mut j = start;
            while matches!(
                self.tokens.get(j).map(|t| &t.kind),
                Some(TokenKind::Newline)
            ) {
                j += 1;
            }
            j
        };
        i = next_non_newline(i);
        let first = self.tokens.get(i).map(|t| t.kind.clone());
        let j = next_non_newline(i + 1);
        let second = self.tokens.get(j).map(|t| t.kind.clone());
        (first, second)
    }

    /// Desugar a destructuring bind `val PATTERN = expr`, where `PATTERN` is a
    /// tuple, array, or struct pattern. The cursor sits on the pattern's opening
    /// token. The right-hand side is bound once to a fresh immutable temporary, then
    /// each pattern leaf is bound to a projection of that temporary — so the only new
    /// AST node any pattern needs is the array-rest remainder ([`Expr::ArrayRest`]).
    pub(super) fn parse_destructure_bind(
        &mut self,
        mutable: bool,
        start_span: Span,
        out: &mut Vec<Stmt>,
    ) -> ParseResult<()> {
        let pattern = self.parse_top_pattern()?;
        self.skip_newlines();
        self.consume(TokenKind::Equal, "'=' in destructuring binding")?;
        self.skip_newlines();
        let init = self.parse_expr(Precedence::Lowest)?;
        let init_span = init.span();

        let tmp = Identifier {
            name: format!("__destructure_{}", self.next_destructure_id()),
            span: start_span,
        };
        out.push(Stmt::VarDecl {
            name: tmp.clone(),
            ty: None,
            init: Some(init),
            mutable: false,
            span: start_span.merge(init_span),
        });
        self.expand_pattern(&pattern, Expr::Identifier(tmp), mutable, start_span, out);
        Ok(())
    }

    /// Parse the top-level destructuring pattern: a tuple `(`, array `[`, or struct
    /// `Name {`. A bare-name pattern is never a top-level destructure (it is an
    /// ordinary `val name = ...`), so it is rejected here.
    pub(super) fn parse_top_pattern(&mut self) -> ParseResult<DestructurePattern> {
        match self.peek_kind() {
            Some(TokenKind::LeftParen) => self.parse_tuple_pattern(),
            Some(TokenKind::LeftBracket) => self.parse_array_pattern(),
            Some(TokenKind::Identifier(_)) => self.parse_struct_pattern(),
            _ => {
                let (found, span) = self
                    .peek()
                    .map(|t| (t.kind.clone(), t.span))
                    .unwrap_or((TokenKind::Eof, Span::new(0, 0)));
                Err(ParseError::UnexpectedToken {
                    found,
                    expected: "a tuple `(`, array `[`, or struct `Name {` destructuring pattern"
                        .to_string(),
                    span,
                })
            }
        }
    }

    /// Allocate a unique id for a destructuring temporary.
    pub(super) fn next_destructure_id(&mut self) -> usize {
        let id = self.destructure_counter;
        self.destructure_counter += 1;
        id
    }

    /// Parse a parenthesized tuple pattern `(p0, p1, ...)`. Requires at least two
    /// elements — a single `(p)` is not a tuple. The cursor sits on the `(`.
    pub(super) fn parse_tuple_pattern(&mut self) -> ParseResult<DestructurePattern> {
        let open = self.consume(TokenKind::LeftParen, "'(' to open destructuring pattern")?;
        let mut subs = Vec::new();
        loop {
            self.skip_newlines();
            subs.push(self.parse_pattern_element()?);
            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume ','
            self.skip_newlines();
            if self.check(&TokenKind::RightParen) {
                break; // trailing comma
            }
        }
        let close = self.consume(TokenKind::RightParen, "')' to close destructuring pattern")?;
        if subs.len() < 2 {
            return Err(ParseError::UnexpectedToken {
                found: TokenKind::RightParen,
                expected: "a tuple pattern with at least two elements `(a, b, ...)`".to_string(),
                span: open.span.merge(close.span),
            });
        }
        Ok(DestructurePattern::Tuple(subs))
    }

    /// Parse a struct pattern `Name { field, field, ... }`. Each field is a
    /// shorthand binding: `Point { x, y }` binds `x` and `y` to the matching fields.
    /// The cursor sits on the type-name identifier.
    pub(super) fn parse_struct_pattern(&mut self) -> ParseResult<DestructurePattern> {
        let name_token = self.consume(TokenKind::Identifier(String::new()), "struct type name")?;
        if !matches!(name_token.kind, TokenKind::Identifier(_)) {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "struct type name".to_string(),
                span: name_token.span,
            });
        }
        self.consume(TokenKind::LeftBrace, "'{' to open struct pattern")?;
        let mut fields = Vec::new();
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::RightBrace) {
                break;
            }
            let field = self.consume(TokenKind::Identifier(String::new()), "struct field name")?;
            let TokenKind::Identifier(field_name) = field.kind else {
                return Err(ParseError::UnexpectedToken {
                    found: field.kind,
                    expected: "struct field name".to_string(),
                    span: field.span,
                });
            };
            fields.push(Identifier {
                name: field_name,
                span: field.span,
            });
            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume ','
        }
        let close = self.consume(TokenKind::RightBrace, "'}' to close struct pattern")?;
        if fields.is_empty() {
            return Err(ParseError::UnexpectedToken {
                found: TokenKind::RightBrace,
                expected: "at least one field in a struct pattern `Name { field, ... }`"
                    .to_string(),
                span: name_token.span.merge(close.span),
            });
        }
        Ok(DestructurePattern::Struct { fields })
    }

    /// Parse an array pattern `[p0, p1, ..rest]`. Elements bind positionally;
    /// an optional trailing rest `..` discards or `..name` captures the remainder. At
    /// most one rest is allowed and it must come last. The cursor sits on the `[`.
    pub(super) fn parse_array_pattern(&mut self) -> ParseResult<DestructurePattern> {
        let open = self.consume(TokenKind::LeftBracket, "'[' to open array pattern")?;
        let mut elems = Vec::new();
        let mut seen_rest = false;
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::RightBracket) {
                break;
            }
            if self.check(&TokenKind::DotDot) {
                let dotdot = self.advance().ok_or(ParseError::UnexpectedEof {
                    expected: "'..' rest pattern".to_string(),
                })?;
                if seen_rest {
                    return Err(ParseError::UnexpectedToken {
                        found: TokenKind::DotDot,
                        expected: "at most one `..` rest pattern in an array pattern".to_string(),
                        span: dotdot.span,
                    });
                }
                seen_rest = true;
                // An optional name binds the remainder; bare `..` discards it.
                let name = if let Some(TokenKind::Identifier(_)) = self.peek_kind() {
                    let tok = self.advance().ok_or(ParseError::UnexpectedEof {
                        expected: "rest binding name".to_string(),
                    })?;
                    let TokenKind::Identifier(n) = tok.kind else {
                        unreachable!("peeked an identifier")
                    };
                    Some(Identifier {
                        name: n,
                        span: tok.span,
                    })
                } else {
                    None
                };
                elems.push(ArrayPatternElem::Rest(name));
            } else {
                if seen_rest {
                    return Err(ParseError::UnexpectedToken {
                        found: self
                            .peek()
                            .map(|t| t.kind.clone())
                            .unwrap_or(TokenKind::Eof),
                        expected: "no elements after a `..` rest pattern".to_string(),
                        span: self.peek().map(|t| t.span).unwrap_or(open.span),
                    });
                }
                elems.push(ArrayPatternElem::Pattern(self.parse_pattern_element()?));
            }
            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume ','
        }
        self.consume(TokenKind::RightBracket, "']' to close array pattern")?;
        Ok(DestructurePattern::Array(elems))
    }

    /// Parse one element of a destructuring pattern: a nested tuple/array/struct
    /// pattern, the `_` wildcard, or a binding name.
    pub(super) fn parse_pattern_element(&mut self) -> ParseResult<DestructurePattern> {
        if self.check(&TokenKind::LeftParen) {
            return self.parse_tuple_pattern();
        }
        if self.check(&TokenKind::LeftBracket) {
            return self.parse_array_pattern();
        }
        // `Name {` opens a nested struct pattern; a bare name is a binding.
        if matches!(self.peek_kind(), Some(TokenKind::Identifier(_)))
            && matches!(self.peek_second_kind(), Some(TokenKind::LeftBrace))
        {
            return self.parse_struct_pattern();
        }
        let token = self.consume(TokenKind::Identifier(String::new()), "binding name or `_`")?;
        let TokenKind::Identifier(name) = token.kind else {
            return Err(ParseError::UnexpectedToken {
                found: token.kind,
                expected: "binding name or `_`".to_string(),
                span: token.span,
            });
        };
        if name == "_" {
            Ok(DestructurePattern::Wildcard)
        } else {
            Ok(DestructurePattern::Bind(Identifier {
                name,
                span: token.span,
            }))
        }
    }

    /// The kind of the token immediately after the current one, skipping newlines.
    pub(super) fn peek_second_kind(&self) -> Option<TokenKind> {
        let mut i = self.current + 1;
        while matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            i += 1;
        }
        self.tokens.get(i).map(|t| t.kind.clone())
    }

    /// Emit the variable declarations a destructuring pattern expands to. `access` is
    /// the expression that reaches the value matched by `pattern` (the temporary for
    /// the whole tuple, or a nested `.N` projection). A wildcard binds nothing.
    pub(super) fn expand_pattern(
        &mut self,
        pattern: &DestructurePattern,
        access: Expr,
        mutable: bool,
        span: Span,
        out: &mut Vec<Stmt>,
    ) {
        match pattern {
            DestructurePattern::Wildcard => {}
            DestructurePattern::Bind(name) => out.push(Stmt::VarDecl {
                name: name.clone(),
                ty: None,
                init: Some(access),
                mutable,
                span,
            }),
            DestructurePattern::Tuple(subs) => {
                for (i, sub) in subs.iter().enumerate() {
                    let elem = Expr::TupleIndex {
                        object: Box::new(access.clone()),
                        index: i,
                        span,
                    };
                    self.expand_pattern(sub, elem, mutable, span, out);
                }
            }
            DestructurePattern::Struct { fields } => {
                for field in fields {
                    let access_field = Expr::FieldAccess {
                        object: Box::new(access.clone()),
                        field: field.clone(),
                        span,
                    };
                    out.push(Stmt::VarDecl {
                        name: field.clone(),
                        ty: None,
                        init: Some(access_field),
                        mutable,
                        span,
                    });
                }
            }
            DestructurePattern::Array(elems) => {
                // Count the leading positional patterns (everything before the rest).
                let lead = elems
                    .iter()
                    .take_while(|e| matches!(e, ArrayPatternElem::Pattern(_)))
                    .count();
                for (i, elem) in elems.iter().enumerate() {
                    match elem {
                        ArrayPatternElem::Pattern(sub) => {
                            let index = Expr::Literal(Literal::Integer(i as i64, None), span);
                            let access_i = Expr::Index {
                                object: Box::new(access.clone()),
                                index: Box::new(index),
                                span,
                            };
                            self.expand_pattern(sub, access_i, mutable, span, out);
                        }
                        ArrayPatternElem::Rest(name) => {
                            let rest = Expr::ArrayRest {
                                array: Box::new(access.clone()),
                                start: lead,
                                exact: false,
                                span,
                            };
                            match name {
                                // A named rest binds the remainder; a bare `..` keeps
                                // the node only as a `start <= N` bounds assertion.
                                Some(n) => out.push(Stmt::VarDecl {
                                    name: n.clone(),
                                    ty: None,
                                    init: Some(rest),
                                    mutable,
                                    span,
                                }),
                                None => out.push(Stmt::Expr(rest)),
                            }
                        }
                    }
                }
                // With no rest, the pattern must match the array length exactly; emit a
                // discarded `ArrayRest` whose `exact` flag carries that arity check.
                if !elems.iter().any(|e| matches!(e, ArrayPatternElem::Rest(_))) {
                    out.push(Stmt::Expr(Expr::ArrayRest {
                        array: Box::new(access),
                        start: lead,
                        exact: true,
                        span,
                    }));
                }
            }
        }
    }
}
