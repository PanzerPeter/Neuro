// Formatted standard output: 600000 interpolated lines. Measures the cost of
// rendering a value into a string and getting the bytes to fd 1, a path
// dominated by formatting and syscalls rather than by arithmetic. `println!`
// flushes a line-buffered stdout on every line, so the lines go through a
// `BufWriter` instead, which is how Rust writes bulk output.

use std::io::{BufWriter, Write};

fn main() {
    let mut out = BufWriter::new(std::io::stdout().lock());
    for i in 0..600000 {
        writeln!(out, "line {i}").unwrap();
    }
}
