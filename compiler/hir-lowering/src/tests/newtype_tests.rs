#[allow(unused_imports)]
use super::{
    binding_init, enum_names, function_body, function_names, impl_method_names, lower, struct_names,
};
use neuro_hir::{HirExprKind, HirStmt, HirType};

#[test]
fn newtype_construction_lowers_to_transparent_wrapper() {
    // `Meters(7)` becomes a NewtypeConstruct whose type is the newtype and whose
    // inner value carries the inner type.
    let program = lower("newtype Meters = i32\nfunc main() -> i32 { val m = Meters(7)\n m.0 }");
    let body = function_body(&program, "main");
    let init = binding_init(body, "m");
    assert_eq!(
        init.ty,
        HirType::Newtype {
            name: "Meters".to_string(),
            inner: Box::new(HirType::I32),
        }
    );
    let HirExprKind::NewtypeConstruct { name, value } = &init.kind else {
        panic!("expected a NewtypeConstruct node, got {:?}", init.kind);
    };
    assert_eq!(name, "Meters");
    assert_eq!(value.ty, HirType::I32);
}

#[test]
fn newtype_inner_access_lowers_to_inner_type() {
    // `m.0` becomes a NewtypeAccess whose type is the inner type.
    let program = lower("newtype Meters = i32\nfunc main() -> i32 { val m = Meters(7)\n m.0 }");
    let body = function_body(&program, "main");
    let HirStmt::Expr(access) = body.last().expect("tail expression") else {
        panic!("expected a trailing expression statement");
    };
    assert_eq!(access.ty, HirType::I32);
    assert!(
        matches!(access.kind, HirExprKind::NewtypeAccess { .. }),
        "expected a NewtypeAccess node, got {:?}",
        access.kind
    );
}
