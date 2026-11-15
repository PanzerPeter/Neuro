// Weights on disk with `std::safetensors`, the format PyTorch, JAX and the Hugging Face tools
// share. `open` checks the header before trusting it, and `file.tensor(name)` copies one tensor
// out, typed by what it is bound to: the stored dtype and shape must match that type exactly,
// or the read is an `Err` naming both. The program writes in the working directory and removes
// what it wrote.

func save(path: &string) -> Result<(), IoError> {
    val w: Tensor<f32, [2, 3]> = [[0.5, -1.0, 2.0], [1.5, 0.0, -0.25]]
    val steps: Tensor<i64, [1]> = [120]
    mut out = std::safetensors::Writer::new()
    out.add("layer.w", &w)
    out.add("steps", &steps)
    out.add_metadata("note", "fit on 6 samples")
    out.save(path)
}

func main() -> i32 {
    val path: &string = "neuro_example_weights.safetensors"
    save(path) ?? panic("cannot save the weights")

    val file = std::safetensors::open(path) ?? panic("cannot open the weights")
    // `?`, `??` and an annotated `val ... else` all hand the read its type.
    val w: Tensor<f32, [2, 3]> = file.tensor("layer.w") ?? panic("no layer.w")
    val Ok(steps): Result<Tensor<i64, [1]>, IoError> = file.tensor("steps") else |e| {
        println(e.message)
        return 1
    }
    val note = file.metadata("note") ?? "no note"
    println("layer.w sums to {w.sum()} after {steps[0]} steps ({note})")

    // The annotation is checked against the file, not trusted.
    val wrong: Result<Tensor<f32, [3, 2]>, IoError> = file.tensor("layer.w")
    match wrong {
        Ok(_) => println("read a [2, 3] tensor as [3, 2]"),
        Err(e) => println("{e.kind:?}: {e.message}"),
    }

    std::fs::remove_file(path) ?? panic("cannot remove the weights")
    0
}
