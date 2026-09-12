// Showcase: range `.rev()` driving a replay buffer, over earlier features.
//
// Cumulative integration example combining:
//   range `.rev()` in a `for` head and on a tensor index axis
//   · static tensor types and tensor slicing · tensor reductions (`.sum()`)
//   · structs with `&self` methods · `@derive(Copy, Clone)`
//   · `.enumerate()` paired with `.rev()` · a `.filter(p)` head adapter
//   · string interpolation · labeled `break`
//
// The point of the combination: a replay buffer is written oldest-first and read
// newest-first, and `.rev()` is the only thing that changes between the two. The
// reversed index produces a FRESH tensor, so the buffer itself keeps its own
// order and can still be reduced, sliced, and re-read afterwards.
//
// Expected exit code: 79

@derive(Copy, Clone)
struct Frame {
    reward: i32,
    step: i32
}

impl Frame {
    // Later frames count for more: the discount is undone by how recent it is.
    func weighted(&self) -> i32 {
        self.reward * (self.step + 1)
    }
}

func main() -> i32 {
    // Written in the order the episode happened.
    val episode: Tensor<i32, [6]> = [2, 4, 6, 8, 10, 12]

    val total = episode.sum()
    println("episode            = {episode[0]} .. {episode[5]}  (sum {total})")

    // Reading it back newest-first is one `.rev()` on the axis, and it copies:
    // `episode` is untouched, which the second reduction below proves.
    val replay: Tensor<i32, [6]> = episode[(0..6).rev()]
    println("replay[0]          = {replay[0]}   (episode[0] still {episode[0]})")

    // The tail three frames, themselves newest-first.
    val tail: Tensor<i32, [3]> = episode[(3..6).rev()]
    println("tail newest-first  = {tail[0]} {tail[1]} {tail[2]}")

    // Walking the buffer backwards while the position climbs: the position is a
    // recency rank, the value is the frame index it names.
    mut weighted = 0
    for (rank, index) in (0..6).rev().enumerate() {
        val frame = Frame { reward: replay[rank as i32], step: index }
        weighted = weighted + frame.weighted()
    }
    println("weighted replay    = {weighted}")

    // Adapters ride above the reversal: descending frames, odd ones dropped.
    mut kept = ""
    for v in (0..6).rev().filter(|n: i32| -> bool { n % 2 == 0 }) {
        kept = kept + "{v} "
    }
    println("even frames        = {kept}")

    // A descending scan that stops early. `break` leaves a reversed loop the way
    // it leaves any other.
    mut first_small = -1
    scan: for i in (0..6).rev() {
        if replay[i] < 6 {
            first_small = i
            break scan
        }
    }
    println("first below six    = index {first_small}")

    val answer = total + first_small + (weighted % 50)
    println("answer             = {answer}")
    answer
}
