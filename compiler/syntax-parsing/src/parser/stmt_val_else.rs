// Parsing for the `val PATTERN = value else |binding| { ... }` statement.

use lexical_analysis::TokenKind;
use shared_types::{Identifier, Span};

use crate::ast::Stmt;
use crate::errors::ParseResult;
use crate::precedence::Precedence;

use super::Parser;

impl Parser {
    /// Whether the statement starting at the current `val` keyword is a `val-else`.
    ///
    /// Two markers, both unambiguous without further lookahead, because a binding
    /// name after `val` is only ever followed by `:`, `=`, or a newline:
    ///
    /// - `Name::` — the qualified head of a variant pattern.
    /// - `Name(`  — the unqualified head, which an import (the prelude included)
    ///   brings into scope. Its payload is what settles the reading.
    ///
    /// `Name {` is deliberately absent: it stays a struct destructure. So does a
    /// payload-less `val None = ...`, which cannot be told from a binding here for
    /// the same reason a bare `None` pattern cannot.
    pub(super) fn starts_val_else(&self) -> bool {
        let (first, second) = self.peek_two_after_keyword();
        matches!(first, Some(TokenKind::Identifier(_)))
            && matches!(
                second,
                Some(TokenKind::ColonColon) | Some(TokenKind::LeftParen)
            )
    }

    /// Parse `PATTERN = value else |binding|? { ... }`. The `val` keyword is already
    /// consumed; `start_span` is its span.
    pub(super) fn parse_val_else(&mut self, start_span: Span) -> ParseResult<Stmt> {
        let pattern = self.parse_pattern()?;

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'=' after a `val-else` pattern")?;
        self.skip_newlines();

        // The scrutinee ends at `else`, which is a keyword and never an operator, so
        // it parses at the lowest precedence without a struct-literal guard.
        let value = self.parse_expr(Precedence::Lowest)?;

        self.skip_newlines();
        self.consume(TokenKind::Else, "'else' after a `val-else` scrutinee")?;
        self.skip_newlines();

        let else_binding = self.parse_else_binding()?;
        self.skip_newlines();
        let else_block = self.parse_block()?;

        let span = start_span.merge(value.span());
        Ok(Stmt::ValElse {
            pattern,
            value,
            else_binding,
            else_block,
            span,
        })
    }

    /// Parse the optional `|name|` that may follow `else`. This is a dedicated
    /// `val-else` production, not a closure literal — the block that follows is the
    /// else branch, not a closure body.
    fn parse_else_binding(&mut self) -> ParseResult<Option<Identifier>> {
        if !self.check(&TokenKind::Pipe) {
            return Ok(None);
        }
        self.advance(); // consume '|'
        self.skip_newlines();
        let name = self.consume_identifier("a binding name after `else |`")?;
        self.skip_newlines();
        self.consume(TokenKind::Pipe, "'|' to close the `else` binding")?;
        Ok(Some(name))
    }
}
