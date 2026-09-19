// Showcase: a heat grid updated in place, written the way the arithmetic reads.
//
// This is an integration example: it exercises several features *together*
// rather than in isolation:
//   2E's place expressions, which are what let every update below name the
//   storage it writes to · structs with `&mut self` methods, whose bodies are
//   now written `self.field += ...` instead of restating the field · arrays and
//   nested arrays · 2B's static tensors, indexed coordinate by coordinate ·
//   1G's `Vec` · `for` over ranges · and 1H's string interpolation with the
//   format mini-language, which prints the result.
//
// Every statement that mutates here writes THROUGH a place. Before the place
// expression existed, only a bare binding could sit on the left of an operator
// assignment, so `self.total += n` had to be spelled `self.total = self.total + n`
// and `t[r, c] = v` had no spelling at all. The difference is not only how it
// reads: a compound assignment on a tensor updates the buffer the DLPack handle
// already addresses, and the restated form would allocate a second one.

// A running tally kept behind `&mut self` methods. Both bodies are compound
// assignments to a field, which is the shape BUG-025 named.
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
struct Grid {
    cells: [[i32; 4]; 3],
    tally: Tally
}

impl Grid {
    func raise(&mut self, row: i32, column: i32, delta: i32) {
        self.cells[row][column] += delta
    }

    func row_total(&self, row: i32) -> i32 {
        mut sum = 0
        for column in 0..4 {
            sum += self.cells[row][column]
        }
        return sum
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
        tally: Tally { total: 0, peak: 0 }
    }

    // A field of a field: the tally lives inside the grid and is raised through it.
    grid.tally.record(5)
    grid.tally.record(9)
    grid.tally.record(4)
    grid.tally.scale(2)

    grid.raise(0, 3, 10)
    grid.raise(2, 0, 7)

    println("grid rows")
    mut totals: Vec<i32> = Vec::new()
    for row in 0..3 {
        val total = grid.row_total(row)
        totals.push(total)
        println("  row {row} total {total}")
    }

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
