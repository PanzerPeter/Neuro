// NEURO Programming Language - Syntax Parsing
// Parser implementation using Pratt parsing for expressions

use lexical_analysis::{Token, TokenKind};

use crate::errors::{ParseError, ParseResult};

mod expressions;
mod items;
mod statements;
mod type_aliases;
mod types;

/// Parser for NEURO source code
pub(crate) struct Parser {
    pub(super) tokens: Vec<Token>,
    pub(super) current: usize,
    pub(super) expr_depth: usize,
    /// When true, an identifier followed by `{` is NOT parsed as a struct literal.
    /// Set to true inside if/while/for conditions to prevent consuming the block's `{`.
    pub(super) no_struct_lit: bool,
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
    pub(super) fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.current)
    }

    /// Get the current token kind
    pub(super) fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|t| &t.kind)
    }

    /// Check if we're at the end of the token stream
    pub(super) fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Eof) | None)
    }

    /// Consume and return the current token
    pub(super) fn advance(&mut self) -> Option<Token> {
        if !self.is_at_end() {
            let token = self.tokens.get(self.current).cloned();
            self.current += 1;
            token
        } else {
            None
        }
    }

    /// Check if the current token matches the given kind
    pub(super) fn check(&self, kind: &TokenKind) -> bool {
        if let Some(current_kind) = self.peek_kind() {
            std::mem::discriminant(current_kind) == std::mem::discriminant(kind)
        } else {
            false
        }
    }

    /// Consume the current token if it matches the expected kind
    pub(super) fn consume(&mut self, expected: TokenKind, message: &str) -> ParseResult<Token> {
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
    pub(super) fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), Some(TokenKind::Newline)) {
            self.advance();
        }
    }
}
