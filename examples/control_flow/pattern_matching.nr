// Pattern matching (§3.6): `match` as an exhaustive expression over enums and
// scalar values — variant deconstruction with payload binding, literal / or /
// range patterns, guard clauses, and the `_` wildcard.

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
    total += eval(Op::Nop)                    // + 0
    total += eval(Op::Push(7))                // + 7
    total += eval(Op::Range { lo: 2, hi: 5 }) // + 3   => 10

    total += classify(0)    // + 10  => 20
    total += classify(2)    // + 20  => 40
    total += classify(8)    // + 30  => 70
    total += classify(-1)   // + 40  => 110
    total += classify(100)  // + 99  => 209

    return total            // 209
}
