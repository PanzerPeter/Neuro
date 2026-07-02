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
    // §3.2: a tuple literal is typed `HirType::Tuple`, and `t.N` reads the N-th type.
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
    // §3.2: `val [a, ..rest] = arr` lowers `rest` to an ArrayRest holding the tail.
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
    // §3.5: each surface form normalizes to a single `EnumConstruct` node carrying
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
    // inner value carries the inner type (§3.15).
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
    // `m.0` becomes a NewtypeAccess whose type is the inner type (§3.15).
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
