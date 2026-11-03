// Showcase: a tiny bit-flag state machine. Integration example: bit-flag constants with
// `<<`, set, mask and toggle with `|=`, `&=` and `^=` on a struct field, a `<<=` probe
// walking the bits in a `&self` method, `.shr(n)`, `~` through a `BitNot` impl, `.abs()`,
// and a `while` loop. The machine's flags are mutated step by step, and the final set-bit
// count is the exit code.

@derive(Copy, Clone)
struct Machine {
    flags: i32
}

// `~m` is the machine holding every known flag that `m` does not.
impl BitNot for Machine {
    type Output = Machine
    func bitnot(self) -> Machine {
        Machine { flags: ~self.flags & 7 }   // 7 is READ | WRITE | EXEC
    }
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
        if step == 1 { m.flags |= READ }
        if step == 2 { m.flags |= WRITE }
        if step == 3 { m.flags |= EXEC }
        if step == 4 { m.flags ^= WRITE }                  // toggle WRITE back off
        if step == 5 { m.flags &= READ | WRITE | EXEC }    // mask to the known bits
        println("step {step}: flags {m.flags:03b}")
        if step == 5 { break }
        step += 1
    }

    val read_set = m.has(READ)
    val write_set = m.has(WRITE)
    val exec_set = m.has(EXEC)
    println("READ  set = {read_set}")
    println("WRITE set = {write_set}   (set on step 2, cleared on step 4)")
    println("EXEC  set = {exec_set}")

    // Walk the flag bits with a shifting probe.
    mut probe: i32 = READ
    while probe <= EXEC {
        val on = m.has(probe)
        println("bit {probe:03b} -> {on}")
        probe <<= 1
    }

    // The denied set is the complement of the granted one; `.abs()` makes the gap
    // between their sizes the same whichever side is larger.
    val denied = ~m
    val gap = (denied.popcount() - m.popcount()).abs()
    println("denied    = {denied.flags:03b}")
    println("gap       = {gap}")

    // Final flags: READ | EXEC (WRITE was set then cleared) -> popcount 2.
    if m.has(READ) {
        if m.has(EXEC) {
            val bits = m.popcount()
            println("popcount  = {bits}")
            return bits           // 2
        }
    }
    println("expected flags were not set")
    return 99
}
