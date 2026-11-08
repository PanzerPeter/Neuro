//! A range written as a value: the prelude's `Range<T>` over its integer bounds, the
//! range-only methods' diagnostic on every other receiver, and the prelude-private
//! intrinsics the walk steps with.

use ast_types::{Item, PRELUDE_MODULE};

use crate::errors::TypeError;

/// A cut-down `Range` whose declarations are stamped as the prelude's, which is where the
/// checker looks for the type a range value builds. Its walk uses both intrinsics.
const PRELUDE: &str = r#"
enum Option<T> { Some(T), None }

trait Iterator {
    type Item
    func next(&mut self) -> Option<Self::Item>
}

struct Range<T> { start: T, end: T }

impl<T: Comparable + Copy> Iterator for Range<T> {
    type Item = T
    func next(&mut self) -> Option<T> {
        if !(self.start < self.end) {
            return Option::None
        }
        val value = self.start
        self.start = self.start.__offset(self.end.__to_u64() - self.end.__to_u64() + 1)
        Option::Some(value)
    }
}

impl<T: Comparable + Copy> Range<T> {
    func step(self, n: T) -> Range<T> { self }
}
"#;

fn errors_of(body: &str) -> Vec<TypeError> {
    let mut items = syntax_parsing::parse(PRELUDE).expect("prelude parses");
    for item in &mut items {
        match item {
            Item::Struct(def) => def.module = PRELUDE_MODULE,
            Item::Impl(def) => def.module = PRELUDE_MODULE,
            _ => {}
        }
    }
    items.extend(syntax_parsing::parse(body).expect("program parses"));
    match crate::type_check(&items) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}

#[test]
fn a_range_value_is_the_prelude_range_over_its_bounds() {
    let errors = errors_of(
        r#"
func total(r: Range<u64>) -> u64 {
    mut s: u64 = 0
    for x in r { s += x }
    s
}
func main() -> i32 {
    val n: u64 = 3
    val typed: Range<i8> = 0..4
    (total(0..n) + total(1..=2)) as i32
}
"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn a_range_value_needs_integer_bounds_of_one_type() {
    let errors = errors_of("func main() -> i32 {\n val r = 0.5..2.0\n 0 }");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::InvalidForRangeType { .. })),
        "{errors:?}"
    );
    let errors = errors_of("func main() -> i32 {\n val a: i64 = 1\n val r = 0u8..a\n 0 }");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "{errors:?}"
    );
}

#[test]
fn rev_and_step_name_the_range_spelling_on_any_other_receiver() {
    let errors = errors_of(
        r#"
func walk<T>(x: T) { val y = x.step(2) }
func main() -> i32 {
    val xs = [1, 2]
    val a = xs.rev()
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::StepOnNonRange { .. })),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::RevOnNonRange { .. })),
        "{errors:?}"
    );
}

#[test]
fn a_constant_stride_below_one_is_refused_on_a_value() {
    let errors = errors_of("func main() -> i32 {\n val r = (0..9).step(0)\n 0 }");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::RangeStepNotPositive { step: 0, .. })),
        "{errors:?}"
    );
}

#[test]
fn the_walk_intrinsics_belong_to_the_prelude() {
    let errors = errors_of("func f<T>(x: T) -> u64 { x.__to_u64() }\nfunc main() -> i32 { 0 }");
    assert!(
        errors.iter().any(
            |e| matches!(e, TypeError::MethodNotFound { method_name, .. } if method_name == "__to_u64")
        ),
        "{errors:?}"
    );
}
