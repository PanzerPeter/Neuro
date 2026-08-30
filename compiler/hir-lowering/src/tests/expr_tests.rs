#[allow(unused_imports)]
use super::{
    binding_init, enum_names, function_body, function_names, impl_method_names, lower, struct_names,
};
use crate::{lower_program, LoweringError};
use neuro_hir::{HirExprKind, HirStmt, HirType};

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
fn both_slice_spellings_lower_to_a_string_borrow() {
    // `.slice` counts bytes and `.char_slice` code points, but the index unit is a
    // backend concern: both lower to the same borrowed `&string`.
    let program = lower(
        "func main() -> i32 { val s = \"hello\"\n val b = s.slice(0..2)\n val c = s.char_slice(0..2)\n 0 }",
    );
    let body = function_body(&program, "main");
    let borrowed = HirType::Reference {
        inner: Box::new(HirType::String),
        mutable: false,
    };
    assert_eq!(binding_init(body, "b").ty, borrowed);
    assert_eq!(binding_init(body, "c").ty, borrowed);
}

#[test]
fn char_slice_keeps_its_range_argument() {
    // The range reaches the backend intact — it is the argument, not a folded pair of
    // offsets, because only the backend knows how to turn code points into bytes.
    let program =
        lower("func main() -> i32 { val s = \"hello\"\n val c = s.char_slice(1..=3)\n 0 }");
    let body = function_body(&program, "main");
    let HirExprKind::Call { args, .. } = &binding_init(body, "c").kind else {
        panic!("char_slice did not lower to a call");
    };
    assert!(matches!(
        args.as_slice(),
        [neuro_hir::HirExpr {
            kind: HirExprKind::Range {
                inclusive: true,
                ..
            },
            ..
        }]
    ));
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
fn output_builtins_lower_to_a_unit_call() {
    // `println` is not a declared function, so lowering must recognize it as a builtin
    // and give the call the unit type rather than failing to resolve it.
    let program = lower("func main() -> i32 { println(\"hi\")\n print(\"there\")\n 0 }");
    let body = function_body(&program, "main");

    for (index, name) in [(0, "println"), (1, "print")] {
        let HirStmt::Expr(call) = &body[index] else {
            panic!("statement {index} should be the builtin call");
        };
        assert_eq!(call.ty, HirType::Void);
        let HirExprKind::Call { callee, args } = &call.kind else {
            panic!("expected a call, got {:?}", call.kind);
        };
        assert_eq!(callee.kind, HirExprKind::Variable(name.to_string()));
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].ty, HirType::String);
    }
}

#[test]
fn a_user_function_shadows_the_output_builtin() {
    // A declared `println` wins, so the call carries that function's return type.
    let program = lower("func println(n: i32) -> i32 { n }\nfunc main() -> i32 { println(3) }");
    let body = function_body(&program, "main");
    let HirStmt::Expr(call) = &body[0] else {
        panic!("expected a trailing expression statement");
    };
    assert_eq!(call.ty, HirType::I32);
}
