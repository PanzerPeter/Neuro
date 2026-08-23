@no_prelude

// Module `stats`: turns a batch of raw readings into a summary value.
//
// Nothing here is fallible — a batch always has a count, a total, and a peak — so this
// file opts out of the implicit prelude entirely. `Some` and `None` are ordinary names
// again inside it, while `main.nr` and `report/mod.nr` beside it still get them bound.
//
// Visibility is per declaration and, for a struct, per field: `total` is the running
// sum this module keeps for itself, readable only through `mean()`.

export struct Summary {
    export count: i32,
    total: i32,
    export peak: i32
}

impl Summary {
    func new(count: i32, total: i32, peak: i32) -> Summary {
        Summary { count: count, total: total, peak: peak }
    }

    func mean(&self) -> i32 {
        if self.count == 0 {
            0
        } else {
            self.total / self.count
        }
    }
}

// A generic helper reused below at two different type arguments.
export func pick<T>(condition: bool, when_true: T, when_false: T) -> T {
    if condition { when_true } else { when_false }
}

// The Vec is built, read, and dropped inside this function: it owns its buffer and
// frees it at scope exit, so nothing about the heap escapes into the summary.
export func summarize(samples: [i32; 5]) -> Summary {
    mut buffer: Vec<i32> = Vec::new()
    for sample in samples {
        buffer.push(sample)
    }

    mut total: i32 = 0
    mut peak: i32 = 0
    for value in buffer {
        total += value
        peak = pick(value > peak, value, peak)
    }

    Summary::new(buffer.len() as i32, total, peak)
}
