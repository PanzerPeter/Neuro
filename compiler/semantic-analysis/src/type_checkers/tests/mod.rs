use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{BinaryOp, Expr, FunctionDef, Parameter, Stmt};
use shared_types::{Identifier, Literal, Span};

fn make_ident(name: &str) -> Identifier {
    Identifier {
        name: name.to_string(),
        span: Span::new(0, 0),
    }
}

fn make_type(name: &str) -> ast_types::Type {
    ast_types::Type::Named(make_ident(name))
}

/// Helper to create a simple function for testing
fn make_function(
    name: &str,
    params: Vec<(String, String)>,
    return_type: Option<String>,
    body: Vec<Stmt>,
) -> FunctionDef {
    FunctionDef {
        name: make_ident(name),
        generics: Vec::new(),
        lifetimes: Vec::new(),
        where_predicates: Vec::new(),
        params: params
            .into_iter()
            .map(|(pname, pty)| Parameter {
                name: make_ident(&pname),
                ty: make_type(&pty),
                span: Span::new(0, 0),
            })
            .collect(),
        return_type: return_type.map(|rt| make_type(&rt)),
        body,
        attributes: Vec::new(),
        span: Span::new(0, 0),
    }
}

#[test]
fn test_integer_literal_infers_from_variable_declaration() {
    // val x: i64 = 42
    // The literal 42 should infer as i64, not i32
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("i64")),
        init: Some(Expr::Literal(Literal::Integer(42, None), Span::new(0, 2))),
        mutable: false,
        span: Span::new(0, 10),
    };

    assert!(checker.check_stmt(&stmt).is_some());
    assert!(!checker.has_errors());

    // Verify variable has correct type
    let symbol_info = checker.symbols.lookup("x").unwrap();
    assert_eq!(symbol_info.ty, Type::I64);
}

#[test]
fn test_integer_literal_infers_from_function_parameter() {
    // func foo(x: u32) {}
    // foo(42) - the literal 42 should infer as u32
    let mut checker = TypeChecker::new();

    // Define function
    let func = make_function(
        "foo",
        vec![("x".to_string(), "u32".to_string())],
        None,
        vec![],
    );

    checker.check_function(&func);
    assert!(!checker.has_errors());

    // Call with literal
    let call_expr = Expr::Call {
        func: Box::new(Expr::Identifier(make_ident("foo"))),
        type_args: Vec::new(),
        args: vec![Expr::Literal(Literal::Integer(42, None), Span::new(0, 2))],
        span: Span::new(0, 10),
    };

    let result_ty = checker.check_expr(&call_expr, None);
    assert_eq!(result_ty, Some(Type::Void));
    assert!(!checker.has_errors());
}

#[test]
fn test_integer_literal_out_of_range_i8() {
    // val x: i8 = 300  - should error (i8 max is 127)
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("i8")),
        init: Some(Expr::Literal(Literal::Integer(300, None), Span::new(0, 3))),
        mutable: false,
        span: Span::new(0, 10),
    };

    checker.check_stmt(&stmt);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert_eq!(errors.len(), 1);
    match &errors[0] {
        TypeError::IntegerLiteralOutOfRange { value, ty, .. } => {
            assert_eq!(*value, 300);
            assert_eq!(*ty, Type::I8);
        }
        _ => panic!("Expected IntegerLiteralOutOfRange error"),
    }
}

#[test]
fn test_integer_literal_negative_u32() {
    // val x: u32 = -42  - should error (u32 can't be negative)
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("u32")),
        init: Some(Expr::Literal(Literal::Integer(-42, None), Span::new(0, 3))),
        mutable: false,
        span: Span::new(0, 10),
    };

    checker.check_stmt(&stmt);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert_eq!(errors.len(), 1);
    match &errors[0] {
        TypeError::IntegerLiteralOutOfRange { value, ty, .. } => {
            assert_eq!(*value, -42);
            assert_eq!(*ty, Type::U32);
        }
        _ => panic!("Expected IntegerLiteralOutOfRange error"),
    }
}

#[test]
fn test_float_literal_infers_f32() {
    // val x: f32 = 2.5
    // The literal 2.5 should infer as f32, not f64
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("f32")),
        init: Some(Expr::Literal(Literal::Float(2.5, None), Span::new(0, 3))),
        mutable: false,
        span: Span::new(0, 10),
    };

    assert!(checker.check_stmt(&stmt).is_some());
    assert!(!checker.has_errors());

    // Verify variable has correct type
    let symbol_info = checker.symbols.lookup("x").unwrap();
    assert_eq!(symbol_info.ty, Type::F32);
}

#[test]
fn test_literal_inference_in_return() {
    // func foo() -> i16 { 42 }
    // The literal 42 should infer as i16
    let mut checker = TypeChecker::new();

    let func = make_function(
        "foo",
        vec![],
        Some("i16".to_string()),
        vec![Stmt::Expr(Expr::Literal(
            Literal::Integer(42, None),
            Span::new(0, 2),
        ))],
    );

    checker.check_function(&func);
    assert!(!checker.has_errors());
}

#[test]
fn test_literal_inference_in_assignment() {
    // mut x: u64 = 100
    // x = 200
    // The literal 200 should infer as u64
    let mut checker = TypeChecker::new();

    let decl = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("u64")),
        init: Some(Expr::Literal(Literal::Integer(100, None), Span::new(0, 3))),
        mutable: true,
        span: Span::new(0, 15),
    };

    checker.check_stmt(&decl);
    assert!(!checker.has_errors());

    let assign = Stmt::Assignment {
        target: make_ident("x"),
        value: Expr::Literal(Literal::Integer(200, None), Span::new(0, 3)),
        span: Span::new(0, 7),
    };

    checker.check_stmt(&assign);
    assert!(!checker.has_errors());
}

#[test]
fn test_literal_defaults_to_i32_without_context() {
    // val x = 42 (no type annotation)
    // Should default to i32
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: None,
        init: Some(Expr::Literal(Literal::Integer(42, None), Span::new(0, 2))),
        mutable: false,
        span: Span::new(0, 10),
    };

    checker.check_stmt(&stmt);
    assert!(!checker.has_errors());

    // Verify variable has i32 type
    let symbol_info = checker.symbols.lookup("x").unwrap();
    assert_eq!(symbol_info.ty, Type::I32);
}

#[test]
fn test_literal_defaults_to_f64_without_context() {
    // val x = 2.5 (no type annotation)
    // Should default to f64
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: None,
        init: Some(Expr::Literal(Literal::Float(2.5, None), Span::new(0, 3))),
        mutable: false,
        span: Span::new(0, 10),
    };

    checker.check_stmt(&stmt);
    assert!(!checker.has_errors());

    // Verify variable has f64 type
    let symbol_info = checker.symbols.lookup("x").unwrap();
    assert_eq!(symbol_info.ty, Type::F64);
}

#[test]
fn test_literal_inference_in_binary_operation() {
    // val x: i16 = 10
    // val y: i16 = x + 5
    // The literal 5 should infer as i16 from x
    let mut checker = TypeChecker::new();

    let decl_x = Stmt::VarDecl {
        name: make_ident("x"),
        ty: Some(make_type("i16")),
        init: Some(Expr::Literal(Literal::Integer(10, None), Span::new(0, 2))),
        mutable: false,
        span: Span::new(0, 10),
    };

    checker.check_stmt(&decl_x);
    assert!(!checker.has_errors());

    let decl_y = Stmt::VarDecl {
        name: make_ident("y"),
        ty: Some(make_type("i16")),
        init: Some(Expr::Binary {
            left: Box::new(Expr::Identifier(make_ident("x"))),
            op: BinaryOp::Add,
            right: Box::new(Expr::Literal(Literal::Integer(5, None), Span::new(0, 1))),
            span: Span::new(0, 5),
        }),
        mutable: false,
        span: Span::new(0, 15),
    };

    checker.check_stmt(&decl_y);
    assert!(!checker.has_errors());
}

#[test]
fn test_large_literal_fails_to_promote_to_i64() {
    // val x = 5000000000  (too large for i32)
    // Should NOT automatically use i64
    let mut checker = TypeChecker::new();

    let stmt = Stmt::VarDecl {
        name: make_ident("x"),
        ty: None,
        init: Some(Expr::Literal(
            Literal::Integer(5000000000, None),
            Span::new(0, 10),
        )),
        mutable: false,
        span: Span::new(0, 15),
    };

    checker.check_stmt(&stmt);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert_eq!(errors.len(), 1);
    match &errors[0] {
        TypeError::IntegerLiteralOutOfRange { value, ty, .. } => {
            assert_eq!(*value, 5000000000);
            assert_eq!(*ty, Type::I32);
        }
        _ => panic!("Expected IntegerLiteralOutOfRange error"),
    }
}

#[test]
fn test_for_range_accepts_integer_bounds() {
    let mut checker = TypeChecker::new();

    let stmt = Stmt::ForRange {
        label: None,
        iterator: make_ident("i"),
        start: Expr::Literal(Literal::Integer(0, None), Span::new(0, 1)),
        end: Expr::Literal(Literal::Integer(5, None), Span::new(4, 5)),
        inclusive: false,
        body: vec![Stmt::Continue {
            label: None,
            span: Span::new(8, 16),
        }],
        span: Span::new(0, 16),
    };

    checker.check_stmt(&stmt);
    assert!(!checker.has_errors());
}

#[test]
fn test_for_range_rejects_non_integer_bound() {
    let mut checker = TypeChecker::new();

    let stmt = Stmt::ForRange {
        label: None,
        iterator: make_ident("i"),
        start: Expr::Literal(Literal::Boolean(true), Span::new(0, 4)),
        end: Expr::Literal(Literal::Integer(5, None), Span::new(7, 8)),
        inclusive: false,
        body: vec![],
        span: Span::new(0, 12),
    };

    checker.check_stmt(&stmt);
    assert!(checker.has_errors());

    let errors = checker.into_errors();
    assert!(errors
        .iter()
        .any(|error| matches!(error, TypeError::InvalidForRangeType { .. })));
}

#[test]
fn test_labeled_break_resolves_to_enclosing_loop() {
    let mut checker = TypeChecker::new();

    let stmt = Stmt::Loop {
        label: Some(make_ident("outer")),
        body: vec![Stmt::Loop {
            label: None,
            body: vec![Stmt::Break {
                label: Some(make_ident("outer")),
                value: None,
                span: Span::new(0, 1),
            }],
            span: Span::new(0, 1),
        }],
        span: Span::new(0, 1),
    };

    checker.check_stmt(&stmt);
    assert!(!checker.has_errors());
}

#[test]
fn test_undefined_loop_label_is_rejected() {
    let mut checker = TypeChecker::new();

    let stmt = Stmt::Loop {
        label: Some(make_ident("outer")),
        body: vec![Stmt::Break {
            label: Some(make_ident("missing")),
            value: None,
            span: Span::new(0, 1),
        }],
        span: Span::new(0, 1),
    };

    checker.check_stmt(&stmt);
    assert!(checker.has_errors());
    let errors = checker.into_errors();
    assert!(errors
        .iter()
        .any(|error| matches!(error, TypeError::UndefinedLabel { .. })));
}

#[test]
fn test_break_outside_loop_still_rejected() {
    let mut checker = TypeChecker::new();

    let stmt = Stmt::Break {
        label: None,
        value: None,
        span: Span::new(0, 1),
    };

    checker.check_stmt(&stmt);
    let errors = checker.into_errors();
    assert!(errors
        .iter()
        .any(|error| matches!(error, TypeError::BreakOutsideLoop { .. })));
}

#[test]
fn test_loop_expression_takes_break_value_type() {
    let mut checker = TypeChecker::new();

    // loop { break 42 }
    let loop_expr = Expr::Loop {
        label: None,
        body: vec![Stmt::Break {
            label: None,
            value: Some(Expr::Literal(Literal::Integer(42, None), Span::new(0, 1))),
            span: Span::new(0, 1),
        }],
        span: Span::new(0, 1),
    };

    let ty = checker.check_expr(&loop_expr, None);
    assert!(!checker.has_errors());
    assert_eq!(ty, Some(Type::I32));
}

#[test]
fn test_break_value_type_disagreement_is_rejected() {
    let mut checker = TypeChecker::new();

    // loop { break 1 \n break "two" }
    let loop_expr = Expr::Loop {
        label: None,
        body: vec![
            Stmt::Break {
                label: None,
                value: Some(Expr::Literal(Literal::Integer(1, None), Span::new(0, 1))),
                span: Span::new(0, 1),
            },
            Stmt::Break {
                label: None,
                value: Some(Expr::Literal(
                    Literal::String("two".to_string()),
                    Span::new(2, 3),
                )),
                span: Span::new(2, 3),
            },
        ],
        span: Span::new(0, 3),
    };

    let _ = checker.check_expr(&loop_expr, None);
    let errors = checker.into_errors();
    assert!(errors
        .iter()
        .any(|error| matches!(error, TypeError::Mismatch { .. })));
}

#[test]
fn test_break_value_in_while_loop_is_rejected() {
    let mut checker = TypeChecker::new();

    // while true { break 5 } — `while` always yields unit.
    let stmt = Stmt::While {
        label: None,
        condition: Expr::Literal(Literal::Boolean(true), Span::new(0, 1)),
        body: vec![Stmt::Break {
            label: None,
            value: Some(Expr::Literal(Literal::Integer(5, None), Span::new(2, 3))),
            span: Span::new(2, 3),
        }],
        span: Span::new(0, 3),
    };

    checker.check_stmt(&stmt);
    let errors = checker.into_errors();
    assert!(errors
        .iter()
        .any(|error| matches!(error, TypeError::BreakValueInUnitLoop { .. })));
}

/// Type-check `source` end to end and return its errors (empty on success).
/// Used by the borrow-exclusivity tests below, which exercise multi-statement
/// programs more naturally through the parser than via hand-built AST.
fn semantic_errors(source: &str) -> Vec<TypeError> {
    let items = syntax_parsing::parse(source).expect("source should parse");
    match crate::type_check(&items) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}

fn is_borrow_conflict(error: &TypeError) -> bool {
    matches!(
        error,
        TypeError::CannotMutablyBorrowWhileBorrowed { .. }
            | TypeError::CannotBorrowWhileMutablyBorrowed { .. }
    )
}

#[test]
fn mutable_borrow_while_shared_borrow_is_live_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    val a: &i32 = &x
    val b: &mut i32 = &mut x
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotMutablyBorrowWhileBorrowed { .. })),
        "a `&mut` while a `&` is live must be rejected; got {errors:?}"
    );
}

#[test]
fn second_mutable_borrow_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    val a: &mut i32 = &mut x
    val b: &mut i32 = &mut x
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotMutablyBorrowWhileBorrowed { .. })),
        "a second `&mut` of the same place must be rejected; got {errors:?}"
    );
}

#[test]
fn shared_borrow_while_mutable_borrow_is_live_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    val a: &mut i32 = &mut x
    val b: &i32 = &x
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotBorrowWhileMutablyBorrowed { .. })),
        "a `&` while a `&mut` is live must be rejected; got {errors:?}"
    );
}

#[test]
fn multiple_shared_borrows_coexist() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    val a: &i32 = &x
    val b: &i32 = &x
    return 0
}
"#,
    );
    assert!(
        !errors.iter().any(is_borrow_conflict),
        "any number of `&` borrows may coexist; got {errors:?}"
    );
}

#[test]
fn mutable_and_shared_borrow_in_one_call_is_rejected() {
    let errors = semantic_errors(
        r#"
func two(a: &mut i32, b: &i32) -> i32 { *a }
func main() -> i32 {
    mut x: i32 = 5
    val r: i32 = two(&mut x, &x)
    return r
}
"#,
    );
    assert!(
        errors.iter().any(is_borrow_conflict),
        "a `&mut` and a `&` of the same place in one call must conflict; got {errors:?}"
    );
}

#[test]
fn borrow_released_at_end_of_block_scope() {
    // The branch-scoped `&mut x` ends when the `if` body scope is left, so the
    // later `&mut x` is free to take its own exclusive borrow.
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    if true {
        val a: &mut i32 = &mut x
        *a = 7
    }
    val b: &mut i32 = &mut x
    *b = 9
    return 0
}
"#,
    );
    assert!(
        !errors.iter().any(is_borrow_conflict),
        "a borrow must be released at the end of its scope; got {errors:?}"
    );
}

#[test]
fn transient_borrows_in_separate_statements_do_not_conflict() {
    let errors = semantic_errors(
        r#"
func inc(n: &mut i32) { *n = *n + 1 }
func main() -> i32 {
    mut x: i32 = 5
    inc(&mut x)
    inc(&mut x)
    return x
}
"#,
    );
    assert!(
        !errors.iter().any(is_borrow_conflict),
        "a `&mut` passed to a call ends with the call; got {errors:?}"
    );
}

#[test]
fn reassigning_a_reference_releases_its_previous_borrow() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut x: i32 = 5
    mut y: i32 = 9
    mut r: &mut i32 = &mut x
    r = &mut y
    val b: &mut i32 = &mut x
    *b = 1
    return 0
}
"#,
    );
    assert!(
        !errors.iter().any(is_borrow_conflict),
        "reassigning `r` away from `x` frees `x` to be borrowed again; got {errors:?}"
    );
}

fn returns_ref_to_local(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::ReturnsReferenceToLocal { .. }))
}

#[test]
fn returning_reference_to_local_is_rejected() {
    let errors = semantic_errors(
        r#"
func dangle() -> &i32 {
    val local: i32 = 5
    return &local
}
"#,
    );
    assert!(
        returns_ref_to_local(&errors),
        "borrowing a body-local and returning it dangles (§2.6); got {errors:?}"
    );
}

#[test]
fn returning_reference_to_owned_parameter_is_rejected() {
    let errors = semantic_errors(
        r#"
func dangle(n: i32) -> &i32 {
    return &n
}
"#,
    );
    assert!(
        returns_ref_to_local(&errors),
        "a by-value parameter does not outlive the call (§2.6); got {errors:?}"
    );
}

#[test]
fn returning_a_reference_parameter_is_accepted() {
    let errors = semantic_errors(
        r#"
func identity(r: &i32) -> &i32 {
    r
}
"#,
    );
    assert!(
        !returns_ref_to_local(&errors),
        "a reference parameter outlives the call (single-input elision, §2.6); got {errors:?}"
    );
}

#[test]
fn returning_reference_through_local_binding_is_rejected() {
    let errors = semantic_errors(
        r#"
func leak() -> &i32 {
    val local: i32 = 7
    val r: &i32 = &local
    r
}
"#,
    );
    assert!(
        returns_ref_to_local(&errors),
        "a local reference binding that borrows a local dangles transitively (§2.6); got {errors:?}"
    );
}

#[test]
fn returning_a_reference_in_an_if_arm_is_checked() {
    // The `else` arm yields a local reference binding whose borrowee is a body
    // local; the `then` arm yields the sound reference parameter. The walk into
    // both arms of the returned `if`-expression must still flag the bad arm.
    let errors = semantic_errors(
        r#"
func pick(cond: bool, r: &i32) -> &i32 {
    val local: i32 = 1
    val bad: &i32 = &local
    return if cond { r } else { bad }
}
"#,
    );
    assert!(
        returns_ref_to_local(&errors),
        "the dangling `else` arm must be caught even when another arm is sound (§2.6); got {errors:?}"
    );
}

#[test]
fn returning_a_borrow_of_self_is_accepted() {
    // `&self` outlives the call, so a method may return a borrow of `self` (the
    // `&self` lifetime is applied to method outputs, §2.6). Without `self` in the
    // outliving set this would be wrongly flagged as a local.
    let errors = semantic_errors(
        r#"
struct Wrapper { value: i32 }

impl Wrapper {
    func me(&self) -> &Wrapper {
        return &self
    }
}
"#,
    );
    assert!(
        !returns_ref_to_local(&errors),
        "a borrow of `&self` outlives the call (§2.6); got {errors:?}"
    );
}

#[test]
fn valid_drop_impl_is_accepted() {
    // `impl Drop for T { func drop(&mut self) }` is the recognized lang-item shape (§2.1).
    let errors = semantic_errors(
        r#"
struct Handle { id: i32 }

impl Drop for Handle {
    func drop(&mut self) { }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors.is_empty(),
        "a well-formed Drop impl must type-check; got {errors:?}"
    );
}

#[test]
fn drop_type_cannot_be_copy() {
    let errors = semantic_errors(
        r#"
@derive(Copy)
struct Bad { x: i32 }

impl Drop for Bad {
    func drop(&mut self) { }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::DropTypeCannotBeCopy { .. })),
        "a Copy type may not implement Drop (§2.1/§2.3); got {errors:?}"
    );
}

#[test]
fn drop_with_ref_self_is_rejected() {
    // `Drop::drop` must take `&mut self` so the destructor can release resources.
    let errors = semantic_errors(
        r#"
struct H { x: i32 }

impl Drop for H {
    func drop(&self) { }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::InvalidDropImpl { .. })),
        "drop must take `&mut self`; got {errors:?}"
    );
}

#[test]
fn drop_with_extra_method_is_rejected() {
    let errors = semantic_errors(
        r#"
struct H { x: i32 }

impl Drop for H {
    func drop(&mut self) { }
    func other(&self) -> i32 { self.x }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::InvalidDropImpl { .. })),
        "an `impl Drop` block must contain only `drop`; got {errors:?}"
    );
}

#[test]
fn array_literal_index_len_and_iteration_type_check() {
    // §3.1 — a typed array literal, index read/write, `.len()`, and `for x in arr`
    // / `for x in &arr` all type-check in one program.
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    mut b = [10, 20, 30]
    b[0] = 99
    val first: i32 = a[0]
    val n: u64 = a.len()
    mut total: i32 = 0
    for x in a {
        total = total + x
    }
    for y in &b {
        total = total + y
    }
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "valid array program; got {errors:?}");
}

#[test]
fn array_length_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ArrayLengthMismatch { .. })),
        "a literal whose length differs from the annotation must be rejected; got {errors:?}"
    );
}

#[test]
fn non_integer_array_index_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    val x: i32 = a[true]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::IndexNotInteger { .. })),
        "a non-integer index must be rejected; got {errors:?}"
    );
}

#[test]
fn indexing_a_non_array_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val n: i32 = 5
    val x: i32 = n[0]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NotIndexable { .. })),
        "indexing a non-array must be rejected; got {errors:?}"
    );
}

#[test]
fn heterogeneous_array_literal_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a = [1, true, 3]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "elements of differing types must be rejected; got {errors:?}"
    );
}

#[test]
fn array_of_non_copy_element_is_rejected() {
    // §3.1 — string elements need per-element move tracking, not yet supported.
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [string; 2] = ["a", "b"]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NonCopyArrayElement { .. })),
        "a non-Copy element array must be rejected; got {errors:?}"
    );
}

#[test]
fn assigning_through_index_of_immutable_array_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    a[0] = 9
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::AssignToImmutable { .. })),
        "writing an element of a `val` array must be rejected; got {errors:?}"
    );
}

// --- Newtype declarations (§3.15) ---------------------------------------------

#[test]
fn newtype_construction_and_inner_access_type_check() {
    let errors = semantic_errors(
        r#"
newtype Meters = i32
func main() -> i32 {
    val m: Meters = Meters(7)
    val raw: i32 = m.0
    return raw
}
"#,
    );
    assert!(
        errors.is_empty(),
        "valid newtype use should check; got {errors:?}"
    );
}

#[test]
fn newtype_is_not_interchangeable_with_inner() {
    // Assigning a `Meters` where an `i32` is expected is a type error — a newtype is
    // a DISTINCT nominal type, unlike a transparent `type` alias (§3.15).
    let errors = semantic_errors(
        r#"
newtype Meters = i32
func main() -> i32 {
    val m: Meters = Meters(7)
    val bad: i32 = m
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "newtype must not be assignable to its inner type; got {errors:?}"
    );
}

#[test]
fn two_newtypes_over_the_same_inner_are_distinct() {
    let errors = semantic_errors(
        r#"
newtype Meters = i32
newtype Seconds = i32
func take(m: Meters) -> i32 { m.0 }
func main() -> i32 {
    return take(Seconds(3))
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "passing Seconds where Meters is expected must be rejected; got {errors:?}"
    );
}

#[test]
fn newtype_construction_arity_is_enforced() {
    let errors = semantic_errors(
        r#"
newtype Meters = i32
func main() -> i32 {
    val m = Meters(1, 2)
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ArgumentCountMismatch { .. })),
        "newtype construction takes exactly one argument; got {errors:?}"
    );
}

#[test]
fn newtype_over_non_copy_inner_is_rejected() {
    let errors = semantic_errors(
        r#"
newtype Name = string
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NewtypeInnerNotCopy { .. })),
        "a newtype over a non-Copy inner type must be rejected; got {errors:?}"
    );
}

#[test]
fn cyclic_newtype_is_rejected() {
    let errors = semantic_errors(
        r#"
newtype A = B
newtype B = A
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CyclicNewtype { .. })),
        "a cyclic newtype must be rejected; got {errors:?}"
    );
}

#[test]
fn newtype_shadowing_a_builtin_is_rejected() {
    let errors = semantic_errors(
        r#"
newtype i32 = i64
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NewtypeAlreadyDefined { .. })),
        "a newtype may not shadow a builtin type name; got {errors:?}"
    );
}

#[test]
fn newtype_inner_index_out_of_range_is_rejected() {
    let errors = semantic_errors(
        r#"
newtype Meters = i32
func main() -> i32 {
    val m = Meters(5)
    return m.1
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TupleIndexOutOfBounds { .. })),
        "a newtype has only index 0; `.1` must be rejected; got {errors:?}"
    );
}

// --- Generics (§3.8) ---

#[test]
fn generic_call_type_checks_and_infers_return_type() {
    // A well-formed generic function and its inferable call are accepted; the call's
    // result flows into a matching return with no error.
    let errors = semantic_errors(
        r#"
func identity<T>(x: T) -> T { x }
func main() -> i32 { return identity(41) }
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn generic_body_operation_without_bound_is_rejected() {
    // A bare `T` has no `+` without a trait bound (the trait system does not exist yet).
    let errors = semantic_errors("func bad<T>(a: T, b: T) -> T { a + b }");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::InvalidBinaryOperator { .. })),
        "arithmetic on an unbounded generic must be rejected; got {errors:?}"
    );
}

#[test]
fn returning_concrete_value_as_type_parameter_is_rejected() {
    // Returning a concrete `string` where the type parameter `T` is expected is a
    // mismatch. (A parameter used only in return position is now permitted at the
    // declaration — turbofish may supply it — and is instead reported at the call
    // site when it cannot be inferred; see the const-generics test suite.)
    let errors = semantic_errors("func p<T>(s: string) -> T { s }");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ReturnTypeMismatch { .. })),
        "returning a concrete value as `T` must be reported; got {errors:?}"
    );
}

#[test]
fn non_inferable_generic_param_is_rejected_at_call_site() {
    // `U` appears in no parameter, so an un-turbofished call cannot bind it.
    let errors = semantic_errors(
        r#"
func firstof<T, U>(x: T) -> T { x }
func main() -> i32 { firstof(5) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::GenericParamNotInferable { .. })),
        "a call that cannot bind a type parameter must be reported; got {errors:?}"
    );
}

#[test]
fn non_copy_generic_argument_is_rejected() {
    let errors = semantic_errors(
        r#"
func identity<T>(x: T) -> T { x }
func main() -> i32 { val s = identity("hi")
    return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::GenericArgumentNotCopy { .. })),
        "a non-Copy type argument must be reported; got {errors:?}"
    );
}

#[test]
fn generic_argument_count_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
func pair<T, U>(a: T, b: U) -> T { a }
func main() -> i32 { return pair(1) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ArgumentCountMismatch { .. })),
        "a wrong-arity generic call must be reported; got {errors:?}"
    );
}

#[test]
fn generic_struct_literal_infers_and_field_access_types() {
    // A generic struct literal infers its type arguments; a field read yields the
    // concrete field type, so `p.first + 1` type-checks as i32 arithmetic (§3.8).
    let errors = semantic_errors(
        r#"
struct Pair<T, U> { first: T, second: U }
func main() -> i32 {
    val p = Pair { first: 10, second: 2.5 }
    return p.first + 1
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn generic_impl_method_dispatches_on_instance() {
    // A generic inherent impl's method returns the concrete element type.
    let errors = semantic_errors(
        r#"
struct Wrapper<T> { value: T }
impl<T> Wrapper<T> { func get(&self) -> T { self.value } }
func main() -> i32 {
    val w = Wrapper { value: 7 }
    return w.get()
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn bare_generic_struct_without_arguments_is_rejected() {
    let errors = semantic_errors(
        r#"
struct Box<T> { v: T }
func take(b: Box) -> i32 { 0 }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::GenericStructNeedsArgs { .. })),
        "a generic struct used without type arguments must be rejected; got {errors:?}"
    );
}

#[test]
fn non_copy_generic_struct_argument_is_rejected() {
    let errors = semantic_errors(
        r#"
struct Box<T> { v: T }
func main() -> i32 {
    val b = Box { v: "hi" }
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::GenericArgumentNotCopy { .. })),
        "a non-Copy struct type argument must be rejected; got {errors:?}"
    );
}

#[test]
fn generic_struct_field_type_mismatch_is_rejected() {
    // Once a type parameter is bound by one field, another field's value must agree.
    let errors = semantic_errors(
        r#"
struct Same<T> { a: T, b: T }
func main() -> i32 {
    val s = Same { a: 1, b: true }
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "conflicting inferred type arguments must be rejected; got {errors:?}"
    );
}

#[test]
fn declared_lifetime_annotation_is_accepted() {
    // The canonical §2.6 example: an explicit lifetime declared in `<'a>` and used on
    // reference parameters and the return type type-checks (returning a borrowed
    // parameter is already permitted by elision).
    let errors = semantic_errors(
        r#"
func longest<'a>(a: &'a string, b: &'a string) -> &'a string {
    if a.len() > b.len() { a } else { b }
}
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors.is_empty(),
        "declared lifetime should type-check; got {errors:?}"
    );
}

#[test]
fn undeclared_lifetime_is_rejected() {
    // `'b` is used but never declared in the parameter list — a well-formedness error.
    let errors = semantic_errors(
        r#"
func f<'a>(a: &'b string) -> i32 { 0 }
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UndeclaredLifetime { name, .. } if name == "b")),
        "an undeclared lifetime must be rejected; got {errors:?}"
    );
}

#[test]
fn lifetime_annotation_does_not_change_reference_type() {
    // `&'a string` is the same type as `&string`: passing an unannotated borrow to a
    // parameter typed with an explicit lifetime type-checks.
    let errors = semantic_errors(
        r#"
func take<'a>(s: &'a string) -> i32 { s.len() as i32 }
func main() -> i32 {
    val msg = "hi"
    return take(&msg)
}
"#,
    );
    assert!(
        errors.is_empty(),
        "an explicit lifetime must not change the reference type; got {errors:?}"
    );
}
