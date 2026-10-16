use super::{binding_init, function_body, lower};
use neuro_hir::{HirExprKind, HirItem, HirType};

#[test]
fn closure_lowers_to_value_and_lifted_item() {
    let program = lower("func main() -> i32 { val base = 10\n val f = |x: i32| x + base\n f(5) }");
    let body = function_body(&program, "main");

    // The binding's initializer is a closure value referencing its lifted item and
    // capturing `base` (a Copy local) by value.
    let init = binding_init(body, "f");
    let (name, captures) = match &init.kind {
        HirExprKind::Closure { name, captures } => (name, captures),
        other => panic!("expected a closure value, got {:?}", other),
    };
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].name, "base");
    assert_eq!(captures[0].ty, HirType::I32);
    assert_eq!(
        init.ty,
        HirType::Function {
            params: vec![HirType::I32],
            ret: Box::new(HirType::I32),
        }
    );

    // A matching top-level closure item is emitted with the parameter and capture.
    let closure = program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Closure(c) if &c.name == name => Some(c),
            _ => None,
        })
        .expect("a lifted closure item should be emitted");
    assert_eq!(closure.params.len(), 1);
    assert_eq!(closure.params[0].name, "x");
    assert_eq!(closure.return_type, HirType::I32);
    assert_eq!(closure.captures.len(), 1);
    assert_eq!(closure.captures[0].name, "base");
}

/// The capture names of the closure `binding` is initialized with in `main`.
fn capture_names(program: &neuro_hir::HirProgram, binding: &str) -> Vec<String> {
    let init = binding_init(function_body(program, "main"), binding);
    match &init.kind {
        HirExprKind::Closure { captures, .. } => captures.iter().map(|c| c.name.clone()).collect(),
        other => panic!("expected a closure value, got {:?}", other),
    }
}

#[test]
fn a_name_bound_inside_a_closure_hides_only_the_reads_in_its_scope() {
    // Each closure reads an enclosing local the body ALSO binds, somewhere the read
    // cannot see: after the read, in another match arm, or as an inner closure's
    // parameter. The read is still a capture.
    let program = lower(
        "func main() -> i32 {
            val x = 7
            val n = 4
            val v = 3
            val k: u64 = 1
            val later = |a: i32| -> i32 {
                val y = x + a
                val x = 100
                y + x
            }
            val arms = |a: i32| -> i32 {
                match a {
                    0 => n,
                    n => n * 2,
                }
            }
            val inner = |a: i32| -> i32 {
                val h = |v: i32| -> i32 { v + 1 }
                h(a) + v
            }
            val place = |a: i32| -> i32 {
                mut buf = [0, 0, 0]
                buf[k] = a
                buf[1]
            }
            later(1) + arms(0) + inner(1) + place(2)
        }",
    );
    assert_eq!(capture_names(&program, "later"), ["x"]);
    assert_eq!(capture_names(&program, "arms"), ["n"]);
    assert_eq!(capture_names(&program, "inner"), ["v"]);
    assert_eq!(capture_names(&program, "place"), ["k"]);
}

/// Each capture of the closure `binding` is initialized with in `main`, with whether
/// the environment borrows it.
fn capture_modes(program: &neuro_hir::HirProgram, binding: &str) -> Vec<(String, bool, HirType)> {
    let init = binding_init(function_body(program, "main"), binding);
    match &init.kind {
        HirExprKind::Closure { captures, .. } => captures
            .iter()
            .map(|c| (c.name.clone(), c.borrowed, c.ty.clone()))
            .collect(),
        other => panic!("expected a closure value, got {:?}", other),
    }
}

fn mut_ref(inner: HirType) -> HirType {
    HirType::Reference {
        inner: Box::new(inner),
        mutable: true,
    }
}

#[test]
fn a_capture_is_copied_borrowed_or_moved_as_the_body_uses_it() {
    let program = lower(
        "func main() -> i32 {
            mut n = 1
            val k = 2
            val s = \"a\" + \"b\"
            val read = |x: i32| -> u64 { s.len() + (k as u64) }
            val write = |x: i32| -> void { n += x }
            val owned = move |x: i32| -> u64 { s.len() }
            0
        }",
    );
    assert_eq!(
        capture_modes(&program, "read"),
        [
            ("s".to_string(), true, mut_ref(HirType::String)),
            ("k".to_string(), false, HirType::I32),
        ]
    );
    assert_eq!(
        capture_modes(&program, "write"),
        [("n".to_string(), true, mut_ref(HirType::I32))]
    );
    assert_eq!(
        capture_modes(&program, "owned"),
        [("s".to_string(), false, HirType::String)]
    );
}

/// The body of a closure that borrows `s` reads it through the reference its
/// environment holds.
#[test]
fn a_borrowed_capture_is_read_through_its_reference() {
    let program = lower(
        "func main() -> i32 {
            val s = \"a\" + \"b\"
            val f = |x: i32| -> u64 { s.len() }
            0
        }",
    );
    let closure = program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Closure(c) => Some(c),
            _ => None,
        })
        .expect("a lifted closure item should be emitted");
    let rendered = format!("{:?}", closure.body);
    assert!(
        rendered.contains("Deref { operand: HirExpr { kind: Variable(\"s\")"),
        "{rendered}"
    );
}

/// An inner closure reading what the outer one borrowed copies the outer reference.
#[test]
fn a_nested_closure_copies_the_reference_its_enclosing_closure_borrowed() {
    let program = lower(
        "func main() -> i32 {
            mut n = 0
            val outer = |x: i32| -> void {
                val inner = |y: i32| -> void { n += y }
                inner(x)
            }
            0
        }",
    );
    let inner = program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Closure(c) if c.captures.iter().all(|cap| !cap.borrowed) => Some(c),
            _ => None,
        })
        .expect("the inner closure copies its capture");
    assert_eq!(inner.captures[0].name, "n");
    assert_eq!(inner.captures[0].ty, mut_ref(HirType::I32));
}

/// The type of the closure `binding` is initialized with in `main`.
fn closure_type(program: &neuro_hir::HirProgram, binding: &str) -> HirType {
    binding_init(function_body(program, "main"), binding)
        .ty
        .clone()
}

fn function_type(params: Vec<HirType>, ret: HirType) -> HirType {
    HirType::Function {
        params,
        ret: Box::new(ret),
    }
}

/// A parameter with no annotation takes the type the binding's annotation gives it, and
/// a block body with no return type returns its last expression, as the checker infers.
#[test]
fn a_closure_takes_its_signature_from_its_context_and_body() {
    let program = lower(
        "func main() -> i32 {
            val square: (i32) -> i32 = |x| x * x
            val widen = |x: i32| {
                val y = x + 1
                y as i64
            }
            val sign = |x: i32| { if x < 0 { -1 } else { 1 } }
            val unit = |x: i32| { val y = x }
            square(2) + widen(1) as i32 + sign(3)
        }",
    );
    assert_eq!(
        closure_type(&program, "square"),
        function_type(vec![HirType::I32], HirType::I32)
    );
    assert_eq!(
        closure_type(&program, "widen"),
        function_type(vec![HirType::I32], HirType::I64)
    );
    assert_eq!(
        closure_type(&program, "sign"),
        function_type(vec![HirType::I32], HirType::I32)
    );
    assert_eq!(
        closure_type(&program, "unit"),
        function_type(vec![HirType::I32], HirType::Void)
    );
}

/// A closure literal handed to a generic higher-order function waits for the other
/// arguments, takes what they bound, and binds the rest through its own result.
#[test]
fn a_closure_argument_infers_a_generic_instance() {
    let program = lower(
        "func first<T, U>(f: (T) -> U, value: T) -> U { f(value) }
        func main() -> i32 {
            val wide = first(|x| x as i64 * 2, 7)
            wide as i32
        }",
    );
    assert!(
        super::function_names(&program)
            .iter()
            .any(|name| name.starts_with("first$") && name.contains("i64")),
        "{:?}",
        super::function_names(&program)
    );
    let init = binding_init(function_body(&program, "main"), "wide");
    let HirExprKind::Call { args, .. } = &init.kind else {
        panic!("expected a call, got {:?}", init.kind);
    };
    assert_eq!(
        args[0].ty,
        function_type(vec![HirType::I32], HirType::I64),
        "the closure keeps its own position"
    );
    assert_eq!(args[1].ty, HirType::I32);
}
