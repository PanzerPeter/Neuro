// Showcase: reassigning a binding releases the value it displaces. Integration example:
// `Drop` counting through a `&mut i32` sink, owned `string` buffers, a `Vec<string>`
// replaced by a fresh vector, and a range `for` loop rotating one binding through many
// values. A `mut` binding holds one live value at a time, so the destructor runs at the
// assignment. A value already moved out is not released twice, a `string` rebound to a
// literal owns nothing to release, and a reassignment may read the value it displaces.

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
