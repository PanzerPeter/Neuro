// Showcase: `.enumerate()` carrying a position through earlier features. Integration
// example: `.enumerate()` on arrays, `Vec<T>` and a range, structs with `&self` methods,
// `@derive(Copy)`, and `match` on the position. The position is a `u64`, the type an index
// expression takes, so a rank can read back into the array that produced it, such as
// comparing each runner with the next. Over a range the position still counts from zero.

@derive(Copy, Clone)
struct Runner {
    bib: i32,
    seconds: i32
}

impl Runner {
    func half_pace(&self) -> i32 {
        self.seconds / 2
    }
}

// Points awarded for a finishing position.
func medal(place: i32) -> i32 {
    match place {
        0 => 6,
        1 => 4,
        2 => 2,
        _ => 1
    }
}

// Count the runners who finished ahead of somebody faster. The comparison needs
// the NEXT element, so it needs the position, not just the element.
func out_of_order(runners: [Runner; 4]) -> i32 {
    mut inversions: i32 = 0
    for (rank, runner) in runners.enumerate() {
        if rank + 1 < runners.len() {
            if runner.seconds > runners[rank + 1].seconds {
                inversions = inversions + 1
            }
        }
    }
    inversions
}

func main() -> i32 {
    val runners: [Runner; 4] = [
        Runner { bib: 11, seconds: 60 },
        Runner { bib: 22, seconds: 74 },
        Runner { bib: 33, seconds: 71 },
        Runner { bib: 44, seconds: 90 }
    ]

    mut points: i32 = 0
    mut splits: Vec<i32> = Vec::new()

    for (rank, runner) in runners.enumerate() {
        val place = rank as i32
        val award = medal(place)
        val pace = runner.half_pace()
        println("rank {place}: bib {runner.bib}, half-pace {pace}, award {award}")
        points = points + award
        splits.push(pace - place)
    }

    // The same head over the growable container the loop just filled.
    mut adjusted: i32 = 0
    for (lane, split) in splits.enumerate() {
        println("lane {lane} split {split}")
        adjusted = adjusted + split
    }

    // Over a range, the position is a count from zero: NOT the value yielded.
    mut laps: i32 = 0
    for (step, lap) in (7..10).enumerate() {
        println("step {step} is lap {lap}")
        laps = laps + lap - (step as i32)
    }

    val inversions = out_of_order(runners)
    println("points {points}, adjusted {adjusted}, laps {laps}, out of order {inversions}")

    val total = points + adjusted + laps + inversions
    println("total = {total}")
    total
}
