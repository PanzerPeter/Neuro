use super::{binding_init, function_body, lower};
use crate::{LoweringError, lower_program};
use neuro_hir::{
    HirExpr, HirExprKind, HirExtent, HirFunction, HirInterpPart, HirItem, HirProgram, HirStmt,
    HirTensorAxis, HirType,
};

/// A one-line rendering of an expression, for probing and for assertions.
pub(super) fn show(e: &HirExpr) -> String {
    match &e.kind {
        HirExprKind::Literal(l) => format!("{l:?}"),
        HirExprKind::Variable(n) => n.clone(),
        HirExprKind::Binary { op, left, right } => {
            format!("({} {op:?} {})", show(left), show(right))
        }
        HirExprKind::Unary { op, operand } => format!("{op:?}({})", show(operand)),
        HirExprKind::Call { callee, args } => format!(
            "{}({})",
            show(callee),
            args.iter().map(show).collect::<Vec<_>>().join(", ")
        ),
        HirExprKind::Reference { operand, mutable } => {
            format!("&{}{}", if *mutable { "mut " } else { "" }, show(operand))
        }
        HirExprKind::TensorExtent { tensor, axis } => format!("extent({}, {axis})", show(tensor)),
        HirExprKind::Block { stmts } => format!(
            "{{ {} }}",
            stmts.iter().map(show_stmt).collect::<Vec<_>>().join("; ")
        ),
        HirExprKind::If {
            condition,
            then_block,
            else_block,
            ..
        } => format!(
            "if {} {{ {} }} else {{ {} }}",
            show(condition),
            then_block
                .iter()
                .map(show_stmt)
                .collect::<Vec<_>>()
                .join("; "),
            else_block
                .as_ref()
                .map(|b| b.iter().map(show_stmt).collect::<Vec<_>>().join("; "))
                .unwrap_or_default()
        ),
        HirExprKind::TensorFill { value, dims } => format!(
            "fill({}, dims=[{}])",
            show(value),
            dims.iter().map(show).collect::<Vec<_>>().join(", ")
        ),
        HirExprKind::TensorIdentity { dims } => format!(
            "identity(dims=[{}])",
            dims.iter().map(show).collect::<Vec<_>>().join(", ")
        ),
        HirExprKind::TensorShapeCast {
            receiver,
            permutation,
            dims,
        } => format!(
            "cast({}, perm={permutation:?}, dims={dims:?})",
            show(receiver)
        ),
        HirExprKind::TensorIndex { object, axes } => format!(
            "index({}, [{}])",
            show(object),
            axes.iter()
                .map(|a| match a {
                    HirTensorAxis::Position(p) => format!("at {}", show(p)),
                    HirTensorAxis::Range {
                        start, end, step, ..
                    } => format!("{start}..{end} step {step}"),
                    HirTensorAxis::Whole => "..".to_string(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        HirExprKind::EnumConstruct {
            variant, payload, ..
        } => format!(
            "{variant}({})",
            payload.iter().map(show).collect::<Vec<_>>().join(", ")
        ),
        HirExprKind::StructLiteral { name, fields, .. } => format!(
            "{name} {{ {} }}",
            fields
                .iter()
                .map(|f| format!("{}: {}", f.name, show(&f.value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        HirExprKind::InterpString { parts } => format!(
            "\"{}\"",
            parts
                .iter()
                .map(|p| match p {
                    HirInterpPart::Text(t) => t.clone(),
                    HirInterpPart::Formatted { expr, .. } => format!("{{{}}}", show(expr)),
                })
                .collect::<String>()
        ),
        HirExprKind::Cast { value } => format!("({} as {})", show(value), e.ty),
        HirExprKind::TensorReduce { receiver, axis, .. } => {
            format!("reduce({}, {axis:?})", show(receiver))
        }
        HirExprKind::TensorSort { receiver, axis, .. } => {
            format!("sort({}, {axis})", show(receiver))
        }
        HirExprKind::Deref { operand } => format!("*{}", show(operand)),
        HirExprKind::FieldAccess { object, field } => format!("{}.{field}", show(object)),
        other => format!("<{other:?}>").chars().take(60).collect(),
    }
}

pub(super) fn show_stmt(s: &HirStmt) -> String {
    match s {
        HirStmt::VarDecl {
            name,
            init,
            mutable,
            ..
        } => format!(
            "{}{name} = {}",
            if *mutable { "mut " } else { "val " },
            init.as_ref().map(show).unwrap_or_default()
        ),
        HirStmt::Assign { place, value, .. } => format!(
            "{} = {}",
            match place {
                neuro_hir::HirPlace::Var { name, .. } => name.clone(),
                _ => "<place>".to_string(),
            },
            show(value)
        ),
        HirStmt::If {
            condition,
            then_block,
            else_block,
            ..
        } => format!(
            "if {} {{ {} }}{}",
            show(condition),
            then_block
                .iter()
                .map(show_stmt)
                .collect::<Vec<_>>()
                .join("; "),
            else_block
                .as_ref()
                .map(|b| format!(
                    " else {{ {} }}",
                    b.iter().map(show_stmt).collect::<Vec<_>>().join("; ")
                ))
                .unwrap_or_default()
        ),
        HirStmt::Expr(e) => show(e),
        HirStmt::ValElse {
            scrutinee,
            else_block,
            ..
        } => format!(
            "valelse {} else {{ {} }}",
            show(scrutinee),
            else_block
                .iter()
                .map(show_stmt)
                .collect::<Vec<_>>()
                .join("; ")
        ),
        HirStmt::Return { value, .. } => {
            format!("return {}", value.as_ref().map(show).unwrap_or_default())
        }
        other => format!("<{other:?}>").chars().take(60).collect(),
    }
}

/// The option enum the `.as_shape` and `val`-else tests need, declared in their source.
const OPTION: &str = "enum Option<T> { Some(T), None }\n";

fn program(body: &str) -> HirProgram {
    lower(&format!(
        "{OPTION}{body}\nfunc main() -> i32 {{ return 0 }}\n"
    ))
}

fn stmts<'a>(program: &'a HirProgram, name: &str) -> &'a [HirStmt] {
    function_body(program, name)
}

fn declared(body: &[HirStmt], name: &str) -> HirType {
    declared_local(body, name).unwrap_or_else(|| panic!("no binding named {name}"))
}

/// The type of a local the body declares, if it declares one.
fn declared_local(body: &[HirStmt], name: &str) -> Option<HirType> {
    body.iter().find_map(|stmt| match stmt {
        HirStmt::VarDecl { name: n, ty, .. } if n == name => Some(ty.clone()),
        _ => None,
    })
}

fn lowered_error(source: &str) -> LoweringError {
    let ast = syntax_parsing::parse(&format!(
        "{OPTION}{source}\nfunc main() -> i32 {{ return 0 }}\n"
    ))
    .expect("source should parse");
    lower_program(&ast).expect_err("the program should be refused")
}

fn function<'a>(program: &'a HirProgram, name: &str) -> &'a HirFunction {
    program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Function(f) if f.name == name => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no function named {name}"))
}

/// The messages of the `panic` calls in `stmt`.
fn panic_messages(stmt: &HirStmt) -> Vec<String> {
    let text = show_stmt(stmt);
    text.split("panic(")
        .skip(1)
        .filter_map(|rest| {
            let rest = rest.strip_prefix("String(").unwrap_or(rest);
            let rest = rest.strip_prefix('"')?;
            rest.split("\")").next().map(str::to_string)
        })
        .collect()
}

/// The expression a block evaluates to: its tail, looked through every block around it.
fn tail(expr: &HirExpr) -> &HirExpr {
    match &expr.kind {
        HirExprKind::Block { stmts } => match stmts.last() {
            Some(HirStmt::Expr(inner)) => tail(inner),
            _ => expr,
        },
        _ => expr,
    }
}

#[test]
fn a_signature_name_is_declared_then_bound_from_its_first_parameter() {
    let program = program("func predict(x: &Tensor<f32, [?B, 784]>) -> i32 { return 0 }");
    let body = stmts(&program, "predict");
    assert!(matches!(
        &body[0],
        HirStmt::VarDecl { name, ty: HirType::U64, mutable: true, init: Some(_), .. }
            if name == "__extent_B"
    ));
    let HirStmt::Assign { value, .. } = &body[1] else {
        panic!("expected the binding assignment, got {:?}", body[1]);
    };
    assert_eq!(show(value), "extent(x, 0)");
}

#[test]
fn a_later_parameter_naming_the_same_extent_is_checked_at_run_time() {
    let program =
        program("func pair(x: Tensor<f32, [?B, 4]>, y: Tensor<f32, [?B, 10]>) -> i32 { return 0 }");
    let body = stmts(&program, "pair");
    let check = body
        .iter()
        .find(|stmt| matches!(stmt, HirStmt::If { .. }))
        .expect("a check");
    assert_eq!(
        panic_messages(check),
        vec!["dynamic extent `B` is {__extent_B} from `x` but {extent(&y, 0)} from `y`"]
    );
    assert!(show_stmt(check).starts_with("if Not((extent(&y, 0) Equal __extent_B))"));
}

#[test]
fn a_name_read_as_a_value_is_its_u64_local() {
    let program = program("func count(x: Tensor<f32, [?B, 4]>) -> u64 { val n: u64 = B\n n }");
    let init = binding_init(stmts(&program, "count"), "n");
    assert_eq!(init.ty, HirType::U64);
    assert_eq!(show(init), "__extent_B");
}

#[test]
fn a_fresh_annotation_name_takes_the_value_extent_after_the_binding() {
    let program = program(
        "func load(y: Tensor<f32, [4, 10]>) -> i32 {\n\
         val z: Tensor<f32, [?C, 10]> = y\n\
         return 0 }",
    );
    let body = stmts(&program, "load");
    let decl = body
        .iter()
        .position(|s| matches!(s, HirStmt::VarDecl { name, .. } if name == "z"))
        .expect("z");
    assert!(matches!(&body[decl - 1], HirStmt::VarDecl { name, .. } if name == "__extent_C"));
    assert_eq!(show_stmt(&body[decl + 1]), "__extent_C = extent(&z, 0)");
}

#[test]
fn a_bound_annotation_name_is_checked_and_names_its_variable() {
    let program = program(
        "func load(x: Tensor<f32, [?B, 4]>, y: Tensor<f32, [?B, 4]>) -> i32 {\n\
         val z: Tensor<f32, [?B, 4]> = y\n\
         return 0 }",
    );
    let body = stmts(&program, "load");
    let decl = body
        .iter()
        .position(|s| matches!(s, HirStmt::VarDecl { name, .. } if name == "z"))
        .expect("z");
    assert_eq!(
        panic_messages(&body[decl + 1]),
        vec!["dynamic extent `B` is {__extent_B} but `z` has {extent(&z, 0)}"]
    );
}

#[test]
fn a_reference_annotation_is_checked_through_the_reference() {
    let program = program(
        "func load(x: Tensor<f32, [?B, 4]>) -> i32 {\n\
         val r: &Tensor<f32, [?B, 4]> = &x\n\
         return 0 }",
    );
    let body = stmts(&program, "load");
    let decl = body
        .iter()
        .position(|s| matches!(s, HirStmt::VarDecl { name, .. } if name == "r"))
        .expect("r");
    assert!(show_stmt(&body[decl + 1]).contains("extent(r, 0) Equal __extent_B"));
}

#[test]
fn a_constructor_takes_one_dims_entry_per_open_axis() {
    let program = program(
        "func make(n: u64) -> i32 {\n\
         val buf = Tensor::<f32, [?, 128]>::zeros([n])\n\
         return 0 }",
    );
    let init = tail(binding_init(stmts(&program, "make"), "buf"));
    let HirExprKind::TensorFill { dims, .. } = &init.kind else {
        panic!("expected a fill, got {init:?}");
    };
    assert_eq!(dims.len(), 1);
    // The entry is bound to a temporary first, so every entry is evaluated in order.
    let whole = show(binding_init(stmts(&program, "make"), "buf"));
    assert!(whole.contains("val __extent_tmp_"), "{whole}");
    assert!(whole.contains(" = n;"), "{whole}");
}

#[test]
fn a_constructor_binds_a_fresh_name_from_its_dims_entry() {
    let program = program(
        "func make(n: u64) -> i32 {\n\
         val buf = Tensor::<f32, [?C, 4]>::zeros([n])\n\
         return 0 }",
    );
    let body = stmts(&program, "make");
    assert!(matches!(
        declared_local(body, "__extent_C"),
        Some(HirType::U64)
    ));
    let init = binding_init(body, "buf");
    assert!(
        show(init).contains("__extent_C = __extent_tmp_"),
        "{}",
        show(init)
    );
    let fill = tail(init);
    let HirExprKind::TensorFill { dims, .. } = &fill.kind else {
        panic!("expected a fill, got {fill:?}");
    };
    assert_eq!(show(&dims[0]), "__extent_C");
}

#[test]
fn a_bound_name_in_a_constructor_reads_its_local_and_takes_no_entry() {
    let program = program(
        "func make(x: Tensor<f32, [?B, 4]>) -> i32 {\n\
         val buf = Tensor::<f32, [?B, 10]>::zeros()\n\
         return 0 }",
    );
    let init = tail(binding_init(stmts(&program, "make"), "buf"));
    let HirExprKind::TensorFill { dims, .. } = &init.kind else {
        panic!("expected a fill, got {init:?}");
    };
    assert_eq!(
        dims.iter().map(show).collect::<Vec<_>>(),
        vec!["__extent_B"]
    );
}

#[test]
fn an_identity_over_one_name_repeats_its_value_for_both_axes() {
    let program = program(
        "func make(n: u64) -> i32 {\n\
         val eye = Tensor::<f32, [?N, ?N]>::identity([n])\n\
         return 0 }",
    );
    let init = binding_init(stmts(&program, "make"), "eye");
    let node = tail(init);
    let HirExprKind::TensorIdentity { dims } = &node.kind else {
        panic!("expected an identity, got {node:?}");
    };
    assert_eq!(dims.len(), 2);
    assert_eq!(show(&dims[0]), "__extent_N");
    assert_eq!(show(&dims[1]), "__extent_N");
}

#[test]
fn random_normal_takes_its_dims_after_mean_and_standard_deviation() {
    let program = program(
        "func draw(n: u64) -> i32 {\n\
         val r = Tensor::<f32, [?, 4]>::random_normal(0.0, 1.0, [n])\n\
         return 0 }",
    );
    let init = tail(binding_init(stmts(&program, "draw"), "r"));
    let HirExprKind::TensorRandomNormal { dims, .. } = &init.kind else {
        panic!("expected a random tensor, got {init:?}");
    };
    assert_eq!(dims.len(), 1);
    assert!(
        show(&dims[0]).starts_with("__extent_tmp_"),
        "{}",
        show(&dims[0])
    );
}

#[test]
fn a_struct_literal_checks_its_names_in_declaration_order() {
    let program = program(
        "struct Batch { x: Tensor<f32, [?B, 784]>, y: Tensor<f32, [?B, 10]> }\n\
         func make(a: Tensor<f32, [?N, 784]>, p: Tensor<f32, [?N, 10]>) -> i32 {\n\
         val b = Batch { x: a, y: p }\n\
         return 0 }",
    );
    let init = binding_init(stmts(&program, "make"), "b");
    let HirExprKind::Block { stmts } = &init.kind else {
        panic!("expected the literal's block, got {init:?}");
    };
    let check = stmts
        .iter()
        .find(|s| matches!(s, HirStmt::If { .. }))
        .expect("a check");
    let message = &panic_messages(check)[0];
    assert!(
        message.starts_with(
            "dynamic extent `B` of `Batch` is {__extent_B} from `x` but {extent(&__extent_tmp_"
        ),
        "{message}"
    );
    assert!(message.ends_with(" from `y`"), "{message}");
}

#[test]
fn a_generic_struct_literal_checks_its_names_too() {
    let program = program(
        "struct Pair<T> { x: Tensor<T, [?B, 2]>, y: Tensor<T, [?B, 2]> }\n\
         func make(a: Tensor<f32, [?N, 2]>, p: Tensor<f32, [?M, 2]>) -> i32 {\n\
         val b = Pair { x: a, y: p }\n\
         return 0 }",
    );
    let init = binding_init(stmts(&program, "make"), "b");
    let HirExprKind::Block { stmts } = &init.kind else {
        panic!("expected the literal's block, got {init:?}");
    };
    let check = stmts
        .iter()
        .find(|s| matches!(s, HirStmt::If { .. }))
        .expect("a check");
    assert!(
        panic_messages(check)[0].contains("of `Pair` is"),
        "{:?}",
        panic_messages(check)
    );
}

#[test]
fn a_constructor_with_too_few_dims_entries_is_refused() {
    let error = lowered_error(
        "func make(n: u64) -> i32 {\n\
         val buf = Tensor::<f32, [?, ?]>::zeros([n])\n\
         return 0 }",
    );
    assert!(
        matches!(error, LoweringError::Malformed { ref detail } if detail.contains("fewer")),
        "{error:?}"
    );
}

#[test]
fn a_constructor_with_too_many_dims_entries_is_refused() {
    let error = lowered_error(
        "func make(n: u64) -> i32 {\n\
         val buf = Tensor::<f32, [?, 4]>::zeros([n, n])\n\
         return 0 }",
    );
    assert!(
        matches!(error, LoweringError::Malformed { ref detail } if detail.contains("more")),
        "{error:?}"
    );
}

#[test]
fn as_shape_with_the_wrong_rank_is_refused() {
    let error = lowered_error(
        "func narrow(t: Tensor<f32, [?B, 3]>) -> i32 {\n\
         val Option::Some(u) = t.as_shape::<[?C]>() else |_| { return 1 }\n\
         return 0 }",
    );
    assert!(
        matches!(error, LoweringError::Malformed { ref detail } if detail.contains("rank")),
        "{error:?}"
    );
}

#[test]
fn flat_over_a_dynamic_axis_reads_its_extent_at_run_time() {
    let program = program(
        "func f(t: Tensor<f32, [4, ?N]>, i: u64) -> i32 {
         val v = t.flat(i)
         return 0 }",
    );
    let init = binding_init(stmts(&program, "f"), "v");
    let text = show(init);
    // Axis 0 strides over the dynamic axis 1, and axis 1 wraps at its run-time extent.
    assert!(
        text.contains("Divide extent(&t, 1)") || text.contains("Divide (extent"),
        "{text}"
    );
    assert!(text.contains("Modulo extent(&t, 1)"), "{text}");
}

#[test]
fn a_struct_literal_names_are_local_to_the_literal() {
    let program = program(
        "struct Batch { x: Tensor<f32, [?B, 784]> }\n\
         func make(a: Tensor<f32, [?N, 784]>) -> i32 {\n\
         val b = Batch { x: a }\n\
         return 0 }",
    );
    let body = stmts(&program, "make");
    assert!(
        declared_local(body, "__extent_B").is_none(),
        "B must not leak out of the literal"
    );
}

#[test]
fn as_shape_is_a_block_choosing_some_or_none() {
    let program = program(
        "func narrow(t: Tensor<f32, [?B, 3]>) -> i32 {\n\
         val Option::Some(u) = t.as_shape::<[?C, 3]>() else |_| { return 1 }\n\
         return 0 }",
    );
    let body = stmts(&program, "narrow");
    assert!(
        declared_local(body, "__extent_C").is_some(),
        "C is bound by the narrowing"
    );
    let text = body.iter().map(show_stmt).collect::<Vec<_>>().join("\n");
    assert!(text.contains("Some(__as_shape_"), "{text}");
    assert!(text.contains("None()"), "{text}");
}

#[test]
fn a_literal_axis_of_as_shape_is_compared_only_on_a_dynamic_receiver() {
    let program = program(
        "func narrow(t: Tensor<f32, [?B, 3]>, s: Tensor<f32, [4, 3]>) -> i32 {\n\
         val Option::Some(u) = t.as_shape::<[4, 3]>() else |_| { return 1 }\n\
         val Option::Some(v) = s.as_shape::<[4, 3]>() else |_| { return 1 }\n\
         return 0 }",
    );
    let text = stmts(&program, "narrow")
        .iter()
        .map(show_stmt)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("extent(&__as_shape_"), "{text}");
    assert_eq!(text.matches("Equal Integer(4, None)").count(), 1, "{text}");
}

#[test]
fn a_call_result_takes_the_extent_its_argument_carries() {
    let program = program(
        "func first(x: &Tensor<f32, [?B, 784]>) -> Tensor<f32, [?B, 784]> { x.clone() }\n\
         func caller(b: Tensor<f32, [32, 784]>) -> i32 {\n\
         val r = first(&b)\n\
         return 0 }",
    );
    assert_eq!(
        declared(stmts(&program, "caller"), "r"),
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: neuro_hir::static_shape(&[32, 784]),
            names: neuro_hir::AxisNames::default(),
        }
    );
}

#[test]
fn a_call_result_stays_dynamic_when_no_argument_fixes_the_extent() {
    let program = program(
        "func first(x: &Tensor<f32, [?B, 784]>) -> Tensor<f32, [?B, 784]> { x.clone() }\n\
         func caller(a: Tensor<f32, [?N, 784]>) -> i32 {\n\
         val r = first(&a)\n\
         return 0 }",
    );
    assert_eq!(
        declared(stmts(&program, "caller"), "r"),
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: vec![None, Some(784)],
            names: neuro_hir::AxisNames::default(),
        }
    );
}

#[test]
fn a_generic_call_takes_the_extent_its_argument_carries() {
    let program = program(
        "func gen<T>(x: Tensor<f32, [?M, 2]>, y: T) -> Tensor<f32, [?M, 2]> { x }\n\
         func caller(s: Tensor<f32, [5, 2]>) -> i32 {\n\
         val g = gen(s, 1)\n\
         return 0 }",
    );
    assert_eq!(
        declared(stmts(&program, "caller"), "g"),
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: neuro_hir::static_shape(&[5, 2]),
            names: neuro_hir::AxisNames::default(),
        }
    );
    let instance = function(&program, "gen$g_i32");
    assert!(matches!(&instance.body[0], HirStmt::VarDecl { name, .. } if name == "__extent_M"));
}

#[test]
fn a_generic_template_binds_its_names_per_instance() {
    let program = program(
        "func gen<T>(x: Tensor<f32, [?M, 2]>, y: T) -> Tensor<f32, [?M, 2]> { x }\n\
         func caller(s: Tensor<f32, [5, 2]>) -> i32 {\n\
         val g = gen(s, 1.0)\n\
         return 0 }",
    );
    let instance = function(&program, "gen$g_f64");
    let text = instance
        .body
        .iter()
        .map(show_stmt)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("__extent_M = extent(&x, 0)"), "{text}");
}

#[test]
fn a_block_scoped_name_is_bound_again_outside_its_block() {
    let program = program(
        "func f(c: bool, a: Tensor<f32, [4, 2]>, b: Tensor<f32, [4, 2]>) -> i32 {\n\
         if c { val x: Tensor<f32, [?C, 2]> = a }\n\
         val y: Tensor<f32, [?C, 2]> = b\n\
         return 0 }",
    );
    let body = stmts(&program, "f");
    let outer = body
        .iter()
        .filter(|s| matches!(s, HirStmt::VarDecl { name, .. } if name == "__extent_C"))
        .count();
    assert_eq!(
        outer, 1,
        "the outer block declares its own C after the inner block"
    );
}

#[test]
fn a_closure_body_reads_a_signature_name_as_a_captured_u64() {
    let program = program(
        "func f(x: Tensor<f32, [?B, 2]>) -> i32 {
         val g = |n: i32| -> i32 { B as i32 }
         return 0 }",
    );
    let closure = program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Closure(c) => Some(c),
            _ => None,
        })
        .expect("a lifted closure");
    assert_eq!(
        closure
            .captures
            .iter()
            .map(|c| (c.name.as_str(), c.ty.clone()))
            .collect::<Vec<_>>(),
        vec![("__extent_B", HirType::U64)]
    );
    assert!(
        closure
            .body
            .iter()
            .map(show_stmt)
            .any(|text| text.contains("__extent_B"))
    );
}

#[test]
fn a_reshape_of_a_dynamic_receiver_checks_its_count_and_derives_its_axes() {
    let program = program(
        "func flat(a: Tensor<f32, [?N, 784]>) -> i32 {\n\
         val r = a.reshape([-1, 28])\n\
         return 0 }",
    );
    let init = binding_init(stmts(&program, "flat"), "r");
    let HirExprKind::Block { stmts } = &init.kind else {
        panic!("expected the reshape block, got {init:?}");
    };
    assert!(panic_messages(&stmts[1])[0].starts_with("tensor reshape: "));
    let Some(HirStmt::Expr(cast)) = stmts.last() else {
        panic!("expected the cast as the tail");
    };
    let HirExprKind::TensorShapeCast {
        dims, permutation, ..
    } = &cast.kind
    else {
        panic!("expected a shape cast, got {cast:?}");
    };
    assert!(permutation.is_none());
    assert_eq!(dims.len(), 1);
    assert_eq!(
        cast.ty,
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: vec![None, Some(28)],
            names: neuro_hir::AxisNames(vec![None, None]),
        }
    );
}

#[test]
fn a_reshape_into_a_dynamic_name_reads_the_name_in_its_count() {
    let program = program(
        "func fold(a: Tensor<f32, [4, 6]>, x: Tensor<f32, [?B, 3]>) -> i32 {\n\
         val r = a.reshape([B, 3])\n\
         return 0 }",
    );
    let text = stmts(&program, "fold")
        .iter()
        .map(show_stmt)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("__extent_B"), "{text}");
}

#[test]
fn a_reshape_naming_a_dynamic_extent_is_a_host_function_over_the_names_value() {
    let program = program(
        "func f(a: Tensor<f32, [?N, 784]>) -> i32 {
         val r = a.reshape([N, -1])
         return 0 }",
    );
    let call = tail(binding_init(stmts(&program, "f"), "r"));
    let HirExprKind::Call { callee, args } = &call.kind else {
        panic!("expected the outlined reshape call, got {call:?}");
    };
    let HirExprKind::Variable(name) = &callee.kind else {
        panic!("expected a named callee, got {callee:?}");
    };
    assert!(name.starts_with("__reshape_"), "{name}");
    assert_eq!(
        args.iter().map(show).collect::<Vec<_>>(),
        vec!["__extent_tmp_1", "__extent_N"]
    );
    let outlined = function(&program, name);
    assert_eq!(outlined.params.len(), 2);
    assert_eq!(outlined.params[1].ty, HirType::U64);
    // The `-1` is the receiver's count over the named extent, the other axis is the name.
    assert_eq!(
        outlined.result_extents,
        vec![
            HirExtent::Scalar(1),
            HirExtent::Div(
                Box::new(HirExtent::Mul(
                    Box::new(HirExtent::Axis {
                        operand: 0,
                        axis: 0
                    }),
                    Box::new(HirExtent::Constant(784)),
                )),
                Box::new(HirExtent::Mul(
                    Box::new(HirExtent::Constant(1)),
                    Box::new(HirExtent::Scalar(1)),
                )),
            ),
        ]
    );
}

#[test]
fn a_flatten_across_a_dynamic_axis_is_dynamic_and_keeps_the_others() {
    let program = program(
        "func flat(a: Tensor<f32, [4, ?N, 6]>) -> i32 {\n\
         val r = a.flatten([1, 2])\n\
         val s = a.flatten()\n\
         return 0 }",
    );
    let body = stmts(&program, "flat");
    assert_eq!(
        declared(body, "r"),
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: vec![Some(4), None],
            names: neuro_hir::AxisNames(vec![None, None]),
        }
    );
    assert_eq!(
        declared(body, "s"),
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: vec![None],
            names: neuro_hir::AxisNames(vec![None]),
        }
    );
    let HirExprKind::TensorShapeCast { dims, .. } = &binding_init(body, "r").kind else {
        panic!("expected a shape cast");
    };
    // The merged run is the dynamic axis times the static one behind it.
    assert_eq!(
        dims,
        &vec![HirExtent::Mul(
            Box::new(HirExtent::Axis {
                operand: 0,
                axis: 1
            }),
            Box::new(HirExtent::Constant(6)),
        )]
    );
}

#[test]
fn a_position_on_a_dynamic_axis_is_checked_before_the_read() {
    let program = program(
        "func pick(a: Tensor<f32, [?N, 4]>, i: u64) -> i32 {\n\
         val row = a[i, ..]\n\
         return 0 }",
    );
    let init = binding_init(stmts(&program, "pick"), "row");
    let HirExprKind::Block { stmts } = &init.kind else {
        panic!("expected the checked read, got {init:?}");
    };
    assert_eq!(
        panic_messages(&stmts[1]),
        vec!["tensor index out of bounds"]
    );
}

#[test]
fn a_whole_dynamic_axis_survives_in_the_slice_type() {
    let program = program(
        "func col(a: Tensor<f32, [?N, 4]>) -> i32 {\n\
         val c = a[.., 0]\n\
         return 0 }",
    );
    let body = stmts(&program, "col");
    assert_eq!(
        declared(body, "c"),
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: vec![None],
            names: neuro_hir::AxisNames(vec![None]),
        }
    );
}

#[test]
fn a_constant_range_over_a_dynamic_axis_keeps_its_static_extent_and_checks_its_end() {
    let program = program(
        "func head(a: Tensor<f32, [?N, 4]>) -> i32 {\n\
         val h = a[0..2, ..]\n\
         return 0 }",
    );
    let body = stmts(&program, "head");
    assert_eq!(
        declared(body, "h"),
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: neuro_hir::static_shape(&[2, 4]),
            names: neuro_hir::AxisNames(vec![None, None]),
        }
    );
    let init = binding_init(body, "h");
    let HirExprKind::Block { stmts } = &init.kind else {
        panic!("expected the checked slice, got {init:?}");
    };
    assert!(show_stmt(&stmts[0]).contains("Integer(2, None) LessEqual extent(&a, 0)"));
}

#[test]
fn a_write_past_a_dynamic_axis_is_checked_before_the_store() {
    let program = program(
        "func put(t: &mut Tensor<f32, [?B, 3]>, i: u64) -> i32 {\n\
         t[i, 0] = 1.0\n\
         return 0 }",
    );
    let body = stmts(&program, "put");
    let check = body
        .iter()
        .position(|s| matches!(s, HirStmt::If { .. }))
        .expect("a check");
    assert!(matches!(&body[check + 1], HirStmt::Assign { .. }));
    assert_eq!(
        panic_messages(&body[check]),
        vec!["tensor index out of bounds"]
    );
}

#[test]
fn an_outlined_matmul_takes_its_rows_from_the_left_operand() {
    let program = program(
        "func apply(x: Tensor<f32, [?N, 784]>, w: Tensor<f32, [784, 10]>) -> i32 {\n\
         val y = x @ w\n\
         return 0 }",
    );
    let outlined = function(&program, "__tensor_op_0");
    assert_eq!(
        outlined.result_extents,
        vec![HirExtent::Axis {
            operand: 0,
            axis: 0
        }]
    );
}

#[test]
fn an_outlined_transpose_takes_its_axis_from_the_permuted_receiver_axis() {
    let program = program(
        "func apply(x: Tensor<f32, [?N, 784]>) -> i32 {\n\
         val y = x.t()\n\
         return 0 }",
    );
    let outlined = function(&program, "__tensor_op_0");
    assert_eq!(
        outlined.result_extents,
        vec![HirExtent::Axis {
            operand: 0,
            axis: 0
        }]
    );
}

#[test]
fn an_outlined_sum_over_one_axis_keeps_the_other_dynamic_axes() {
    let program = program(
        "func apply(x: Tensor<f32, [?N, 4, ?M]>) -> i32 {\n\
         val y = x.sum(1)\n\
         return 0 }",
    );
    let outlined = function(&program, "__tensor_op_0");
    assert_eq!(
        outlined.result_extents,
        vec![
            HirExtent::Axis {
                operand: 0,
                axis: 0
            },
            HirExtent::Axis {
                operand: 0,
                axis: 2
            }
        ]
    );
}

#[test]
fn an_outlined_topk_reports_one_extent_per_part_and_checks_k_on_a_dynamic_axis() {
    let program = program(
        "func apply(x: Tensor<f32, [?N, 10]>) -> i32 {\n\
         val y = x.topk(3, 0)\n\
         return 0 }",
    );
    let outlined = function(&program, "__tensor_op_0");
    assert!(
        outlined.result_extents.is_empty(),
        "k fixes the selected axis"
    );
    let body = stmts(&program, "apply");
    let text = body.iter().map(show_stmt).collect::<Vec<_>>().join("\n");
    assert!(
        text.contains("LessEqual extent(__index_source_operand, 0)"),
        "{text}"
    );
}

#[test]
fn an_outlined_topk_over_a_dynamic_last_axis_keeps_its_row_extent() {
    let program = program(
        "func apply(x: Tensor<f32, [?N, 10]>) -> i32 {\n\
         val y = x.topk(3)\n\
         return 0 }",
    );
    let outlined = function(&program, "__tensor_op_0");
    assert_eq!(
        outlined.result_extents,
        vec![
            HirExtent::Axis {
                operand: 0,
                axis: 0
            },
            HirExtent::Axis {
                operand: 0,
                axis: 0
            }
        ]
    );
}

#[test]
fn an_outlined_element_wise_op_takes_its_extent_from_the_operand_that_walks_it() {
    let program = program(
        "func apply(x: Tensor<f32, [?N, 10]>, b: Tensor<f32, [10]>) -> i32 {\n\
         val y = x + b\n\
         return 0 }",
    );
    let outlined = function(&program, "__tensor_op_0");
    assert_eq!(
        outlined.result_extents,
        vec![HirExtent::Axis {
            operand: 0,
            axis: 0
        }]
    );
}

#[test]
fn an_outlined_slice_takes_its_whole_axis_extent_from_the_object() {
    let program = program(
        "func apply(x: Tensor<f32, [?N, 4]>) -> i32 {\n\
         val y = x[.., 1]\n\
         return 0 }",
    );
    let outlined = function(&program, "__tensor_op_0");
    assert_eq!(
        outlined.result_extents,
        vec![HirExtent::Axis {
            operand: 0,
            axis: 0
        }]
    );
}

#[test]
fn an_einsum_over_a_dynamic_letter_reads_it_from_an_operand() {
    let program = program(
        "func apply(x: Tensor<f32, [?N, 4]>, w: Tensor<f32, [4, 3]>) -> i32 {\n\
         val y = einsum(\"bi,ij->bj\", x, w)\n\
         return 0 }",
    );
    let outlined = function(&program, "__tensor_op_0");
    assert_eq!(
        outlined.result_extents,
        vec![HirExtent::Axis {
            operand: 0,
            axis: 0
        }]
    );
}

#[test]
fn a_dynamic_argument_leaves_the_result_of_a_static_return_alone() {
    let program = program(
        "func id(x: Tensor<f32, [4, 2]>) -> Tensor<f32, [4, 2]> { x }\n\
         func caller(a: Tensor<f32, [4, 2]>) -> i32 {\n\
         val r = id(a)\n\
         return 0 }",
    );
    assert_eq!(
        declared(stmts(&program, "caller"), "r"),
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: neuro_hir::static_shape(&[4, 2]),
            names: neuro_hir::AxisNames::default(),
        }
    );
}
