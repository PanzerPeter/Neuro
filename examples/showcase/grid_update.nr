// Showcase: a heat grid updated in place, with each mutation naming the storage it writes
// to. Integration example: place expressions (a field, a field of a field, a tuple element
// reached through a field, a `&mut` parameter or a closure capture, a nested array element,
// a `Vec` element and a tensor coordinate), `&mut self` methods, `for` over ranges, and
// string interpolation. `self.total += n` and `t[r, c] = v` need no restated form. On a
// tensor, a compound assignment updates the buffer the DLPack handle already addresses
// instead of allocating a second one.

// A running tally kept behind `&mut self` methods. Both bodies are compound
// assignments to a field.

struct Tally {
    total: i32,
    peak: i32
}

impl Tally {
    func record(&mut self, sample: i32) {
        self.total += sample
        if sample > self.peak {
            self.peak = sample
        }
    }

    func scale(&mut self, factor: i32) {
        self.total *= factor
    }
}

// A grid of readings plus the tally over them. `cells` is reached as
// `self.cells[r]`: a field of `self`, then an element of that field, which is a
// two-step place.
// `bounds` is the lowest and highest cell, a tuple whose elements are written
// one at a time as places, the way a struct's fields are.
struct Grid {
    cells: [[i32; 4]; 3],
    tally: Tally,
    bounds: (i32, i32)
}

impl Grid {
    func raise(&mut self, row: i32, column: i32, delta: i32) {
        self.cells[row][column] += delta
        if self.cells[row][column] > self.bounds.1 {
            self.bounds.1 = self.cells[row][column]
        }
    }

    func row_total(&self, row: i32) -> i32 {
        mut sum = 0
        for column in 0..4 {
            sum += self.cells[row][column]
        }
        return sum
    }
}

// A tuple element written through a `&mut` borrow: the caller's tuple changes.
func widen(span: &mut (i32, i32), sample: i32) {
    if sample < span.0 {
        span.0 = sample
    }
    if sample > span.1 {
        span.1 = sample
    }
}

// Fill a tensor coordinate by coordinate. `t[r, c] = v` is the tensor place: it
// stores into the buffer the handle already owns rather than building a new one.
func ramp() -> Tensor<i32, [3, 4]> {
    mut t: Tensor<i32, [3, 4]> = [
        [0, 0, 0, 0],
        [0, 0, 0, 0],
        [0, 0, 0, 0]
    ]
    for row in 0..3 {
        for column in 0..4 {
            t[row, column] = row * 4 + column
        }
    }
    return t
}

func main() -> i32 {
    mut grid = Grid {
        cells: [
            [1, 1, 1, 1],
            [2, 2, 2, 2],
            [3, 3, 3, 3]
        ],
        tally: Tally { total: 0, peak: 0 },
        bounds: (1, 3)
    }

    // A field of a field: the tally lives inside the grid and is raised through it.
    grid.tally.record(5)
    grid.tally.record(9)
    grid.tally.record(4)
    grid.tally.scale(2)

    // A closure that counts the raises in a tuple it captures by `&mut`; the block
    // ends that borrow before `steps` is read.
    mut steps = (0, 0)
    {
        mut raise = |row: i32, column: i32, delta: i32| {
            grid.raise(row, column, delta)
            steps.0 += 1
            steps.1 += delta
        }
        raise(0, 3, 10)
        raise(2, 0, 7)
    }
    println("{steps.0} raises adding {steps.1}")
    println("grid bounds {grid.bounds.0}..{grid.bounds.1}")

    println("grid rows")
    mut totals: Vec<i32> = Vec::new()
    mut seen = (100, 0)
    for row in 0..3 {
        val total = grid.row_total(row)
        totals.push(total)
        widen(&mut seen, total)
        println("  row {row} total {total}")
    }
    println("  totals span {seen.0}..{seen.1}")

    // An element of a `Vec` is a place too, so the correction is applied in place.
    totals[1] += 100
    println("  row 1 corrected to {totals[1]}")

    mut heat = ramp()
    heat[1, 1] += 100
    heat[2, 3] = 99

    println("heat corners")
    println("  [0, 0] = {heat[0, 0]}")
    println("  [1, 1] = {heat[1, 1]}")
    println("  [2, 3] = {heat[2, 3]}")

    println("tally total {grid.tally.total} peak {grid.tally.peak}")

    // 14 + 8 + 19 = 41 across the rows, and 9 is the highest sample the tally saw.
    return grid.row_total(0) + grid.row_total(1) + grid.row_total(2) + grid.tally.peak
}
