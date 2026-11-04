// Patterns that look inside a value. A payload takes any pattern, so an arm can test a value
// or another enum; alternatives may bind if each binds the same names; `name @ pattern` keeps
// the value a pattern tested; an array pattern splits a `[T; N]` or walks a `&[T]`. The checker
// reads inside all of these, so a value no arm takes is a compile error that names it.

enum Tok {
    Num(i32),
    Neg(i32),
    Word(i32, bool),
    End
}

enum Pair {
    Two(Tok, Tok),
    Solo(Tok)
}

enum Shape {
    Circle { radius: i32 },
    Rect { w: i32, h: i32 }
}

// The arm has one body: `x` comes from whichever alternative matched.
func magnitude(t: Tok) -> i32 {
    match t {
        Tok::Num(x) | Tok::Neg(x) | Tok::Word(x, _) => x,
        Tok::End => 0
    }
}

// Payloads test values and nest enums. A failed guard tries the next alternative.
func score(p: Pair) -> i32 {
    match p {
        Pair::Solo(Tok::Num(0)) => 1,
        Pair::Solo(Tok::Word(n, true)) => n + 100,
        Pair::Solo(_) => 2,
        Pair::Two(Tok::Num(a), Tok::Num(b)) | Pair::Two(Tok::Neg(a), Tok::Neg(b)) if a > b => a - b,
        Pair::Two(Tok::End, _) => 3,
        Pair::Two(_, _) => 4
    }
}

// A struct variant's field takes a pattern after a colon, and `..` skips the rest.
func thin(s: Shape) -> bool {
    match s {
        Shape::Rect { w: 1, .. } | Shape::Rect { h: 1, .. } => true,
        Shape::Rect { .. } | Shape::Circle { .. } => false
    }
}

// `@` binds the whole value at its position, here and one payload down.
func bucket(code: Option<i32>) -> i32 {
    match code {
        Some(n @ 1..=9) => n * 10,
        whole @ Some(_) => (whole ?? 0) + 1,
        None => 0
    }
}

// Over `[T; N]`, the length is already known, and `rest` is a shorter array.
func corners(a: [i32; 4]) -> i32 {
    match a {
        [0, ..rest] => rest[0] + rest[2],
        [first, _, _, last] => first * last
    }
}

// Over a slice, a pattern tests the length, and `rest` is a shorter slice.
func total(s: &[i32]) -> i32 {
    match s {
        [] => 0,
        [head, ..rest] => head + total(rest)
    }
}

// `if val` and `while val` take the same patterns.
func pair_products(s: &[i32]) -> i32 {
    mut cursor = s
    mut sum = 0
    while val [a, b, ..rest] = cursor {
        sum += a * b
        cursor = rest
    }
    if val [odd] = cursor {
        sum += odd
    }
    sum
}

func main() -> i32 {
    println("magnitude {magnitude(Tok::Num(4))} {magnitude(Tok::Neg(5))} {magnitude(Tok::Word(6, false))}")
    println("score {score(Pair::Solo(Tok::Num(0)))} {score(Pair::Solo(Tok::Word(7, true)))} {score(Pair::Solo(Tok::End))}")
    println("score {score(Pair::Two(Tok::Num(9), Tok::Num(2)))} {score(Pair::Two(Tok::Num(2), Tok::Num(9)))} {score(Pair::Two(Tok::End, Tok::End))}")
    println("thin {thin(Shape::Rect { w: 1, h: 8 })} {thin(Shape::Rect { w: 8, h: 8 })} {thin(Shape::Circle { radius: 1 })}")
    println("bucket {bucket(Some(3))} {bucket(Some(30))} {bucket(None)}")
    println("corners {corners([0, 1, 2, 3])} {corners([2, 1, 1, 5])}")

    val numbers = [1, 2, 3, 4, 5]
    val s: &[i32] = &numbers
    println("total {total(s)}, pair products {pair_products(s)}")
    total(s) + pair_products(s)
}
