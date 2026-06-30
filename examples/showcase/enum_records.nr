// Showcase — enums working alongside structs, methods, arrays, and loops.
//
// Integration example combining several Phase-1E features:
//   enums with associated data (§3.5, all three variant forms)
//   · structs with an enum field · impl methods (&self)
//   · fixed-size arrays + for-in iteration (§3.1) · compound assignment.
//
// Enums are constructed (unit, tuple, and struct variants) and one is stored in
// a struct field; the numeric result is composed from the struct and array data.

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

// All three §3.5 variant shapes in one enum.
enum Signal {
    Stop,
    Go(i32),
    Range { lo: i32, hi: i32 }
}

func main() -> i32 {
    val w1 = Worker::new(Status::Active, 5)
    val w2 = Worker { status: Status::Idle, work: 8 }

    // Construct each enum form; they ride alongside the numeric computation.
    val a = Signal::Stop
    val b = Signal::Go(3)
    val c = Signal::Range { lo: 1, hi: 2 }

    // Array + for-in loop.
    val nums: [i32; 4] = [1, 2, 3, 4]
    mut sum: i32 = 0
    for n in nums {
        sum += n            // 1 + 2 + 3 + 4 = 10
    }

    sum += w1.doubled()     // + (5 * 2) = 20
    sum += w2.work          // + 8       = 28
    return sum              // 28
}
