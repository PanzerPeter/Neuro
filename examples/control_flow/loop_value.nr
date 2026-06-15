// `loop` as a value expression (§3.7).
//
// Because blocks are expressions, a `loop` can produce a value: `break v`
// exits the loop and makes `v` the value of the whole `loop` expression. All
// value-carrying `break`s for one loop must agree on type. `while` and `for`
// always yield unit — only `loop` can yield a value, because only `loop` is
// guaranteed to leave solely via a `break`.
//
// Here the first loop searches for the first even candidate (2) and the second
// runs until a counter reaches 5, yielding 5 * 10 = 50. The program returns
// 2 + 50 - 52 = 0.

func main() -> i32 {
    mut i: i32 = 0
    val first_even = loop {
        i = i + 1
        if i % 2 == 0 {
            break i
        }
    }

    mut j: i32 = 0
    val scaled = loop {
        j = j + 1
        if j >= 5 {
            break j * 10
        }
    }

    return first_even + scaled - 52
}
