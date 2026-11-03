// `Drop`: deterministic destruction at scope exit. A type implementing `Drop` runs its
// `drop(&mut self)` when its owner leaves scope, on normal exit only (a panic aborts
// without running destructors). Each guard bumps a counter through a `&mut` when it is
// dropped. Guards are destroyed in reverse declaration order, and a guard moved into
// another binding is dropped once, by its new owner. A struct field is destroyed with the
// struct that holds it.

struct Guard {
    id: i32,
    sink: &mut i32
}

struct Slot {
    guard: Guard
}

impl Drop for Guard {
    func drop(&mut self) {
        *self.sink = *self.sink + 1
        val id = self.id
        println("drop guard {id}")
    }
}

func main() -> i32 {
    mut first: i32 = 0
    mut second: i32 = 0
    mut third: i32 = 0
    {
        val a = Guard { id: 1, sink: &mut first }
        val b = Guard { id: 2, sink: &mut second }
        val c = Guard { id: 3, sink: &mut third }
        val moved = c   // `c` is moved into `moved`; only one of them is dropped
    }
    // a, b, and (moved) have each run their destructor exactly once.
    mut fourth: i32 = 0
    mut fifth: i32 = 0
    {
        // The field is destroyed with the struct that holds it, with no `drop` call
        // of its own to write.
        val slot = Slot { guard: Guard { id: 4, sink: &mut fourth } }
        val boxed = Slot { guard: Guard { id: 5, sink: &mut fifth } }
        val taken = boxed.guard   // given up first, so `boxed` no longer owns it
    }
    val dropped = first + second + third + fourth + fifth
    println("destructors run: {dropped}")
    return dropped
}
