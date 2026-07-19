//! Unit tests: lower representative programs and assert on the re-derived HIR types.

use crate::{lower_program, LoweringError};
use neuro_hir::{HirExpr, HirExprKind, HirItem, HirProgram, HirStmt, HirType};

/// Parse and lower `src`, expecting success.
fn lower(src: &str) -> HirProgram {
    let ast = syntax_parsing::parse(src).expect("source should parse");
    lower_program(&ast).expect("well-typed program should lower")
}

/// The body of the first function named `name`.
fn function_body<'a>(program: &'a HirProgram, name: &str) -> &'a [HirStmt] {
    for item in &program.items {
        if let HirItem::Function(f) = item {
            if f.name == name {
                return &f.body;
            }
        }
    }
    panic!("function '{}' not found", name);
}

/// The initializer expression of the first `val`/`mut` named `name` in `body`.
fn binding_init<'a>(body: &'a [HirStmt], name: &str) -> &'a HirExpr {
    for stmt in body {
        if let HirStmt::VarDecl { name: n, init, .. } = stmt {
            if n == name {
                return init.as_ref().expect("binding should have an initializer");
            }
        }
    }
    panic!("binding '{}' not found", name);
}

#[test]
fn literal_default_and_suffix_types() {
    let program =
        lower("func main() -> i32 { val a = 1\n val b = 2i64\n val c = 3.0\n val d = 1.5f32\n 0 }");
    let body = function_body(&program, "main");
    assert_eq!(binding_init(body, "a").ty, HirType::I32);
    assert_eq!(binding_init(body, "b").ty, HirType::I64);
    assert_eq!(binding_init(body, "c").ty, HirType::F64);
    assert_eq!(binding_init(body, "d").ty, HirType::F32);
}

#[test]
fn declared_type_drives_literal_inference() {
    let program = lower("func main() -> i32 { val a: u8 = 255\n 0 }");
    let body = function_body(&program, "main");
    // The annotation flows into the literal: 255 is a u8, not the default i32.
    assert_eq!(binding_init(body, "a").ty, HirType::U8);
}

#[test]
fn comparison_yields_bool_and_arithmetic_keeps_operand_type() {
    let program =
        lower("func main() -> i32 { val a: i64 = 7\n val cmp = a < a\n val sum = a + a\n 0 }");
    let body = function_body(&program, "main");
    assert_eq!(binding_init(body, "cmp").ty, HirType::Bool);
    assert_eq!(binding_init(body, "sum").ty, HirType::I64);
}

#[test]
fn tuple_literal_and_index_carry_resolved_types() {
    // A tuple literal is typed `HirType::Tuple`, and `t.N` reads the N-th type.
    let program = lower(
        "func main() -> i32 { val t: (i32, bool) = (1, true)\n val first = t.0\n val second = t.1\n 0 }",
    );
    let body = function_body(&program, "main");
    assert_eq!(
        binding_init(body, "t").ty,
        HirType::Tuple(vec![HirType::I32, HirType::Bool])
    );
    let first = binding_init(body, "first");
    assert_eq!(first.ty, HirType::I32);
    assert!(matches!(
        first.kind,
        HirExprKind::TupleIndex { index: 0, .. }
    ));
    assert_eq!(binding_init(body, "second").ty, HirType::Bool);
}

#[test]
fn paren_is_normalized_away() {
    let program = lower("func main() -> i32 { val a = (1 + 2)\n 0 }");
    let body = function_body(&program, "main");
    let init = binding_init(body, "a");
    // The grouping node is dropped: the initializer is the binary directly.
    assert!(matches!(init.kind, HirExprKind::Binary { .. }));
    assert_eq!(init.ty, HirType::I32);
}

#[test]
fn string_concat_yields_string_and_len_yields_u64() {
    let program = lower("func main() -> i32 { val s = \"a\" + \"b\"\n val n = s.len()\n 0 }");
    let body = function_body(&program, "main");
    assert_eq!(binding_init(body, "s").ty, HirType::String);
    assert_eq!(binding_init(body, "n").ty, HirType::U64);
}

#[test]
fn struct_field_access_and_method_call_resolve_types() {
    let src = "struct Point { x: f64, y: f64 }\n\
               impl Point {\n\
                 func origin() -> Point { Point { x: 0.0, y: 0.0 } }\n\
                 func get_x(&self) -> f64 { self.x }\n\
               }\n\
               func main() -> i32 {\n\
                 val p = Point::origin()\n\
                 val x = p.get_x()\n\
                 val fx = p.x\n\
                 0\n\
               }";
    let program = lower(src);
    let body = function_body(&program, "main");
    assert_eq!(
        binding_init(body, "p").ty,
        HirType::Struct("Point".to_string())
    );
    assert_eq!(binding_init(body, "x").ty, HirType::F64);
    assert_eq!(binding_init(body, "fx").ty, HirType::F64);
}

#[test]
fn reference_and_deref_types() {
    let program = lower("func main() -> i32 { mut a: i32 = 1\n val r = &mut a\n val v = *r\n 0 }");
    let body = function_body(&program, "main");
    assert_eq!(
        binding_init(body, "r").ty,
        HirType::Reference {
            inner: Box::new(HirType::I32),
            mutable: true,
        }
    );
    assert_eq!(binding_init(body, "v").ty, HirType::I32);
}

#[test]
fn array_literal_index_and_len() {
    let program = lower(
        "func main() -> i32 { val arr = [1, 2, 3]\n val first = arr[0]\n val n = arr.len()\n 0 }",
    );
    let body = function_body(&program, "main");
    assert_eq!(
        binding_init(body, "arr").ty,
        HirType::Array {
            element: Box::new(HirType::I32),
            size: 3,
        }
    );
    assert_eq!(binding_init(body, "first").ty, HirType::I32);
    assert_eq!(binding_init(body, "n").ty, HirType::U64);
}

#[test]
fn if_expression_and_loop_value_types() {
    let program = lower(
        "func main() -> i32 {\n\
           val a: i64 = 1\n\
           val cond = if a < a { 1i64 } else { 2i64 }\n\
           val looped = loop { break 7i64 }\n\
           0\n\
         }",
    );
    let body = function_body(&program, "main");
    assert_eq!(binding_init(body, "cond").ty, HirType::I64);
    assert_eq!(binding_init(body, "looped").ty, HirType::I64);
}

#[test]
fn trailing_expression_typed_against_return_type() {
    // The implicit return `42` is typed as the declared i64, not the default i32.
    let program = lower("func answer() -> i64 { 42 }");
    let body = function_body(&program, "answer");
    let HirStmt::Expr(tail) = body.last().expect("non-empty body") else {
        panic!("expected a trailing expression statement");
    };
    assert_eq!(tail.ty, HirType::I64);
}

#[test]
fn unresolved_binding_is_an_error_not_a_panic() {
    // Lowering is defensive: an un-type-checked program yields an error, never a panic.
    let ast =
        syntax_parsing::parse("func main() -> i32 { val a = missing\n 0 }").expect("source parses");
    assert!(matches!(
        lower_program(&ast),
        Err(LoweringError::UnresolvedBinding { .. })
    ));
}

#[test]
fn array_rest_remainder_is_sized_subarray() {
    // `val [a, ..rest] = arr` lowers `rest` to an ArrayRest holding the tail.
    // For a `[i64; 4]` source with one leading element, rest is `[i64; 3]`.
    let program = lower(
        "func main() -> i32 { val arr: [i64; 4] = [1, 2, 3, 4]\n val [a, ..rest] = arr\n 0 }",
    );
    let body = function_body(&program, "main");
    let rest = binding_init(body, "rest");
    let HirExprKind::ArrayRest { start, .. } = &rest.kind else {
        panic!("rest binding should lower to an ArrayRest node");
    };
    assert_eq!(*start, 1);
    assert_eq!(
        rest.ty,
        HirType::Array {
            element: Box::new(HirType::I64),
            size: 3,
        }
    );
}

#[test]
fn enum_construction_lowers_to_enum_construct() {
    // Each surface form normalizes to a single `EnumConstruct` node carrying
    // the variant's discriminant tag and a payload in declared field order. The
    // struct-variant form reorders provided fields into declaration order.
    let program = lower(
        "enum Shape { Circle { radius: f64 }, Rectangle { width: f64, height: f64 } }\n\
         enum Msg { Quit, Move(i32, i32) }\n\
         func main() -> i32 {\n\
            val a = Msg::Quit\n\
            val b = Msg::Move(1, 2)\n\
            val c = Shape::Rectangle { height: 3.0, width: 2.0 }\n\
            0\n\
         }",
    );
    let body = function_body(&program, "main");

    let a = binding_init(body, "a");
    let HirExprKind::EnumConstruct { tag, payload, .. } = &a.kind else {
        panic!("unit variant should lower to EnumConstruct");
    };
    assert_eq!(*tag, 0);
    assert!(payload.is_empty());
    assert_eq!(a.ty, HirType::Enum("Msg".to_string()));

    let b = binding_init(body, "b");
    let HirExprKind::EnumConstruct { tag, payload, .. } = &b.kind else {
        panic!("tuple variant should lower to EnumConstruct");
    };
    assert_eq!(*tag, 1);
    assert_eq!(payload.len(), 2);
    assert_eq!(payload[0].ty, HirType::I32);

    let c = binding_init(body, "c");
    let HirExprKind::EnumConstruct { tag, payload, .. } = &c.kind else {
        panic!("struct variant should lower to EnumConstruct");
    };
    // Rectangle is the second variant of Shape.
    assert_eq!(*tag, 1);
    // Fields are reordered into declaration order: width (2.0) then height (3.0).
    assert_eq!(payload.len(), 2);
    assert_eq!(c.ty, HirType::Enum("Shape".to_string()));
}

#[test]
fn match_lowers_to_resolved_tests_and_bindings() {
    use neuro_hir::{HirBindingSource, HirMatchTest};

    let program = lower(
        r#"
enum Shape { Circle(i32), Rect { w: i32, h: i32 }, Unit }
func area(s: Shape) -> i32 {
    match s {
        Shape::Circle(r) => r,
        Shape::Rect { w, h } => w + h,
        Shape::Unit => 0
    }
}
func classify(n: i32) -> i32 {
    match n {
        1 | 2 => 10,
        3..=9 => 20,
        _ => 0
    }
}
func main() -> i32 { area(Shape::Unit) + classify(1) }
"#,
    );

    let area = function_body(&program, "area");
    let HirStmt::Expr(m) = &area[0] else {
        panic!("area body should be a match expression statement");
    };
    let HirExprKind::Match { arms, .. } = &m.kind else {
        panic!("expected a match expression");
    };
    assert_eq!(arms.len(), 3);
    assert_eq!(m.ty, HirType::I32);

    // Circle(r): tag 0, one payload-slot binding of type i32.
    assert!(matches!(arms[0].tests[0], HirMatchTest::Tag { tag: 0 }));
    assert_eq!(arms[0].bindings.len(), 1);
    assert_eq!(arms[0].bindings[0].name, "r");
    assert!(matches!(
        arms[0].bindings[0].source,
        HirBindingSource::EnumPayload { slot: 0 }
    ));
    // Rect { w, h }: tag 1, two payload-slot bindings.
    assert!(matches!(arms[1].tests[0], HirMatchTest::Tag { tag: 1 }));
    assert_eq!(arms[1].bindings.len(), 2);

    let classify = function_body(&program, "classify");
    let HirStmt::Expr(m) = &classify[0] else {
        panic!("classify body should be a match expression statement");
    };
    let HirExprKind::Match { arms, .. } = &m.kind else {
        panic!("expected a match expression");
    };
    // Or-pattern: two IntEq tests, no bindings.
    assert_eq!(arms[0].tests.len(), 2);
    assert!(matches!(arms[0].tests[0], HirMatchTest::IntEq { value: 1 }));
    assert!(matches!(arms[0].tests[1], HirMatchTest::IntEq { value: 2 }));
    // Inclusive range 3..=9.
    assert!(matches!(
        arms[1].tests[0],
        HirMatchTest::IntRange { lo: 3, hi: 9 }
    ));
    // Wildcard catch-all.
    assert!(matches!(arms[2].tests[0], HirMatchTest::Wildcard));
}

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

/// Names of every free function in the lowered program (monomorphized instances
/// included; generic templates are erased).
fn function_names(program: &HirProgram) -> Vec<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            HirItem::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect()
}

/// Names of every struct in the lowered program (monomorphized instances included;
/// generic templates are erased).
fn struct_names(program: &HirProgram) -> Vec<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            HirItem::Struct(s) => Some(s.name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn generic_struct_monomorphizes_per_type_argument() {
    // `Pair<T, U>` used at two distinct argument sets yields two concrete structs,
    // and the generic template does not survive into the HIR.
    let program = lower(
        "struct Pair<T, U> { first: T, second: U }\n\
         func main() -> i32 { val a = Pair { first: 1, second: 2.0 }\n\
         val b = Pair { first: true, second: 3 }\n 0 }",
    );
    let names = struct_names(&program);
    assert!(
        !names.iter().any(|n| n == "Pair"),
        "the generic template must not survive into the HIR: {names:?}"
    );
    let instances = names.iter().filter(|n| n.starts_with("Pair_g")).count();
    assert_eq!(
        instances, 2,
        "one struct instance per distinct type-argument set: {names:?}"
    );
    // A monomorphized struct name must never contain `__`, which codegen uses to split
    // a method symbol back into its receiver struct name.
    assert!(
        names.iter().all(|n| !n.contains("__")),
        "instance names must avoid `__`: {names:?}"
    );
}

#[test]
fn generic_struct_literal_type_is_the_concrete_instance() {
    let program = lower(
        "struct Wrapper<T> { value: T }\n\
         func main() -> i32 { val w = Wrapper { value: 9 }\n 0 }",
    );
    let body = function_body(&program, "main");
    let init = binding_init(body, "w");
    let HirType::Struct(name) = &init.ty else {
        panic!("expected a struct type, got {:?}", init.ty);
    };
    assert!(
        name.starts_with("Wrapper_g") && name.contains("i32"),
        "literal should carry the monomorphized instance type, got {name}"
    );
}

#[test]
fn generic_impl_emits_one_impl_per_instance() {
    // Two instances of `Cell<T>` each get their own emitted impl with a `get` method.
    let program = lower(
        "struct Cell<T> { value: T }\n\
         impl<T> Cell<T> { func get(&self) -> T { self.value } }\n\
         func main() -> i32 { val a = Cell { value: 1 }\n val b = Cell { value: true }\n a.get() }",
    );
    let impl_count = program
        .items
        .iter()
        .filter(|i| matches!(i, HirItem::Impl(_)))
        .count();
    assert_eq!(
        impl_count, 2,
        "one impl block per monomorphized struct instance"
    );
}

#[test]
fn generic_function_monomorphizes_per_type_argument() {
    // `identity<T>` used at i32 and f64 produces two concrete instances and no
    // generic template survives into the HIR.
    let program = lower(
        "func identity<T>(x: T) -> T { x }\n\
         func main() -> i32 { val a = identity(1)\n val b = identity(2.0)\n a }",
    );
    let names = function_names(&program);
    assert!(names.contains(&"main".to_string()));
    assert!(
        !names.contains(&"identity".to_string()),
        "the generic template must not survive into the HIR: {:?}",
        names
    );
    let instances = names.iter().filter(|n| n.starts_with("identity_g")).count();
    assert_eq!(
        instances, 2,
        "one instance per distinct type argument: {:?}",
        names
    );
    // A monomorphized function name must never contain `__`: that separator is reserved
    // for `Receiver__method`, and codegen splits on it to recover the receiver struct.
    assert!(
        names.iter().all(|n| !n.contains("__")),
        "instance names must avoid `__`: {names:?}"
    );
}

#[test]
fn repeated_instantiation_is_emitted_once() {
    // Two calls at the same type share a single monomorphized instance.
    let program = lower(
        "func identity<T>(x: T) -> T { x }\n\
         func main() -> i32 { val a = identity(1)\n identity(a) }",
    );
    let instances = function_names(&program)
        .iter()
        .filter(|n| n.starts_with("identity_g"))
        .count();
    assert_eq!(instances, 1);
}

#[test]
fn generic_instance_return_type_is_concrete() {
    // The call expression's type is the substituted concrete type, never a placeholder.
    let program = lower("func identity<T>(x: T) -> T { x }\nfunc main() -> i32 { identity(7) }");
    let body = function_body(&program, "main");
    let HirStmt::Expr(call) = body.last().expect("tail expression") else {
        panic!("expected a trailing expression statement");
    };
    assert_eq!(call.ty, HirType::I32);
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

#[test]
fn const_generic_function_monomorphizes_by_value() {
    // Two calls with different array lengths produce two distinct instances, each
    // named by its concrete const value (mangled `..._cN`).
    let program = lower(
        "func first<const N: u32>(a: [i32; N]) -> i32 { a[0] }\n\
         func main() -> i32 {\n\
             val two: [i32; 2] = [1, 2]\n\
             val three: [i32; 3] = [1, 2, 3]\n\
             first(two) + first(three)\n\
         }",
    );
    let names: Vec<&str> = program
        .items
        .iter()
        .filter_map(|it| match it {
            HirItem::Function(f) => Some(f.name.as_str()),
            _ => None,
        })
        .collect();
    assert!(names
        .iter()
        .any(|n| n.contains("first") && n.contains("c2")));
    assert!(names
        .iter()
        .any(|n| n.contains("first") && n.contains("c3")));
}

#[test]
fn const_generic_struct_field_has_concrete_size() {
    // The `[T; CAP]` field is lowered to a concrete `[i32; 4]` in the instance.
    let program = lower(
        "struct Buffer<T, const CAP: u32> { data: [T; CAP] }\n\
         func main() -> i32 {\n\
             val b = Buffer { data: [1, 2, 3, 4] }\n\
             b.data[0]\n\
         }",
    );
    let has_sized_field = program.items.iter().any(|it| match it {
        HirItem::Struct(s) => s.fields.iter().any(|f| {
            matches!(&f.ty, HirType::Array { element, size }
                if **element == HirType::I32 && *size == 4)
        }),
        _ => false,
    });
    assert!(
        has_sized_field,
        "expected a monomorphized struct with a concrete [i32; 4] field"
    );
}

#[test]
fn const_param_value_reference_lowers_to_literal() {
    // A const parameter used as a value lowers to its concrete integer literal.
    let program = lower(
        "func cap<const N: u32>(a: [i32; N]) -> u32 { N }\n\
         func main() -> i32 {\n\
             val xs: [i32; 4] = [1, 2, 3, 4]\n\
             cap(xs) as i32\n\
         }",
    );
    let instance = program.items.iter().find_map(|it| match it {
        HirItem::Function(f) if f.name.contains("cap") && f.name.contains("c4") => Some(f),
        _ => None,
    });
    let instance = instance.expect("cap<4> instance should exist");
    // The body's trailing expression is the integer literal 4, typed u32.
    let last = instance.body.last().expect("non-empty body");
    let HirStmt::Expr(e) = last else {
        panic!("expected a trailing expression")
    };
    assert!(matches!(
        &e.kind,
        HirExprKind::Literal(shared_types::Literal::Integer(4, _))
    ));
    assert_eq!(e.ty, HirType::U32);
}

/// The method names of the first impl on `type_name` in the lowered program.
fn impl_method_names(program: &HirProgram, type_name: &str) -> Vec<String> {
    for item in &program.items {
        if let HirItem::Impl(imp) = item {
            if imp.type_name == type_name {
                return imp.methods.iter().map(|m| m.name.clone()).collect();
            }
        }
    }
    panic!("impl for '{}' not found", type_name);
}

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
