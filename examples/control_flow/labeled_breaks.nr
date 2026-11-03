// Loop labels with `break 'label` and `continue 'label`. A label names an enclosing `for`,
// `while` or `loop`, so a nested loop can leave or continue an outer one. The quote marks a
// label: `break outer` would carry the value of a binding named `outer`. The labeled `break
// 'outer` below stops after four visited pairs, at (0, 3), where a plain `break` would not
// leave both loops.

func main() -> i32 {
    mut count: i32 = 0

    'outer: for i in 0..5 {
        for j in 0..5 {
            count = count + 1
            println("visit ({i}, {j})")
            if i + j >= 3 {
                println("break 'outer at ({i}, {j})")
                break 'outer
            }
        }
    }

    println("iterations = {count}")
    return count
}
