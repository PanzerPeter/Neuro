use super::{binding_init, function_body, lower};
use neuro_hir::{HirExprKind, HirType};

/// The `&[i32]` a slice parameter and a `.slice(range)` call both produce.
fn shared_i32_slice() -> HirType {
    HirType::Reference {
        inner: Box::new(HirType::Slice(Box::new(HirType::I32))),
        mutable: false,
    }
}

const SUM_OVER_SLICE: &str = r#"
func sum(xs: &[i32]) -> i32 { return 0 }

func main() -> i32 {
    val fixed: [i32; 4] = [1, 2, 3, 4]
    mut grown: Vec<i32> = Vec::new()
    grown.push(5)
    val a = sum(&fixed)
    val b = sum(&grown)
    val c = sum(fixed.slice(1..3))
    return a + b + c
}
"#;

/// The `n`-th `val` initializer in `main`, as its call arguments.
fn call_argument_kind(src: &str, binding: &str) -> HirExprKind {
    let program = lower(src);
    let body = function_body(&program, "main");
    let init = binding_init(body, binding);
    let HirExprKind::Call { args, .. } = &init.kind else {
        panic!("expected a call, got {:?}", init.kind);
    };
    args[0].kind.clone()
}

#[test]
fn borrowing_an_array_for_a_slice_parameter_emits_the_unsizing_coercion() {
    assert!(
        matches!(
            call_argument_kind(SUM_OVER_SLICE, "a"),
            HirExprKind::SliceCoerce { .. }
        ),
        "`&[i32; 4]` must be unsized to `&[i32]` at the argument position"
    );
}

#[test]
fn borrowing_a_vec_for_a_slice_parameter_emits_the_unsizing_coercion() {
    assert!(
        matches!(
            call_argument_kind(SUM_OVER_SLICE, "b"),
            HirExprKind::SliceCoerce { .. }
        ),
        "`&Vec<i32>` must be unsized to `&[i32]` at the argument position"
    );
}

/// A `.slice(range)` result is already a `&[T]`, so re-wrapping it would coerce twice.
#[test]
fn an_existing_slice_argument_is_not_coerced_again() {
    assert!(
        !matches!(
            call_argument_kind(SUM_OVER_SLICE, "c"),
            HirExprKind::SliceCoerce { .. }
        ),
        "a `.slice` result is already the fat pointer the parameter wants"
    );
}

#[test]
fn slice_of_an_array_lowers_to_a_borrowed_slice_type() {
    let program = lower(
        r#"
func main() -> i32 {
    val fixed: [i32; 4] = [1, 2, 3, 4]
    val view = fixed.slice(1..3)
    return view[0]
}
"#,
    );
    let body = function_body(&program, "main");
    assert_eq!(binding_init(body, "view").ty, shared_i32_slice());
}

#[test]
fn slice_of_a_vec_lowers_to_a_borrowed_slice_type() {
    let program = lower(
        r#"
func main() -> i32 {
    mut grown: Vec<i32> = Vec::new()
    grown.push(1)
    val view = grown.slice(0..1)
    return view[0]
}
"#,
    );
    let body = function_body(&program, "main");
    assert_eq!(binding_init(body, "view").ty, shared_i32_slice());
}

#[test]
fn slice_len_lowers_to_u64() {
    let program = lower(
        r#"
func size(xs: &[i32]) -> u64 {
    xs.len()
}
func main() -> i32 { return 0 }
"#,
    );
    let body = function_body(&program, "size");
    let [neuro_hir::HirStmt::Expr(tail)] = body else {
        panic!("expected a single tail expression, got {body:?}");
    };
    assert_eq!(tail.ty, HirType::U64);
}
