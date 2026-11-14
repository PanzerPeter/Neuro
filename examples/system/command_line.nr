// The process around a program: `std::env::args()` is its command line, `std::env::var` one
// environment variable, `std::io::read_line()` the next line of standard input, and
// `std::time::Instant` a reading of a monotonic clock. Arguments for the program follow `--`,
// and input can be piped in:
//
//     printf 'a\nb\n' | neurc run examples/system/command_line.nr -- 4

func sum_to(n: u64) -> u64 {
    mut total: u64 = 0
    for i in 0..n {
        total = total + i
    }
    total
}

func main() -> i32 {
    // The program's own name is first, so the first argument is at index 1.
    val args = std::env::args()
    val steps = args.get(1).map(|a| a.parse::<u64>() ?? 10) ?? 10
    println("steps: {steps}")

    // `None` when the variable is not set.
    val mode = std::env::var("NEURO_EXAMPLE_MODE") ?? "default"
    println("mode: {mode}")

    val start = std::time::Instant::now()
    val total = sum_to(steps * 1000)
    val took = start.elapsed()
    // The time itself changes from run to run; that it was measured does not.
    println("sum {total}, timed: {took.as_secs_f64() >= 0.0}")

    // Each line arrives without its terminator, and `None` ends the input.
    mut lines = 0
    loop {
        match std::io::read_line() {
            Ok(Some(line)) => {
                lines += 1
                println("line {lines}: {line}")
            }
            Ok(None) => break,
            Err(e) => {
                println("cannot read input: {e.message}")
                return 1
            }
        }
    }
    println("{lines} lines of input")
    0
}
