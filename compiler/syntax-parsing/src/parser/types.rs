use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::Type;
use crate::errors::{ParseError, ParseResult};

use super::Parser;

impl Parser {
    /// Parse a type annotation
    pub(crate) fn parse_type(&mut self) -> ParseResult<Type> {
        // Borrow type `&T` (§2.4) / `&mut T` (§2.5). The referent is parsed
        // recursively, so the `&` distributes over whatever type follows. A `mut`
        // keyword immediately after `&` marks a mutable borrow.
        if self.check(&TokenKind::Amp) {
            let amp = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'&'".to_string(),
            })?;
            let mutable = self.check(&TokenKind::Mut);
            if mutable {
                self.advance(); // consume 'mut'
            }
            let inner = self.parse_type()?;
            let span = amp.span.merge(inner.span());
            return Ok(Type::Reference {
                inner: Box::new(inner),
                mutable,
                span,
            });
        }

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
}
