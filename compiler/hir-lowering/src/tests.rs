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
