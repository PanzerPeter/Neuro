// Operator traits on a type that owns its data. `+` hands both operands to `add`, so
// they move; `*` takes its right operand by reference, so that one stays with its owner.
// `+=` and `|=` update in place through `*Assign` impls, and `!=`, `<=`, `>` and `>=`
// come from the required `eq` and `lt`.

struct Samples {
    values: Vec<i32>
}

impl Samples {
    func of(first: i32) -> Samples {
        mut values: Vec<i32> = Vec::new()
        values.push(first)
        Samples { values: values }
    }

    func total(&self) -> i32 {
        mut sum = 0
        for v in &self.values {
            sum += *v
        }
        sum
    }
}

impl Add for Samples {
    type Output = Samples
    func add(self, rhs: Samples) -> Samples {
        mut values = self.values
        for v in rhs.values {
            values.push(v)
        }
        Samples { values: values }
    }
}

impl Mul<&Samples> for Samples {
    type Output = Samples
    func mul(self, rhs: &Samples) -> Samples {
        val scale = rhs.total()
        mut values: Vec<i32> = Vec::new()
        for v in self.values {
            values.push(v * scale)
        }
        Samples { values: values }
    }
}

impl AddAssign<i32> for Samples {
    func add_assign(&mut self, rhs: i32) {
        self.values.push(rhs)
    }
}

impl BitOrAssign<&Samples> for Samples {
    func bitor_assign(&mut self, rhs: &Samples) {
        self.values.push(rhs.total())
    }
}

impl PartialEq for Samples {
    func eq(&self, rhs: &Samples) -> bool {
        self.total() == rhs.total()
    }
}

impl Comparable for Samples {
    func lt(&self, rhs: &Samples) -> bool {
        self.total() < rhs.total()
    }
}

func main() -> i32 {
    val a = Samples::of(1)
    val b = Samples::of(2)
    val joined = a + b
    println("joined {joined.total()}")

    val scale = Samples::of(3)
    mut scaled = joined * &scale
    println("scaled {scaled.total()} by {scale.total()}")

    scaled += 10
    scaled |= &scale
    println("updated {scaled.total()}")

    mut score = 0
    if scaled != scale { score += 1 }
    if scale <= scaled { score += 2 }
    if scaled > scale { score += 4 }
    if scaled >= scaled { score += 8 }
    println("score {score}")
    score
}
