// Neuro Programming Language - Syntax Parsing Tests
// Function parsing tests

use syntax_parsing::{parse, Item};

#[test]
fn test_parse_empty_function() {
    let source = "func empty() {}";
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    assert_eq!(items.len(), 1);
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.name.name, "empty");
            assert_eq!(func.params.len(), 0);
            assert!(func.return_type.is_none());
            assert_eq!(func.body.len(), 0);
        }
        _ => panic!("expected function item"),
    }
}

#[test]
fn test_parse_function_with_return_type() {
    let source = "func get_number() -> i32 { return 42 }";
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    assert_eq!(items.len(), 1);
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.name.name, "get_number");
            assert!(func.return_type.is_some());
        }
        _ => panic!("expected function item"),
    }
}

#[test]
fn test_parse_function_with_one_parameter() {
    let source = "func increment(x: i32) -> i32 { return x + 1 }";
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.params.len(), 1);
            assert_eq!(func.params[0].name.name, "x");
        }
        _ => panic!("expected function item"),
    }
}

#[test]
fn test_parse_function_with_multiple_parameters() {
    let source = "func add(a: i32, b: i32) -> i32 { return a + b }";
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.params.len(), 2);
            assert_eq!(func.params[0].name.name, "a");
            assert_eq!(func.params[1].name.name, "b");
        }
        _ => panic!("expected function item"),
    }
}

#[test]
fn test_parse_function_with_many_parameters() {
    let source = "func many(a: i32, b: i32, c: i32, d: i32, e: i32) -> i32 { return a }";
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.params.len(), 5);
        }
        _ => panic!("expected function item"),
    }
}

#[test]
fn test_parse_function_with_body() {
    let source = r#"
        func compute(x: i32) -> i32 {
            val doubled = x * 2
            val result = doubled + 10
            return result
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.body.len(), 3);
        }
        _ => panic!("expected function item"),
    }
}

#[test]
fn test_parse_function_with_control_flow() {
    let source = r#"
        func abs(x: i32) -> i32 {
            if x < 0 {
                return -x
            } else {
                return x
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_multiple_functions() {
    let source = r#"
        func first() -> i32 {
            return 1
        }

        func second() -> i32 {
            return 2
        }

        func third() -> i32 {
            return 3
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
    let items = result.unwrap();
    assert_eq!(items.len(), 3);
}

#[test]
fn test_parse_function_with_newlines_in_params() {
    let source = r#"
        func multi_param(
            a: i32,
            b: i32,
            c: i32
        ) -> i32 {
            return a + b + c
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_calling_another() {
    let source = r#"
        func helper() -> i32 {
            return 42
        }

        func main() -> i32 {
            return helper()
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_with_complex_body() {
    let source = r#"
        func factorial(n: i32) -> i32 {
            if n <= 1 {
                return 1
            } else {
                val n_minus_1 = n - 1
                val recursive = factorial(n_minus_1)
                return n * recursive
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_with_string_param() {
    let source = r#"
        func greet(name: String) {
            val message = "Hello"
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_with_bool_param() {
    let source = r#"
        func check(flag: bool) -> i32 {
            if flag {
                return 1
            } else {
                return 0
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_no_return_type() {
    let source = r#"
        func print_hello() {
            val msg = "Hello"
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_with_expression_statement() {
    let source = r#"
        func caller() {
            do_something()
            do_something_else()
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_long_name() {
    let source = r#"
        func very_long_function_name_with_many_words() {
            val x = 1
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_parse_function_with_allow_attribute() {
    use syntax_parsing::Item;

    let source = r#"
        @allow(prefer_loop_over_while_true)
        func main() -> i32 {
            0
        }
    "#;
    let items = parse(source).expect("parse should succeed");
    assert_eq!(items.len(), 1);
    let func = match &items[0] {
        Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    assert_eq!(func.attributes.len(), 1);
    assert_eq!(func.attributes[0].name.name, "allow");
    assert_eq!(func.attributes[0].args.len(), 1);
    assert_eq!(
        func.attributes[0].args[0].name,
        "prefer_loop_over_while_true"
    );
}

#[test]
fn test_parse_function_with_bare_attribute() {
    use syntax_parsing::Item;

    let source = r#"
        @inline
        func small() -> i32 { 1 }
    "#;
    let items = parse(source).expect("parse should succeed");
    let func = match &items[0] {
        Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    assert_eq!(func.attributes.len(), 1);
    assert_eq!(func.attributes[0].name.name, "inline");
    assert!(func.attributes[0].args.is_empty());
}

#[test]
fn test_parse_function_with_multi_arg_attribute() {
    use syntax_parsing::Item;

    let source = r#"
        @grad(a, b, c)
        func forward() -> i32 { 0 }
    "#;
    let items = parse(source).expect("parse should succeed");
    let func = match &items[0] {
        Item::Function(f) => f,
        _ => panic!("expected function"),
    };
    let args: Vec<_> = func.attributes[0]
        .args
        .iter()
        .map(|a| a.name.as_str())
        .collect();
    assert_eq!(args, vec!["a", "b", "c"]);
}

#[test]
fn test_parse_method_with_allow_attribute() {
    use syntax_parsing::Item;

    let source = r#"
        struct Counter { value: i32 }

        impl Counter {
            @allow(prefer_loop_over_while_true)
            func tick(&self) -> i32 { 0 }
        }
    "#;
    let items = parse(source).expect("parse should succeed");
    let impl_def = items
        .iter()
        .find_map(|item| {
            if let Item::Impl(i) = item {
                Some(i)
            } else {
                None
            }
        })
        .expect("expected impl block");
    assert_eq!(impl_def.methods.len(), 1);
    assert_eq!(impl_def.methods[0].attributes.len(), 1);
    assert_eq!(impl_def.methods[0].attributes[0].name.name, "allow");
}

#[test]
fn test_parse_inherent_impl_has_no_trait_name() {
    use syntax_parsing::Item;

    let source = r#"
        struct Counter { value: i32 }

        impl Counter {
            func tick(&self) -> i32 { 0 }
        }
    "#;
    let items = parse(source).expect("parse should succeed");
    let impl_def = items
        .iter()
        .find_map(|item| match item {
            Item::Impl(i) => Some(i),
            _ => None,
        })
        .expect("expected impl block");
    assert!(impl_def.trait_name.is_none());
    assert_eq!(impl_def.type_name.name, "Counter");
}

#[test]
fn test_parse_drop_trait_impl_records_trait_name() {
    use syntax_parsing::Item;

    let source = r#"
        struct Handle { id: i32 }

        impl Drop for Handle {
            func drop(&mut self) { }
        }
    "#;
    let items = parse(source).expect("parse should succeed");
    let impl_def = items
        .iter()
        .find_map(|item| match item {
            Item::Impl(i) => Some(i),
            _ => None,
        })
        .expect("expected impl block");
    assert_eq!(
        impl_def.trait_name.as_ref().map(|t| t.name.as_str()),
        Some("Drop")
    );
    assert_eq!(impl_def.type_name.name, "Handle");
    assert_eq!(impl_def.methods.len(), 1);
    assert_eq!(impl_def.methods[0].name.name, "drop");
}

#[test]
fn test_parse_struct_with_derive_attribute() {
    use syntax_parsing::Item;

    let source = r#"
        @derive(Copy, Clone)
        struct Point { x: i32, y: i32 }
    "#;
    let items = parse(source).expect("parse should succeed");
    let struct_def = match &items[0] {
        Item::Struct(s) => s,
        _ => panic!("expected struct"),
    };
    assert_eq!(struct_def.attributes.len(), 1);
    assert_eq!(struct_def.attributes[0].name.name, "derive");
    let args: Vec<_> = struct_def.attributes[0]
        .args
        .iter()
        .map(|a| a.name.as_str())
        .collect();
    assert_eq!(args, vec!["Copy", "Clone"]);
}

#[test]
fn test_parse_struct_without_attributes_has_empty_list() {
    use syntax_parsing::Item;

    let source = r#"
        struct Point { x: i32, y: i32 }
    "#;
    let items = parse(source).expect("parse should succeed");
    let struct_def = match &items[0] {
        Item::Struct(s) => s,
        _ => panic!("expected struct"),
    };
    assert!(struct_def.attributes.is_empty());
}

#[test]
fn test_dangling_attribute_at_eof_is_rejected() {
    let source = r#"
        @derive(Copy)
    "#;
    assert!(
        parse(source).is_err(),
        "an attribute followed by neither func nor struct should be a parse error"
    );
}
