// Loop labels with `break label` / `continue label` (§3.7).
//
// A label (`outer:`) names an enclosing `for` / `while` / `loop` so that a
// nested loop can break or continue an *outer* loop rather than just the
// innermost one.
//
// The labeled `break outer` below leaves both loops the moment `i + j` first
// reaches 3. That happens on the 4th inner iteration of the first outer pass
// (i = 0, j = 3), so `count` is 4 and the program returns 4.

func main() -> i32 {
    mut count: i32 = 0

    outer: for i in 0..5 {
        for j in 0..5 {
            count = count + 1
            if i + j >= 3 {
                break outer
            }
        }
    }

    return count
}
