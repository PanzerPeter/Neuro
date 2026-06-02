// Showcase — a tiny bit-flag state machine.
//
// Integration example combining bitwise operators with struct state:
//   bit-flag constants via `<<` · set/clear/toggle with `|`, `&`, `^`
//   · struct holding integer state · &self predicate + popcount methods
//   · while loop with `break` · `.shr(n)` right shift · nested if.
//
// A machine's permission flags are mutated step by step; the final set-bit
// count is returned as the exit code.

struct Machine {
    flags: i32
}

impl Machine {
    func new() -> Machine {
        Machine { flags: 0 }
    }

    // Is a given single-bit flag set?
    func has(&self, flag: i32) -> bool {
        (self.flags & flag) == flag
    }

    // Number of set bits (Kernighan-style scan via right shift).
    func popcount(&self) -> i32 {
        mut n: i32 = self.flags
        mut c: i32 = 0
        while n != 0 {
            c += (n & 1)
            n = n.shr(1)
        }
        return c
    }
}

func main() -> i32 {
    val READ: i32 = 1 << 0     // 1
    val WRITE: i32 = 1 << 1    // 2
    val EXEC: i32 = 1 << 2     // 4

    mut m = Machine::new()

    // Step through a sequence of operations, mutating the flag field directly.
    mut step: i32 = 0
    while step < 10 {
        if step == 1 { m.flags = m.flags | READ }
        if step == 2 { m.flags = m.flags | WRITE }
        if step == 3 { m.flags = m.flags | EXEC }
        if step == 4 { m.flags = m.flags ^ WRITE }   // toggle WRITE back off
        if step == 5 { break }
        step += 1
    }

    // Final flags: READ | EXEC (WRITE was set then cleared) -> popcount 2.
    if m.has(READ) {
        if m.has(EXEC) {
            return m.popcount()   // 2
        }
    }
    return 99
}
