// Drop: deterministic destruction at scope exit.
//
// A type that implements `Drop` runs its `drop(&mut self)` destructor when its
// owner leaves scope, on normal exit only (a panic aborts without running
// destructors). Here each `Guard` increments a shared counter through a mutable
// borrow when it is dropped, so the counter is observable once the scope closes.
//
// Three guards are created in an inner block and destroyed at its end (in reverse
// declaration order); one of them is moved into a new binding first, so it is
// dropped exactly once, not twice.
//
// A guard held inside another value follows the same rule: the holder owns what it
// holds, so a struct field is destroyed when the struct is. The program returns 5.

struct Guard {
    sink: &mut i32
}

struct Slot {
    guard: Guard
}

impl Drop for Guard {
    func drop(&mut self) {
        *self.sink = *self.sink + 1
        val count = *self.sink
        println("drop #{count}")
    }
}

func main() -> i32 {
    mut dropped: i32 = 0
    {
        val a = Guard { sink: &mut dropped }
        val b = Guard { sink: &mut dropped }
        val c = Guard { sink: &mut dropped }
        val moved = c   // `c` is moved into `moved`; only one of them is dropped
    }
    // a, b, and (moved) have each run their destructor exactly once.
    {
        // The field is destroyed with the struct that holds it, with no `drop` call
        // of its own to write.
        val slot = Slot { guard: Guard { sink: &mut dropped } }
        val boxed = Slot { guard: Guard { sink: &mut dropped } }
        val taken = boxed.guard   // given up first, so `boxed` no longer owns it
    }
    println("destructors run: {dropped}")
    return dropped
}
