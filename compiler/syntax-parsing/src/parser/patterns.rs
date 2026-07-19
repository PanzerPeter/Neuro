// Parsing for `match` expressions and their patterns.

use lexical_analysis::TokenKind;
use shared_types::{Identifier, Literal, Span};

use crate::ast::{EnumPatternPayload, Expr, FieldPattern, MatchArm, Pattern};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;

impl Parser {
    /// Parse a `match` expression. The `match` keyword is already consumed;
    /// `start_span` is its span. The scrutinee is parsed with struct-literals
    /// suppressed so `match x { ... }` reads `x` as the scrutinee, not `x { ... }`.
    pub(super) fn parse_match_expr(&mut self, start_span: Span) -> ParseResult<Expr> {
        self.skip_newlines();
        let saved_no_struct_lit = self.no_struct_lit;
        self.no_struct_lit = true;
        let scrutinee = self.parse_expr(Precedence::Lowest)?;
        self.no_struct_lit = saved_no_struct_lit;
        self.skip_newlines();

        self.consume(TokenKind::LeftBrace, "'{' to open match body")?;
        self.skip_newlines();

        let mut arms = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            arms.push(self.parse_match_arm()?);
            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }

        let close = self.consume(TokenKind::RightBrace, "'}' to close match body")?;
        Ok(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: start_span.merge(close.span),
        })
    }

    /// Parse one match arm: `pattern ('|' pattern)* ('if' guard)? '=>' body`.
    fn parse_match_arm(&mut self) -> ParseResult<MatchArm> {
        let first = self.parse_pattern()?;
        let start_span = first.span();
        let mut patterns = vec![first];
        while self.check(&TokenKind::Pipe) {
            self.advance(); // consume '|'
            self.skip_newlines();
            patterns.push(self.parse_pattern()?);
        }

        // Optional guard. Struct literals are suppressed inside the guard so a
        // trailing brace cannot be read as one; `=>` is not an operator and always
        // ends the guard expression.
        let guard = if self.check(&TokenKind::If) {
            self.advance(); // consume 'if'
            self.skip_newlines();
            let saved_no_struct_lit = self.no_struct_lit;
            self.no_struct_lit = true;
            let guard = self.parse_expr(Precedence::Lowest)?;
            self.no_struct_lit = saved_no_struct_lit;
            Some(Box::new(guard))
        } else {
            None
        };

        self.skip_newlines();
        self.consume(TokenKind::FatArrow, "'=>' after match pattern")?;
        self.skip_newlines();
        let body = self.parse_expr(Precedence::Lowest)?;
        let span = start_span.merge(body.span());

        Ok(MatchArm {
            patterns,
            guard,
            body: Box::new(body),
            span,
        })
    }

    /// Parse a single pattern: a wildcard, binding, literal, range, or enum
    /// variant pattern.
    fn parse_pattern(&mut self) -> ParseResult<Pattern> {
        self.skip_newlines();
        let token = self.peek().ok_or(ParseError::UnexpectedEof {
            expected: "pattern".to_string(),
        })?;

        match &token.kind {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                let span = token.span;
                self.advance();
                if name == "_" {
                    return Ok(Pattern::Wildcard(span));
                }
                if self.check(&TokenKind::ColonColon) {
                    let enum_name = Identifier { name, span };
                    return self.parse_enum_pattern(enum_name);
                }
                Ok(Pattern::Binding(Identifier { name, span }))
            }
            _ => {
                // A literal-headed pattern: a bare literal, or the start of a range.
                let (start_lit, start_span) = self.parse_pattern_literal()?;
                if self.check(&TokenKind::DotDot) || self.check(&TokenKind::DotDotEqual) {
                    let inclusive = self.check(&TokenKind::DotDotEqual);
                    self.advance(); // consume '..' / '..='
                    let (end_lit, end_span) = self.parse_pattern_literal()?;
                    return Ok(Pattern::Range {
                        start: start_lit,
                        end: end_lit,
                        inclusive,
                        span: start_span.merge(end_span),
                    });
                }
                Ok(Pattern::Literal(start_lit, start_span))
            }
        }
    }

    /// Parse the `::Variant payload?` tail of an enum pattern; `enum_name` and the
    /// following `::` are already positioned (the `::` not yet consumed).
    fn parse_enum_pattern(&mut self, enum_name: Identifier) -> ParseResult<Pattern> {
        self.advance(); // consume '::'
        let variant = self.consume_identifier("enum variant name after '::'")?;

        let (payload, end_span) = if self.check(&TokenKind::LeftParen) {
            self.advance(); // consume '('
            self.skip_newlines();
            let mut subs = Vec::new();
            if !self.check(&TokenKind::RightParen) {
                loop {
                    subs.push(self.parse_pattern()?);
                    self.skip_newlines();
                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance(); // consume ','
                    self.skip_newlines();
                }
            }
            let close = self.consume(TokenKind::RightParen, "')' to close variant pattern")?;
            (EnumPatternPayload::Tuple(subs), close.span)
        } else if self.check(&TokenKind::LeftBrace) {
            self.advance(); // consume '{'
            self.skip_newlines();
            let mut fields = Vec::new();
            if !self.check(&TokenKind::RightBrace) {
                loop {
                    fields.push(self.parse_field_pattern()?);
                    self.skip_newlines();
                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance(); // consume ','
                    self.skip_newlines();
                }
            }
            let close =
                self.consume(TokenKind::RightBrace, "'}' to close struct variant pattern")?;
            (EnumPatternPayload::Struct(fields), close.span)
        } else {
            (EnumPatternPayload::Unit, variant.span)
        };

        Ok(Pattern::Enum {
            span: enum_name.span.merge(end_span),
            enum_name,
            variant,
            payload,
        })
    }

    /// Parse one `field` (shorthand → binds `field`) or `field: sub_pattern` entry of
    /// a struct-variant pattern.
    fn parse_field_pattern(&mut self) -> ParseResult<FieldPattern> {
        let field = self.consume_identifier("field name in struct variant pattern")?;
        let (pattern, span) = if self.check(&TokenKind::Colon) {
            self.advance(); // consume ':'
            self.skip_newlines();
            let pat = self.parse_pattern()?;
            let span = field.span.merge(pat.span());
            (pat, span)
        } else {
            (Pattern::Binding(field.clone()), field.span)
        };
        Ok(FieldPattern {
            field,
            pattern,
            span,
        })
    }

    /// Parse a literal in pattern position, including a leading `-` on a numeric
    /// literal. Returns the literal and its source span.
    fn parse_pattern_literal(&mut self) -> ParseResult<(Literal, Span)> {
        let token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "literal pattern".to_string(),
        })?;

        match token.kind {
            TokenKind::Minus => {
                let num = self.advance().ok_or(ParseError::UnexpectedEof {
                    expected: "number after '-'".to_string(),
                })?;
                let span = token.span.merge(num.span);
                match num.kind {
                    TokenKind::Integer(n) => Ok((Literal::Integer(-n, None), span)),
                    TokenKind::IntegerSuffix(tok) => {
                        Ok((Literal::Integer(-tok.value, Some(tok.suffix)), span))
                    }
                    TokenKind::Float(f) => Ok((Literal::Float(-f, None), span)),
                    TokenKind::FloatSuffix(tok) => {
                        Ok((Literal::Float(-tok.value, Some(tok.suffix)), span))
                    }
                    other => Err(ParseError::UnexpectedToken {
                        found: other,
                        expected: "number after '-' in pattern".to_string(),
                        span: num.span,
                    }),
                }
            }
            TokenKind::Integer(n) => Ok((Literal::Integer(n, None), token.span)),
            TokenKind::IntegerSuffix(tok) => {
                Ok((Literal::Integer(tok.value, Some(tok.suffix)), token.span))
            }
            TokenKind::Float(f) => Ok((Literal::Float(f, None), token.span)),
            TokenKind::FloatSuffix(tok) => {
                Ok((Literal::Float(tok.value, Some(tok.suffix)), token.span))
            }
            TokenKind::Char(c) => Ok((Literal::Char(c), token.span)),
            TokenKind::String(s) => Ok((Literal::String(s), token.span)),
            TokenKind::True => Ok((Literal::Boolean(true), token.span)),
            TokenKind::False => Ok((Literal::Boolean(false), token.span)),
            other => Err(ParseError::UnexpectedToken {
                found: other,
                expected: "a literal or variant pattern".to_string(),
                span: token.span,
            }),
        }
    }
}
