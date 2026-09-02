// Showcase — associated types: one trait, three implementors, three sample types.
//
// Cumulative integration example combining:
//   associated types (`type Sample`) and `Self::Sample` paths
//   · `Channel<Sample = i32>` bounds constraining that member on a type parameter
//   · trait declarations with a default method · @derive(Copy) structs with `&self` methods
//   · `Option<T>` + `match` · `Vec<T>` + `for`-in · string interpolation and the
//     format mini-language
//
// The point of the combination: a `Channel` trait without an associated type would have
// to pick ONE sample type for every instrument, so the thermocouple's `f64`, the tally's
// `i32`, and the interlock's `bool` could not share a trait at all. The trait names the
// member; each impl says what it is; `Self::Sample` reads back whatever that impl chose,
// including nested inside `Option<...>` where the drain loop needs it.
//
// A generic reader needs the other half. `T: Channel` erases the instrument, and with it
// the only thing that says what `Sample` is, so `T: Channel<Sample = i32>` puts the
// answer in the bound — one body serving every counted channel and no float channel.
//
// Expected exit code: 108

trait Channel {
    type Sample

    // Every implementor supplies the reading itself...
    func sample(&self) -> Self::Sample

    // ...and inherits this, unless it has a better answer.
    func channel_id(&self) -> i32 { 0 }
}

@derive(Copy, Clone)
struct Thermocouple {
    millivolts: f64
}

@derive(Copy, Clone)
struct Tally {
    counted: i32
}

@derive(Copy, Clone)
struct Interlock {
    latched: bool
}

impl Channel for Thermocouple {
    type Sample = f64

    // 40 microvolts per degree, the usual type-K approximation.
    func sample(&self) -> Self::Sample { self.millivolts * 25.0 }

    func channel_id(&self) -> i32 { 1 }
}

impl Channel for Tally {
    type Sample = i32

    func sample(&self) -> Self::Sample { self.counted }
}

impl Channel for Interlock {
    type Sample = bool

    func sample(&self) -> Self::Sample { self.latched }

    func channel_id(&self) -> i32 { 3 }
}

@derive(Copy, Clone)
struct Cycle {
    ticks: i32
}

impl Channel for Cycle {
    type Sample = i32

    func sample(&self) -> Self::Sample { self.ticks * 2 }

    func channel_id(&self) -> i32 { 4 }
}

// One body, every counted channel. The bound is what makes `sample()` typeable here:
// the trait declaration alone says only that `Sample` exists. Passing the thermocouple
// would be rejected — its impl binds `Sample` to f64.
func scaled<T: Channel<Sample = i32>>(source: &T, factor: i32) -> i32 {
    source.sample() * factor
}

// The same bound spelled anonymously, for a reader that names its parameter once.
func labelled(source: &impl Channel<Sample = i32>) -> i32 {
    source.sample() + source.channel_id()
}

// A drain whose element type is its own associated type: the loop below never names
// `i32`, only `Self::Sample`.
struct Backlog {
    pending: i32
}

impl Channel for Backlog {
    type Sample = i32

    func sample(&self) -> Self::Sample { self.pending }
}

impl Backlog {
    func take(&mut self) -> Option<i32> {
        if self.pending <= 0 {
            return None
        }
        self.pending = self.pending - 1
        Some(self.pending + 1)
    }
}

func main() -> i32 {
    val probe = Thermocouple { millivolts: 2.4 }
    val counter = Tally { counted: 17 }
    val guard = Interlock { latched: true }

    val degrees = probe.sample()
    val counted = counter.sample()
    val latched = guard.sample()

    println("channel {probe.channel_id()} thermocouple -> {degrees:.1} C")
    println("channel {counter.channel_id()} tally       -> {counted:>4}")
    println("channel {guard.channel_id()} interlock   -> {latched}")

    // The drain: `take()` hands back `Option<Self::Sample>`, unwrapped by `match`
    // into a `Vec` of what the impl bound the associated type to.
    mut backlog = Backlog { pending: 4 }
    mut drained: Vec<i32> = Vec::new()
    mut draining = true
    while draining {
        val next = backlog.take()
        match next {
            Some(v) => { drained.push(v) }
            None => { draining = false }
        }
    }

    mut sum: i32 = 0
    for item in drained {
        println("drained {item}")
        sum = sum + item
    }

    // The constrained bound: two implementors, one monomorphized body each.
    val ticker = Cycle { ticks: 3 }
    val scaled_tally = scaled(&counter, 1)   // 17
    val scaled_cycle = scaled(&ticker, 1)    //  6
    val marked = labelled(&ticker)           //  6 + 4 = 10
    println("scaled tally {scaled_tally}, scaled cycle {scaled_cycle}, labelled {marked}")

    val holdover = backlog.sample()
    val temperature = degrees as i32
    val bonus = if latched { 5 } else { 0 }

    println("sum {sum}, holdover {holdover}, temperature {temperature}, bonus {bonus}")

    val total = sum + holdover + temperature + counted + bonus + scaled_cycle + marked
    println("total = {total}")
    total
}
