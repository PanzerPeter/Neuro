use super::semantic_errors;
use crate::errors::TypeError;

/// The protocol traits live in the prelude, which these tests do not load, so each
/// program declares them itself — a local declaration shadows the prelude's entry, so
/// this is the same shape a real program sees.
const PROTOCOL_TRAITS: &str = r#"
trait Iterator {
    type Item
    func next(&mut self) -> Option<Self::Item>
}

trait IntoIterator {
    type Item
    type Iter
    func into_iter(self) -> Self::Iter
}

enum Option<T> {
    Some(T),
    None
}

@derive(Copy, Clone)
struct CountIter { at: i32, end: i32 }

impl Iterator for CountIter {
    type Item = i32
    func next(&mut self) -> Option<i32> {
        if self.at >= self.end { return Option::None }
        val current = self.at
        self.at = self.at + 1
        Option::Some(current)
    }
}

@derive(Copy, Clone)
struct Count { end: i32 }

impl IntoIterator for Count {
    type Item = i32
    type Iter = CountIter
    func into_iter(self) -> CountIter {
        CountIter { at: 0, end: self.end }
    }
}
"#;

fn program(body: &str) -> String {
    format!("{PROTOCOL_TRAITS}\nfunc main() -> i32 {{\n{body}\n}}\n")
}

#[test]
fn a_for_head_over_an_into_iterator_binds_the_iterators_item() {
    let errors = semantic_errors(&program(
        r#"
    val c = Count { end: 3 }
    mut total = 0
    for v in c {
        total = total + v
    }
    total
"#,
    ));
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn a_for_head_over_an_iterator_needs_no_into_iterator_impl() {
    let errors = semantic_errors(&program(
        r#"
    val it = CountIter { at: 0, end: 3 }
    mut total = 0
    for v in it {
        total = total + v
    }
    total
"#,
    ));
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

/// The binding takes the `Item` the impl chose, so using it at another type is a
/// `Mismatch` rather than passing silently.
#[test]
fn the_element_binding_carries_the_associated_item_type() {
    let errors = semantic_errors(&program(
        r#"
    val c = Count { end: 3 }
    for v in c {
        val flag: bool = v
    }
    0
"#,
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "binding an i32 item as bool must be a mismatch; got {errors:?}"
    );
}

#[test]
fn a_head_implementing_neither_trait_is_not_iterable() {
    let errors = semantic_errors(&program(
        r#"
    val p = Count { end: 3 }
    for v in p.end {
        val n: i32 = v
    }
    0
"#,
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NotIterable { .. })),
        "an i32 head must be reported as not iterable; got {errors:?}"
    );
}

/// A struct with no protocol impl at all is the plain case, and must be reported as
/// not iterable rather than as the "not indexable" diagnostic the arm previously reused.
#[test]
fn a_struct_with_no_protocol_impl_is_not_iterable() {
    let source = format!(
        r#"{PROTOCOL_TRAITS}
@derive(Copy, Clone)
struct Plain {{ n: i32 }}

func main() -> i32 {{
    val p = Plain {{ n: 1 }}
    for v in p {{
        val n: i32 = v
    }}
    0
}}
"#
    );
    let errors = semantic_errors(&source);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NotIterable { .. })),
        "a struct implementing neither trait must be not iterable; got {errors:?}"
    );
}

/// One iterator nested inside another type-checks: each `for` head opens its own scope,
/// and a stateless container may be walked by both.
#[test]
fn protocol_loops_nest() {
    let errors = semantic_errors(&program(
        r#"
    val c = Count { end: 3 }
    mut pairs = 0
    for v in c {
        for w in c {
            pairs = pairs + v * w
        }
    }
    pairs
"#,
    ));
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

/// The codepoint iterator as the prelude declares it, so `.chars()` has something to
/// resolve to here. The `__char_at` step is prelude-private; inside these tests every
/// declaration is a user declaration, so the body stands in for it.
const CHARS_DECL: &str = r#"
struct Chars { source: &string, offset: u64 }

impl Iterator for Chars {
    type Item = char
    func next(&mut self) -> Option<char> {
        if self.offset >= self.source.len() { return Option::None }
        self.offset = self.offset + 1
        Option::Some('a')
    }
}
"#;

#[test]
fn a_chars_head_binds_a_char() {
    let source = format!(
        "{PROTOCOL_TRAITS}{CHARS_DECL}
func main() -> i32 {{
    mut wide = 0
    for c in \"héllo\".chars() {{
        if (c as u32) > 127 {{ wide = wide + 1 }}
    }}
    wide
}}
"
    );
    let errors = semantic_errors(&source);
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

/// `.char_indices()` is a head form, not a method: its position binding is a `u64` byte
/// offset and its value binding the code point standing there.
#[test]
fn a_char_indices_head_binds_a_byte_offset_and_a_char() {
    let source = format!(
        "{PROTOCOL_TRAITS}{CHARS_DECL}
func main() -> i32 {{
    val text = \"héllo\"
    mut last: u64 = 0
    for (off, c) in text.char_indices() {{
        if c == 'o' {{ last = off }}
    }}
    last as i32
}}
"
    );
    let errors = semantic_errors(&source);
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

/// Outside a `for` head there is no position to bind, so the call names no method at
/// all — the diagnostic is the ordinary one for a method a type does not have.
#[test]
fn char_indices_outside_a_for_head_is_not_a_method() {
    let source = format!(
        "{PROTOCOL_TRAITS}{CHARS_DECL}
func main() -> i32 {{
    val it = \"hi\".char_indices()
    0
}}
"
    );
    let errors = semantic_errors(&source);
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::MethodNotFound { method_name, .. } if method_name == "char_indices"
        )),
        "expected MethodNotFound for char_indices, got {errors:?}"
    );
}

/// The decode step behind `Chars::next` belongs to the prelude. A program is not the
/// prelude, so it cannot reach it — the language specifies no byte-indexed read.
#[test]
fn the_decode_intrinsic_is_out_of_reach_of_a_program() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val c = "hi".__char_at(0)
    0
}
"#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::MethodNotFound { method_name, .. } if method_name == "__char_at"
        )),
        "expected MethodNotFound for __char_at, got {errors:?}"
    );
}

/// Without the prelude there is no iterator to hand out, so `.chars()` reports the
/// missing declaration rather than resolving to a type that is not there.
#[test]
fn chars_without_the_iterator_declaration_reports_it() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val it = "hi".chars()
    0
}
"#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::UnknownTypeName { name, .. } if name == "Chars"
        )),
        "expected UnknownTypeName for Chars, got {errors:?}"
    );
}
