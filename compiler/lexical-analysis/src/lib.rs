// NEURO Programming Language - Lexical Analysis
// Feature slice for tokenization and lexical processing
//
// This slice follows Vertical Slice Architecture (VSA) principles:
// - Self-contained tokenization functionality
// - Minimal dependencies (only infrastructure)
// - Clear module boundaries with pub(crate) for internals
// - Public API: tokenize() and Lexer struct

mod errors;
mod tokens;

// Public exports
pub use errors::{LexError, LexResult};
pub use tokens::{Token, TokenKind};

use logos::Logos;
use shared_types::Span;

/// Lexer for the NEURO language
pub struct Lexer<'source> {
    source: &'source str,
    inner: logos::Lexer<'source, TokenKind>,
}

impl<'source> Lexer<'source> {
    /// Create a new lexer for the given source code
    pub fn new(source: &'source str) -> Self {
        Self {
            source,
            inner: TokenKind::lexer(source),
        }
    }

    fn classify_error(&self, err: LexError, span: Span) -> LexError {
        match err {
            LexError::UnexpectedChar { character: '\0', .. } => {
                let start = span.start;
                if let Some(remaining) = self.source.get(start..) {
                    if remaining.starts_with('"') {
                        let end = remaining
                            .find('\n')
                            .map(|offset| start + offset)
                            .unwrap_or(self.source.len());

                        return LexError::UnterminatedString {
                            span: Span::new(start, end),
                        };
                    }
                }

                let character = self
                    .inner
                    .slice()
                    .chars()
                    .next()
                    .unwrap_or('\0');
                LexError::UnexpectedChar { character, span }
            }
            other => other,
        }
    }

    /// Check if a string is a valid identifier
    ///
    /// Follows Unicode XID standard: first character must be XID_Start or underscore,
    /// remaining characters must be XID_Continue.
    pub fn is_valid_identifier(s: &str) -> bool {
        let mut chars = s.chars();

        // Check first character
        let Some(first) = chars.next() else {
            return false;
        };

        if first != '_' && !unicode_ident::is_xid_start(first) {
            return false;
        }

        // Check remaining characters
        chars.all(unicode_ident::is_xid_continue)
    }
}

impl<'source> Iterator for Lexer<'source> {
    type Item = LexResult<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        let kind = self.inner.next()?;
        let span = Span::new(self.inner.span().start, self.inner.span().end);

        Some(match kind {
            Ok(kind) => Ok(Token::new(kind, span)),
            Err(err) => Err(self.classify_error(err, span)),
        })
    }
}

/// Convenience function to tokenize NEURO source code
///
/// This is the main entry point for lexical analysis. It takes NEURO source code
/// and produces a stream of tokens. Returns early on the first lexical error.
///
/// # Arguments
///
/// * `source` - The NEURO source code as a string
///
/// # Returns
///
/// * `Ok(Vec<Token>)` - Successfully tokenized source (includes EOF token)
/// * `Err(LexError)` - First lexical error encountered (invalid character, unterminated string, etc.)
///
/// # Examples
///
/// ```ignore
/// use lexical_analysis::tokenize;
///
/// let source = "func add(a: i32, b: i32) -> i32 { return a + b }";
/// let tokens = tokenize(source)?;
/// ```
pub fn tokenize(source: &str) -> LexResult<Vec<Token>> {
    let lexer = Lexer::new(source);
    let mut tokens = Vec::new();

    // Collect tokens, returning early on first error
    for result in lexer {
        tokens.push(result?);
    }

    // Add EOF token
    let eof_span = Span::new(source.len(), source.len());
    tokens.push(Token::new(TokenKind::Eof, eof_span));

    Ok(tokens)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_empty_source() {
        let result = tokenize("").unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].kind, TokenKind::Eof));
    }

    #[test]
    fn tokenize_keywords() {
        let result = tokenize("func val mut if else return").unwrap();
        assert_eq!(result.len(), 7); // 6 keywords + EOF

        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Val));
        assert!(matches!(result[2].kind, TokenKind::Mut));
        assert!(matches!(result[3].kind, TokenKind::If));
        assert!(matches!(result[4].kind, TokenKind::Else));
        assert!(matches!(result[5].kind, TokenKind::Return));
    }

    #[test]
    fn tokenize_identifiers() {
        let result = tokenize("foo bar_baz _underscore").unwrap();
        assert_eq!(result.len(), 4); // 3 identifiers + EOF

        match &result[0].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "foo"),
            _ => panic!("Expected identifier"),
        }
        match &result[1].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "bar_baz"),
            _ => panic!("Expected identifier"),
        }
        match &result[2].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "_underscore"),
            _ => panic!("Expected identifier"),
        }
    }

    #[test]
    fn tokenize_unicode_identifiers() {
        let result = tokenize("αβγ 変数 identifier_with_数字").unwrap();
        assert_eq!(result.len(), 4); // 3 identifiers + EOF

        match &result[0].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "αβγ"),
            _ => panic!("Expected identifier"),
        }
        match &result[1].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "変数"),
            _ => panic!("Expected identifier"),
        }
        match &result[2].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "identifier_with_数字"),
            _ => panic!("Expected identifier"),
        }
    }

    #[test]
    fn tokenize_integers() {
        let result = tokenize("42 0 1234567890 100_000").unwrap();
        assert_eq!(result.len(), 5); // 4 integers + EOF

        assert!(matches!(result[0].kind, TokenKind::Integer(42)));
        assert!(matches!(result[1].kind, TokenKind::Integer(0)));
        assert!(matches!(result[2].kind, TokenKind::Integer(1234567890)));
        assert!(matches!(result[3].kind, TokenKind::Integer(100000)));
    }

    #[test]
    fn tokenize_integer_bases() {
        let result = tokenize("0b1010 0o755 0xDEADBEEF").unwrap();
        assert_eq!(result.len(), 4); // 3 integers + EOF

        assert!(matches!(result[0].kind, TokenKind::Integer(0b1010)));
        assert!(matches!(result[1].kind, TokenKind::Integer(0o755)));
        assert!(matches!(result[2].kind, TokenKind::Integer(0xDEADBEEF)));
    }

    #[test]
    fn tokenize_floats() {
        let result = tokenize("3.15 0.5 2.0 1e10 1.5e-5").unwrap();
        assert_eq!(result.len(), 6); // 5 floats + EOF

        match result[0].kind {
            TokenKind::Float(f) => assert!((f - 3.15).abs() < 1e-10),
            _ => panic!("Expected float"),
        }
        match result[1].kind {
            TokenKind::Float(f) => assert!((f - 0.5).abs() < 1e-10),
            _ => panic!("Expected float"),
        }
        match result[2].kind {
            TokenKind::Float(f) => assert!((f - 2.0).abs() < 1e-10),
            _ => panic!("Expected float"),
        }
        match result[3].kind {
            TokenKind::Float(f) => assert!((f - 1e10).abs() < 1e-10),
            _ => panic!("Expected float"),
        }
        match result[4].kind {
            TokenKind::Float(f) => assert!((f - 1.5e-5).abs() < 1e-10),
            _ => panic!("Expected float"),
        }
    }

    #[test]
    fn tokenize_strings() {
        let result = tokenize(r#""hello" "world" "with spaces""#).unwrap();
        assert_eq!(result.len(), 4); // 3 strings + EOF

        match &result[0].kind {
            TokenKind::String(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected string"),
        }
        match &result[1].kind {
            TokenKind::String(s) => assert_eq!(s, "world"),
            _ => panic!("Expected string"),
        }
        match &result[2].kind {
            TokenKind::String(s) => assert_eq!(s, "with spaces"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn tokenize_string_escapes() {
        let result = tokenize(r#""hello\nworld" "tab\there" "quote\"here""#).unwrap();

        match &result[0].kind {
            TokenKind::String(s) => assert_eq!(s, "hello\nworld"),
            _ => panic!("Expected string"),
        }
        match &result[1].kind {
            TokenKind::String(s) => assert_eq!(s, "tab\there"),
            _ => panic!("Expected string"),
        }
        match &result[2].kind {
            TokenKind::String(s) => assert_eq!(s, "quote\"here"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn tokenize_string_unicode_escape() {
        let result = tokenize(r#""unicode: \u{1F600}""#).unwrap();
        match &result[0].kind {
            TokenKind::String(s) => assert_eq!(s, "unicode: 😀"),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn tokenize_operators() {
        let result = tokenize("+ - * / % = == != < > <= >=").unwrap();
        assert_eq!(result.len(), 13); // 12 operators + EOF

        assert!(matches!(result[0].kind, TokenKind::Plus));
        assert!(matches!(result[1].kind, TokenKind::Minus));
        assert!(matches!(result[2].kind, TokenKind::Star));
        assert!(matches!(result[3].kind, TokenKind::Slash));
        assert!(matches!(result[4].kind, TokenKind::Percent));
        assert!(matches!(result[5].kind, TokenKind::Equal));
        assert!(matches!(result[6].kind, TokenKind::EqualEqual));
        assert!(matches!(result[7].kind, TokenKind::NotEqual));
        assert!(matches!(result[8].kind, TokenKind::Less));
        assert!(matches!(result[9].kind, TokenKind::Greater));
        assert!(matches!(result[10].kind, TokenKind::LessEqual));
        assert!(matches!(result[11].kind, TokenKind::GreaterEqual));
    }

    #[test]
    fn tokenize_logical_operators() {
        let result = tokenize("&& || !").unwrap();
        assert_eq!(result.len(), 4); // 3 operators + EOF

        assert!(matches!(result[0].kind, TokenKind::AmpAmp));
        assert!(matches!(result[1].kind, TokenKind::PipePipe));
        assert!(matches!(result[2].kind, TokenKind::Bang));
    }

    #[test]
    fn tokenize_delimiters() {
        let result = tokenize("( ) { } [ ] , : ;").unwrap();
        assert_eq!(result.len(), 10); // 9 delimiters + EOF

        assert!(matches!(result[0].kind, TokenKind::LeftParen));
        assert!(matches!(result[1].kind, TokenKind::RightParen));
        assert!(matches!(result[2].kind, TokenKind::LeftBrace));
        assert!(matches!(result[3].kind, TokenKind::RightBrace));
        assert!(matches!(result[4].kind, TokenKind::LeftBracket));
        assert!(matches!(result[5].kind, TokenKind::RightBracket));
        assert!(matches!(result[6].kind, TokenKind::Comma));
        assert!(matches!(result[7].kind, TokenKind::Colon));
        assert!(matches!(result[8].kind, TokenKind::Semicolon));
    }

    #[test]
    fn tokenize_special_operators() {
        let result = tokenize("-> @ :: . .. ..=").unwrap();
        assert_eq!(result.len(), 7); // 6 operators + EOF

        assert!(matches!(result[0].kind, TokenKind::Arrow));
        assert!(matches!(result[1].kind, TokenKind::At));
        assert!(matches!(result[2].kind, TokenKind::ColonColon));
        assert!(matches!(result[3].kind, TokenKind::Dot));
        assert!(matches!(result[4].kind, TokenKind::DotDot));
        assert!(matches!(result[5].kind, TokenKind::DotDotEqual));
    }

    #[test]
    fn tokenize_line_comments() {
        let result = tokenize("func // this is a comment\nval").unwrap();
        assert_eq!(result.len(), 4); // func, newline, val, EOF

        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Newline));
        assert!(matches!(result[2].kind, TokenKind::Val));
    }

    #[test]
    fn tokenize_block_comments() {
        let result = tokenize("func /* block comment */ val").unwrap();
        assert_eq!(result.len(), 3); // func, val, EOF

        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Val));
    }

    #[test]
    fn tokenize_multiline_block_comments() {
        let result = tokenize("func /*\nmulti\nline\ncomment\n*/ val").unwrap();
        assert_eq!(result.len(), 3); // func, val, EOF

        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Val));
    }

    #[test]
    fn tokenize_simple_function() {
        let source = r#"
func add(a: i32, b: i32) -> i32 {
    return a + b
}
"#;
        let result = tokenize(source).unwrap();

        // Verify we got tokens (not checking exact count due to newlines)
        assert!(result.len() > 10);
        assert!(matches!(result[0].kind, TokenKind::Newline));
        assert!(matches!(result[1].kind, TokenKind::Func));
        // More detailed checks would go here
    }

    #[test]
    fn tokenize_complex_expression() {
        let result = tokenize("val x = (a + b) * c - d / e").unwrap();

        assert!(matches!(result[0].kind, TokenKind::Val));
        match &result[1].kind {
            TokenKind::Identifier(s) => assert_eq!(s, "x"),
            _ => panic!("Expected identifier"),
        }
        assert!(matches!(result[2].kind, TokenKind::Equal));
        assert!(matches!(result[3].kind, TokenKind::LeftParen));
    }

    #[test]
    fn error_on_unterminated_string() {
        let result = tokenize(r#""unterminated"#);
        assert!(result.is_err());
        match result.unwrap_err() {
            LexError::UnterminatedString { .. } => {}
            err => panic!("Expected UnterminatedString, got: {:?}", err),
        }
    }

    #[test]
    fn error_on_invalid_escape() {
        let result = tokenize(r#""invalid\q""#);
        assert!(result.is_err());
        match result.unwrap_err() {
            LexError::InvalidEscape { .. } => {}
            err => panic!("Expected InvalidEscape, got: {:?}", err),
        }
    }

    #[test]
    fn error_on_unexpected_char() {
        let result = tokenize("$invalid");
        assert!(result.is_err());
        match result.unwrap_err() {
            LexError::UnexpectedChar { character, .. } => assert_eq!(character, '$'),
            _ => panic!("Expected unexpected char error"),
        }
    }

    #[test]
    fn span_tracking() {
        let result = tokenize("func add").unwrap();

        assert_eq!(result[0].span, Span::new(0, 4)); // "func"
        assert_eq!(result[1].span, Span::new(5, 8)); // "add"
    }

    #[test]
    fn newline_handling() {
        let result = tokenize("func\n\nval").unwrap();

        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Newline));
        assert!(matches!(result[2].kind, TokenKind::Val));
    }

    #[test]
    fn whitespace_handling() {
        let result = tokenize("func   \t  val").unwrap();

        // Whitespace should be skipped
        assert_eq!(result.len(), 3); // func, val, EOF
        assert!(matches!(result[0].kind, TokenKind::Func));
        assert!(matches!(result[1].kind, TokenKind::Val));
    }

    #[test]
    fn boolean_literals() {
        let result = tokenize("true false").unwrap();

        assert!(matches!(result[0].kind, TokenKind::True));
        assert!(matches!(result[1].kind, TokenKind::False));
    }

    #[test]
    fn is_valid_identifier_test() {
        assert!(Lexer::is_valid_identifier("foo"));
        assert!(Lexer::is_valid_identifier("_bar"));
        assert!(Lexer::is_valid_identifier("baz123"));
        assert!(Lexer::is_valid_identifier("αβγ"));
        assert!(Lexer::is_valid_identifier("変数"));

        assert!(!Lexer::is_valid_identifier(""));
        assert!(!Lexer::is_valid_identifier("123abc"));
        assert!(!Lexer::is_valid_identifier("-invalid"));
    }

    #[test]
    fn stress_test_large_input() {
        let mut source = String::new();
        for i in 0..1000 {
            source.push_str(&format!("val x{} = {}\n", i, i));
        }

        let result = tokenize(&source);
        assert!(result.is_ok());
        let tokens = result.unwrap();
        // Each line has: val, identifier, =, number, newline
        // Plus one EOF at the end
        assert_eq!(tokens.len(), 1000 * 5 + 1);
    }
}
