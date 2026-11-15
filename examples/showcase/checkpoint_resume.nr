// Showcase: a least-squares fit that stops halfway, saves a checkpoint, and resumes from it.
// Integration example: `@grad` with `.backward()`, `.grad()` and `.zero_grad()` trains the
// weights, `std::safetensors` writes them with the step count in the metadata, an annotated
// `val ... else` reads them back typed by the annotation, `.parse::<i32>()` recovers the step,
// and `std::fs` removes the file. The resumed run must end on the same bits as a run that
// never stopped, and the exit code is the step it resumed from.

const RATE: f32 = 0.05
const STEPS: i32 = 12
const SAVED_AT: i32 = 5

@grad(wrt: [w])
func loss(w: &mut Tensor<f32, [3, 1]>) -> Tensor<f32, []> {
    val x: Tensor<f32, [4, 3]> = [
        [1.0, 0.0, 1.0], [0.0, 1.0, 1.0], [1.0, 1.0, 1.0], [2.0, 1.0, 1.0]
    ]
    val y: Tensor<f32, [4, 1]> = [[2.0], [-1.0], [1.0], [3.0]]
    val error = x @ w - y
    return Tensor::scalar(error.pow(2.0).mean())
}

// `count` steps of gradient descent from `start`, returning the weights and the last loss.
func train(start: Tensor<f32, [3, 1]>, count: i32) -> (Tensor<f32, [3, 1]>, f32) {
    mut w = start
    mut last = 0.0f32
    for _ in 0..count {
        val l = loss(&mut w)
        l.backward()
        w -= RATE * w.grad()
        w.zero_grad()
        last = l.sum()
    }
    (w, last)
}

func save(path: &string, w: &Tensor<f32, [3, 1]>, step: i32) -> Result<(), IoError> {
    mut out = std::safetensors::Writer::new()
    out.add("w", w)
    val step_text = "{step}"
    out.add_metadata("step", &step_text)
    out.save(path)
}

func main() -> i32 {
    val path: &string = "neuro_showcase_checkpoint.safetensors"

    val (straight, straight_loss) = train(Tensor::zeros(), STEPS)

    val (first, halfway) = train(Tensor::zeros(), SAVED_AT)
    save(path, &first, SAVED_AT) ?? panic("cannot save the checkpoint")
    println("saved at step {SAVED_AT}, loss {halfway:.4}")

    val file = std::safetensors::open(path) ?? panic("cannot open the checkpoint")
    val Ok(resumed): Result<Tensor<f32, [3, 1]>, IoError> = file.tensor("w") else |e| {
        println(e.message)
        return -1
    }
    val step_text = file.metadata("step") ?? "0"
    val step = step_text.parse::<i32>() ?? 0
    val (w, resumed_loss) = train(resumed, STEPS - step)
    println("resumed at step {step}, loss {resumed_loss:.4}")

    mut same = straight_loss == resumed_loss
    for i in 0..3 {
        same = same && straight[i, 0] == w[i, 0]
    }
    println("same as an uninterrupted run: {same}")
    std::fs::remove_file(path) ?? panic("cannot remove the checkpoint")
    step
}
