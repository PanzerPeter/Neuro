// Formatted standard output: 200000 interpolated lines. Measures the cost of
// rendering a value into a string and getting the bytes to fd 1 — a path
// dominated by formatting and syscalls rather than by arithmetic.

func main() -> i32 {
    mut i: i32 = 0
    while i < 600000 {
        println("line {i}")
        i += 1
    }
    return 0
}
