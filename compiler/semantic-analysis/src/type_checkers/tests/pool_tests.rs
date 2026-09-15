// `pool { }` block escape rules.

#[allow(unused_imports)]
use super::{make_function, make_ident, make_type, semantic_errors};
use crate::errors::TypeError;

#[test]
fn a_pool_block_type_checks_like_a_scope() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut total: i32 = 0
    pool {
        val label = "a" + "b"
        total = total + label.len() as i32
    }
    total
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a pool body is an ordinary scope; got {errors:?}"
    );
}

#[test]
fn a_pool_label_is_accepted() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    pool training {
        val note = "x" + "y"
    }
    0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a labeled pool parses and checks: {errors:?}"
    );
}

#[test]
fn a_binding_declared_in_the_pool_may_hold_arena_memory() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    pool {
        mut text = "a" + "b"
        text = "c" + "d"
    }
    0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "the block's own bindings die with the arena; got {errors:?}"
    );
}

#[test]
fn storing_a_string_into_an_outer_binding_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut out: string = ""
    pool {
        out = "a" + "b"
    }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::PoolStoreEscapes { .. })),
        "an outer string binding outlives the arena; got {errors:?}"
    );
}

#[test]
fn storing_a_scalar_into_an_outer_binding_is_allowed() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut count: i32 = 0
    pool {
        count = count + 1
    }
    count
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a scalar carries no arena address; got {errors:?}"
    );
}

#[test]
fn a_return_may_not_leave_a_pool() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    pool {
        return 1
    }
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::PoolControlFlowEscapes { .. })),
        "a return jumps past the arena release; got {errors:?}"
    );
}

#[test]
fn a_break_targeting_an_outer_loop_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    for i in 0..3 {
        pool {
            break
        }
    }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::PoolControlFlowEscapes { .. })),
        "the loop encloses the pool, so the break leaves it; got {errors:?}"
    );
}

#[test]
fn a_break_targeting_a_loop_inside_the_pool_is_allowed() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    pool {
        for i in 0..3 {
            break
        }
    }
    0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a jump that stays inside the block is fine; got {errors:?}"
    );
}

#[test]
fn a_labeled_break_out_of_the_pool_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    outer: for i in 0..3 {
        pool {
            for j in 0..3 {
                break outer
            }
        }
    }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::PoolControlFlowEscapes { .. })),
        "the label names a loop outside the pool; got {errors:?}"
    );
}

#[test]
fn assigning_a_string_through_a_reference_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut text: string = ""
    val slot = &mut text
    pool {
        *slot = "a" + "b"
    }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::PoolStoreEscapes { .. })),
        "the referent is not known to die with the block; got {errors:?}"
    );
}

#[test]
fn a_pool_outside_a_function_body_still_scopes_its_bindings() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    pool {
        val hidden = 7
    }
    hidden
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UndefinedVariable { .. })),
        "a pool-local binding must not outlive the block; got {errors:?}"
    );
}
