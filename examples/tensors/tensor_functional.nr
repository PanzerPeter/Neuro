// Functional tensor operations: `.map(f)`, `.zip(other, f)`, `.reduce(init, f)`.
//
// All three walk the receiver's elements once, in row-major order, calling a function per
// element. They differ only in what that function is handed and what becomes of its answer:
//
//     .map(f)            f(element)          -> a tensor of the same shape
//     .zip(other, f)     f(element, other)   -> a tensor of the same shape
//     .reduce(init, f)   f(accumulator, x)   -> one value, not a tensor
//
// `.map` is the one construct that changes what a tensor's buffer holds: the result's
// element type is the FUNCTION's return type, not the receiver's.
//
// A fold answers one number of the seed's type, the way a whole-tensor `.sum()` does, so
// nothing has to be unwrapped afterwards to use it. The accumulator is the FIRST parameter,
// which is what `|acc, x|` means, and the seed takes its type from that parameter: an
// untyped `0.0` folded over an `f32` tensor is an `f32`.
//
// All three READ their operands. Each allocates its own result and leaves the buffers it
// walked alone, so a traversal works through a borrow and a shared weight survives one.
//
// There is deliberately no `.filter`. A filter's output length depends on the VALUES in the
// buffer, so its result would have no shape the type system can name, which is the whole
// point of a shape being part of the type. Use a `Vec<T>` where a data-dependent length is
// genuinely what you want.

func halve(x: f32) -> f32 {
    x / 2.0
}

func clamp_low(x: f32) -> f32 {
    if x < 0.0 { return 0.0 }
    return x
}

// A borrowed weight: `w` stays with the caller, and the scale is captured from the
// enclosing frame rather than passed per element.
func scaled(w: &Tensor<i32, [2, 3]>, k: i32) -> Tensor<i32, [2, 3]> {
    return w.map(|x: i32| -> i32 { x * k })
}

func main() -> i32 {
    val t: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]

    // Elementwise, same shape, same element type.
    val doubled: Tensor<i32, [2, 3]> = t.map(|x: i32| -> i32 { x * 2 })
    println("doubled    = [{doubled[0, 0]}, {doubled[0, 1]}, {doubled[0, 2]}] [{doubled[1, 0]}, {doubled[1, 1]}, {doubled[1, 2]}]")

    // The result's element type is the function's return type, so a `.map` is also how a
    // tensor changes dtype elementwise.
    val halves: Tensor<f64, [2, 3]> = t.map(|x: i32| -> f64 { x as f64 / 2.0 })
    println("halves     = {halves[0, 0]:.1}, {halves[1, 2]:.1}")

    // Two buffers walked at one index. They need not hold the same element type: the
    // function's parameters say what each one holds.
    val weights: Tensor<f64, [2, 3]> = [
        [1.0, 0.5, 0.0],
        [0.0, 0.5, 1.0]
    ]
    val weighted: Tensor<f64, [2, 3]> = t.zip(weights, |n: i32, w: f64| -> f64 { n as f64 * w })
    println("weighted   = {weighted[0, 0]:.1}, {weighted[0, 1]:.1}, {weighted[1, 2]:.1}")

    // A fold over the whole buffer. `.sum()` is this one, named.
    val total: i32 = t.reduce(0, |acc: i32, x: i32| -> i32 { acc + x })
    println("total      = {total}")

    // The accumulator carries whatever the fold needs, not just a running element: here
    // it counts rather than sums.
    val evens: i32 = t.reduce(0, |acc: i32, x: i32| -> i32 {
        if x % 2 == 0 { return acc + 1 }
        return acc
    })
    println("even count = {evens}")

    // Traversals chain: each produces an ordinary tensor value, and the intermediate is
    // released rather than kept.
    val chained: i32 = t
        .map(|x: i32| -> i32 { x + 1 })
        .map(|x: i32| -> i32 { x * x })
        .reduce(0, |acc: i32, x: i32| -> i32 { acc + x })
    println("chained    = {chained}")

    // The function is an ordinary function VALUE, so a `>>` composition is a stage too.
    val rescale = halve >> clamp_low
    val raw: Tensor<f32, [4]> = [4.0, -2.0, 8.0, 6.0]
    val clean: Tensor<f32, [4]> = raw.map(rescale)
    println("rescaled   = {clean[0]:.1}, {clean[1]:.1}, {clean[2]:.1}, {clean[3]:.1}")

    // Through a borrow, and the caller's tensor survives the call.
    val s = scaled(&t, 10)
    println("scaled     = {s[0, 0]}, {s[1, 2]}")
    println("source     = {t[0, 0]}, still readable")

    return total + evens
}
