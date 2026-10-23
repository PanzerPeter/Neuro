// Showcase: values that hold borrows, working together with earlier features.
//
// Cumulative integration example combining:
//   a struct and an enum declaring a lifetime parameter, each holding `&'a string`
//   · `Option<&'a string>` returned from a search, read with `match`, `??` and `.map`
//   · a struct holding a `&mut i32`, written through by a `&mut self` method
//   · `@derive(Copy, Clone)` on a struct with a reference field · fixed arrays
//   · `for` over a range · a borrowed `.slice` view · string interpolation
//
// No name is copied: every `Entry` points into one of the three strings `main` owns,
// and each value carrying such a view holds its borrow for as long as it lives, so
// none of those strings can be moved or replaced while a record still reads it.

@derive(Copy, Clone)
struct Entry<'a> {
    name: &'a string,
    score: i32
}

impl<'a> Entry<'a> {
    // The view outlives this call: it borrows what the entry borrows, not the entry.
    func initial(&self) -> &'a string {
        self.name.slice(0..1)
    }
}

enum Verdict<'a> {
    Leader(&'a string),
    Tie(i32)
}

// A running total kept through a borrow held in a field.
struct Tally<'a> {
    seen: &'a mut i32
}

impl<'a> Tally<'a> {
    func count(&mut self, entry: Entry) {
        *self.seen = *self.seen + entry.score
    }
}

// The best scorer's name, or nothing on a tie for first place.
func top<'a>(entries: [Entry<'a>; 3]) -> Option<&'a string> {
    mut best = entries[0]
    mut tied = false
    for i in 1..3 {
        val e = entries[i]
        if e.score > best.score {
            best = e
            tied = false
        } else if e.score == best.score {
            tied = true
        }
    }
    if tied { None } else { Some(best.name) }
}

func judge<'a>(entries: [Entry<'a>; 3]) -> Verdict<'a> {
    match top(entries) {
        Some(name) => Verdict::Leader(name),
        None => Verdict::Tie(entries[0].score)
    }
}

func describe(v: Verdict) -> i32 {
    match v {
        Verdict::Leader(name) => {
            println("verdict: {name} leads")
            name.len() as i32
        },
        Verdict::Tie(score) => {
            println("verdict: tie at {score}")
            score
        }
    }
}

func main() -> i32 {
    val ada = "Ada"
    val brian = "Brian"
    val cleo = "Cleo"

    val round_one = [
        Entry { name: &ada, score: 7 },
        Entry { name: &brian, score: 9 },
        Entry { name: &cleo, score: 4 }
    ]
    val round_two = [
        Entry { name: &ada, score: 5 },
        Entry { name: &brian, score: 5 },
        Entry { name: &cleo, score: 2 }
    ]

    // The tally's `&mut total` ends with this block, so `total` is readable after it.
    mut total = 0
    {
        mut tally = Tally { seen: &mut total }
        for i in 0..3 {
            tally.count(round_one[i])
            tally.count(round_two[i])
        }
    }
    println("points scored: {total}")

    val first = top(round_one)                     // Some(&brian)
    val second = top(round_two)                    // None: Ada and Brian tie
    val lead = first ?? &ada
    val second_len = second.map(|n| n.len()) ?? 0u64
    println("round one leader: {lead}")
    println("round two leader name length: {second_len}")

    val init = round_one[1].initial()
    println("leader initial: {init}")

    // 5 ("Brian") from the first verdict, 5 (the tied score) from the second.
    val verdicts = describe(judge(round_one)) + describe(judge(round_two))

    // 32 points + 5 + 0 + 1 + 10 = 48
    total + lead.len() as i32 + second_len as i32 + init.len() as i32 + verdicts
}
