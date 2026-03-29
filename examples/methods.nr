// Methods example — Phase 2 feature
//
// Demonstrates impl blocks, instance methods (&self), and
// associated functions (TypeName::func) on structs.

struct Counter {
    value: i32
}

impl Counter {
    func new(start: i32) -> Counter {
        Counter { value: start }
    }

    func get(&self) -> i32 {
        self.value
    }

    func plus(&self, n: i32) -> i32 {
        self.value + n
    }
}

struct Rect {
    width: i32,
    height: i32
}

impl Rect {
    func new(w: i32, h: i32) -> Rect {
        Rect { width: w, height: h }
    }

    func area(&self) -> i32 {
        self.width * self.height
    }

    func perimeter(&self) -> i32 {
        (self.width + self.height) * 2
    }
}

func main() -> i32 {
    // Associated function call
    val c = Counter::new(10)

    // Instance method call
    val v = c.get()           // 10
    val sum = c.plus(32)      // 42

    // Rectangle methods
    val r = Rect::new(6, 7)
    val a = r.area()          // 42
    val p = r.perimeter()     // 26

    return sum  // exits with 42
}
