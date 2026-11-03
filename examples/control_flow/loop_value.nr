// `loop` as a value: `break v` exits the loop and makes `v` the value of the whole `loop`
// expression. All value-carrying breaks of one loop must agree on type. Only `loop` yields
// a value, because only `loop` leaves solely through a `break`. `while` and `for` always
// yield unit. The two loops here print their values, and the exit code combines them.

func main() -> i32 {
    mut i: i32 = 0
    val first_even = loop {
        i = i + 1
        if i % 2 == 0 {
            break i
        }
    }
    println("first even candidate = {first_even}")

    mut j: i32 = 0
    val scaled = loop {
        j = j + 1
        if j >= 5 {
            break j * 10
        }
    }
    println("scaled counter       = {scaled}")

    val balance = first_even + scaled - 52
    println("balance              = {balance}")

    return balance
}
