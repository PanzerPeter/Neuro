// End-to-end tests for the function composition operator `>>`.
//
// A composition is a *value*: it is bound, passed, and called later, which is what
// separates it from `|>`. So what these prove is that the closure the chain lowers to
// behaves like any other function value, and that the diagnostics reject the operand
// spellings the closure model cannot carry yet.

use crate::compile_harness::CompileTest;

#[test]
fn a_bound_composition_applies_its_stages_left_to_right() {
    let test = CompileTest::new();
    let source = r#"
func double(x: i32) -> i32 { x * 2 }
func increment(x: i32) -> i32 { x + 1 }

func main() -> i32 {
    // (5 * 2) + 1 = 11. The reverse order would give 12.
    val prepare = double >> increment
    prepare(5)
}
"#;
    let exit = test
        .compile_and_run("compose_order.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 11);
}

#[test]
fn a_composition_is_reusable() {
    let test = CompileTest::new();
    let source = r#"
func double(x: i32) -> i32 { x * 2 }
func increment(x: i32) -> i32 { x + 1 }

func main() -> i32 {
    val prepare = double >> increment
    prepare(1) + prepare(2) + prepare(3)
}
"#;
    let exit = test
        .compile_and_run("compose_reuse.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 3 + 5 + 7);
}

#[test]
fn a_chain_of_three_runs_in_order() {
    let test = CompileTest::new();
    let source = r#"
func double(x: i32) -> i32 { x * 2 }
func increment(x: i32) -> i32 { x + 1 }
func square(x: i32) -> i32 { x * x }

func main() -> i32 {
    val pipeline = increment >> double >> square
    pipeline(2)
}
"#;
    let exit = test
        .compile_and_run("compose_three.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 36);
}

#[test]
fn the_stages_may_change_type_along_the_chain() {
    let test = CompileTest::new();
    let source = r#"
func label(x: i32) -> string { "n={x}" }
func width(s: string) -> i32 { s.len() as i32 }

func main() -> i32 {
    val measure = label >> width
    measure(42)
}
"#;
    let exit = test
        .compile_and_run("compose_types.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn a_composition_passes_as_a_function_argument() {
    let test = CompileTest::new();
    let source = r#"
func double(x: i32) -> i32 { x * 2 }
func increment(x: i32) -> i32 { x + 1 }
func apply(value: i32, f: (i32) -> i32) -> i32 { f(value) }

func main() -> i32 {
    apply(4, double >> increment)
}
"#;
    let exit = test
        .compile_and_run("compose_argument.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}

#[test]
fn a_composition_applied_where_it_is_written_needs_no_binding() {
    let test = CompileTest::new();
    let source = r#"
func double(x: i32) -> i32 { x * 2 }
func increment(x: i32) -> i32 { x + 1 }

func main() -> i32 {
    (double >> increment)(10)
}
"#;
    let exit = test
        .compile_and_run("compose_applied.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 21);
}

#[test]
fn a_pipeline_applies_the_composition_to_its_left_value() {
    let test = CompileTest::new();
    // Appendix B rows 16 and 17: `>>` binds tighter than `|>`, so the whole chain is
    // one pipeline target.
    let source = r#"
func double(x: i32) -> i32 { x * 2 }
func increment(x: i32) -> i32 { x + 1 }

func main() -> i32 {
    10 |> double >> increment
}
"#;
    let exit = test
        .compile_and_run("compose_piped.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 21);
}

#[test]
fn a_composition_carries_an_owned_string_through() {
    let test = CompileTest::new();
    // The by-value ABI 2E built: each stage takes its argument by value, so the
    // intermediate buffers are released as the chain runs.
    let source = r#"
func shout(text: string) -> string { text + "!" }
func wrap(text: string) -> string { "[" + text + "]" }

func main() -> i32 {
    val decorate = shout >> wrap
    val decorated = decorate("hi")
    println("{decorated}")
    decorated.len() as i32
}
"#;
    let exit = test
        .compile_and_run("compose_string.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}

#[test]
fn a_stage_that_cannot_take_the_previous_result_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func label(x: i32) -> string { "n={x}" }
func double(x: i32) -> i32 { x * 2 }

func main() -> i32 {
    val broken = label >> double
    broken(1)
}
"#;
    let error = test
        .check("compose_mismatch.nr", source)
        .expect_err("a stage mismatch should be reported");
    assert!(
        error.contains("cannot take"),
        "expected a stage mismatch, got: {}",
        error
    );
}

#[test]
fn a_multi_parameter_stage_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func add(a: i32, b: i32) -> i32 { a + b }
func double(x: i32) -> i32 { x * 2 }

func main() -> i32 {
    val broken = add >> double
    broken(1)
}
"#;
    let error = test
        .check("compose_arity.nr", source)
        .expect_err("a two-parameter stage should be reported");
    assert!(
        error.contains("one parameter"),
        "expected an arity diagnostic, got: {}",
        error
    );
}

#[test]
fn a_function_typed_binding_is_rejected_by_name() {
    let test = CompileTest::new();
    // Composing a *value* would capture it, and a function type is not Copy: the
    // diagnostic points at the `func` instead of letting the capture rule report it.
    let source = r#"
func double(x: i32) -> i32 { x * 2 }

func main() -> i32 {
    val held = |x: i32| -> i32 { x + 1 }
    val broken = held >> double
    broken(1)
}
"#;
    let error = test
        .check("compose_binding.nr", source)
        .expect_err("a bound function value should be reported");
    assert!(
        error.contains("is a binding"),
        "expected a binding diagnostic, got: {}",
        error
    );
}

#[test]
fn a_generic_stage_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func identity<T>(value: T) -> T { value }
func double(x: i32) -> i32 { x * 2 }

func main() -> i32 {
    val broken = identity >> double
    broken(1)
}
"#;
    let error = test
        .check("compose_generic.nr", source)
        .expect_err("a generic stage should be reported");
    assert!(
        error.contains("generic function"),
        "expected a generic diagnostic, got: {}",
        error
    );
}
