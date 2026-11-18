// End-to-end return-position `impl Trait` over any body.
//
// The concrete type is what the body produces: a call result, a binding, an adapter
// chain, a call to another `impl Trait` function in either source order, or a generic
// instance. The caller sees that type, so the value binds, passes and iterates.

use crate::compile_harness::{CompileTest, assert_check_error, stdout_of};

/// The specification's own example: an adapter chain over a range.
#[test]
fn the_evens_adapter_chain_iterates() {
    let out = stdout_of(
        "evens.nr",
        r#"
func evens(n: u32) -> impl Iterator<Item = u32> {
    (0..n).filter(|x| x % 2 == 0)
}

func main() -> i32 {
    mut total: u32 = 0
    for x in evens(10) { total += x }
    println("{total}")
    0
}
"#,
    );
    assert_eq!(out, "20\n");
}

/// One computation, the sum of the even numbers below 10, reached five ways: a tail
/// adapter chain, a bound value returned, an `if` arm, a generic consumer and a closure.
/// Every form must print 20.
#[test]
fn equivalent_forms_agree() {
    let out = stdout_of(
        "forms.nr",
        r#"
func evens(n: u32) -> impl Iterator<Item = u32> { (0..n).filter(|x| x % 2 == 0) }

func bound(n: u32) -> impl Iterator<Item = u32> {
    val it = evens(n)
    it
}

func chosen(n: u32, odd: bool) -> impl Iterator<Item = u32> {
    if odd { (0..n).filter(|x| x % 2 == 1) } else { (0..n).filter(|x| x % 2 == 0) }
}

func sum<I: Iterator<Item = u32>>(it: I) -> u32 {
    mut t: u32 = 0
    for x in it { t += x }
    t
}

func main() -> i32 {
    val via_closure = |n: u32| sum(evens(n))
    val direct = sum(evens(10))
    val stored = evens(10)
    println("{direct} {sum(bound(10))} {sum(chosen(10, false))} {sum(stored)} {via_closure(10)}")
    0
}
"#,
    );
    assert_eq!(out, "20 20 20 20 20\n");
}

/// A call result, a binding, and a body that calls an `impl Trait` function declared
/// after it all resolve, and the adapters chain on a returned adapter.
#[test]
fn call_results_bindings_and_later_callees_resolve() {
    let out = stdout_of(
        "bodies.nr",
        r#"
trait Shape { func area(&self) -> i32 }
struct Sq { side: i32 }
impl Shape for Sq { func area(&self) -> i32 { self.side * self.side } }

func base(n: i32) -> Sq { Sq { side: n } }
func via_call(n: i32) -> impl Shape { base(n) }
func via_val(n: i32) -> impl Shape {
    val s = Sq { side: n + 1 }
    s
}
func outer(n: u32) -> impl Iterator<Item = u32> { odds(n).map(|x| x * 10) }
func odds(n: u32) -> impl Iterator<Item = u32> { (0..n).filter(|x| x % 2 == 1) }

func main() -> i32 {
    mut t: u32 = 0
    for x in outer(6) { t += x }
    println("{via_call(3).area()} {via_val(3).area()} {t}")
    0
}
"#,
    );
    assert_eq!(out, "9 16 90\n");
}

/// A generic function's `impl Trait` return is its instance's body type. This used to
/// pass the checker and then fail in lowering, which could not resolve `impl Show`.
#[test]
fn a_generic_function_returns_impl_trait() {
    let out = stdout_of(
        "generic.nr",
        r#"
trait Show { func show(&self) -> i32 }
struct S { v: i32 }
impl Show for S { func show(&self) -> i32 { self.v } }

func make<T>(x: T) -> impl Show { S { v: 7 } }

func without<I: Iterator<Item = u32>>(it: I, skip: u32) -> impl Iterator<Item = u32> {
    it.filter(move |x| *x != skip)
}

func main() -> i32 {
    mut t: u32 = 0
    for x in without(1u32..6u32, 3) { t += x }
    println("{make(1).show()} {make(true).show()} {t}")
    0
}
"#,
    );
    assert_eq!(out, "7 7 12\n");
}

/// A `move` closure owning a `string` and a `Vec` travels out inside the adapter and is
/// released once.
#[test]
fn an_adapter_owning_its_captures_is_released() {
    let exit = CompileTest::new().run_leak_checked(
        "owned.nr",
        r#"
func tagged(n: u64, tag: string) -> impl Iterator<Item = u64> {
    (0..n).filter(move |x| *x < tag.len())
}
func shifted(n: u64) -> impl Iterator<Item = u64> {
    mut v: Vec<u64> = Vec::new()
    v.push(n)
    v.push(n + 1)
    (0..n).map(move |x| x + v.len())
}
func main() -> i32 {
    mut total: u64 = 0
    mut i = 0
    while i < 20 {
        val base: string = "abcd"
        for x in tagged(10, base.clone()) { total += x }
        for x in shifted(3) { total += x }
        val unused = shifted(2)
        i += 1
    }
    (total / 20) as i32
}
"#,
    );
    // 0+1+2+3 from `tagged`, 2+3+4 from `shifted`.
    assert_eq!(exit, 15);
}

#[test]
fn a_body_depending_on_its_own_result_is_rejected() {
    assert_check_error(
        "recursive.nr",
        r#"
trait Shape { func area(&self) -> i32 }
struct Sq { side: i32 }
impl Shape for Sq { func area(&self) -> i32 { self.side } }
func f(n: i32) -> impl Shape { if n == 0 { Sq { side: 1 } } else { f(n - 1) } }
func main() -> i32 { 0 }
"#,
        "does not depend on this function's own result",
    );
}

#[test]
fn a_returned_adapter_borrowing_a_local_is_rejected() {
    assert_check_error(
        "escape.nr",
        r#"
func below(n: u64, v: Vec<u64>) -> impl Iterator<Item = u64> {
    (0..n).filter(|x| *x < v.len())
}
func main() -> i32 { 0 }
"#,
        "cannot return a reference to 'v'",
    );
}

#[test]
fn a_promised_item_type_must_match_the_body() {
    assert_check_error(
        "item.nr",
        r#"
func evens(n: u32) -> impl Iterator<Item = i32> { (0..n).filter(|x| x % 2 == 0) }
func main() -> i32 { 0 }
"#,
        "binds `Item` to u32",
    );
}
