// Showcase: an incident desk whose collections own the tickets they file.
//
// Non-`Copy` collection elements carried by earlier features:
//   a `Vec` of structs that own a `string` and a `Vec` · an enum with an `impl`,
//   matched on its variants · `&mut self` methods called on a slot in place ·
//   a closure passed to a generic higher-order function over a borrowed `Vec` ·
//   `Option::map` over the `Option<&T>` a lookup lends · `??` reading a borrowed
//   count through · a `HashMap` of `Vec`s edited through `get_mut` · a user `Drop`
//   that runs once for every way a slot gives its ticket up.
//
// The queue is never copied: `push` and `insert` move a ticket in, the readers lend
// one, and `remove`, `swap_remove`, a displacing `insert` and a consuming `for` hand
// one over. Every closing line below is a `Drop`, so the output is also the proof
// that each ticket was destroyed exactly once, at the point its owner gave it up.

enum Severity {
    Low,
    High,
    Outage
}

impl Severity {
    func weight(&self) -> i32 {
        match self {
            Severity::Low => 1,
            Severity::High => 5,
            Severity::Outage => 20
        }
    }
}

struct Ticket {
    id: i32,
    title: string,
    notes: Vec<string>,
    severity: Severity
}

impl Ticket {
    func open(id: i32, title: string, severity: Severity) -> Ticket {
        Ticket { id: id, title: title, notes: Vec::new(), severity: severity }
    }

    func note(&mut self, text: string) {
        self.notes.push(text)
    }

    func escalate(&mut self) {
        self.severity = Severity::Outage
    }
}

impl Drop for Ticket {
    func drop(&mut self) {
        println("  closed #{self.id} {self.title}, notes: {self.notes.len()}")
    }
}

// Counts the items a predicate keeps. The head borrows, so the loop reads each
// ticket in place, and the predicate is lent a borrow of it in turn.
func count_where<T>(items: &Vec<T>, keep: (&T) -> bool) -> i32 {
    mut n = 0
    for item in items {
        if keep(item) {
            n += 1
        }
    }
    n
}

func main() -> i32 {
    mut queue: Vec<Ticket> = Vec::new()
    queue.push(Ticket::open(1, "login slow", Severity::Low))
    queue.push(Ticket::open(2, "db replica lag", Severity::High))
    queue.push(Ticket::open(3, "typo on banner", Severity::Low))
    queue.push(Ticket::open(4, "payments down", Severity::High))

    // Methods and nested pushes reach the slot itself.
    queue[1].note("replica 3 behind by 40s")
    queue[3].escalate()
    queue[3].note("paged on-call")
    queue[3].note("rolled back deploy")

    val severe = count_where(&queue, |t: &Ticket| t.severity.weight() >= 5)
    println("severe tickets: {severe}")

    // A lookup lends the slot; `.map` reads a field off the borrow.
    val second_title = queue.get(1).map(|t: &Ticket| t.title.len()) ?? 0
    val beyond = queue.get(9).map(|t: &Ticket| t.title.len()) ?? 0
    println("title widths:   {second_title} and {beyond}")

    // Overwriting a slot drops the ticket it held.
    println("replace #3:")
    queue[2] = Ticket::open(5, "cert expiring", Severity::High)

    // Ownership by on-call engineer: each map value is a `Vec` the map owns.
    mut owned: HashMap<string, Vec<Ticket>> = HashMap::new()
    owned.insert("ada", Vec::new())
    owned.insert("bo", Vec::new())
    println("assign:")
    val outage = queue.remove(3)
    match owned.get_mut("ada") {
        Option::Some(list) => list.push(outage),
        Option::None => {}
    }
    val lag = queue.swap_remove(1)
    match owned.get_mut("bo") {
        Option::Some(list) => list.push(lag),
        Option::None => {}
    }
    val ada_load = match owned.get("ada") {
        Option::Some(list) => count_where(list, |t: &Ticket| t.notes.len() > 0),
        Option::None => 0
    }
    println("ada's noted:    {ada_load}")

    // Handing bo a fresh list gives the old one back, tickets and all; it drops at
    // the end of `main`, with the binding that took it.
    println("reassign bo:")
    val handed_back = owned.insert("bo", Vec::new())
    println("bo had a list:  {handed_back.is_some()}")

    // Hours logged per engineer; `??` reads a borrowed count through.
    mut hours: BTreeMap<string, i32> = BTreeMap::new()
    hours.insert("ada", 6)
    val ada_hours = hours.get("ada") ?? 0
    val cy_hours = hours.get("cy") ?? 0

    // Drain what is left: the loop owns each ticket, and leaving early releases the
    // ones no pass reached.
    println("drain:")
    mut score = 0
    for ticket in queue {
        score += ticket.severity.weight()
        if score > 100 {
            break
        }
    }
    println("score:          {score}")
    println("hours:          ada {ada_hours}, cy {cy_hours}")
    println("end of shift:")

    // A process exit status is 8 bits on Linux and 32 on Windows, so the total stays
    // under 256 to read the same everywhere.
    severe * 10 + score + ada_load + ada_hours + cy_hours
}
