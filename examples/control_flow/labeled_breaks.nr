// Loop labels with `break label` / `continue label`.
//
// A label (`outer:`) names an enclosing `for` / `while` / `loop` so that a
// nested loop can break or continue an *outer* loop rather than just the
// innermost one.
//
// The labeled `break outer` below leaves both loops the moment `i + j` first
// reaches 3. Every visited pair is printed, so the output stops at (0, 3) after
// four iterations rather than running the full 5x5 grid — which is exactly what
// distinguishes `break outer` from a plain `break`.

func main() -> i32 {
    mut count: i32 = 0

    outer: for i in 0..5 {
        for j in 0..5 {
            count = count + 1
            println("visit ({i}, {j})")
            if i + j >= 3 {
                println("break outer at ({i}, {j})")
                break outer
            }
        }
    }

    println("iterations = {count}")
    return count
}
