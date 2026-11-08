use super::semantic_errors;
use crate::errors::TypeError;

/// The protocol and the adapter types live in the prelude, which these tests do not load,
/// so each program declares the part it uses.
const PROTOCOL: &str = r#"
trait Iterator {
    type Item
    func next(&mut self) -> Option<Self::Item>
}

enum Option<T> {
    Some(T),
    None
}

struct Count { at: i32 }

impl Iterator for Count {
    type Item = i32
    func next(&mut self) -> Option<i32> { Option::Some(self.at) }
}

struct Map<I, A, B> { inner: I, f: mut (A) -> B }

impl<I: Iterator<Item = A>, A, B> Iterator for Map<I, A, B> {
    type Item = B
    func next(&mut self) -> Option<B> {
        match self.inner.next() {
            Option::Some(value) => Option::Some((self.f)(value)),
            Option::None => Option::None
        }
    }
}
"#;

fn errors_of(body: &str) -> Vec<TypeError> {
    semantic_errors(&format!("{PROTOCOL}{body}"))
}

#[test]
fn map_on_an_iterator_types_as_the_prelude_adapter() {
    let errors = errors_of(
        r#"
func main() -> i32 {
    val m: Map<Count, i32, f64> = Count { at: 1 }.map(|v| v as f64)
    0
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn map_over_a_bounded_parameter_takes_the_bound_item() {
    let errors = errors_of(
        r#"
func lift<I: Iterator<Item = i32>>(it: I) -> Map<I, i32, bool> {
    it.map(|v| v > 0)
}

func main() -> i32 { 0 }
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

/// Without the generic `Filter` the prelude declares there is no adapter to build, and the
/// call is an ordinary method lookup that finds nothing.
#[test]
fn without_the_adapter_type_the_method_is_missing() {
    let errors = errors_of(
        r#"
struct Filter { cells: i32 }

func main() -> i32 {
    val m = Count { at: 1 }.filter(|v| v > 0)
    0
}
"#,
    );
    assert!(
        errors.iter().any(
            |e| matches!(e, TypeError::MethodNotFound { method_name, .. } if method_name == "filter")
        ),
        "got {errors:?}"
    );
}

#[test]
fn a_declared_method_shadows_the_adapter() {
    let errors = errors_of(
        r#"
impl Count {
    func map(&self) -> i32 { self.at }
}

func main() -> i32 {
    val c = Count { at: 1 }
    c.map() + c.map()
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}
