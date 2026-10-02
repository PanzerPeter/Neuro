// Float-to-text throughput: 600000 interpolated floating-point holes.
//
// Rendering an `f64` at a fixed precision is what a training loop does every time
// it reports a loss. The value is irrational-looking on purpose: a short decimal
// expansion would let the conversion finish early and measure the wrong thing.
// `println!` flushes a line-buffered stdout on every line, so the lines go through
// a `BufWriter` instead, which is how Rust writes bulk output.

use std::io::{BufWriter, Write};

fn main() {
    let mut out = BufWriter::new(std::io::stdout().lock());
    for i in 0..600000 {
        let x = i as f64 * 1.41421356237309 - 0.618033988749895;
        writeln!(out, "loss {x:.6}").unwrap();
    }
}
