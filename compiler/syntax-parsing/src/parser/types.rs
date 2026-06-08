use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::Type;
use crate::errors::{ParseError, ParseResult};

use super::Parser;

impl Parser {
    /// Parse a type annotation
    pub(crate) fn parse_type(&mut self) -> ParseResult<Type> {
        // Immutable borrow type `&T` (§2.4). The referent is parsed recursively, so
        // the `&` distributes over whatever type follows.
        if self.check(&TokenKind::Amp) {
            let amp = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'&'".to_string(),
            })?;
            let inner = self.parse_type()?;
            let span = amp.span.merge(inner.span());
            return Ok(Type::Reference {
                inner: Box::new(inner),
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
