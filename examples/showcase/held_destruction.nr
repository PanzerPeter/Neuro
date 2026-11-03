// Showcase: a value held inside another value is destroyed with its holder. Integration
// example: `Drop` announcing each destruction, with a struct field, an array element, a
// tuple element, an enum payload and a newtype's inner value as holders, plus a `Vec<i32>`
// field and a `match` over a non-scalar payload. A holder owns what it holds. When the
// holder goes, its contents go too, in declaration order and exactly once. A value given up
// first, by a move or an assignment, is destroyed on its own path instead.

struct Handle {
    id: i32,
}

impl Drop for Handle {
    func drop(&mut self) {
        println("  closed handle {self.id}")
    }
}

struct Channel {
    primary: Handle,
    backup: Handle,
}

// A newtype is transparent: it forwards its inner type's ownership along with its name.
newtype Session = Handle

enum Slot {
    Live(Handle),
    Empty,
}

// A struct owning a heap buffer rather than another `Drop` value. The buffer follows
// the same rule, because the rule is about holding an owner and not about which kind.
struct Registry {
    open: Vec<i32>,
}

const CHANNELS: i32 = 2

func main() -> i32 {
    println("struct fields:")
    {
        val ch = Channel { primary: Handle { id: 1 }, backup: Handle { id: 2 } }
    }

    println("array and tuple elements:")
    {
        val bank = [Handle { id: 3 }, Handle { id: 4 }]
        val labelled = (Handle { id: 5 }, CHANNELS)
    }

    println("enum payload and newtype inner value:")
    {
        val stored = Slot::Live(Handle { id: 6 })
        val idle = Slot::Empty
        val session = Session(Handle { id: 7 })
    }

    println("a payload taken out of its variant:")
    {
        // The arm binds the payload by value, so the handle leaves the enum and is
        // destroyed by the binding that received it.
        val rebound = match Slot::Live(Handle { id: 8 }) {
            Slot::Live(h) => h
            Slot::Empty => Handle { id: 0 }
        }
    }

    println("one element given up first:")
    {
        val ch = Channel { primary: Handle { id: 9 }, backup: Handle { id: 10 } }
        // `ch.primary` leaves the holder here, so `taken` destroys it and `ch` destroys
        // only the sibling it still owns.
        val taken = ch.primary
    }

    println("a field displaced by an assignment:")
    {
        mut ch = Channel { primary: Handle { id: 11 }, backup: Handle { id: 12 } }
        ch.primary = Handle { id: 13 }
    }

    println("a holder rotated through a loop:")
    {
        mut current = Session(Handle { id: 14 })
        for round in 0..CHANNELS {
            current = Session(Handle { id: 15 + round })
        }
    }

    println("a heap buffer held in a field:")
    mut open: Vec<i32> = Vec::new()
    open.push(1)
    open.push(2)
    open.push(3)
    // The vector moves into the field, and the field is what releases its buffer.
    val registry = Registry { open: open }
    val tracked = registry.open.len() as i32

    println("channels = {CHANNELS}, tracked = {tracked}")
    CHANNELS + tracked
}
