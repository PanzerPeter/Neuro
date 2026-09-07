// Regression: a binding declared inside a nested block must stop existing when that
// block ends.
//
// The type checker has always scoped these correctly (naming a block-local after its
// block is `undefined variable`), but the backend's name maps are flat per function:
// a `val` inside a block overwrote the outer entry for its name and nothing put the
// outer one back at block exit. Every statement after the block then resolved the name
// to the inner binding's slot, which the drop machinery had already released for an
// owning type. So the same program could read a stale value or write through a freed
// buffer, and neither the type checker nor the LLVM verifier could see it: with both
// bindings at the same type the IR is well formed, just wrong.
//
// One case per block form, because each opens its scope through a different lowering.
mod common;
use common::CompileTest;

#[test]
fn regression_bare_block_binding_does_not_outlive_its_block() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x = 1
    {
        val x = 10
    }
    return x
}
"#;
    let exit = test
        .compile_and_run("bare_block_scope.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1);
}

#[test]
fn regression_block_expression_binding_does_not_outlive_its_block() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x = 1
    val inner = {
        val x = 10
        x
    }
    return inner * 10 + x
}
"#;
    let exit = test
        .compile_and_run("block_expr_scope.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 101);
}

#[test]
fn regression_if_branch_binding_does_not_outlive_its_branch() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x = 1
    if true {
        val x = 10
    } else {
        val x = 20
    }
    return x
}
"#;
    let exit = test
        .compile_and_run("if_branch_scope.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1);
}

#[test]
fn regression_loop_body_binding_does_not_outlive_the_loop() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x = 1
    for i in 0..3 {
        val x = 10
    }
    mut j = 0
    while j < 2 {
        val x = 20
        j = j + 1
    }
    return x
}
"#;
    let exit = test
        .compile_and_run("loop_body_scope.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1);
}

#[test]
fn regression_match_arm_binding_does_not_outlive_the_arm() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x = 1
    val r = match 0 {
        0 => {
            val x = 40
            x
        }
        _ => 0
    }
    return r + x
}
"#;
    let exit = test
        .compile_and_run("match_arm_scope.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 41);
}

// The half the exit code alone cannot prove: an owning binding is *released* at block
// exit, so resolving the outer name to the inner slot afterwards is a use-after-free,
// not merely a wrong read. Pushing through the leaked name used to grow the freed
// vector and report its length.
#[test]
fn regression_block_local_collection_is_not_reachable_after_its_block() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    {
        mut v: Vec<i32> = Vec::new()
        v.push(99)
        v.push(98)
    }
    v.push(1)
    return v.len() as i32
}
"#;
    let exit = test
        .compile_and_run("block_scope_vec.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1);
}

#[test]
fn regression_block_local_string_is_not_reachable_after_its_block() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val s = "outer"
    {
        val s = "a much longer inner string"
    }
    return s.len() as i32
}
"#;
    let exit = test
        .compile_and_run("block_scope_string.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}

// A shadow of a different type is the one shape the LLVM verifier could already catch,
// because the leaked binding makes the function return the wrong LLVM type. It belongs
// here so the fix is checked on the typed path too, not only on the silent one.
#[test]
fn regression_block_local_shadow_of_another_type_does_not_escape() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x = 1
    {
        val x = "hello"
    }
    return x
}
"#;
    let exit = test
        .compile_and_run("block_scope_retype.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1);
}
