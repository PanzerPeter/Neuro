// Regression: a stack slot allocated inside a loop body must not be re-allocated per
// iteration.
//
// Every local binding and every result/scratch slot used to be `alloca`'d at the
// current builder position, so a slot inside a loop body grew the stack by one slot per
// iteration until the process ran out of it — an ordinary counted loop segfaulted once
// it ran long enough. LLVM's `mem2reg` could not rescue it either: the pass only
// promotes allocas already in the entry block, so the leak survived `-O3`.
//
// Each program below runs enough iterations to exhaust a default 8 MiB stack under the
// old lowering (~1M slots) while still finishing in milliseconds once the slot is
// hoisted and the loop becomes optimizable.
mod common;
use common::CompileTest;

#[test]
fn regression_local_binding_in_loop_does_not_grow_the_stack() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut i = 0
    mut acc = 0
    while i < 2000000 {
        val v = i % 2
        acc = acc + v
        i = i + 1
    }
    if acc == 1000000 { 7 } else { 1 }
}
"#;
    let exit = test
        .compile_and_run("loop_slot_binding.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn regression_if_expression_slot_in_loop_does_not_grow_the_stack() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut i = 0
    mut acc = 0
    while i < 2000000 {
        val v = if i % 2 == 0 { 1 } else { 0 }
        acc = acc + v
        i = i + 1
    }
    if acc == 1000000 { 7 } else { 1 }
}
"#;
    let exit = test
        .compile_and_run("loop_slot_ifexpr.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn regression_match_slots_in_loop_do_not_grow_the_stack() {
    let test = CompileTest::new();
    let source = r#"
enum E { A(i32), B }

func main() -> i32 {
    mut i = 0
    mut acc = 0
    while i < 2000000 {
        val e = if i % 2 == 0 { E::A(1) } else { E::B }
        acc = acc + match e { E::A(v) => v, E::B => 0 }
        i = i + 1
    }
    if acc == 1000000 { 7 } else { 1 }
}
"#;
    let exit = test
        .compile_and_run("loop_slot_match.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn regression_inner_for_loop_slot_does_not_grow_the_stack() {
    // The inner `for`'s induction variable is allocated once per outer iteration.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut i = 0
    mut acc = 0
    while i < 2000000 {
        for j in 0..2 { acc = acc + j }
        i = i + 1
    }
    if acc == 2000000 { 7 } else { 1 }
}
"#;
    let exit = test
        .compile_and_run("loop_slot_nested_for.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn recursion_still_gets_a_fresh_slot_per_frame() {
    // Hoisting to the entry block is per-call-frame, so recursion must be unaffected.
    let test = CompileTest::new();
    let source = r#"
func sum_to(n: i32) -> i32 {
    if n == 0 { 0 } else {
        val rest = sum_to(n - 1)
        n + rest
    }
}

func main() -> i32 {
    sum_to(10)
}
"#;
    let exit = test
        .compile_and_run("loop_slot_recursion.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 55);
}
