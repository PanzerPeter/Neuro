// Unit tests for the tokenizer (moved out of lib.rs to keep the slice entry point lean).

use super::*;
use shared_types::{FloatSuffix, IntSuffix};

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
fn tokenize_unsafe_keyword() {
    let result = tokenize("unsafe").unwrap();
    assert_eq!(result.len(), 2); // keyword + EOF
    assert!(matches!(result[0].kind, TokenKind::Unsafe));
}

#[test]
fn tokenize_loop_keyword() {
    let result = tokenize("loop").unwrap();
    assert_eq!(result.len(), 2); // keyword + EOF
    assert!(matches!(result[0].kind, TokenKind::Loop));
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
fn tokenize_integer_suffixes_decimal() {
    let result = tokenize("42i64 255u8 1000i32 0u16").unwrap();
    assert_eq!(result.len(), 5); // 4 suffixed integers + EOF
    match &result[0].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 42);
            assert_eq!(tok.suffix, IntSuffix::I64);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[1].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 255);
            assert_eq!(tok.suffix, IntSuffix::U8);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[2].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 1000);
            assert_eq!(tok.suffix, IntSuffix::I32);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[3].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 0);
            assert_eq!(tok.suffix, IntSuffix::U16);
        }
        _ => panic!("expected IntegerSuffix"),
    }
}

#[test]
fn tokenize_integer_suffixes_other_bases() {
    let result = tokenize("0b1010i32 0o755u64 0xFFu8").unwrap();
    assert_eq!(result.len(), 4); // 3 suffixed integers + EOF
    match &result[0].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 0b1010);
            assert_eq!(tok.suffix, IntSuffix::I32);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[1].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 0o755);
            assert_eq!(tok.suffix, IntSuffix::U64);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[2].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 0xFF);
            assert_eq!(tok.suffix, IntSuffix::U8);
        }
        _ => panic!("expected IntegerSuffix"),
    }
}

#[test]
fn unsuffixed_integers_unchanged() {
    // Ensure plain integers still produce Integer tokens, not IntegerSuffix
    let result = tokenize("42 0 1000").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Integer(42)));
    assert!(matches!(result[1].kind, TokenKind::Integer(0)));
    assert!(matches!(result[2].kind, TokenKind::Integer(1000)));
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
fn tokenize_float_suffixes() {
    let result = tokenize("1.5f32 2.0f64 1e10f32 1.5e-5f64").unwrap();
    assert_eq!(result.len(), 5); // 4 suffixed floats + EOF
    match &result[0].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 1.5).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::F32);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[1].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 2.0).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::F64);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[2].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 1e10).abs() < 1.0);
            assert_eq!(tok.suffix, FloatSuffix::F32);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[3].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 1.5e-5).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::F64);
        }
        _ => panic!("expected FloatSuffix"),
    }
}

#[test]
fn tokenize_half_precision_float_suffixes() {
    // §1.2 — `f16` / `bf16` half-precision literals. `bf16` must not be mis-split
    // as `b` + `f16`, and `1.5f16` must not be read as `1.5` + the `f16` ident.
    let result = tokenize("1.5f16 0.02bf16 2e3f16 1.0bf16").unwrap();
    assert_eq!(result.len(), 5); // 4 suffixed floats + EOF
    match &result[0].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 1.5).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::F16);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[1].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 0.02).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::BF16);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[2].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 2e3).abs() < 1.0);
            assert_eq!(tok.suffix, FloatSuffix::F16);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[3].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 1.0).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::BF16);
        }
        _ => panic!("expected FloatSuffix"),
    }
}

#[test]
fn unsuffixed_floats_unchanged() {
    // Ensure plain floats still produce Float tokens, not FloatSuffix.
    let result = tokenize("3.15 0.5 1e10").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Float(_)));
    assert!(matches!(result[1].kind, TokenKind::Float(_)));
    assert!(matches!(result[2].kind, TokenKind::Float(_)));
}

#[test]
fn underscore_separators_decimal() {
    let result = tokenize("1_000_000 1_2_3").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Integer(1_000_000)));
    assert!(matches!(result[1].kind, TokenKind::Integer(123)));
}

#[test]
fn underscore_separators_hex_binary_octal() {
    let result = tokenize("0xFF_FF 0b1010_0011 0o7_5_5").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Integer(0xFFFF)));
    assert!(matches!(result[1].kind, TokenKind::Integer(0b1010_0011)));
    assert!(matches!(result[2].kind, TokenKind::Integer(0o755)));
}

#[test]
fn underscore_separators_float() {
    let result = tokenize("1_000.000_5 1_0e1_0").unwrap();
    match result[0].kind {
        TokenKind::Float(f) => assert!((f - 1000.0005).abs() < 1e-9),
        _ => panic!("expected float"),
    }
    match result[1].kind {
        // 10e10 == 1.0e11
        TokenKind::Float(f) => assert!((f - 1.0e11).abs() < 1.0),
        _ => panic!("expected float"),
    }
}

#[test]
fn underscore_separators_suffixed() {
    let result = tokenize("1_000i64 0xFF_FFu32 2_000.5f32").unwrap();
    match &result[0].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 1000);
            assert_eq!(tok.suffix, IntSuffix::I64);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[1].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 0xFFFF);
            assert_eq!(tok.suffix, IntSuffix::U32);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[2].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 2000.5).abs() < 1e-9);
            assert_eq!(tok.suffix, FloatSuffix::F32);
        }
        _ => panic!("expected FloatSuffix"),
    }
}

#[test]
fn leading_underscore_is_identifier_not_number() {
    // A leading underscore must bind as an identifier, never a numeric literal —
    // the digit-separator rule applies only between digits.
    let result = tokenize("_1000").unwrap();
    match &result[0].kind {
        TokenKind::Identifier(s) => assert_eq!(s, "_1000"),
        _ => panic!("expected identifier"),
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
fn tokenize_char_literals() {
    let result = tokenize("'a' '\\n' '\\u{1F44D}' '\\''").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Char('a')));
    assert!(matches!(result[1].kind, TokenKind::Char('\n')));
    assert!(matches!(result[2].kind, TokenKind::Char('\u{1F44D}')));
    assert!(matches!(result[3].kind, TokenKind::Char('\'')));
}

#[test]
fn empty_and_multi_char_literals_are_rejected() {
    // Neither `''` nor `'ab'` matches the single-scalar char-literal regex, so
    // both surface as lex errors rather than tokenizing.
    assert!(tokenize("''").is_err());
    assert!(tokenize("'ab'").is_err());
    assert!(tokenize("'a").is_err());
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
