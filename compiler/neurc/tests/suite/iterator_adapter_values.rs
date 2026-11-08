// End-to-end tests for `.map(f)`, `.filter(p)` and `.enumerate()` called on an `Iterator`
// value: the prelude adapter each builds binds, passes, returns and drives a `for` head;
// chains compose; a generic receiver bounded by `Iterator<Item = T>` adapts; the function
// is moved into the adapter under the closure rules; and the diagnostics for a malformed
// call.
use crate::compile_harness::{CompileTest, assert_check_error, assert_exit};

/// A counter yielding `1..=limit`, the source most tests adapt.
const COUNT: &str = r#"
struct Count { n: i32, limit: i32 }

impl Iterator for Count {
    type Item = i32
    func next(&mut self) -> Option<i32> {
        if self.n >= self.limit {
            return Option::None
        }
        self.n = self.n + 1
        Option::Some(self.n)
    }
}

func count(limit: i32) -> Count {
    Count { n: 0, limit: limit }
}
"#;

fn with_count(body: &str) -> String {
    format!("{COUNT}{body}")
}

#[test]
fn a_mapped_iterator_binds_and_drives_a_for_head() {
    let source = with_count(
        r#"
func main() -> i32 {
    val tripled = count(4).map(|v| v * 3)
    mut total = 0
    for v in tripled {
        total += v
    }
    total
}
"#,
    );
    assert_exit("map_value.nr", &source, 30);
}

/// The predicate is handed a borrow of each element, which it reads through.
#[test]
fn a_filtered_iterator_keeps_what_its_predicate_accepts() {
    let source = with_count(
        r#"
func main() -> i32 {
    val odd = count(7).filter(|v| v % 2 == 1)
    mut total = 0
    for v in odd {
        total += v
    }
    total
}
"#,
    );
    assert_exit("filter_value.nr", &source, 16);
}

/// The position counts what the adapter yielded, so it stays dense behind a filter.
#[test]
fn an_enumerated_iterator_yields_dense_positions() {
    let source = with_count(
        r#"
func main() -> i32 {
    val ranked = count(9).filter(|v| v % 3 == 0).enumerate()
    mut total: u64 = 0
    for pair in ranked {
        total += pair.0 * 10 + pair.1 as u64
    }
    total as i32
}
"#,
    );
    // Positions 0, 1, 2 over 3, 6, 9.
    assert_exit("enumerate_value.nr", &source, 48);
}

/// An adapter is an ordinary value: a function returns one by its written type, and
/// another takes it through an `Iterator<Item = T>` bound.
#[test]
fn an_adapter_chain_is_returned_and_passed() {
    let source = with_count(
        r#"
func scaled_evens(limit: i32, k: i32) -> Map<Filter<Count, i32>, i32, i32> {
    count(limit).filter(|v| v % 2 == 0).map(|v| v * k)
}

func sum<I: Iterator<Item = i32>>(it: I) -> i32 {
    mut total = 0
    for v in it {
        total += v
    }
    total
}

func main() -> i32 {
    sum(scaled_evens(6, 10))
}
"#,
    );
    assert_exit("adapter_pass_return.nr", &source, 120);
}

/// Inside a generic body the element type is the one the bound names.
#[test]
fn a_generic_receiver_adapts_through_its_bound() {
    let source = with_count(
        r#"
func doubled<I: Iterator<Item = i32>>(it: I) -> Map<I, i32, i32> {
    it.map(|v| v * 2)
}

func main() -> i32 {
    mut total = 0
    for v in doubled(doubled(count(3))) {
        total += v
    }
    total
}
"#,
    );
    assert_exit("adapter_generic_receiver.nr", &source, 24);
}

/// `.map` may change the element type, and a later adapter sees the new one.
#[test]
fn map_changes_the_element_type_for_the_next_adapter() {
    let source = with_count(
        r#"
func main() -> i32 {
    val halves = count(4).map(|v| v as f64 / 2.0).filter(|h| *h > 0.9)
    mut total = 0.0
    for h in halves {
        total += h
    }
    (total * 10.0) as i32
}
"#,
    );
    assert_exit("adapter_retype.nr", &source, 45);
}

/// An `Fn` value is copied into the adapter, so its binding stays usable.
#[test]
fn an_fn_binding_is_copied_into_the_adapter() {
    let source = with_count(
        r#"
func main() -> i32 {
    val offset = |v: i32| -> i32 { v + 100 }
    val shifted = count(2).map(offset)
    mut total = 0
    for v in shifted {
        total += v
    }
    total - offset(0)
}
"#,
    );
    assert_exit("adapter_fn_copy.nr", &source, 103);
}

/// A closure that writes what it captured drives the adapter, and holds its borrow for as
/// long as the adapter lives.
#[test]
fn a_writing_closure_drives_the_adapter_it_is_stored_in() {
    let source = with_count(
        r#"
func main() -> i32 {
    mut seen = 0
    mut total = 0
    {
        val counted = count(5).map(|v| {
            seen += 1
            v
        })
        for v in counted {
            total += v
        }
    }
    seen * 10 + total
}
"#,
    );
    assert_exit("adapter_fnmut.nr", &source, 65);
}

/// A string iterator adapts too, its closures reading a captured `string`, and nothing is
/// freed twice or left behind.
#[test]
fn a_chars_pipeline_releases_what_it_holds() {
    let source = r#"
func main() -> i32 {
    val skip = "lo"
    mut total: u32 = 0
    mut round = 0
    while round < 4 {
        val text = "hello, world"
        val kept = text.chars().filter(|c| *c != 'l').map(|c| if skip == "lo" { c as u32 } else { 0 })
        for code in kept {
            total += code
        }
        round += 1
    }
    println("{total}")
    0
}
"#;
    let test = CompileTest::new();
    assert_eq!(test.run_leak_checked("adapter_chars.nr", source), 0);
}

/// A type's own method of the name is the one called.
#[test]
fn a_declared_method_of_the_same_name_wins() {
    let source = with_count(
        r#"
impl Count {
    func map(self, k: i32) -> i32 {
        self.limit * k
    }
}

func main() -> i32 {
    count(3).map(5)
}
"#,
    );
    assert_exit("adapter_own_method.nr", &source, 15);
}

#[test]
fn a_non_iterator_has_no_adapter() {
    let source = r#"
struct Plain { n: i32 }

func main() -> i32 {
    val m = Plain { n: 1 }.map(|v: i32| -> i32 { v })
    0
}
"#;
    assert_check_error("adapter_not_iterator.nr", source, "has no method 'map'");
}

#[test]
fn an_adapter_needs_a_function_of_the_element() {
    let not_a_function = with_count(
        r#"
func main() -> i32 {
    val m = count(3).map(5)
    0
}
"#,
    );
    assert_check_error(
        "adapter_not_callable.nr",
        &not_a_function,
        "`.map()` needs a function of one parameter",
    );
    let wrong_input = with_count(
        r#"
func main() -> i32 {
    val m = count(3).filter(|v: i32| -> bool { v > 1 })
    0
}
"#,
    );
    assert_check_error(
        "adapter_wrong_input.nr",
        &wrong_input,
        "`.filter()` is applied to elements of type &i32, but its function takes i32",
    );
    let wrong_output = with_count(
        r#"
func half(v: &i32) -> i32 { *v / 2 }

func main() -> i32 {
    val m = count(3).filter(half)
    0
}
"#,
    );
    assert_check_error(
        "adapter_wrong_output.nr",
        &wrong_output,
        "`.filter()` needs a function returning bool",
    );
}

#[test]
fn enumerate_takes_no_argument() {
    let source = with_count(
        r#"
func main() -> i32 {
    val m = count(3).enumerate(1)
    0
}
"#,
    );
    assert_check_error("adapter_enumerate_arity.nr", &source, "expected 0, found 1");
}

#[test]
fn a_function_that_runs_once_is_refused() {
    let source = with_count(
        r#"
func eat(s: string) {}

func main() -> i32 {
    val s = "spent"
    val m = count(3).map(move |v| {
        eat(s)
        v
    })
    0
}
"#,
    );
    assert_check_error(
        "adapter_once.nr",
        &source,
        "calls its function once per element",
    );
}

/// The adapter takes its receiver and an `FnMut` function by value.
#[test]
fn the_receiver_and_a_writing_function_are_moved_in() {
    let receiver = with_count(
        r#"
func main() -> i32 {
    val c = count(3)
    val m = c.map(|v| v)
    mut d = c
    0
}
"#,
    );
    assert_check_error(
        "adapter_moves_receiver.nr",
        &receiver,
        "use of moved value 'c'",
    );
    let function = with_count(
        r#"
func main() -> i32 {
    mut n = 0
    val bump = |v: i32| -> i32 {
        n += 1
        v
    }
    val m = count(3).map(bump)
    bump(1)
}
"#,
    );
    assert_check_error(
        "adapter_moves_fnmut.nr",
        &function,
        "use of moved value 'bump'",
    );
    let borrowed = with_count(
        r#"
func main() -> i32 {
    mut c = count(3)
    val r = &mut c
    val m = r.map(|v| v)
    0
}
"#,
    );
    assert_check_error(
        "adapter_borrowed_receiver.nr",
        &borrowed,
        "cannot move out of 'r'",
    );
}

/// What must stay in its call cannot be stored in an adapter, and an adapter holding a
/// borrow cannot outlive the borrowee.
#[test]
fn an_adapter_obeys_the_escape_rules() {
    let lent = with_count(
        r#"
func wrap(it: Count, f: mut (i32) -> i32) -> Map<Count, i32, i32> {
    it.map(f)
}

func main() -> i32 { 0 }
"#,
    );
    assert_check_error(
        "adapter_lent_function.nr",
        &lent,
        "this function value stays in the call that made it",
    );
    let dying = with_count(
        r#"
func leak() -> Map<Count, i32, i32> {
    val s = "abc"
    count(3).map(|v| v + s.len() as i32)
}

func main() -> i32 { 0 }
"#,
    );
    assert_check_error(
        "adapter_dying_borrow.nr",
        &dying,
        "cannot return a reference to 's'",
    );
}

#[test]
fn a_bound_must_name_the_element_type() {
    let source = r#"
func g<I: Iterator>(it: I) -> i32 {
    val m = it.map(|v| v)
    0
}

func main() -> i32 { 0 }
"#;
    assert_check_error(
        "adapter_open_item.nr",
        source,
        "write `Iterator<Item = T>` to say what it is",
    );
}

/// A program's own `Map` displaces the prelude adapter, so `.map` on an iterator is an
/// ordinary lookup again, while `.filter` still builds the prelude's `Filter`.
#[test]
fn a_declared_map_type_displaces_only_its_adapter() {
    let shadowed = with_count(
        r#"
struct Map { rows: i32 }

func main() -> i32 {
    val m = count(3).map(|v| v)
    0
}
"#,
    );
    assert_check_error("adapter_shadowed_map.nr", &shadowed, "has no method 'map'");
    let kept = with_count(
        r#"
struct Map { rows: i32 }

func main() -> i32 {
    val grid = Map { rows: 4 }
    mut total = 0
    val big = count(5).filter(|v| v > 3)
    for v in big {
        total += v
    }
    total + grid.rows
}
"#,
    );
    assert_exit("adapter_shadowed_kept.nr", &kept, 13);
}
