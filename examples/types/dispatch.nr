// Static and dynamic dispatch — the two ways to satisfy a trait bound.
//
// `impl Trait` is static dispatch: it is shorthand for an anonymous type parameter,
// so the compiler emits one specialized copy of the function per concrete type that
// flows through it. There is no indirection and no runtime cost.
//
// `dyn Trait` is dynamic dispatch: `&dyn Speaker` is a single runtime type that can
// hold ANY implementor. The reference carries two words — a pointer to the value and
// a pointer to that type's method table — and a call jumps through the table. That
// costs one indirection, and buys the ability to write one function that serves
// values of different concrete types.
//
// A trait object is unsized, so `dyn Trait` only ever appears behind a reference.
// A trait is usable as `dyn` only if it is object-safe: every method must take
// `&self` or `&mut self`, so the table has a fixed layout.

trait Speaker {
    func volume(&self) -> i32

    // A default method: an implementor inherits this unless it defines its own.
    func legs(&self) -> i32 { 4 }
}

@derive(Copy)
struct Dog {
    size: i32,
}

@derive(Copy)
struct Bird {
    size: i32,
}

impl Speaker for Dog {
    func volume(&self) -> i32 { self.size * 3 }
}

impl Speaker for Bird {
    func volume(&self) -> i32 { self.size * 2 }
    func legs(&self) -> i32 { 2 }
}

// Static dispatch. Monomorphized: the compiler generates a separate copy of this
// body for Dog and for Bird, each calling its method directly.
func loud_static(s: &impl Speaker) -> i32 {
    s.volume()
}

// Dynamic dispatch. ONE copy of this body serves both types; `volume` and `legs`
// are looked up in the receiver's method table at runtime. Note that Dog reaches
// the inherited default `legs`, while Bird reaches its own override.
func loud_dyn(s: &dyn Speaker) -> i32 {
    s.volume() + s.legs()
}

func main() -> i32 {
    val d = Dog { size: 5 }    // volume 15, legs 4 (inherited default)
    val b = Bird { size: 4 }   // volume  8, legs 2 (overridden)

    val stat = loud_static(&d) // 15

    // The same function, two different concrete types behind one interface.
    val a = loud_dyn(&d)       // 15 + 4 = 19
    val c = loud_dyn(&b)       //  8 + 2 = 10

    stat + a + c               // 15 + 19 + 10 = 44
}
