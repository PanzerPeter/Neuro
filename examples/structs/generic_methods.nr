// Generic methods: a method declares its own type parameters after its name.
// `Shelf::pick<U>` works for any `U`. The call infers `U` from its arguments, or a
// turbofish after the method name supplies it. Each `U` a program uses compiles to one copy
// of the method.

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

// A trait method takes parameters of its own too. An implementor may rename them, and a call
// through a bound infers them as any other call does.
trait Labeler {
    func label<U>(&self, value: U) -> (bool, U)
}

impl Labeler for Shelf {
    func label<V>(&self, value: V) -> (bool, V) {
        (self.left, value)
    }
}

func labeled_value<T: Labeler>(source: &T) -> i32 {
    source.label(7).1
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
    println("labeled = {labeled_value(&shelf)}")
    count + wide as i32
}
