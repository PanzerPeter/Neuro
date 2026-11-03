// The `loop { }` statement: an infinite loop with no condition, so `break` is the only way
// out. It is the form the `prefer_loop_over_while_true` lint suggests in place of `while
// true`. `continue` re-enters the body. This program skips 3 with `continue` and stops once
// the counter passes 5 with `break`.

func main() -> i32 {
    mut i: i32 = 0
    mut sum: i32 = 0

    loop {
        i = i + 1
        if i == 3 {
            println("continue past {i}")
            continue
        }
        if i > 5 {
            println("break at {i}")
            break
        }
        sum = sum + i
        println("add {i}, sum = {sum}")
    }

    println("sum = {sum}")
    return sum
}
