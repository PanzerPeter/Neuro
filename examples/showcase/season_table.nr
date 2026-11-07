// Showcase: a league table built from operator traits on non-`Copy` values. Integration
// example: `+` moves whole records together, `+=` writes through a `Vec` element, a field
// and a `&mut` parameter, `Comparable` supplies `<=` and `>` from one `lt` and meets the
// bound of a generic `best`, and closures, `Option` and `for` over a borrow carry the rest.

struct Record {
    results: Vec<i32>,
    bonus: i32
}

impl Record {
    func new() -> Record {
        Record { results: Vec::new(), bonus: 0 }
    }

    func points(&self) -> i32 {
        mut sum = self.bonus
        for r in &self.results {
            sum += *r
        }
        sum
    }
}

impl AddAssign<i32> for Record {
    func add_assign(&mut self, rhs: i32) {
        self.results.push(rhs)
    }
}

impl SubAssign<i32> for Record {
    func sub_assign(&mut self, rhs: i32) {
        self.bonus = self.bonus - rhs
    }
}

// Two half-seasons join into one record, which is why both operands move.
impl Add for Record {
    type Output = Record
    func add(self, rhs: Record) -> Record {
        mut results = self.results
        for r in rhs.results {
            results.push(r)
        }
        Record { results: results, bonus: self.bonus + rhs.bonus }
    }
}

impl PartialEq for Record {
    func eq(&self, rhs: &Record) -> bool {
        self.points() == rhs.points()
    }
}

impl Comparable for Record {
    func lt(&self, rhs: &Record) -> bool {
        self.points() < rhs.points()
    }
}

struct Team {
    name: string,
    record: Record
}

// The generic body sees only the bound; each instance finds its own `<`.
func best<T: Comparable>(a: &T, b: &T) -> bool {
    b < a
}

func award(record: &mut Record, points: i32) {
    *record += points
}

func half(points: Vec<i32>) -> Record {
    mut record = Record::new()
    for p in points {
        record += p
    }
    record
}

func main() -> i32 {
    mut first: Vec<i32> = Vec::new()
    first.push(3)
    first.push(1)
    mut second: Vec<i32> = Vec::new()
    second.push(3)
    val season = half(first) + half(second)
    println("season {season.points()}")

    mut rivals = Team { name: "Rivals", record: Record::new() }
    rivals.record += 3
    rivals.record += 3
    rivals.record -= 2
    award(&mut rivals.record, 1)
    println("{rivals.name} {rivals.record.points()}")

    mut table: Vec<Record> = Vec::new()
    table.push(season)
    table.push(rivals.record)
    table[1] += 1
    table[0] -= 1

    mut draws = 0
    val tally = |a: &Record, b: &Record| -> i32 {
        if a == b { 1 } else { 0 }
    }
    draws += tally(&table[0], &table[1])

    mut leader: Option<i32> = Option::None
    if table[0] > table[1] {
        leader = Option::Some(0)
    } else if table[0] <= table[1] {
        leader = Option::Some(1)
    }
    val index = leader ?? 9
    println("leader {index}, draws {draws}")

    mut score = table[0].points() * 10 + table[1].points()
    if best(&table[0], &table[1]) {
        score += 100
    }
    if best(&2, &1) {
        score += 50
    }
    score
}
