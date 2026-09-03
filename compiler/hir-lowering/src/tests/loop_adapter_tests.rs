//! The `.map(f)` / `.filter(p)` desugar. No HIR node is added for it, so these tests
//! assert on the wrapping `Block` and the statements it folds into the loop body.

use super::{function_body, lower};
use neuro_hir::{HirExprKind, HirStmt, HirType};

/// The `Block` an adapted `for` lowers to, taken from the first block statement of
/// `main`.
fn adapted_block(body: &[HirStmt]) -> &[HirStmt] {
    for stmt in body {
        let HirStmt::Expr(expr) = stmt else { continue };
        if let HirExprKind::Block { stmts } = &expr.kind {
            return stmts;
        }
    }
    panic!("no adapter block in {body:?}");
}

/// The adapter's function is evaluated once, ahead of the loop: `xs.map(make_rule())`
/// must not rebuild its rule per element.
#[test]
fn each_adapter_function_is_bound_once_before_the_loop() {
    let program = lower(
        r#"func main() -> i32 {
             for v in [1, 2, 3].map(|x: i32| -> i32 { x * 2 }) { }
             0
           }"#,
    );
    let stmts = adapted_block(function_body(&program, "main"));

    let HirStmt::VarDecl { name, ty, .. } = &stmts[0] else {
        panic!("expected the function binding, got {:?}", stmts[0]);
    };
    assert!(
        name.starts_with("__adapt_fn_"),
        "generated name, got {name}"
    );
    assert!(
        matches!(ty, HirType::Function { .. }),
        "the binding holds the adapter's function, got {ty:?}"
    );
    assert!(
        matches!(stmts[1], HirStmt::ForEach { .. }),
        "the loop follows its function bindings, got {:?}",
        stmts[1]
    );
}

/// A counted head keeps its counted loop: the chain is folded into the body, not
/// turned into an iterator value.
#[test]
fn an_adapted_array_head_keeps_its_counted_loop() {
    let program = lower(
        r#"func main() -> i32 {
             for v in [1, 2, 3].filter(|x: i32| -> bool { x > 1 }) { }
             0
           }"#,
    );
    let stmts = adapted_block(function_body(&program, "main"));
    let HirStmt::ForEach { iterator, body, .. } = &stmts[1] else {
        panic!("expected a counted for-each, got {:?}", stmts[1]);
    };
    assert!(
        iterator.starts_with("__adapt_elem_"),
        "the loop binds the source element, got {iterator}"
    );
    assert!(
        matches!(body[0], HirStmt::If { .. }),
        "a filter opens the body with its skip test, got {:?}",
        body[0]
    );
}

#[test]
fn an_adapted_range_head_keeps_its_counted_loop() {
    let program = lower(
        r#"func main() -> i32 {
             for v in (0..4).map(|x: i32| -> i32 { x + 1 }) { }
             0
           }"#,
    );
    let stmts = adapted_block(function_body(&program, "main"));
    assert!(
        matches!(stmts[1], HirStmt::ForRange { .. }),
        "expected a counted range loop, got {:?}",
        stmts[1]
    );
}

/// The user's binding is the LAST statement the chain emits, so it names whatever
/// the final adapter produced.
#[test]
fn the_user_binding_closes_the_chain() {
    let program = lower(
        r#"func main() -> i32 {
             for v in [1, 2].map(|x: i32| -> f64 { x as f64 }) { }
             0
           }"#,
    );
    let stmts = adapted_block(function_body(&program, "main"));
    let HirStmt::ForEach { body, .. } = &stmts[1] else {
        panic!("expected a counted for-each, got {:?}", stmts[1]);
    };
    let HirStmt::VarDecl { name, ty, .. } = &body[1] else {
        panic!("expected the user binding, got {:?}", body[1]);
    };
    assert_eq!(name, "v");
    assert_eq!(*ty, HirType::F64, "`.map` retypes the binding");
}

/// An enumerated adapted head counts what the chain YIELDS. The counted loop's own
/// index counts source steps, so it is dropped in favour of a cursor advanced past
/// the filters.
#[test]
fn an_enumerated_adapted_head_counts_yielded_elements() {
    let program = lower(
        r#"func main() -> i32 {
             for (i, v) in [1, 2, 3].filter(|x: i32| -> bool { x > 1 }).enumerate() { }
             0
           }"#,
    );
    let stmts = adapted_block(function_body(&program, "main"));

    let cursor = stmts
        .iter()
        .find_map(|stmt| match stmt {
            HirStmt::VarDecl { name, mutable, .. } if name.starts_with("__adapt_pos_") => {
                Some(*mutable)
            }
            _ => None,
        })
        .expect("an enumerated adapted head reserves a cursor");
    assert!(cursor, "the cursor is advanced per yielded element");

    let Some(HirStmt::ForEach { index, body, .. }) = stmts
        .iter()
        .find(|stmt| matches!(stmt, HirStmt::ForEach { .. }))
    else {
        panic!("expected a counted for-each in {stmts:?}");
    };
    assert!(
        index.is_none(),
        "the counted index counts source steps and must not be used"
    );
    // Skip test, then the position read, then its advance, then the user binding.
    assert!(matches!(body[0], HirStmt::If { .. }));
    assert!(
        matches!(&body[1], HirStmt::VarDecl { name, .. } if name == "i"),
        "the position is read after the filters, got {:?}",
        body[1]
    );
    assert!(
        matches!(body[2], HirStmt::Assignment { .. }),
        "the cursor advances before the user's statements, got {:?}",
        body[2]
    );
}
