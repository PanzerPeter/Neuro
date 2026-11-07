// Showcase: closures and higher-order functions. Integration example: `(i32) -> i32`
// parameters, copy-by-value capture, `move` closures, fixed arrays, a struct method, and
// type inference from the generic function a closure is passed to. Capture follows use: a
// closure that writes a local borrows it `&mut` and goes to a `mut (..)` parameter; one
// that reads a `string` or pushes to a `Vec` borrows it; a `move` closure that gives its
// `string` away is `once`. Closures are also returned, stored in struct fields and lent to
// `mut` parameters. Without `move`, a closure still takes by value whatever its body gives
// away, one capture at a time. A closure that borrows is a value too: it sits in an array or
// a struct, is assigned, and comes back out of `compose`, while what it borrows stays frozen.

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

// Fold an array into one value. `T` and `U` come from the call, and the closure literal
// handed to `f` takes its parameter types from them, so it needs no annotations.
func fold<T: Copy, U>(xs: [T; 4], init: U, f: (U, T) -> U) -> U {
    mut acc = init
    for i in 0..4 {
        acc = f(acc, xs[i])
    }
    acc
}

// A transform chosen at run time and kept in a struct field. The closure is built here and
// returned inside the struct, so it outlives the call that made it.
struct Transform {
    label: string,
    apply: (i32) -> i32
}

func pick_transform(offset: i32, squared: bool) -> Transform {
    if squared {
        Transform { label: "square", apply: |x| x * x + offset }
    } else {
        Transform { label: "shift", apply: |x| x + offset }
    }
}

// A `move` closure owns the `string` it captured, so it may be returned as well.
func length_of(word: string) -> () -> u64 {
    move || word.len()
}

// The body gives its only capture away, so the closure owns it and may be returned with no
// `move`.
func deliver_later(item: string) -> once () -> string {
    || item
}

// A running tally that owns its `string` and writes its own total: a `mut` closure.
func tally_of(unit: string) -> mut (i32) -> u64 {
    mut total: u64 = 0
    move |x: i32| -> u64 {
        total += unit.len() * (x as u64)
        total
    }
}

// Feed every element to `f` and keep its last answer. The call borrows `f` exclusively
// while it runs and hands it back when it returns.
func feed(xs: [i32; 4], f: mut (i32) -> u64) -> u64 {
    mut last: u64 = 0
    for i in 0..4 {
        last = f(xs[i])
    }
    last
}

struct Meter {
    label: string,
    read: mut (i32) -> u64
}

// A labelled step of a pipeline. Its function may borrow, and a `Stage` then holds that borrow.
struct Stage {
    label: string,
    run: (i32) -> i32
}

// Chain two functions. The closure copies both, so the caller's result holds whatever they
// borrow, and it stays valid after this function returns.
func compose(f: (i32) -> i32, g: (i32) -> i32) -> (i32) -> i32 {
    |x| g(f(x))
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

    // No `move`: `title` is given away, so it is taken by value and the closure is `once`,
    // while `unit`, only read, stays borrowed and stays readable.
    val title = "neu" + "ro"
    val unit = "by" + "tes"
    val inferred_move = finish(|| -> u64 { shout(title) + unit.len() })        // 5 + 5 = 10
    val later = deliver_later("deferred" + "!")
    val parcel = later()
    val delivered = parcel.len() as i32                                        // 9

    println("inferred move     shout(title)       = {inferred_move}, {unit} kept")
    println("returned once     {parcel}.len()     = {delivered}")

    val captured = count + (seen.len() as i32) + measured + (announced as i32)
    val total = biased + scaled + doubled + captured
    // Inference: none of these closures is annotated.
    val summed = fold(data, 0, |acc, x| acc + x)                               // 10
    val widest = fold(data, 0i64, |acc, x| if x as i64 > acc { x as i64 } else { acc })   // 4

    // Escaping closures: each was built inside a function that has since returned.
    val chosen = pick_transform(1, false)
    val shift = chosen.apply
    val stored = Option::Some(pick_transform(0, true))
    val square = match stored {
        Option::Some(t) => t.apply,
        Option::None => shift
    }
    val escaped = shift(2) + square(3)                                         // 3 + 9 = 12
    val count_word = length_of("ab" + "cd")
    val owned = count_word() as i32                                            // 4

    println("generic fold      |acc, x| acc + x   = {summed}")
    println("generic fold      widest as i64      = {widest}")
    println("returned closure  {chosen.label}(2) + square(3) = {escaped}")
    println("owned capture     word.len()         = {owned}")

    // Lending and moving: `feed` borrows the tally and hands it back, so the same total keeps
    // growing. The struct field takes it by move: from there on `tally` names nothing.
    val tally = tally_of("abc")
    val fed = feed(data, tally)                                                // 3 * 10 = 30
    val meter = Meter { label: "tally", read: tally }
    val read = meter.read
    val grown = read(10)                                                       // 30 + 30 = 60

    println("lent closure      feed(data, tally)  = {fed}")
    println("moved closure     {meter.label} read(10)     = {grown}")

    // Borrowing closures as values. `prefix` is borrowed by the array's first closure, by
    // `chain` and by `stage`, so it cannot be moved or written while they live.
    val prefix = "neu" + "ral"
    val steps = [|x: i32| x + prefix.len() as i32, |x: i32| x * 2]
    val chain = compose(steps[0], steps[1])
    val stage = Stage { label: "chain", run: chain }
    mut current: (i32) -> i32 = |x| x
    val before = current(1)                                                    // 1
    current = stage.run
    val piped = current(1)                                                     // (1 + 6) * 2 = 14

    println("borrowing values  {stage.label}(1) from {before}      = {piped}")

    val inferred = summed + (widest as i32) + escaped + owned + (inferred_move as i32) + delivered
    val lent = (grown - fed) as i32
    println("total                                = {total + inferred + lent + piped}")
    total + inferred + lent + piped                  // 143 + 10 + 4 + 12 + 4 + 10 + 9 + 30 + 14 = 236
}
