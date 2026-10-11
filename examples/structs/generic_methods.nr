// Generic methods: a method declares type parameters of its own after its name.
//
// `Shelf::pick<U>` works for any `U`: the call infers it from the arguments, or a
// turbofish after the method name supplies it. Each `U` a program uses is one compiled
// copy of the method. An associated function takes its own parameters the same way.

struct Shelf {
    left: bool
}

impl Shelf {
    func pick<U>(&self, a: U, b: U) -> U {
        if self.left { a } else { b }
    }

    func empty<U>() -> Option<U> {
        Option::None
    }
}

func main() -> i32 {
    val shelf = Shelf { left: true }

    val count = shelf.pick(40, 7)                // U = i32, inferred
    val name = shelf.pick("left", "right")       // U = string, inferred
    val wide = shelf.pick::<i64>(2i64, 9i64)     // U = i64, from the turbofish
    val nothing = Shelf::empty::<f64>()          // nothing to infer from: turbofish

    println("count   = {count}")
    println("name    = {name}")
    println("wide    = {wide}")
    println("nothing = {nothing ?? 0.5}")
    count + wide as i32
}
