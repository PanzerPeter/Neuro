// Owned strings in every position: a `string` knows whether it owns its buffer, so a
// struct field, an enum payload, a by-value argument, a return value and a store through
// `&mut` each release what they hold by the ordinary drop rules. A literal in the same
// position owns nothing and is never freed. The exit code is the total bytes read.

struct Label {
    text: string,
    weight: i32
}

enum Event {
    Note(string),
    Count(i32)
}

// Takes the label by value: its text is released when this call returns.
func weigh(label: Label) -> u64 {
    label.text.len() * label.weight as u64
}

// Every path hands back a `string` the caller owns or a literal it does not.
func describe(e: Event) -> string {
    match e {
        Event::Note(text) => text,
        Event::Count(n) => if n == 0 { "none" } else { "count {n}" },
    }
}

// Replaces the caller's text in place; the text it displaces is released here.
func rename(label: &mut Label, round: i32) {
    label.text = "round {round}"
}

func apply(f: (string) -> string, s: string) -> string {
    f(s)
}

func main() -> i32 {
    mut total: u64 = 0

    total += weigh(Label { text: "ab" + "cd", weight: 2 })
    total += weigh(Label { text: "static", weight: 1 })

    val events = [Event::Note("saved " + "file"), Event::Count(3), Event::Count(0)]
    for e in events {
        val line = describe(e)
        println(line)
        total += line.len()
    }

    mut label = Label { text: "first", weight: 1 }
    for round in 0..3 {
        rename(&mut label, round)
    }
    println(label.text)
    total += label.text.len()

    val loud = apply(|s: string| -> string { s + "!" }, "done" + "")
    println(loud)
    total += loud.len()

    total as i32
}
