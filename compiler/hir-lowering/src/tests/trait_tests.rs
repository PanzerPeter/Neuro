#[allow(unused_imports)]
use super::{
    binding_init, enum_names, function_body, function_names, impl_method_names, lower, struct_names,
};
use neuro_hir::{HirExprKind, HirItem, HirStmt, HirType};

#[test]
fn trait_default_method_lowers_as_concrete_method() {
    // A trait impl that omits a default method still lowers with that method present —
    // the parser injects it, so codegen sees an ordinary inherent method.
    let program = lower(
        r#"
trait Describable {
    func value(&self) -> i32
    func doubled(&self) -> i32 { self.value() * 2 }
}

struct Widget { id: i32 }

impl Describable for Widget {
    func value(&self) -> i32 { self.id }
}

func main() -> i32 {
    val w = Widget { id: 21 }
    w.doubled()
}
"#,
    );
    let names = impl_method_names(&program, "Widget");
    assert!(names.contains(&"value".to_string()), "explicit: {names:?}");
    assert!(
        names.contains(&"doubled".to_string()),
        "injected default: {names:?}"
    );
}

#[test]
fn generic_trait_bound_monomorphizes_to_concrete_dispatch() {
    // `total<T: Shape>` monomorphizes to a concrete instance whose `s.area()` dispatches
    // to `Square`'s impl method — traits carry no runtime cost.
    let program = lower(
        r#"
trait Shape { func area(&self) -> i32 }
@derive(Copy)
struct Square { side: i32 }
impl Shape for Square { func area(&self) -> i32 { self.side * self.side } }
func total<T: Shape>(s: &T) -> i32 { s.area() }
func main() -> i32 {
    val sq = Square { side: 5 }
    total(&sq)
}
"#,
    );
    // A monomorphized instance of `total` is emitted (the generic template is erased).
    assert!(
        function_names(&program)
            .iter()
            .any(|n| n.starts_with("total")),
        "a concrete `total` instance must be emitted: {:?}",
        function_names(&program)
    );
}

#[test]
fn binary_operator_desugars_to_method_call() {
    // `a + b` on a struct with `impl Add` lowers to the method call `a.add(b)`, not a
    // `Binary` node.
    let program = lower(
        r#"
@derive(Copy, Clone)
struct Vec2 { x: i32, y: i32 }
impl Add for Vec2 { type Output = Vec2
    func add(self, rhs: Vec2) -> Vec2 { Vec2 { x: self.x + rhs.x, y: self.y + rhs.y } } }
func main() -> i32 {
    val a = Vec2 { x: 1, y: 2 }
    val b = Vec2 { x: 3, y: 4 }
    val c = a + b
    0
}
"#,
    );
    let body = function_body(&program, "main");
    let c = binding_init(body, "c");
    assert_eq!(c.ty, HirType::Struct("Vec2".to_string()));
    match &c.kind {
        HirExprKind::Call { callee, args } => {
            assert_eq!(args.len(), 1, "add takes one explicit argument");
            match &callee.kind {
                HirExprKind::FieldAccess { field, .. } => assert_eq!(field, "add"),
                other => {
                    panic!("operator call callee must be a method field access, got {other:?}")
                }
            }
        }
        other => panic!("`a + b` must desugar to a Call, got {other:?}"),
    }
}

#[test]
fn comparison_operator_desugars_and_yields_bool() {
    // `a == b` lowers to `a.eq(&b)` returning bool; the argument is borrowed.
    let program = lower(
        r#"
@derive(Copy, Clone)
struct P { v: i32 }
impl PartialEq for P {
    func eq(&self, rhs: &P) -> bool { self.v == rhs.v }
    func ne(&self, rhs: &P) -> bool { self.v != rhs.v } }
func main() -> i32 {
    val a = P { v: 1 }
    val b = P { v: 2 }
    val e = a == b
    0
}
"#,
    );
    let body = function_body(&program, "main");
    let e = binding_init(body, "e");
    assert_eq!(e.ty, HirType::Bool);
    match &e.kind {
        HirExprKind::Call { callee, args } => {
            assert!(
                matches!(args[0].ty, HirType::Reference { .. }),
                "comparison method takes the rhs by reference"
            );
            match &callee.kind {
                HirExprKind::FieldAccess { field, .. } => assert_eq!(field, "eq"),
                other => panic!("callee must be `eq`, got {other:?}"),
            }
        }
        other => panic!("`a == b` must desugar to a Call, got {other:?}"),
    }
}

/// A trait declaration lowers to a vtable-layout item carrying its methods in
/// declaration order — the slot order every implementor shares.
#[test]
fn trait_lowers_to_its_vtable_method_order() {
    let program = lower(
        r#"
trait Shape {
    func area(&self) -> i32
    func sides(&self) -> i32 { 0 }
}
func main() -> i32 { 0 }
"#,
    );
    let trait_item = program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Trait(t) if t.name == "Shape" => Some(t),
            _ => None,
        })
        .expect("trait item should be lowered");
    assert_eq!(
        trait_item.methods,
        vec!["area".to_string(), "sides".to_string()]
    );
}

/// Passing `&Concrete` where `&dyn Trait` is expected inserts the unsizing coercion, so
/// the backend has an explicit node at which to build the fat pointer.
#[test]
fn concrete_reference_coerces_to_a_trait_object_at_a_call() {
    let program = lower(
        r#"
trait Shape {
    func area(&self) -> i32
}
@derive(Copy, Clone)
struct Square { side: i32 }
impl Shape for Square {
    func area(&self) -> i32 { self.side * self.side }
}
func measure(s: &dyn Shape) -> i32 { s.area() }
func main() -> i32 {
    val sq = Square { side: 2 }
    measure(&sq)
}
"#,
    );
    let body = function_body(&program, "main");
    let call = body
        .iter()
        .find_map(|stmt| match stmt {
            HirStmt::Expr(e) => Some(e),
            _ => None,
        })
        .expect("main should end in the call expression");
    let HirExprKind::Call { args, .. } = &call.kind else {
        panic!("expected a call, got {:?}", call.kind);
    };
    assert!(
        matches!(args[0].kind, HirExprKind::DynCoerce { .. }),
        "the `&Square` argument must be wrapped in a trait-object coercion"
    );
    assert_eq!(
        args[0].ty,
        HirType::Reference {
            inner: Box::new(HirType::DynObject("Shape".to_string())),
            mutable: false,
        }
    );
}

/// A `&dyn Trait` value already is a trait object, so forwarding it must not re-coerce.
#[test]
fn an_existing_trait_object_is_not_re_coerced() {
    let program = lower(
        r#"
trait Shape {
    func area(&self) -> i32
}
func inner(s: &dyn Shape) -> i32 { s.area() }
func outer(s: &dyn Shape) -> i32 { inner(s) }
func main() -> i32 { 0 }
"#,
    );
    let body = function_body(&program, "outer");
    let call = body
        .iter()
        .find_map(|stmt| match stmt {
            HirStmt::Expr(e) => Some(e),
            _ => None,
        })
        .expect("outer should forward the trait object");
    let HirExprKind::Call { args, .. } = &call.kind else {
        panic!("expected a call, got {:?}", call.kind);
    };
    assert!(
        matches!(args[0].kind, HirExprKind::Variable(_)),
        "forwarding a trait object must not insert a second coercion"
    );
}

/// Return-position `impl Trait` is static dispatch, so it resolves transparently to the
/// single concrete type the body constructs.
#[test]
fn impl_trait_return_resolves_to_the_concrete_type() {
    let program = lower(
        r#"
trait Shape {
    func area(&self) -> i32
}
@derive(Copy, Clone)
struct Square { side: i32 }
impl Shape for Square {
    func area(&self) -> i32 { self.side * self.side }
}
func make() -> impl Shape { Square { side: 3 } }
func main() -> i32 { 0 }
"#,
    );
    let make = program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Function(f) if f.name == "make" => Some(f),
            _ => None,
        })
        .expect("make should be lowered");
    assert_eq!(make.return_type, HirType::Struct("Square".to_string()));
}
