// A sweep over the core language: arithmetic, comparison, logic, if/else,
// nested if, function calls, the numeric types, and mutation.
//
// Every check prints what it produced and the run ends with a verdict line, so
// a failure is readable in the output rather than only in the exit code. Exit 0
// means every check held.

func test_arithmetic() -> i32 {
    val a: i32 = 10
    val b: i32 = 5
    val add: i32 = a + b
    val sub: i32 = a - b
    val mul: i32 = a * b
    val div: i32 = a / b
    val mod: i32 = a % b
    return add + sub + mul + div + mod
}

func test_comparison() -> bool {
    val a: i32 = 10
    val b: i32 = 5
    val eq: bool = a == a
    val ne: bool = a != b
    val lt: bool = b < a
    val gt: bool = a > b
    val le: bool = b <= a
    val ge: bool = a >= a
    return eq && ne && lt && gt && le && ge
}

func test_logical() -> bool {
    val t: bool = true
    val f: bool = false
    val and_result: bool = t && t
    val or_result: bool = t || f
    val not_result: bool = !f
    return and_result && or_result && not_result
}

func test_if_else(x: i32) -> i32 {
    if x > 5 {
        return 1
    }
    return 0
}

func test_nested_if(x: i32, y: i32) -> i32 {
    // Nested if/else with returns in all branches hits the Phase 1 CFG limitation;
    // use a mutable accumulator + single return to stay within supported patterns.
    mut result: i32 = 0
    if x > 0 {
        if y > 0 {
            result = 3
        } else {
            result = 2
        }
    } else {
        if y > 0 {
            result = 1
        }
    }
    return result
}

func test_function_call(a: i32, b: i32) -> i32 {
    return a * b
}

func test_nested_calls() -> i32 {
    val x: i32 = test_function_call(2, 3)
    val y: i32 = test_function_call(x, 4)
    return y
}

func test_floats(a: f32, b: f32) -> f32 {
    return a * b
}

func test_f64(a: f64, b: f64) -> f64 {
    return a + b
}

func test_i64(a: i64, b: i64) -> i64 {
    return a + b
}

func test_bool_return() -> bool {
    return true
}

func test_multiple_vars() -> i32 {
    val a: i32 = 1
    val b: i32 = 2
    val c: i32 = 3
    val d: i32 = 4
    val e: i32 = 5
    return a + b + c + d + e
}

func test_mut_variable() -> i32 {
    mut x: i32 = 25
    return x
}

func test_shadowing() -> i32 {
    mut result: i32 = 10
    if true {
        result = 20
    }
    return result
}

func main() -> i32 {
    val arith: i32 = test_arithmetic()
    val cmp: bool = test_comparison()
    val log: bool = test_logical()
    val if1: i32 = test_if_else(10)
    val if2: i32 = test_nested_if(1, 1)
    val call: i32 = test_nested_calls()
    val mult: i32 = test_multiple_vars()
    val mut_test: i32 = test_mut_variable()
    val shadow: i32 = test_shadowing()
    val bool_res: bool = test_bool_return()

    // The three wider numeric types get exercised here rather than in the
    // verdict below, which only deals in i32 and bool.
    val f32_product: f32 = test_floats(1.5, 4.0)
    val f64_sum: f64 = test_f64(0.25, 0.5)
    val i64_sum: i64 = test_i64(4000000000i64, 1000000000i64)

    println("arithmetic      {arith}")
    println("comparison      {cmp}")
    println("logical         {log}")
    println("if / else       {if1}")
    println("nested if       {if2}")
    println("nested calls    {call}")
    println("multiple vars   {mult}")
    println("mut variable    {mut_test}")
    println("shadowed mut    {shadow}")
    println("bool return     {bool_res}")
    println("f32 product     {f32_product}")
    println("f64 sum         {f64_sum}")
    println("i64 sum         {i64_sum}")

    if cmp {
        if log {
            if bool_res {
                if if1 == 1 {
                    if if2 == 3 {
                        if call == 24 {
                            if mult == 15 {
                                if mut_test == 25 {
                                    if shadow == 20 {
                                        println("all checks passed")
                                        return 0
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    println("a check failed")
    return 1
}
