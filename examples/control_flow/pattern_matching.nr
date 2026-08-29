// Pattern matching: `match` as an exhaustive expression over enums and
// scalar values — variant deconstruction with payload binding, literal / or /
// range patterns, guard clauses, and the `_` wildcard.
//
// Every scrutinee prints the arm it selected, so the range and guard arms are
// distinguishable from the wildcard in the output.

enum Op {
    Nop,
    Push(i32),
    Range { lo: i32, hi: i32 }
}

// Deconstruct each variant form; a tuple/struct payload binds its fields.
func eval(op: Op) -> i32 {
    match op {
        Op::Nop => 0,
        Op::Push(n) => n,
        Op::Range { lo, hi } => hi - lo
    }
}

// Value patterns over an integer scrutinee.
func classify(n: i32) -> i32 {
    match n {
        0 => 10,
        1 | 2 => 20,
        3..=9 => 30,
        n if n < 0 => 40,
        _ => 99
    }
}

func main() -> i32 {
    mut total = 0

    val nop = eval(Op::Nop)
    val push = eval(Op::Push(7))
    val range = eval(Op::Range { lo: 2, hi: 5 })
    println("Op::Nop                    -> {nop}")
    println("Op::Push(7)                -> {push}")
    // A literal `{` in interpolated text is escaped `\{` so it is not read as
    // opening a hole. A closing `}` is unambiguous outside a hole and stands as
    // itself — `\}` is not an escape sequence.
    println("Op::Range \{ lo: 2, hi: 5 } -> {range}")
    total += nop + push + range               // => 10

    val zero = classify(0)      // literal arm
    val small = classify(2)     // or-pattern arm
    val mid = classify(8)       // range arm
    val negative = classify(-1) // guarded arm
    val other = classify(100)   // wildcard arm
    println("classify(0)    literal  -> {zero}")
    println("classify(2)    or       -> {small}")
    println("classify(8)    range    -> {mid}")
    println("classify(-1)   guard    -> {negative}")
    println("classify(100)  wildcard -> {other}")
    total += zero + small + mid + negative + other

    println("total                      -> {total}")
    return total            // 209
}
