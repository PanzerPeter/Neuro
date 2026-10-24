// Collections of owned values: a `Vec`, `HashMap` or `BTreeMap` element may be any sized
// value, and the collection owns what its slots hold.
//
//   - Insertion moves: `v.push(x)` and `m.insert(k, x)` take their arguments.
//   - Indexing names a place: `v[i] = x` writes the slot, `v[i].field` and
//     `v[i].method()` work on it in place, and `&v[i]` borrows it. Using a
//     non-`Copy` `v[i]` by value is a compile error, since the slot cannot be left
//     empty; `&v[i]`, `v[i].clone()` and `v.remove(i)` are the ways out.
//   - Lookup borrows: `get` lends `Option<&T>` and `get_mut` lends `Option<&mut T>`,
//     while `pop`, `remove` and a displacing `insert` hand the value over.
//   - A slot that is overwritten, removed or cleared drops what it held.

struct Task {
    title: string,
    tags: Vec<string>,
    hours: i32
}

impl Task {
    func new(title: string, hours: i32) -> Task {
        Task { title: title, tags: Vec::new(), hours: hours }
    }

    func log(&mut self, extra: i32) {
        self.hours += extra
    }
}

func main() -> i32 {
    mut board: Vec<Task> = Vec::new()
    board.push(Task::new("parser", 3))
    board.push(Task::new("lexer", 2))
    board.push(Task::new("docs", 1))

    // The slot is the place: a method, a field write and a nested push all reach it.
    board[0].log(4)
    board[1].tags.push("urgent")
    board[2] = Task::new("release notes", 2)
    println("first:  {board[0].title} ({board[0].hours}h)")

    // A borrow of an element, and of a field, read in place.
    {
        val second = &board[1]
        val title = &second.title
        println("second: {title}, {second.tags.len()} tag")
    }

    // `remove` takes an element out and shifts the tail down over its slot.
    val done = board.remove(0)
    println("removed {done.title}, {board.len()} left")

    // Who owns which tasks: the map owns its `Vec`s, and `get_mut` edits one in place.
    mut owners: HashMap<string, Vec<string>> = HashMap::new()
    owners.insert("ada", Vec::new())
    match owners.get_mut("ada") {
        Option::Some(list) => list.push(done.title),
        Option::None => {}
    }
    val previous = owners.insert("bo", Vec::new())
    println("bo had a list before: {previous.is_some()}")
    val ada_count = match owners.get("ada") {
        Option::Some(list) => list.len() as i32,
        Option::None => 0
    }

    // `??` reads a borrowed `Copy` value through: `0` has no address to lend.
    mut hours: BTreeMap<string, i32> = BTreeMap::new()
    hours.insert("lexer", 2)
    val lexer = hours.get("lexer") ?? 0
    val missing = hours.get("tests") ?? 0
    println("hours:  lexer {lexer}, tests {missing}")

    // A by-value loop consumes the `Vec`: each pass owns the task it is handed.
    mut total = 0
    for task in board {
        println("drain:  {task.title}")
        total += task.hours
    }
    println("total:  {total}")

    total * 10 + ada_count
}
