// The `|>` operator: apply a function to the value on its left.
//
// `x |> f` is `f(x)`. It exists to turn a nest of calls, which has to be read from
// the inside out, into a chain read top to bottom in the order the stages run.
//
// The thing on the right of `|>` is a function *value*, never a call. Three
// spellings produce one: a function name, a bound method `receiver.method`, and a
// parenthesized closure literal. A binding of function type is a name like any
// other, so it works too. `|>` binds looser than every other operator, so the whole
// expression to its left is what gets piped.

struct Scaler {
    factor: i32
}

impl Scaler {
    func apply(&self, value: i32) -> i32 {
        value * self.factor
    }
}

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

func label(reading: i32) -> string {
    "reading {reading}"
}

func main() -> i32 {
    // Written as a nest, the first step to run is the innermost one.
    val nested = clamp_low(normalize(140))
    println("nested  {nested}")

    // The same computation as a chain. The stages appear in the order they run.
    val piped = 140
        |> normalize
        |> clamp_low
    println("piped   {piped}")

    // A bound method is a function value: the receiver is written where it is
    // evaluated, and the piped value becomes the method's argument.
    val scaler = Scaler { factor: 2 }
    val scaled = 130
        |> normalize
        |> clamp_low
        |> scaler.apply
    println("scaled  {scaled}")

    // A closure literal parenthesized inline, for a stage not worth naming.
    val adjusted = 110
        |> normalize
        |> (|value: i32| -> i32 { value * value })
    println("adjusted {adjusted}")

    // A closure bound to a name pipes exactly like a function does.
    val halve = |value: i32| -> i32 { value / 2 }
    val halved = adjusted |> halve
    println("halved  {halved}")

    // `|>` binds looser than arithmetic, so the whole sum is the piped value.
    val summed = 100 + 45 |> normalize
    println("summed  {summed}")

    // A chain carries an owned `string` as readily as a number: each stage takes
    // ownership of what the stage before it produced.
    val described = summed |> label
    println("{described}")

    piped + scaled + halved + summed
}
