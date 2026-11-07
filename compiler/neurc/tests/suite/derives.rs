// End-to-end tests for `@derive` argument validation and the `Debug` / `PartialEq`
// derives.
//
// `@derive(Debug)` gives a struct the `{x:?}` rendering; `@derive(PartialEq)` gives it
// field-wise `==` / `!=`. Neither routes through a method, so these drive the whole
// pipeline and assert on the bytes the program wrote and the exit code it returned.
use crate::compile_harness::{compile_tagged, rejected};
use std::process::{Command, Output};

/// Compile and run `source`, returning the process output.
fn run_program(source: &str, tag: &str) -> Output {
    let exe = compile_tagged(source, &format!("derive_{tag}"), "0");
    Command::new(&exe).output().expect("run executable")
}

/// Type-check `source`, returning the compiler's combined diagnostics on rejection.
fn check_error(source: &str, tag: &str) -> String {
    rejected(&format!("neuro_derive_{tag}.nr"), source)
}

/// See `print_builtins.rs`: fd 1 is a text-mode descriptor on Windows, and these tests
/// assert which text was written rather than the platform's line-ending policy.
fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

#[test]
fn derived_debug_renders_fields_in_declaration_order() {
    let output = run_program(
        r#"
@derive(Debug)
struct Point {
    x: i32,
    y: f64,
    label: string,
    ok: bool
}

func main() -> i32 {
    val p = Point { x: -3, y: 2.5, label: "origin", ok: true }
    println("{p:?}")
    return 0
}
"#,
        "debug_fields",
    );

    assert_eq!(
        stdout(&output),
        "Point { x: -3, y: 2.5, label: \"origin\", ok: true }\n"
    );
    assert_eq!(output.status.code(), Some(0));
}

/// The rendering recurses: a nested struct renders under the same debug kind, which is
/// also what quotes the `string` inside it.
#[test]
fn derived_debug_recurses_into_nested_structs() {
    let output = run_program(
        r#"
@derive(Debug)
struct Inner { tag: string }

@derive(Debug)
struct Outer { id: i32, inner: Inner }

func main() -> i32 {
    val o = Outer { id: 7, inner: Inner { tag: "hi" } }
    println("{o:?}")
    return 0
}
"#,
        "debug_nested",
    );

    assert_eq!(
        stdout(&output),
        "Outer { id: 7, inner: Inner { tag: \"hi\" } }\n"
    );
}

/// A field-less struct renders as its bare name: there are no braces to hold nothing.
#[test]
fn derived_debug_renders_a_field_less_struct_as_its_name() {
    let output = run_program(
        r#"
@derive(Debug)
struct Marker {}

func main() -> i32 {
    val m = Marker {}
    println("[{m:?}]")
    return 0
}
"#,
        "debug_unit",
    );

    assert_eq!(stdout(&output), "[Marker]\n");
}

/// A monomorphized instance renders under the name the programmer wrote, not the
/// mangled instance key the backend indexes it by.
#[test]
fn derived_debug_renders_a_generic_instance_under_its_written_name() {
    let output = run_program(
        r#"
@derive(Debug)
struct Wrapper<T> { value: T }

func main() -> i32 {
    val w = Wrapper { value: 4 }
    println("{w:?}")
    return 0
}
"#,
        "debug_generic",
    );

    assert_eq!(stdout(&output), "Wrapper { value: 4 }\n");
}

#[test]
fn derived_debug_holes_take_the_field_width() {
    let output = run_program(
        r#"
@derive(Debug)
struct P { x: i32 }

func main() -> i32 {
    val p = P { x: 1 }
    println("[{p:>16?}]")
    return 0
}
"#,
        "debug_width",
    );

    assert_eq!(stdout(&output), "[      P { x: 1 }]\n");
}

#[test]
fn derived_partial_eq_compares_every_field() {
    let output = run_program(
        r#"
@derive(PartialEq)
struct Rec { n: i32, ratio: f64, tag: string, ok: bool }

func main() -> i32 {
    val a = Rec { n: 1, ratio: 0.5, tag: "x", ok: true }
    val b = Rec { n: 1, ratio: 0.5, tag: "x", ok: true }
    val c = Rec { n: 1, ratio: 0.5, tag: "y", ok: true }
    println("{a == b} {a == c} {a != c}")
    if a == b && a != c { return 9 }
    return 0
}
"#,
        "eq_fields",
    );

    assert_eq!(stdout(&output), "true false true\n");
    assert_eq!(output.status.code(), Some(9));
}

#[test]
fn derived_partial_eq_recurses_into_nested_structs() {
    let output = run_program(
        r#"
@derive(PartialEq)
struct Inner { a: i32, b: i32 }

@derive(PartialEq)
struct Outer { inner: Inner }

func main() -> i32 {
    val x = Outer { inner: Inner { a: 1, b: 2 } }
    val y = Outer { inner: Inner { a: 1, b: 3 } }
    if x != y { return 5 }
    return 0
}
"#,
        "eq_nested",
    );

    assert_eq!(output.status.code(), Some(5));
}

/// A `&self` receiver compares against a borrowed argument: the operator has no meaning
/// on the reference itself, so both sides compare their referents.
#[test]
fn derived_partial_eq_compares_through_borrows() {
    let output = run_program(
        r#"
@derive(Copy, Clone, PartialEq)
struct P { x: i32 }

func same(a: &P, b: &P) -> bool {
    a == b
}

func main() -> i32 {
    val p = P { x: 3 }
    val q = P { x: 3 }
    if same(&p, &q) { return 4 }
    return 0
}
"#,
        "eq_borrow",
    );

    assert_eq!(output.status.code(), Some(4));
}

#[test]
fn unknown_derive_argument_is_rejected() {
    let stderr = check_error(
        r#"
@derive(Bogus)
struct P { x: i32 }

func main() -> i32 { return 0 }
"#,
        "unknown",
    );
    assert!(
        stderr.contains("names no derivable trait"),
        "expected an unknown-derive diagnostic, got: {stderr}"
    );
}

#[test]
fn a_struct_without_the_debug_derive_cannot_be_interpolated() {
    let stderr = check_error(
        r#"
struct P { x: i32 }

func main() -> i32 {
    val p = P { x: 1 }
    println("{p:?}")
    return 0
}
"#,
        "no_debug",
    );
    assert!(
        stderr.contains("add `@derive(Debug)`"),
        "expected a missing-derive diagnostic, got: {stderr}"
    );
}

/// A struct has no display form, so the bare hole stays an error even with the derive.
#[test]
fn a_debug_struct_still_needs_the_debug_specifier() {
    let stderr = check_error(
        r#"
@derive(Debug)
struct P { x: i32 }

func main() -> i32 {
    val p = P { x: 1 }
    println("{p}")
    return 0
}
"#,
        "no_spec",
    );
    assert!(
        stderr.contains("no display form"),
        "expected a missing-specifier diagnostic, got: {stderr}"
    );
}

#[test]
fn deriving_and_implementing_partial_eq_is_rejected() {
    let stderr = check_error(
        r#"
@derive(Copy, Clone, PartialEq)
struct P { x: i32 }

impl PartialEq for P {
    func eq(&self, rhs: &P) -> bool { self.x == rhs.x }
    func ne(&self, rhs: &P) -> bool { self.x != rhs.x }
}

func main() -> i32 { return 0 }
"#,
        "conflict",
    );
    assert!(
        stderr.contains("keep one of them"),
        "expected a derive/impl conflict diagnostic, got: {stderr}"
    );
}

/// The hand-written impl still works: it is only the *combination* that is rejected.
#[test]
fn a_hand_written_partial_eq_impl_still_dispatches() {
    let output = run_program(
        r#"
@derive(Copy, Clone)
struct P { x: i32 }

impl PartialEq for P {
    func eq(&self, rhs: &P) -> bool { self.x == rhs.x }
    func ne(&self, rhs: &P) -> bool { self.x != rhs.x }
}

func main() -> i32 {
    val a = P { x: 2 }
    val b = P { x: 2 }
    if a == b { return 6 }
    return 0
}
"#,
        "impl_route",
    );

    assert_eq!(output.status.code(), Some(6));
}

/// A derived `.clone()` copied the struct's bytes, so a field that owns a buffer came out
/// shared between the original and the copy. A field with no `.clone()` of its own is
/// refused; every other one is cloned part by part (see the next test).
#[test]
fn regression_derived_clone_refuses_a_field_with_no_clone() {
    for (field, tag) in [
        ("Vec<i32>", "clone_vec"),
        ("StringBuilder", "clone_builder"),
        ("[Vec<i32>; 2]", "clone_vec_array"),
    ] {
        let source = format!(
            "@derive(Clone)\nstruct S {{ f: {field}, n: i32 }}\nfunc main() -> i32 {{ 0 }}\n"
        );
        let err = check_error(&source, tag);
        assert!(
            err.contains("struct 'S' cannot derive `Clone`: field 'f'"),
            "{field}: {err}"
        );
    }

    let generic = check_error(
        "@derive(Clone)\nstruct W<T> { v: T }\nfunc mk() -> Vec<i32> { Vec::new() }\nfunc main() -> i32 {\n    val w = W { v: mk() }\n    val c = w.clone()\n    0\n}\n",
        "clone_generic_vec",
    );
    assert!(
        generic.contains("struct 'W' cannot derive `Clone`: field 'v'"),
        "{generic}"
    );

    let output = run_program(
        r#"
@derive(Copy, Clone)
struct P { x: i32, y: i32 }

@derive(Clone)
struct Pair { a: P, b: [P; 2], n: (i32, f64) }

@derive(Clone)
struct W<T> { v: T }

func main() -> i32 {
    val p = Pair { a: P { x: 1, y: 2 }, b: [P { x: 3, y: 4 }, P { x: 5, y: 6 }], n: (7, 0.5) }
    val q = p.clone()
    val w = W { v: 8 }
    val u = w.clone()
    q.a.x + q.b[1].y + q.n.0 + u.v
}
"#,
        "clone_copyable_fields",
    );
    assert_eq!(output.status.code(), Some(22));
}

/// `string` and tensor fields, a tuple, array or newtype holding them, a nested derive and
/// a generic field instantiated with one are each cloned part by part: the copy owns its
/// own buffers, so it outlives the original and nothing is released twice.
#[test]
fn regression_bug_098_derived_clone_copies_owning_fields_part_by_part() {
    let output = run_program(
        r#"
newtype Label = string

@derive(Clone)
struct Named { name: string, id: i32 }

@derive(Clone)
struct Deep {
    n: Named,
    t: Tensor<f32, [2]>,
    pair: (string, i32),
    arr: [string; 2],
    label: Label
}

@derive(Clone)
struct W<T> { v: T }

func copy_of(d: &Deep) -> Deep {
    val c = d.clone()
    c
}

func main() -> i32 {
    mut total = 0
    for i in 0..3 {
        val d = Deep {
            n: Named { name: "a" + "b", id: 1 },
            t: [1.0, 2.0],
            pair: ("x" + "y", 1),
            arr: ["p" + "q", "lit"],
            label: Label("hey" + "!")
        }
        val c = copy_of(&d)
        total += c.n.name.len() as i32 + c.n.id + c.pair.0.len() as i32
        total += c.arr[0].len() as i32 + c.arr[1].len() as i32 + c.label.0.len() as i32
        total += c.t[1] as i32
        val w = W { v: "ab" + "c" }
        val u = w.clone()
        total += u.v.len() as i32
    }
    total
}
"#,
        "clone_owning_fields",
    );
    // Per pass: 2 + 1 + 2 + 2 + 3 + 4 + 2 + 3.
    assert_eq!(output.status.code(), Some(57));
}
