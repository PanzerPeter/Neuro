// Showcase — `val-else` early exit carrying a small job queue, combined with
// earlier features.
//
// Cumulative integration example combining:
//   `val-else` binding a pattern for the rest of the block, with the three
//     type-directed `else` forms: `|reason|` on a Result, `|verdict|` on a plain
//     enum, and a bare `else { break }` on an Option
//   · `Option<T>` / `Result<T, E>` from the implicit prelude
//   · `??` supplying a default for an absent lookup
//   · a `Vec<i32>` collection: `push`, `pop`, `for`-in, scope-exit free
//   · `@derive(Copy)` structs with an associated function and `&self` methods
//   · an enum with tuple and unit variants · `match` with a guard
//   · a fixed-size array `[i32; 5]` with indexed reads · range-`for` · `loop`
//
// The point of the combination: `val-else` is the shape that replaces a `match`
// whose success arm would otherwise swallow the entire rest of the function. Here
// every stage validates, exits on failure, and keeps writing straight-line code.

@derive(Copy, Clone)
struct Job {
    id: i32,
    cost: i32
}

impl Job {
    func new(id: i32, cost: i32) -> Job {
        Job { id: id, cost: cost }
    }

    func weight(&self) -> i32 {
        self.id * self.cost
    }
}

enum Verdict {
    Accept(i32),
    Defer(i32),
    Drop
}

// A cost is valid when it is positive and within budget; the `Err` payload is the
// reason code the caller forwards.
func priced(cost: i32) -> Result<i32, i32> {
    if cost <= 0 {
        return Result::Err(7)
    }
    if cost > 50 {
        return Result::Err(3)
    }
    Result::Ok(cost)
}

// Reading past the end of the queue is absence, not failure.
func slot_cost(costs: [i32; 5], index: i32) -> Option<i32> {
    if index < 5 {
        Option::Some(costs[index])
    } else {
        Option::None
    }
}

func classify(weight: i32) -> Verdict {
    match weight {
        0 => Verdict::Drop,
        w if w > 60 => Verdict::Defer(w),
        _ => Verdict::Accept(weight)
    }
}

// `else |reason|` names the `Err` payload — the reason code, returned negated so an
// invalid job is visible in the total without a second channel.
func score(id: i32, cost: i32) -> i32 {
    val Result::Ok(valid) = priced(cost) else |reason| { return 0 - reason }
    val job = Job::new(id, valid)
    job.weight()
}

// `Verdict` is neither Option nor Result, so `else |verdict|` binds the scrutinee
// itself and the branch discriminates further with a nested `match`.
func settle(weight: i32) -> i32 {
    val Verdict::Accept(points) = classify(weight) else |verdict| {
        match verdict {
            Verdict::Defer(deferred) => { return deferred / 2 },
            _ => { return 0 }
        }
    }
    points
}

func main() -> i32 {
    val costs: [i32; 5] = [4, 12, 0, 60, 9]

    mut accepted: Vec<i32> = Vec::new()
    mut total: i32 = 0

    for i in 0..5 {
        // An absent slot falls back to 0, which `priced` then rejects — the two
        // fallible styles (`??` for "I don't care why", `val-else` for "forward it")
        // side by side.
        val cost = slot_cost(costs, i) ?? 0
        val points = score(i + 1, cost)
        if points > 0 {
            accepted.push(points)
        }
        total = total + points
    }
    // scores: 1*4=4, 2*12=24, cost 0 -> -7, cost 60 -> -3, 5*9=45  => total 63

    mut settled: i32 = 0
    for points in accepted {
        settled = settled + settle(points)
    }
    // 4 -> Accept 4, 24 -> Accept 24, 45 -> Accept 45  => settled 73

    // `Vec::pop` yields `Option<i32>`, so the drain loop is a `val-else` whose
    // failure branch simply leaves the loop.
    mut drained: i32 = 0
    loop {
        val Option::Some(top) = accepted.pop() else { break }
        drained = drained + 1
    }
    // three entries were pushed => drained 3

    total + settled + drained  // 63 + 73 + 3 = 139
}
