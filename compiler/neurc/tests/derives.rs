// End-to-end tests for `@derive` argument validation and the `Debug` / `PartialEq`
// derives.
//
// `@derive(Debug)` gives a struct the `{x:?}` rendering; `@derive(PartialEq)` gives it
// field-wise `==` / `!=`. Neither routes through a method, so these drive the whole
// pipeline and assert on the bytes the program wrote and the exit code it returned.
use std::path::PathBuf;
use std::process::{Command, Output};

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()` — that assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

/// Compile and run `source`, returning the process output.
fn run_program(source: &str, tag: &str) -> Output {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("neuro_derive_{tag}.nr"));
    let exe = dir.join(format!("neuro_derive_{tag}"));
    std::fs::write(&src, source).expect("write source");

    let output = Command::new(neurc_path())
        .arg("compile")
        .arg(&src)
        .arg("-O")
        .arg("0")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run neurc");
    assert!(
        output.status.success(),
        "compile failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    Command::new(&exe).output().expect("run executable")
}

/// Type-check `source`, returning the compiler's combined diagnostics on rejection.
fn check_error(source: &str, tag: &str) -> String {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("neuro_derive_{tag}.nr"));
    std::fs::write(&src, source).expect("write source");

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&src)
        .output()
        .expect("run neurc");

    assert!(!output.status.success(), "expected `check` to reject {tag}");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
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

/// A field-less struct renders as its bare name — there are no braces to hold nothing.
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
fn unimplemented_derive_argument_is_rejected() {
    let stderr = check_error(
        r#"
@derive(Hashable)
struct P { x: i32 }

func main() -> i32 { return 0 }
"#,
        "pending",
    );
    assert!(
        stderr.contains("not implemented yet"),
        "expected an unimplemented-derive diagnostic, got: {stderr}"
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

/// The hand-written impl still works — it is only the *combination* that is rejected.
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
