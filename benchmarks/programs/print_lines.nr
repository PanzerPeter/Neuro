// Formatted standard output: 600000 interpolated lines. Measures the cost of
// rendering a value into a string and getting the bytes to fd 1, a path
// dominated by formatting and syscalls rather than by arithmetic.

func main() -> i32 {
    for i in 0..600000 {
        println("line {i}")
    }
    return 0
}
