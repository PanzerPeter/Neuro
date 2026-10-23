// Drop: deterministic destruction at scope exit.
//
// A type that implements `Drop` runs its `drop(&mut self)` destructor when its
// owner leaves scope, on normal exit only (a panic aborts without running
// destructors). Here each `Guard` increments a counter through a mutable borrow
// when it is dropped, so the counters are observable once the scope closes. A
// `&mut` is exclusive, so each live guard holds a counter of its own.
//
// Three guards are created in an inner block and destroyed at its end (in reverse
// declaration order); one of them is moved into a new binding first, so it is
// dropped exactly once, not twice.
//
// A guard held inside another value follows the same rule: the holder owns what it
// holds, so a struct field is destroyed when the struct is. The program returns 5.

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
