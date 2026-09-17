// Neuro Programming Language - Consuming `self` receivers
//
// A method may take its receiver three ways: `&self` borrows it, `&mut self`
// borrows it exclusively, and a bare `self` CONSUMES it. A consuming call hands
// the value to the method, so the caller loses it: reading the receiver after
// the call is a compile error, exactly as if it had been passed by value to a
// free function.
//
// Consuming is what an `into_*` conversion needs. The method owns the receiver's
// fields, so it may move a non-Copy one straight out instead of cloning it.

struct Point {
    x: f64,
    y: f64
}

impl Point {
    func new(x: f64, y: f64) -> Point {
        Point { x: x, y: y }
    }

    // `&self` borrows: the caller keeps the point.
    func length_squared(&self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    // `self` consumes: the caller does not.
    func into_tuple(self) -> (f64, f64) {
        (self.x, self.y)
    }
}

// A non-Copy struct: `string` owns a buffer, so `Label` moves rather than copies.
struct Label {
    text: string
}

impl Label {
    // The receiver's field is moved out, not cloned. Only a consuming receiver
    // may do this: a `&self` body would release a buffer its caller still owns.
    func into_text(self) -> string {
        self.text
    }
}

// A `Drop` type proves WHERE the destructor runs. The callee owns what it was
// handed, so it destroys the receiver at its own exit — once, never twice.
struct Ticket {
    spent: &mut i32
}

impl Drop for Ticket {
    func drop(&mut self) {
        *self.spent = *self.spent + 1
    }
}

impl Ticket {
    func redeem(self) -> i32 {
        7
    }
}

func main() -> i32 {
    val p = Point::new(3.0, 4.0)
    val squared = p.length_squared()      // borrows: `p` survives
    val coords = p.into_tuple()           // moves: `p` is gone after this
    // val again = p.length_squared()     // <- rejected: use of moved value 'p'
    println("length_squared = {squared}")
    println("coords         = {coords.0} {coords.1}")

    val label = Label { text: "sensor-a" }
    val text = label.into_text()
    println("label text     = {text}")

    mut spent: i32 = 0
    val value = Ticket { spent: &mut spent }.redeem()
    println("ticket value   = {value}")
    println("tickets spent  = {spent}")

    // 25 + 7 + 1 = 33
    val code = squared as i32 + value + spent
    println("packed code    = {code}")
    return code
}
