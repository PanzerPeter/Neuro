// `pool { }` block escape rules.

use super::semantic_errors;
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
fn building_a_string_into_an_outer_binding_is_allowed() {
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
        errors.is_empty(),
        "the concatenation is routed off the arena because 'out' outlives it; got {errors:?}"
    );
}

#[test]
fn storing_the_blocks_own_allocation_into_an_outer_binding_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut out: string = ""
    pool {
        val local = "a" + "b"
        out = local
    }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::PoolStoreEscapes { .. })),
        "routing cannot move a buffer the block already took from the arena; got {errors:?}"
    );
}

/// `string` `+` copies both operands into a buffer of its own, so a routed store keeps
/// none of the block's arena memory even when an operand is the block's (BUG-104).
#[test]
fn concatenating_the_blocks_own_allocation_into_an_outer_binding_is_allowed() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut out: string = ""
    pool {
        val local = "a" + "b"
        out = local + "c"
        out = "c" + local + local
    }
    0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a string concatenation copies its operands into a routed buffer; got {errors:?}"
    );
}

/// Interpolation with text around its holes builds one fresh buffer from copies.
#[test]
fn interpolating_an_arena_binding_into_an_outer_binding_is_allowed() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut out: string = ""
    pool {
        val local = "a" + "b"
        out = "row {local}"
    }
    0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "the rendered text is copied into a routed buffer; got {errors:?}"
    );
}

/// A lone hole is still walked: nothing but the hole's own rendering is in it.
#[test]
fn a_lone_hole_over_an_arena_binding_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut out: string = ""
    pool {
        val local = "a" + "b"
        out = "{local}"
    }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::PoolStoreEscapes { .. })),
        "a lone hole is not proven to copy; got {errors:?}"
    );
}

/// A user `Add` may hand its left operand back, so only a `string` left operand is the
/// copying concatenation.
#[test]
fn a_user_operator_over_an_arena_binding_is_rejected() {
    let errors = semantic_errors(
        r#"
@derive(Copy)
struct Text { s: &string }
impl Add for Text {
    type Output = Text
    func add(self, other: Text) -> Text { self }
}
func main() -> i32 {
    val empty = ""
    mut out = Text { s: &empty }
    pool {
        val local = "a" + "b"
        out = Text { s: &local } + Text { s: &empty }
    }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::PoolStoreEscapes { .. })),
        "a user operator may return its arena operand; got {errors:?}"
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
fn building_a_string_through_a_reference_is_allowed() {
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
        errors.is_empty(),
        "the referent is not known to die with the block, so the store is routed; got {errors:?}"
    );
}

#[test]
fn assigning_an_arena_value_through_a_reference_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut text: string = ""
    val slot = &mut text
    pool {
        val local = "a" + "b"
        *slot = local
    }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::PoolStoreEscapes { .. })),
        "the referent may outlive the block and the value is the arena's; got {errors:?}"
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

/// A reference the block declares may point at a place that outlives it, and the
/// backend emits a store through it at the point it is written, inside the arena.
#[test]
fn a_store_through_a_reference_the_block_declares_is_rejected() {
    let errors = semantic_errors(
        r#"
struct Holder { t: Tensor<f32, [4]> }
func main() -> i32 {
    mut h = Holder { t: Tensor::<f32, [4]>::zeros() }
    pool {
        val r = &mut h
        r.t = Tensor::<f32, [4]>::ones()
    }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::PoolStoreEscapes { .. })),
        "the referent outlives the block; got {errors:?}"
    );
}

/// What the block's own reference stores is still accepted when nothing in it can hold
/// arena memory.
#[test]
fn a_literal_or_scalar_through_a_reference_the_block_declares_is_allowed() {
    let errors = semantic_errors(
        r#"
struct Holder { name: string, n: i32 }
func main() -> i32 {
    mut h = Holder { name: "", n: 0 }
    pool {
        val r = &mut h
        r.name = "fixed"
        r.n = 3
    }
    0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "nothing here is the arena's; got {errors:?}"
    );
}

/// A `&mut` the block declares is a channel back to whatever it borrows, for a callee as
/// much as for a store the block writes.
#[test]
fn a_callee_reached_through_a_reference_the_block_declares_is_rejected() {
    let errors = semantic_errors(
        r#"
struct Holder { t: Tensor<f32, [4]> }
impl Holder {
    func set(&mut self, n: Tensor<f32, [4]>) { self.t = n }
}
func put(dst: &mut Tensor<f32, [4]>, v: Tensor<f32, [4]>) { *dst = v }
func main() -> i32 {
    mut h = Holder { t: Tensor::<f32, [4]>::zeros() }
    mut g = Tensor::<f32, [4]>::zeros()
    mut v: Vec<string> = Vec::new()
    pool {
        val r = &mut h
        r.set(Tensor::<f32, [4]>::ones())
        val q = &mut g
        put(q, Tensor::<f32, [4]>::ones())
        val w = &mut v
        w.push("a" + "b")
    }
    0
}
"#,
    );
    let retained = errors
        .iter()
        .filter(|e| matches!(e, TypeError::PoolValueRetainedByCallee { .. }))
        .count();
    assert_eq!(retained, 3, "every channel is caught; got {errors:?}");
}

/// Argument binding rewrites a reordered named call into a block of temporaries ahead
/// of the call; each temporary stands for an initializer the walk proves.
#[test]
fn a_block_of_proven_declarations_into_an_outer_binding_is_allowed() {
    let errors = semantic_errors(
        r#"
func a() -> string { "a" + "1" }
func b() -> string { "b" + "2" }
func mk(first: string, second: string) -> string { first + second }
func main() -> i32 {
    mut out = ""
    pool p {
        out = {
            val t0 = b()
            val t1 = a()
            mk(t1, t0)
        }
        out = {
            val t = "x" + "y"
            t
        }
    }
    0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "every temporary is proven; got {errors:?}"
    );
}

/// An arm that declares a binding is walked the same way (BUG-049).
#[test]
fn a_branch_whose_arm_declares_a_binding_is_allowed() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    mut out: Tensor<i32, [2, 2]> = [[0, 0], [0, 0]]
    pool scratch {
        out = if a[0, 0] > 0 {
            val k = 2
            &a * k
        } else {
            a.clone()
        }
    }
    out[1, 1]
}
"#,
    );
    assert!(errors.is_empty(), "both arms are routed; got {errors:?}");
}

/// A declaration bound to the block's arena memory, or any statement that is not a
/// declaration, keeps the block refused.
#[test]
fn a_block_over_arena_memory_is_rejected() {
    for body in [
        "out = {\n val t = local\n t\n }",
        "out = if true {\n val t = local\n t\n } else { \"\" }",
        "out = {\n mut t = \"\"\n t = local\n t\n }",
    ] {
        let errors = semantic_errors(&format!(
            "func main() -> i32 {{
    mut out = \"\"
    pool {{
        val local = \"a\" + \"b\"
        {body}
    }}
    0
}}"
        ));
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeError::PoolStoreEscapes { .. })),
            "{body}: the block hands arena memory out; got {errors:?}"
        );
    }
}
