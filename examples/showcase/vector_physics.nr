// Vector physics step — operator traits working together with prior features.
//
// Combines: operator traits (`+` / `-` / unary `-` / `==`) on a `Copy` struct,
// an `impl` block with an `&self` method, compound assignment (`+=` desugaring
// through the `Add` impl), `if`-expressions, and a `while` loop. The operators dispatch
// to the trait methods; everything is monomorphized to plain calls.

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

impl Vec2 {
    // Manhattan distance from the origin — an `&self` query method.
    func manhattan(&self) -> i32 {
        val ax = if self.x < 0 { -self.x } else { self.x }
        val ay = if self.y < 0 { -self.y } else { self.y }
        ax + ay
    }
}

func main() -> i32 {
    mut pos = Vec2 { x: 0, y: 0 }
    val vel = Vec2 { x: 2, y: 3 }

    // Integrate four steps: `+=` dispatches through the `Add` impl.
    mut steps = 0
    while steps < 4 {
        pos += vel
        steps += 1
    }
    // pos = (8, 12)

    mut result = 0
    val target = Vec2 { x: 8, y: 12 }
    if pos == target {
        result = result + pos.manhattan()   // 8 + 12 = 20
    }

    val back = pos - vel      // (6, 9)
    val flipped = -back       // (-6, -9)
    result = result + flipped.manhattan()   // 20 + 15 = 35
    result
}
