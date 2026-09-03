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
