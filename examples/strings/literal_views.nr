// A string literal where `&string` is expected: a parameter, an annotated binding, a return,
// a struct field, an `Option` payload and a fallback. The literal is a view of read-only text
// already, so no `&` is written and nothing is borrowed or freed. Any other `string` value is
// still borrowed with `&`.

struct Label {
    name: &string,
    width: u64
}

func length(s: &string) -> u64 {
    s.len()
}

func unit_of(metric: &string) -> &string {
    if metric == "loss" { "nats" } else { "%" }
}

func find(key: &string) -> Option<&string> {
    if key == "lr" { Some("0.01") } else { None }
}

func main() -> i32 {
    val direct = length("text")                // 4, no `&` needed
    val title: &string = "Training run"        // an annotated binding
    val label = Label { name: "epoch", width: 8 }
    val unit = unit_of("loss")                 // returned literal -> "nats"
    val lr = find("lr") ?? "unset"             // -> "0.01"
    val momentum = find("momentum") ?? "unset" // -> "unset"

    val built = "batch " + "32"                // an owned string: borrowed with `&`
    val built_len = length(&built)             // 8

    println("{title}")
    println("length(\"text\") = {direct}")
    println("{label.name} is {label.width} wide")
    println("loss is measured in {unit}")
    println("lr = {lr}, momentum = {momentum}")
    println("length(&built) = {built_len}")

    // 4 + 8 + 4 ("nats") + 4 ("0.01") + 5 ("unset") + 8 = 33.
    (direct + label.width + unit.len() + lr.len() + momentum.len() + built_len) as i32
}
