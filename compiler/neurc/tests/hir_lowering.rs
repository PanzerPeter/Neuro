//! End-to-end integration: parse → type-check → lower to typed HIR.
//!
//! Exercises the Phase 1.8 lowering through the same library entry points the
//! `neurc` driver uses, on representative programs spanning the language surface.

mod common;

use common::CompileTest;
use hir_lowering::lower_program;
use neuro_hir::{HirExpr, HirExprKind, HirItem, HirProgram, HirStmt, HirType};

/// Parse, type-check, then lower `src`, asserting each stage succeeds.
fn lower(src: &str) -> HirProgram {
    let ast = syntax_parsing::parse(src).expect("source should parse");
    semantic_analysis::type_check(&ast).expect("source should type-check");
    lower_program(&ast).expect("type-checked source should lower")
}

fn function_body<'a>(program: &'a HirProgram, name: &str) -> &'a [HirStmt] {
    for item in &program.items {
        if let HirItem::Function(f) = item {
            if f.name == name {
                return &f.body;
            }
        }
    }
    panic!("function '{}' not found in HIR", name);
}

fn binding_init<'a>(body: &'a [HirStmt], name: &str) -> &'a HirExpr {
    for stmt in body {
        if let HirStmt::VarDecl { name: n, init, .. } = stmt {
            if n == name {
                return init.as_ref().expect("binding has an initializer");
            }
        }
    }
    panic!("binding '{}' not found in HIR body", name);
}

#[test]
fn lowers_a_struct_method_program_end_to_end() {
    let src = "struct Neuron { weight: f64, bias: f64 }\n\
               impl Neuron {\n\
                 func new(weight: f64, bias: f64) -> Neuron { Neuron { weight: weight, bias: bias } }\n\
                 func activate(&self, input: f64) -> f64 {\n\
                   val z = (input * self.weight) + self.bias\n\
                   if z > 0.0 { z } else { 0.0 }\n\
                 }\n\
               }\n\
               func main() -> i32 {\n\
                 val n = Neuron::new(0.5, -0.1)\n\
                 val a = n.activate(1.0)\n\
                 0\n\
               }";
    let program = lower(src);

    // Struct, impl, and two functions all lowered.
    assert!(program
        .items
        .iter()
        .any(|i| matches!(i, HirItem::Struct(s) if s.name == "Neuron")));
    assert!(program
        .items
        .iter()
        .any(|i| matches!(i, HirItem::Impl(im) if im.type_name == "Neuron")));

    let body = function_body(&program, "main");
    assert_eq!(
        binding_init(body, "n").ty,
        HirType::Struct("Neuron".to_string())
    );
    // The method call result type is re-derived from the method signature.
    assert_eq!(binding_init(body, "a").ty, HirType::F64);
}

#[test]
fn lowers_control_flow_and_borrows() {
    let src = "func sum(n: i32) -> i32 {\n\
                 mut total: i32 = 0\n\
                 for i in 0..n { total = total + i }\n\
                 total\n\
               }\n\
               func main() -> i32 {\n\
                 mut x: i32 = 3\n\
                 val r = &mut x\n\
                 *r = 4\n\
                 val s = sum(x)\n\
                 0\n\
               }";
    let program = lower(src);
    let body = function_body(&program, "main");
    assert_eq!(
        binding_init(body, "r").ty,
        HirType::Reference {
            inner: Box::new(HirType::I32),
            mutable: true,
        }
    );
    assert_eq!(binding_init(body, "s").ty, HirType::I32);

    // The trailing `total` of `sum` is an implicit return typed as i32.
    let sum_body = function_body(&program, "sum");
    let HirStmt::Expr(tail) = sum_body.last().expect("non-empty body") else {
        panic!("expected trailing expression");
    };
    assert_eq!(tail.ty, HirType::I32);
}

#[test]
fn lowers_string_and_array_builtins() {
    let src = "func main() -> i32 {\n\
                 val s = \"hello\"\n\
                 val sub = s.slice(0..2)\n\
                 val n = s.len()\n\
                 val arr = [10, 20, 30]\n\
                 val len = arr.len()\n\
                 val first = arr[0]\n\
                 0\n\
               }";
    let program = lower(src);
    let body = function_body(&program, "main");
    assert_eq!(
        binding_init(body, "sub").ty,
        HirType::Reference {
            inner: Box::new(HirType::String),
            mutable: false,
        }
    );
    assert_eq!(binding_init(body, "n").ty, HirType::U64);
    assert_eq!(binding_init(body, "len").ty, HirType::U64);
    assert_eq!(binding_init(body, "first").ty, HirType::I32);

    // The `.slice` argument lowered to a Range node.
    let sub_init = binding_init(body, "sub");
    let HirExprKind::Call { args, .. } = &sub_init.kind else {
        panic!("expected a method call for .slice");
    };
    assert!(matches!(
        args.first().map(|a| &a.kind),
        Some(HirExprKind::Range { .. })
    ));
}

#[test]
fn compiling_a_program_still_produces_a_working_binary() {
    // The pipeline now lowers to HIR on every compile; confirm an end-to-end
    // compile + run is unaffected.
    let test = CompileTest::new();
    let src = "func main() -> i32 { return 7 }\n";
    let exit = test
        .compile_and_run("hir_pipeline.nr", src)
        .expect("program should compile and run");
    assert_eq!(exit, 7);
}
