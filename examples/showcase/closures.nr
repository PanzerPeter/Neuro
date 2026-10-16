// Showcase: closures and higher-order functions.
//
// Integration example combining closures with several prior features:
//   · a higher-order function taking a `(i32) -> i32` closure parameter
//   · Copy-by-value capture of an enclosing variable
//   · a `move` closure with a block body and an explicit return type
//   · fixed-size arrays and indexed iteration
//   · a struct method, working alongside closures
//   · capture modes: a closure that writes a local borrows it `&mut` and is passed to a
//     `mut (..)` parameter; one that reads a heap `string` or pushes to a `Vec` borrows
//     it; a `move` closure that gives its `string` away is `once`
//
// The results are composed into a single deterministic exit code.

// Apply `f` to each element of a 4-element array and sum the results.
func map_sum(xs: [i32; 4], f: (i32) -> i32) -> i32 {
    mut total: i32 = 0
    mut i: i32 = 0
    while i < 4 {
        total += f(xs[i])
        i += 1
    }
    return total
}

// Call `f` on each element. The parameter is `mut`, so `f` may write what it captured.
func each(xs: [i32; 4], f: mut (i32) -> void) {
    for i in 0..4 {
        f(xs[i])
    }
}

// Run `report` exactly once: a `once` closure may give away what it captured.
func finish(report: once () -> u64) -> u64 {
    report()
}

func shout(message: string) -> u64 {
    message.len()
}

struct Scaler {
    factor: i32
}

impl Scaler {
    func apply(&self, x: i32) -> i32 {
        x * self.factor
    }
}

func main() -> i32 {
    val data: [i32; 4] = [1, 2, 3, 4]

    // A closure capturing a Copy local (`bias`) by value.
    val bias = 10
    val biased = map_sum(data, |x: i32| x + bias)   // 11+12+13+14 = 50

    // A `move` closure with a block body and early return.
    val scale = 3
    val scaled = map_sum(data, move |x: i32| -> i32 {
        val y = x * scale
        return y
    })                                              // 3+6+9+12 = 30

    // A struct method still resolves alongside closures.
    val s = Scaler { factor: 2 }
    val doubled = s.apply(5)                         // 10

    // Writing `count` borrows it `&mut`, so the closure is `mut` and the increments land
    // in `count` itself. The block ends the borrow before `count` is read again.
    mut count = 0
    mut seen: Vec<i32> = Vec::new()
    {
        val tally = |x: i32| -> void { count += x }
        each(data, tally)                            // 1+2+3+4 = 10
        val record = |x: i32| -> void { seen.push(x * x) }
        each(data, record)                           // [1, 4, 9, 16]
    }

    // Reading a heap `string` borrows it: `name` is still usable afterwards.
    val name = "ne" + "uro"
    val measured = map_sum(data, |x: i32| -> i32 { (name.len() as i32) + x })   // 6+7+8+9 = 30

    // A `move` closure owns `banner`; handing it to `shout` makes the closure `once`.
    val banner = "closures" + "!"
    val announced = finish(move || -> u64 { shout(banner) })                   // 9

    println("capture by value  |x| x + bias      = {biased}")
    println("move closure      move |x| x * scale = {scaled}")
    println("struct method     s.apply(5)         = {doubled}")
    println("mut capture       count += x         = {count}")
    println("vec push capture  seen.push(x * x)   = {seen.len()} items, last {seen[3]}")
    println("borrowed string   {name}.len() + x     = {measured}")
    println("once closure      shout(banner)      = {announced}")

    val captured = count + (seen.len() as i32) + measured + (announced as i32)
    val total = biased + scaled + doubled + captured
    println("total                                = {total}")
    total                                            // 90 + 10 + 4 + 30 + 9 = 143
}
