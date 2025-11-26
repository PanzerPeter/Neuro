// Test: i16 type inference in return expression
// Literal 256 should infer as i16

func foo() -> i16 {
    256
}

func main() -> i32 {
    val result: i16 = foo()
    return 0
}
