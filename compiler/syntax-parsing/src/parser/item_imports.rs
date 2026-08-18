// Parsing for `import` declarations.

use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::{ImportDef, ImportName, ImportSelection};
use crate::errors::{ParseError, ParseResult};

use super::Parser;

impl Parser {
    /// Parse one `import` declaration.
    ///
    /// Five surface forms share a single shape — a path, then an optional selection:
    /// `import math`, `import ./utils::io`, `import math::{sqrt, sin}`,
    /// `import math::matrix as mat`, `import Option::{Some, None}`. What each path
    /// segment *names* — a module file, an item inside one, an enum — is a question
    /// about the file system, so the parser records the syntax and module resolution
    /// answers it.
    pub(crate) fn parse_import(&mut self) -> ParseResult<ImportDef> {
        let start = self.consume(TokenKind::Import, "'import'")?;
        self.skip_newlines();

        // `./utils` — a leading `./` marks the path explicitly relative to this file.
        let relative = self.check(&TokenKind::Dot);
        if relative {
            self.advance(); // consume '.'
            self.consume(TokenKind::Slash, "'/' after '.' in a relative import path")?;
        }

        let mut path = vec![self.consume_identifier("module name after 'import'")?];
        let mut end_span = path[0].span;
        let mut selection = ImportSelection::Module;

        while self.check(&TokenKind::ColonColon) {
            self.advance(); // consume '::'
            if self.check(&TokenKind::LeftBrace) {
                let (names, span) = self.parse_import_list()?;
                selection = ImportSelection::List(names);
                end_span = span;
                break;
            }
            let segment = self.consume_identifier("name after '::' in an import path")?;
            end_span = segment.span;
            path.push(segment);
        }

        if matches!(selection, ImportSelection::Module) && self.check_import_as() {
            let alias = self.parse_import_alias()?;
            end_span = alias.span;
            selection = ImportSelection::Alias(alias);
        }

        Ok(ImportDef {
            relative,
            path,
            selection,
            span: start.span.merge(end_span),
        })
    }

    /// Parse the `{a, b as c}` tail of an import, returning the entries and the closing
    /// brace's span. The `{` is the current token.
    fn parse_import_list(&mut self) -> ParseResult<(Vec<ImportName>, shared_types::Span)> {
        self.advance(); // consume '{'
        self.skip_newlines();

        let mut names = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            let name = self.consume_identifier("name inside an import list")?;
            let mut span = name.span;
            let alias = if self.check_import_as() {
                let alias = self.parse_import_alias()?;
                span = span.merge(alias.span);
                Some(alias)
            } else {
                None
            };
            names.push(ImportName { name, alias, span });

            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume ','
            self.skip_newlines();
        }

        let close = self.consume(TokenKind::RightBrace, "'}' to close the import list")?;
        if names.is_empty() {
            return Err(ParseError::UnexpectedToken {
                found: TokenKind::RightBrace,
                expected: "at least one name inside an import list".to_string(),
                span: close.span,
            });
        }
        Ok((names, close.span))
    }

    /// `as` doubles as the cast operator, so an import only reads it as a rename marker
    /// when a name follows it.
    fn check_import_as(&self) -> bool {
        if !self.check(&TokenKind::As) {
            return false;
        }
        matches!(
            self.tokens.get(self.current + 1).map(|t| &t.kind),
            Some(TokenKind::Identifier(_))
        )
    }

    fn parse_import_alias(&mut self) -> ParseResult<Identifier> {
        self.advance(); // consume 'as'
        self.consume_identifier("name after 'as' in an import")
    }
}
