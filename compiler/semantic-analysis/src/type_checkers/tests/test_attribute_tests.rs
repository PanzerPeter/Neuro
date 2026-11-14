// `@test` form rules.

use super::semantic_errors;
use crate::errors::TypeError;

fn test_form_problems(source: &str) -> Vec<String> {
    semantic_errors(source)
        .into_iter()
        .filter_map(|error| match error {
            TypeError::TestForm { problem, .. } => Some(problem),
            _ => None,
        })
        .collect()
}

#[test]
fn a_test_of_no_parameters_returning_unit_is_accepted() {
    for source in [
        "@test\nfunc adds() {\n    assert(1 + 1 == 2)\n}\n",
        "@test\nfunc adds() -> () {\n    assert(1 + 1 == 2)\n}\n",
        "@allow(prefer_loop_over_while_true)\n@test\nfunc adds() {\n    while true { break }\n}\n",
    ] {
        let errors = semantic_errors(source);
        assert!(errors.is_empty(), "{source}: got {errors:?}");
    }
}

#[test]
fn test_is_refused_with_a_signature_the_harness_cannot_call() {
    for (source, problem) in [
        ("@test(slow)\nfunc t() {\n}\n", "takes no arguments"),
        ("@test(timeout: 5)\nfunc t() {\n}\n", "takes no arguments"),
        (
            "@test\nfunc t(x: i32) {\n    assert(x == 0)\n}\n",
            "function takes no parameters, found 1",
        ),
        ("@test\nfunc t<T>() {\n}\n", "function may not be generic"),
        (
            "@test\nfunc t() -> i32 {\n    0\n}\n",
            "function returns nothing, found `i32`",
        ),
    ] {
        assert_eq!(test_form_problems(source), [problem], "{source}");
    }
}

#[test]
fn test_is_refused_on_a_method_and_a_struct() {
    assert_eq!(
        test_form_problems(
            "struct S { x: i32 }\nimpl S {\n    @test\n    func t(&self) {\n    }\n}\n"
        ),
        ["marks a free function, not a method"]
    );
    assert_eq!(
        test_form_problems("@test\nstruct S { x: i32 }\n"),
        ["marks a function, not a struct"]
    );
}
