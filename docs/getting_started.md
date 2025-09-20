Getting Started with NEURO

After installation (`docs/installation.md`), try the following.

Build and test
```bash
cargo build --release
cargo test --workspace
```

Run examples
```bash
# Evaluate an expression
./target/release/neurc eval "2 + 3 * 4"

# Parse and check a program
./target/release/neurc check examples/05_expressions.nr

# Generate LLVM IR (printed to stdout)
./target/release/neurc llvm examples/06_functions.nr

# Compile one or more files (driver scaffold)
./target/release/neurc compile examples/07_control_if.nr examples/08_control_while.nr
```

Project layout
- `compiler/` — vertical slices for lexer, parser, semantics, backends
- `examples/` — small, one-topic `.nr` programs
- `docs/specification/` — one-topic reference pages

Language quickstart
```neuro
fn add(a: int, b: int) -> int {
    return a + b;
}

fn main() -> int {
    let x = add(2, 3);
    if x > 3 { return x; } else { return 0; }
}
```

Notes
- File extension: `.nr`
- Semicolons are required for statements
- Imports accept `import a::b;` or `import "./relative.nr";` (resolver paths: `.`, `src/`, `lib/`)

