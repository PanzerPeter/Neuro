// Showcase — pattern matching working alongside enums, structs, methods, arrays,
// and loops.
//
// Integration example combining several Phase-1E features:
//   pattern matching (enum deconstruction + payload binding, value/or/range
//   patterns, guards, wildcard) · enums with associated data (all three
//   variant forms) · structs with an enum field · impl methods (&self) ·
//   fixed-size arrays + for-in iteration · compound assignment.
//
// Enums are constructed and then deconstructed with `match`; the numeric result is
// composed from the struct, array, and matched enum data.

enum Status { Active, Idle, Done }

struct Worker {
    status: Status,
    work: i32
}

impl Worker {
    func new(s: Status, w: i32) -> Worker {
        Worker { status: s, work: w }
    }

    func doubled(&self) -> i32 {
        self.work * 2
    }
}

// All three variant shapes in one enum.
enum Signal {
    Stop,
    Go(i32),
    Range { lo: i32, hi: i32 }
}

// Deconstruct each variant form: unit, tuple (binds `n`), struct (binds `lo`/`hi`).
func signal_value(s: Signal) -> i32 {
    match s {
        Signal::Stop => 0,
        Signal::Go(n) => n,
        Signal::Range { lo, hi } => hi - lo
    }
}

// Value patterns over an integer: literal, or-pattern, range, guard, wildcard.
func bucket(n: i32) -> i32 {
    match n {
        0 => 0,
        1 | 2 => 1,
        3..=9 => 2,
        n if n < 0 => 9,
        _ => 3
    }
}

func status_bonus(st: Status) -> i32 {
    match st {
        Status::Active => 1,
        Status::Idle => 2,
        Status::Done => 3
    }
}

func main() -> i32 {
    val w1 = Worker::new(Status::Active, 5)
    val w2 = Worker { status: Status::Idle, work: 8 }

    val a = Signal::Stop
    val b = Signal::Go(3)
    val c = Signal::Range { lo: 1, hi: 2 }

    // Array + for-in loop.
    val nums: [i32; 4] = [1, 2, 3, 4]
    mut sum: i32 = 0
    for n in nums {
        sum += n            // 1 + 2 + 3 + 4 = 10
    }

    println("array for-in sum        = {sum}")

    sum += w1.doubled()     // + (5 * 2) = 20
    sum += w2.work          // + 8       = 28
    println("+ w1.doubled() + w2.work = {sum}")

    // Deconstruct the constructed signals.
    val sa = signal_value(a)
    val sb = signal_value(b)
    val sc = signal_value(c)
    println("Signal::Stop            -> {sa}")
    println("Signal::Go(3)           -> {sb}")
    println("Signal::Range 1..2      -> {sc}")
    sum += sa               // + 0 = 28
    sum += sb               // + 3 = 31
    sum += sc               // + 1 = 32

    // Value-pattern buckets.
    val b7 = bucket(7)
    val bneg = bucket(-4)
    println("bucket(7)   range arm   -> {b7}")
    println("bucket(-4)  guard arm   -> {bneg}")
    sum += b7               // + 2 = 34
    sum += bneg             // + 9 = 43

    // Match on the enum stored in a struct field.
    val bonus1 = status_bonus(w1.status)
    val bonus2 = status_bonus(w2.status)
    println("Status::Active in field -> {bonus1}")
    println("Status::Idle   in field -> {bonus2}")
    sum += bonus1                   // + 1 = 44
    sum += bonus2                   // + 2 = 46

    println("total                   = {sum}")
    return sum              // 46
}
