// NEURO Programming Language - Syntax Parsing Tests
// Integration tests with complete programs

use syntax_parsing::parse;

#[test]
fn test_complete_program_simple() {
    let source = r#"
        func main() -> i32 {
            val result = 42
            return result
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_complete_program_with_arithmetic() {
    let source = r#"
        func calculate() -> i32 {
            val a = 10
            val b = 20
            val sum = a + b
            val product = a * b
            val difference = b - a
            val quotient = b / a
            return sum + product + difference + quotient
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_complete_program_with_conditionals() {
    let source = r#"
        func max(a: i32, b: i32) -> i32 {
            if a > b {
                return a
            } else {
                return b
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_complete_program_multiple_functions() {
    let source = r#"
        func add(a: i32, b: i32) -> i32 {
            return a + b
        }

        func subtract(a: i32, b: i32) -> i32 {
            return a - b
        }

        func multiply(a: i32, b: i32) -> i32 {
            return a * b
        }

        func main() -> i32 {
            val x = add(5, 3)
            val y = subtract(10, 4)
            val z = multiply(x, y)
            return z
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_complete_program_nested_conditionals() {
    let source = r#"
        func classify(x: i32) -> i32 {
            if x > 0 {
                if x > 100 {
                    return 3
                } else {
                    return 2
                }
            } else if x < 0 {
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
fn test_complete_program_with_mutation() {
    let source = r#"
        func increment_and_return(x: i32) -> i32 {
            mut result = x
            result = result + 1
            return result
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_complete_program_complex_expressions() {
    let source = r#"
        func evaluate(a: i32, b: i32, c: i32) -> i32 {
            val result = (a + b) * c - (a - b) / c
            return result
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_complete_program_with_booleans() {
    let source = r#"
        func logical_ops(a: bool, b: bool) -> bool {
            val and_result = a && b
            val or_result = a || b
            val not_a = !a
            if and_result || or_result {
                return true
            } else {
                return not_a
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_complete_program_with_comparisons() {
    let source = r#"
        func compare(x: i32, y: i32) -> bool {
            val eq = x == y
            val ne = x != y
            val lt = x < y
            val gt = x > y
            val le = x <= y
            val ge = x >= y
            return eq || ne || lt || gt || le || ge
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_complete_program_fibonacci_style() {
    let source = r#"
        func fib(n: i32) -> i32 {
            if n <= 1 {
                return n
            } else {
                val n1 = n - 1
                val n2 = n - 2
                val fib1 = fib(n1)
                val fib2 = fib(n2)
                return fib1 + fib2
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_complete_program_with_strings() {
    let source = r#"
        func greet(name: String) {
            val greeting = "Hello"
            val message = "World"
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_complete_program_expression_statements() {
    let source = r#"
        func caller() {
            helper1()
            helper2()
            helper3()
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_milestone_example() {
    let source = r#"
        func add(a: i32, b: i32) -> i32 {
            return a + b
        }

        func main() -> i32 {
            val x = 10
            val y = 20
            val sum = add(x, y)

            if sum > 25 {
                return sum * 2
            } else {
                return sum
            }
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_complex_nested_structure() {
    let source = r#"
        func process(x: i32, y: i32, z: i32) -> i32 {
            mut result = 0

            if x > 0 {
                if y > 0 {
                    if z > 0 {
                        result = x + y + z
                    } else {
                        result = x + y
                    }
                } else {
                    result = x
                }
            } else {
                result = 0
            }

            return result
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_program_with_all_operators() {
    let source = r#"
        func all_ops(a: i32, b: i32) -> bool {
            val add = a + b
            val sub = a - b
            val mul = a * b
            val div = a / b
            val mod = a % b

            val eq = a == b
            val ne = a != b
            val lt = a < b
            val gt = a > b
            val le = a <= b
            val ge = a >= b

            val and = (a > 0) && (b > 0)
            val or = (a > 0) || (b > 0)
            val not = !(a == b)

            return and || or
        }
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}

#[test]
fn test_program_empty_functions() {
    let source = r#"
        func empty1() {}
        func empty2() {}
        func empty3() {}
    "#;
    let result = parse(source);
    assert!(result.is_ok(), "Parse error: {:?}", result.err());
}
