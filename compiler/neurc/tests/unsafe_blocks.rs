// End-to-end coverage for `unsafe { }` block expressions (Phase 1.7 groundwork).
// `unsafe` is a reserved keyword and a distinct AST node, but is inert: it
// type-checks and lowers exactly like a bare block, evaluating to its trailing
// expression. These tests pin that behaviour through the full pipeline.
mod common;
use common::CompileTest;

#[test]
fn unsafe_block_as_implicit_return() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    unsafe { 7 }
}
"#;
    let exit = test
        .compile_and_run("unsafe_implicit_return.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn unsafe_block_binds_value_to_variable() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x = unsafe {
        val a = 20
        a + 22
    }
    x - 42
}
"#;
    let exit = test
        .compile_and_run("unsafe_bind_value.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn unsafe_block_as_void_statement() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total: i32 = 0
    unsafe {
        total = total + 5
    }
    total - 5
}
"#;
    let exit = test
        .compile_and_run("unsafe_void_stmt.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn unsafe_is_reserved_and_cannot_be_an_identifier() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val unsafe = 1
    0
}
"#;
    let source_path = test.write_source("unsafe_reserved.nr", source);
    assert!(
        test.compile(&source_path).is_err(),
        "using `unsafe` as an identifier must fail to compile"
    );
}
