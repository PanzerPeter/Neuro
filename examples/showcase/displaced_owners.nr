// Showcase: reassigning a binding releases the value it displaces.
//
// Cumulative integration example combining:
//   deterministic `Drop` destructors counting through a `&mut i32` sink
//   · owned `string` buffers built by `+` and displaced by the next assignment
//   · a `Vec<string>` whose buffer goes back when a fresh vector replaces it
//   · a range `for` loop rotating one binding through many values
//   · a named constant, a struct, an `impl Drop` block, and `println` interpolation
//
// The rule the program leans on: a `mut` binding has exactly one live value. Giving it
// a new one ends the previous value's ownership right there, so the destructor runs at
// the assignment instead of being skipped. Three cases fall out of it, and all three
// are visible below: a value already moved out is not released a second time, a
// `string` rebound to a literal owns nothing to release, and a reassignment may read
// the very value it is about to displace.

struct Rotation {
    index: i32,
    closed: &mut i32,
}

impl Drop for Rotation {
    func drop(&mut self) {
        *self.closed = *self.closed + 1
    }
}

const ROUNDS: i32 = 4

func main() -> i32 {
    mut closed: i32 = 0

    {
        // One binding, five values. Four are displaced by the assignment in the loop
        // and the fifth leaves at the closing brace, so the destructor runs five times.
        mut active = Rotation { index: 0, closed: &mut closed }
        for round in 0..ROUNDS {
            active = Rotation { index: round + 1, closed: &mut closed }
        }
    }
    println("rotations closed = {closed}")

    // A heap `string` follows the same rule. The second line reads the buffer it then
    // displaces, which is why the new value is built before the old one is released.
    mut banner: string = "neuro" + "-report"
    println("banner           = {banner}")
    banner = banner + " v2"
    println("banner           = {banner}")
    // A literal points into `.rodata`, so the binding is left owning nothing.
    banner = "done"
    println("banner           = {banner}")

    // So does a collection: the draft vector's buffer goes back at the reassignment.
    mut batch: Vec<string> = Vec::new()
    batch.push("draft")
    val drafted = batch.len() as i32
    batch = Vec::new()
    batch.push("first")
    batch.push("second")
    println("batch replaced   = {drafted} -> {batch.len()}")

    closed + drafted + (batch.len() as i32)
}
