// End-to-end tests for the pipeline operator `|>`.
//
// The parser rewrites `value |> target` into a call, so what is worth proving here is
// that every target spelling reaches a *working* callee after that rewrite: a free
// function, a bound method, an associated function, a closure-typed binding, and an
// inline closure literal. The non-`Copy` case is the reason 2E had to land first, so a
// `string` carried through a chain gets a test of its own.

use crate::compile_harness::CompileTest;

#[test]
fn a_chain_applies_its_stages_left_to_right() {
    let test = CompileTest::new();
    let source = r#"
func double(x: i32) -> i32 { x * 2 }
func increment(x: i32) -> i32 { x + 1 }

func main() -> i32 {
    // (5 * 2) + 1 = 11, then 11 * 2 = 22. The reverse order would give 21.
    5 |> double |> increment |> double
}
"#;
    let exit = test
        .compile_and_run("pipe_chain.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 22);
}

#[test]
fn the_operator_binds_looser_than_every_other_operator() {
    let test = CompileTest::new();
    // Appendix B row 17. `2 + 3 * 4 |> half` must halve 14, not add 2 to half of 12.
    let source = r#"
func half(x: i32) -> i32 { x / 2 }

func main() -> i32 {
    2 + 3 * 4 |> half
}
"#;
    let exit = test
        .compile_and_run("pipe_precedence.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn a_bound_method_receives_the_piped_value() {
    let test = CompileTest::new();
    let source = r#"
struct Scaler {
    factor: i32
}

impl Scaler {
    func apply(&self, value: i32) -> i32 {
        value * self.factor
    }
}

func main() -> i32 {
    val scaler = Scaler { factor: 7 }
    6 |> scaler.apply
}
"#;
    let exit = test
        .compile_and_run("pipe_method.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn an_associated_function_is_a_pipeline_target() {
    let test = CompileTest::new();
    let source = r#"
struct Counter {
    value: i32
}

impl Counter {
    func of(value: i32) -> Counter {
        Counter { value: value }
    }
}

func main() -> i32 {
    val counter = 9 |> Counter::of
    counter.value
}
"#;
    let exit = test
        .compile_and_run("pipe_associated.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}

#[test]
fn a_closure_literal_is_a_pipeline_target() {
    let test = CompileTest::new();
    // Two literal stages in one chain: each gets its own temporary, so the second
    // does not shadow the first.
    let source = r#"
func main() -> i32 {
    4 |> (|x: i32| -> i32 { x * x }) |> (|x: i32| -> i32 { x + 4 })
}
"#;
    let exit = test
        .compile_and_run("pipe_closure_literal.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 20);
}

#[test]
fn a_closure_bound_to_a_name_is_a_pipeline_target() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val triple = |x: i32| -> i32 { x * 3 }
    11 |> triple
}
"#;
    let exit = test
        .compile_and_run("pipe_closure_binding.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 33);
}

#[test]
fn a_multiline_chain_parses_with_a_leading_operator() {
    let test = CompileTest::new();
    let source = r#"
func double(x: i32) -> i32 { x * 2 }
func increment(x: i32) -> i32 { x + 1 }

func main() -> i32 {
    val result = 3
        |> double
        |> increment
        |> double
    result
}
"#;
    let exit = test
        .compile_and_run("pipe_multiline.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 14);
}

#[test]
fn a_non_copy_value_survives_a_chain() {
    let test = CompileTest::new();
    // The by-value ABI the operator needs: each stage takes ownership of the `string`
    // the stage before it produced, and the intermediates are released, not leaked.
    let source = r#"
func shout(text: string) -> string {
    "{text}!"
}

func wrap(text: string) -> string {
    "[{text}]"
}

func main() -> i32 {
    val greeting = "hi"
    val loud = greeting |> shout |> wrap |> shout
    println("{loud}")
    loud.len() as i32
}
"#;
    let exit = test
        .compile_and_run("pipe_string.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 6);
}

#[test]
fn an_applied_call_is_not_a_pipeline_target() {
    let test = CompileTest::new();
    let source = r#"
func add(a: i32, b: i32) -> i32 { a + b }

func main() -> i32 {
    1 |> add(2)
}
"#;
    let err = test
        .check("pipe_applied_target.nr", source)
        .expect_err("an applied call must be rejected");
    assert!(
        err.contains("must be a function value"),
        "unexpected diagnostic: {err}"
    );
}
