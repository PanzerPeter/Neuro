// The `>>` operator: build a new function out of two.
//
// `f >> g` is the function `|x| g(f(x))`, applied left to right. Where `|>` pushes a
// value through a chain, `>>` names the chain itself, so a pipeline written once can
// be applied in several places.
//
// An operand is any function value: a function name, an associated function, a
// generic function's instance `f::<T>`, a bound method or a closure literal. The
// composition is a closure, and `>>` binds tighter than `|>` (Appendix B), so
// `x |> f >> g` applies the composed function to `x`.
//
// `>>` is not right shift in this language. Shifting is the `.shr(n)` method, which
// leaves the token free for the operator an AI-first language uses far more often.

func normalize(reading: i32) -> i32 {
    reading - 100
}

func clamp_low(reading: i32) -> i32 {
    if reading < 0 {
        0
    } else {
        reading
    }
}

func amplify(reading: i32) -> i32 {
    reading * 2
}

func label(reading: i32) -> string {
    "reading {reading}"
}

func width(text: string) -> i32 {
    text.len() as i32
}

func main() -> i32 {
    // One preparation step, named once and applied three times.
    val prepare = normalize >> clamp_low
    println("prepare(140) {prepare(140)}")
    println("prepare(80)  {prepare(80)}")
    println("prepare(220) {prepare(220)}")

    // A chain of three. The stages run in the order they are written, so this is
    // `amplify(clamp_low(normalize(reading)))`.
    val full = normalize >> clamp_low >> amplify
    println("full(160)    {full(160)}")

    // The stages need not share a type: only each stage's result and the next
    // stage's parameter have to agree.
    val measure = label >> width
    println("measure(45)  {measure(45)}")

    // A composition is an ordinary function value, so it passes as an argument.
    val applied = apply(150, prepare)
    println("applied      {applied}")

    // `>>` binds tighter than `|>`: the whole composition is one pipeline stage.
    val piped = 130
        |> normalize
        |> clamp_low >> amplify
    println("piped        {piped}")

    // A closure literal composes too. A stage after the first takes its parameter
    // type from the stage before it, so this one needs no annotation.
    val offset = clamp_low >> (|reading| reading + 5)
    println("offset(90)   {offset(90)}")

    // A composition called where it is written needs no name at all.
    val direct = (normalize >> amplify)(120)
    println("direct       {direct}")

    // A total small enough to survive an exit code's one byte.
    prepare(140) + measure(45) + piped + direct
}

func apply(value: i32, stage: (i32) -> i32) -> i32 {
    stage(value)
}
