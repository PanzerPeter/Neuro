// Static and dynamic dispatch: the two ways to satisfy a trait bound. `impl Trait` is
// shorthand for an anonymous type parameter, so the compiler emits one copy per concrete
// type, with no indirection. `dyn Trait` is a single runtime type that can hold any
// implementor. The reference carries a value pointer and a method-table pointer, and each
// call costs one indirection. `dyn Trait` appears only behind a reference, and only for an
// object-safe trait: every method takes `&self` or `&mut self`.

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
    println("loud_static(&Dog)  static  -> {stat}")

    // The same function, two different concrete types behind one interface.
    val a = loud_dyn(&d)       // 15 + 4 = 19
    val c = loud_dyn(&b)       //  8 + 2 = 10
    println("loud_dyn(&Dog)     dynamic -> {a}   (volume 15 + inherited legs 4)")
    println("loud_dyn(&Bird)    dynamic -> {c}   (volume 8 + overridden legs 2)")

    val total = stat + a + c
    println("total                      -> {total}")
    total                      // 15 + 19 + 10 = 44
}
