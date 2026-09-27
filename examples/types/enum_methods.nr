// Enum methods and trait impls: an enum takes `impl` blocks exactly as a struct does.
// A traffic light steps through its cycle with a `&mut self` method, reports itself through
// `&self` methods and a trait (called both through a bound and through `&dyn`), and
// compares with `==` because it implements `PartialEq`.

trait Signal {
    func wait_secs(&self) -> i32

    // A default method: every implementor gets it from `wait_secs`.
    func wait_ms(&self) -> i32 {
        return self.wait_secs() * 1000
    }
}

enum Light {
    Red,
    Amber,
    Green
}

impl Light {
    // An associated function: `Light::start()`, next to the variant paths.
    func start() -> Light {
        return Light::Red
    }

    func name(&self) -> string {
        match self {
            Light::Red => "red",
            Light::Amber => "amber",
            Light::Green => "green"
        }
    }

    // `&mut self` may replace the whole value, not only a field of it.
    func advance(&mut self) {
        self = match self {
            Light::Red => Light::Green,
            Light::Green => Light::Amber,
            Light::Amber => Light::Red
        }
    }
}

impl Signal for Light {
    func wait_secs(&self) -> i32 {
        match self {
            Light::Red => 30,
            Light::Amber => 3,
            Light::Green => 25
        }
    }
}

impl PartialEq for Light {
    func eq(&self, other: &Light) -> bool {
        return self.wait_secs() == other.wait_secs()
    }
}

func total_ms<S: Signal>(s: S) -> i32 {
    return s.wait_ms()
}

func describe(s: &dyn Signal) -> i32 {
    return s.wait_secs()
}

func main() -> i32 {
    mut light = Light::start()
    mut cycle = 0
    for step in 0..3 {
        println("step {step}: {light.name()} for {describe(&light)}s")
        cycle += total_ms(light)
        light.advance()
    }

    val red = Light::Red
    if light == red {
        println("back to red after a {cycle} ms cycle")
    }
    return light.wait_secs()
}
