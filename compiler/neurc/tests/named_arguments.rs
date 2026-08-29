// End-to-end tests for named arguments.
//
// The unit tests in `argument-binding` prove the permutation is right; these prove the
// *program* is right. Every program here is written so that binding the arguments in the
// order they appear would produce a different exit code from binding them by name — a
// pass that silently ignored a label would still compile, and only the answer would be
// wrong.

mod common;
use common::CompileTest;

/// Compile and run `source`, asserting it exits with `expected_exit`.
fn assert_exit(name: &str, source: &str, expected_exit: i32) {
    let test = CompileTest::new();
    let exit_code = test
        .compile_and_run(name, source)
        .expect("compilation or execution failed");
    assert_eq!(exit_code, expected_exit, "{name} returned the wrong value");
}

/// Compile a program expected to be rejected, returning the diagnostic text.
fn compile_error(name: &str, source: &str) -> String {
    let test = CompileTest::new();
    let path = test.write_source(name, source);
    match test.compile(&path) {
        Ok(_) => panic!("{name} compiled but should have been rejected"),
        Err(message) => message,
    }
}

#[test]
fn a_named_argument_binds_by_name_not_position() {
    // 10 - 3 = 7 either way round the subtraction is written; only binding by name
    // yields 7 rather than -7.
    assert_exit(
        "named_binds_by_name.nr",
        "func difference(from a: i32, take b: i32) -> i32 {\n\
         \x20   return a - b\n\
         }\n\
         func main() -> i32 {\n\
         \x20   return difference(take: 3, from: 10)\n\
         }\n",
        7,
    );
}

#[test]
fn a_named_call_matches_the_positional_one() {
    assert_exit(
        "named_matches_positional.nr",
        "func blend(a: i32, b: i32, c: i32) -> i32 {\n\
         \x20   return a * 100 + b * 10 + c\n\
         }\n\
         func main() -> i32 {\n\
         \x20   val positional = blend(1, 2, 3)\n\
         \x20   val named = blend(c: 3, a: 1, b: 2)\n\
         \x20   val mixed = blend(1, c: 3, b: 2)\n\
         \x20   if positional != named { return 1 }\n\
         \x20   if positional != mixed { return 2 }\n\
         \x20   return 0\n\
         }\n",
        0,
    );
}

#[test]
fn an_external_label_is_what_the_caller_writes() {
    assert_exit(
        "external_label.nr",
        "func clamp(_ value: i32, min lo: i32, max hi: i32) -> i32 {\n\
         \x20   if value < lo { return lo }\n\
         \x20   if value > hi { return hi }\n\
         \x20   return value\n\
         }\n\
         func main() -> i32 {\n\
         \x20   return clamp(99, max: 40, min: 5)\n\
         }\n",
        40,
    );
}

#[test]
fn an_associated_function_takes_named_arguments() {
    assert_exit(
        "named_assoc_fn.nr",
        "struct Rect { w: i32, h: i32 }\n\
         impl Rect {\n\
         \x20   func make(width: i32, height: i32) -> Rect {\n\
         \x20       return Rect { w: width, h: height }\n\
         \x20   }\n\
         }\n\
         func main() -> i32 {\n\
         \x20   val r = Rect::make(height: 3, width: 20)\n\
         \x20   return r.w - r.h\n\
         }\n",
        17,
    );
}

#[test]
fn an_instance_method_takes_named_arguments() {
    assert_exit(
        "named_method.nr",
        "struct Acc { total: i32 }\n\
         impl Acc {\n\
         \x20   func step(&mut self, add a: i32, times t: i32) {\n\
         \x20       self.total = self.total + a * t\n\
         \x20   }\n\
         }\n\
         func main() -> i32 {\n\
         \x20   mut acc = Acc { total: 1 }\n\
         \x20   acc.step(times: 5, add: 8)\n\
         \x20   return acc.total\n\
         }\n",
        41,
    );
}

#[test]
fn a_generic_function_takes_named_arguments() {
    assert_exit(
        "named_generic.nr",
        "func pick<T>(first a: T, second b: T, take_first: bool) -> T {\n\
         \x20   if take_first { return a }\n\
         \x20   return b\n\
         }\n\
         func main() -> i32 {\n\
         \x20   return pick(second: 5, take_first: false, first: 70)\n\
         }\n",
        5,
    );
}

#[test]
fn a_trait_method_reached_through_dyn_takes_named_arguments() {
    assert_exit(
        "named_dyn_dispatch.nr",
        "trait Shape {\n\
         \x20   func area(&self, scale k: i32) -> i32\n\
         }\n\
         struct Square { side: i32 }\n\
         impl Shape for Square {\n\
         \x20   func area(&self, scale k: i32) -> i32 { return self.side * self.side * k }\n\
         }\n\
         func main() -> i32 {\n\
         \x20   val sq = Square { side: 3 }\n\
         \x20   val shape: &dyn Shape = &sq\n\
         \x20   return shape.area(scale: 2)\n\
         }\n",
        18,
    );
}

#[test]
fn a_named_argument_may_be_a_call_of_its_own() {
    assert_exit(
        "named_nested_call.nr",
        "func difference(from a: i32, take b: i32) -> i32 { return a - b }\n\
         func main() -> i32 {\n\
         \x20   return difference(take: difference(take: 1, from: 4), from: 20)\n\
         }\n",
        17,
    );
}

#[test]
fn a_named_argument_is_bound_inside_every_nested_position() {
    // A loop body, a match arm, an array element, a struct literal field, and an
    // interpolation hole are five separate arms of the binding pass's traversal; a
    // program that reaches all of them fails loudly if any one is missed.
    assert_exit(
        "named_nested_positions.nr",
        "struct Box { v: i32 }\n\
         func difference(from a: i32, take b: i32) -> i32 { return a - b }\n\
         func main() -> i32 {\n\
         \x20   mut total: i32 = 0\n\
         \x20   for i in 0..3 {\n\
         \x20       total = total + difference(take: i, from: 10)\n\
         \x20   }\n\
         \x20   val arr = [difference(take: 1, from: 5)]\n\
         \x20   val boxed = Box { v: difference(take: 2, from: 9) }\n\
         \x20   val chosen = match total {\n\
         \x20       27 => difference(take: 3, from: 10),\n\
         \x20       _ => 0,\n\
         \x20   }\n\
         \x20   val text = \"{difference(take: 1, from: 3)}\"\n\
         \x20   if text != \"2\" { return 1 }\n\
         \x20   return total + arr[0] + boxed.v + chosen\n\
         }\n",
        45,
    );
}

#[test]
fn a_positional_argument_may_not_follow_a_named_one() {
    let message = compile_error(
        "positional_after_named.nr",
        "func f(a: i32, b: i32) -> i32 { return a }\n\
         func main() -> i32 { return f(b: 1, 2) }\n",
    );
    assert!(
        message.contains("cannot follow a named one"),
        "unexpected diagnostic: {message}"
    );
}

#[test]
fn an_unknown_label_is_rejected() {
    let message = compile_error(
        "unknown_label.nr",
        "func f(a: i32) -> i32 { return a }\n\
         func main() -> i32 { return f(nope: 1) }\n",
    );
    assert!(
        message.contains("has no parameter named 'nope'"),
        "unexpected diagnostic: {message}"
    );
}

#[test]
fn omitting_a_required_label_is_rejected() {
    let message = compile_error(
        "missing_label.nr",
        "func f(_ v: i32, min lo: i32) -> i32 { return v + lo }\n\
         func main() -> i32 { return f(1, 2) }\n",
    );
    assert!(
        message.contains("must be named"),
        "unexpected diagnostic: {message}"
    );
}

#[test]
fn a_positional_only_parameter_may_not_be_named() {
    let message = compile_error(
        "suppressed_label.nr",
        "func f(_ v: i32) -> i32 { return v }\n\
         func main() -> i32 { return f(v: 1) }\n",
    );
    assert!(
        message.contains("passed positionally"),
        "unexpected diagnostic: {message}"
    );
}

#[test]
fn one_parameter_may_not_be_named_twice() {
    let message = compile_error(
        "duplicate_label.nr",
        "func f(a: i32, b: i32) -> i32 { return a + b }\n\
         func main() -> i32 { return f(a: 1, a: 2) }\n",
    );
    assert!(
        message.contains("given twice"),
        "unexpected diagnostic: {message}"
    );
}

#[test]
fn two_parameters_may_not_share_a_call_site_name() {
    let message = compile_error(
        "clashing_labels.nr",
        "func f(width w: i32, width h: i32) -> i32 { return w + h }\n\
         func main() -> i32 { return f(width: 1) }\n",
    );
    assert!(
        message.contains("share the call-site name"),
        "unexpected diagnostic: {message}"
    );
}

#[test]
fn a_closure_call_rejects_a_named_argument() {
    let message = compile_error(
        "closure_label.nr",
        "func main() -> i32 {\n\
         \x20   val double = |x: i32| x * 2\n\
         \x20   return double(x: 4)\n\
         }\n",
    );
    assert!(
        message.contains("no declared parameter names"),
        "unexpected diagnostic: {message}"
    );
}

#[test]
fn a_reordered_named_call_evaluates_its_arguments_in_source_order() {
    // `bump` runs first because it is written first, so `x` is read as 1 and the call is
    // `combine(first: 1, second: 0)` = 10. Binding the arguments into declaration order
    // before evaluating them would read `x` as 0 and answer 0. The positional control
    // proves the two forms agree, which is the property the specification claims.
    assert_exit(
        "named_source_order.nr",
        "func bump(r: &mut i32) -> i32 {\n\
         \x20   *r = *r + 1\n\
         \x20   return 0\n\
         }\n\
         func combine(first: i32, second: i32) -> i32 {\n\
         \x20   return first * 10 + second\n\
         }\n\
         func main() -> i32 {\n\
         \x20   mut x = 0\n\
         \x20   val named = combine(second: bump(&mut x), first: x)\n\
         \x20   mut y = 0\n\
         \x20   val positional = combine(bump(&mut y), y)\n\
         \x20   if named != 10 { return 1 }\n\
         \x20   if positional != 1 { return 2 }\n\
         \x20   return 0\n\
         }\n",
        0,
    );
}

#[test]
fn a_reordered_named_call_returning_unit_still_runs_in_source_order() {
    // The rewrite puts a unit call in a block's tail position, which the backend has to
    // discard rather than ask for a value.
    assert_exit(
        "named_source_order_unit.nr",
        "func tick(r: &mut i32, by: i32) -> i32 {\n\
         \x20   *r = *r * 10 + by\n\
         \x20   return 0\n\
         }\n\
         func record(first: i32, second: i32) { }\n\
         func main() -> i32 {\n\
         \x20   mut log = 0\n\
         \x20   record(second: tick(&mut log, 2), first: tick(&mut log, 1))\n\
         \x20   if log != 21 { return 1 }\n\
         \x20   return 0\n\
         }\n",
        0,
    );
}

#[test]
fn a_reordered_named_call_types_its_arguments_by_their_parameters() {
    // Each reordered argument is bound to a temporary, and a temporary that took its type
    // from its initializer alone would infer `i32` for the literal and lose `Vec::new()`'s
    // element type entirely.
    assert_exit(
        "named_source_order_inference.nr",
        "func hold(v: Vec<i32>, wide: i64, n: i32) -> i32 {\n\
         \x20   return n\n\
         }\n\
         func one() -> i32 { return 1 }\n\
         func main() -> i32 {\n\
         \x20   return hold(n: one(), wide: 3, v: Vec::new()) - 1\n\
         }\n",
        0,
    );
}
