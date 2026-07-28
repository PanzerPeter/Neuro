#[allow(unused_imports)]
use super::{
    binding_init, enum_names, function_body, function_names, impl_method_names, lower, struct_names,
};
use neuro_hir::{HirExprKind, HirItem, HirStmt, HirType};

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
