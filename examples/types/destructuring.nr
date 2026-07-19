// Struct and array destructuring patterns.
//
// A destructuring `val`/`mut` binds the parts of an aggregate in one step. It is a
// binding form, not a new value: the compiler desugars it to ordinary bindings.

struct Point {
    x: i32,
    y: i32
}

func main() -> i32 {
    // Struct destructuring binds each named field by its own name.
    val p = Point { x: 12, y: 18 }
    val Point { x, y } = p               // x = 12, y = 18

    // Array destructuring binds positionally; a rest-less pattern must match the
    // array's length exactly.
    val triple: [i32; 3] = [1, 2, 3]
    val [a, b, c] = triple               // a=1 b=2 c=3

    // A trailing `..rest` captures the remainder as a fresh `[i32; 3]` array.
    val nums: [i32; 5] = [4, 5, 6, 7, 8]
    val [first, ..rest] = nums           // first = 4, rest = [5, 6, 7, 8]
    mut rest_sum: i32 = 0
    for r in rest {
        rest_sum = rest_sum + r          // 5 + 6 + 7 + 8 = 26
    }

    // A bare `..` ignores the remainder; `_` discards a single element.
    val [head, ..] = nums                // head = 4
    val [_, second, _] = triple          // second = 2

    // 12 + 18 + (1+2+3) + 4 + 26 + 4 + 2 = 72
    return x + y + a + b + c + first + rest_sum + head + second
}
