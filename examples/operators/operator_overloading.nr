// Operator overloading via operator traits.
//
// Operators on a user type are sugar for trait method calls. A `Copy` struct that
// implements `Add` / `Sub` / `Neg` / `PartialEq` gets `+`, `-`, unary `-`, and
// `==` / `!=` — each dispatched to its impl method and monomorphized to a plain call
// (no vtable, zero runtime cost).

@derive(Copy, Clone)
struct Vec2 {
    x: i32,
    y: i32
}

impl Add for Vec2 {
    type Output = Vec2
    func add(self, rhs: Vec2) -> Vec2 {
        Vec2 { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

impl Sub for Vec2 {
    type Output = Vec2
    func sub(self, rhs: Vec2) -> Vec2 {
        Vec2 { x: self.x - rhs.x, y: self.y - rhs.y }
    }
}

impl Neg for Vec2 {
    type Output = Vec2
    func neg(self) -> Vec2 {
        Vec2 { x: -self.x, y: -self.y }
    }
}

impl PartialEq for Vec2 {
    func eq(&self, rhs: &Vec2) -> bool {
        self.x == rhs.x && self.y == rhs.y
    }
    func ne(&self, rhs: &Vec2) -> bool {
        self.x != rhs.x || self.y != rhs.y
    }
}

func main() -> i32 {
    val a = Vec2 { x: 5, y: 9 }
    val b = Vec2 { x: 2, y: 3 }

    val sum  = a + b     // (7, 12)
    val diff = a - b     // (3, 6)
    val neg  = -b        // (-2, -3)

    mut total = sum.x + sum.y + diff.x + diff.y + neg.x + neg.y   // 7+12+3+6-2-3 = 23
    if a == b {
        total = total + 100
    }
    if a != b {
        total = total + 1   // -> 24
    }
    total
}
