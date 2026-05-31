// Integration tests that compile and run every file in examples/
// Expected exit codes are derived from the program logic — see examples/README.md.
use std::path::PathBuf;
use std::process::Command;

fn neurc_path() -> PathBuf {
    std::env::current_exe()
        .expect("get current exe")
        .parent()
        .expect("parent")
        .parent()
        .expect("grandparent")
        .join("neurc")
}

fn compile_example(name: &str) -> Result<PathBuf, String> {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let src = workspace_root.join("examples").join(name);

    let tmp = std::env::temp_dir().join(format!("neuro_example_{}", name.replace(".nr", "")));

    let output = Command::new(neurc_path())
        .arg("compile")
        .arg(&src)
        .arg("-o")
        .arg(&tmp)
        .output()
        .map_err(|e| format!("failed to run neurc: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "compile failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(tmp)
}

fn run_example(exe: &PathBuf) -> i32 {
    Command::new(exe)
        .output()
        .expect("run example")
        .status
        .code()
        .unwrap_or(-1)
}

macro_rules! example_test {
    ($test_name:ident, $filename:literal, $expected:expr) => {
        #[test]
        fn $test_name() {
            let exe = compile_example($filename).expect(concat!("compile ", $filename));
            let code = run_example(&exe);
            assert_eq!(
                code, $expected,
                "example {} returned {} but expected {}",
                $filename, code, $expected
            );
        }
    };
}

example_test!(example_assignment_test, "assignment_test.nr", 15);
example_test!(example_bitwise_ops, "bitwise_ops.nr", 1);
example_test!(example_compound_assignment, "compound_assignment.nr", 21);
example_test!(example_constants, "constants.nr", 51);
example_test!(example_control_flow, "control_flow.nr", 31);
example_test!(example_division, "division.nr", 16);
example_test!(example_expression_returns, "expression_returns.nr", 37);
example_test!(example_extended_types, "extended_types.nr", 0);
example_test!(example_extended_types_test, "extended_types_test.nr", 0);
example_test!(example_factorial, "factorial.nr", 120);
example_test!(example_fibonacci, "fibonacci.nr", 55);
example_test!(example_float_ops, "float_ops.nr", 42);
example_test!(example_for_range_inclusive, "for_range_inclusive.nr", 15);
example_test!(example_for_range, "for_range.nr", 10);
example_test!(example_hello, "hello.nr", 26);
example_test!(example_integer_methods, "integer_methods.nr", 0);
example_test!(example_integer_overflow, "integer_overflow.nr", 55);
example_test!(example_integer_suffixes, "integer_suffixes.nr", 0);
example_test!(example_methods, "methods.nr", 42);
example_test!(example_milestone, "milestone.nr", 8);
example_test!(example_phase1_complete, "phase1_complete_test.nr", 0);
example_test!(example_string_test, "string_test.nr", 0);
example_test!(example_string_len, "string_len.nr", 0);
example_test!(example_structs, "structs.nr", 50);
example_test!(example_type_inference_demo, "type_inference_demo.nr", 42);
example_test!(example_underscore_separators, "underscore_separators.nr", 0);
example_test!(example_if_block_expressions, "if_block_expressions.nr", 149);
example_test!(example_while_true_lint, "while_true_lint.nr", 7);
