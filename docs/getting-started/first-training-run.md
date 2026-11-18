# Your First Training Run

This tutorial trains a small linear model on data Python wrote. Python saves a fixed batch as a
`.safetensors` file, a Neuro program reads it, trains five steps on the CPU, saves a checkpoint,
reads the checkpoint back and trains five more steps on the GPU. Last, NumPy repeats the ten steps
and compares its weights with the ones Neuro saved.

You need:

- `neuroc`, from the [Installation Guide](installation.md)
- Python 3 with NumPy and the `safetensors` package: `pip install numpy safetensors`
- For the GPU half, Linux and an NVIDIA GPU. Without one, the CPU half still runs, and
  [Without a GPU](#without-a-gpu) shows what you see.

Work in an empty directory. Every file below goes in it.

## Step 1: Write a Batch from Python

Create `make_batch.py`:

```python
import numpy as np
from safetensors.numpy import save_file

x = np.array([
    [1.0, 2.0, 0.5], [0.5, -1.0, 2.0], [1.5, 0.25, -0.5], [2.0, 1.0, 1.0],
    [-1.0, 0.5, 0.75], [0.0, 1.5, -1.0], [1.25, -0.5, 0.0], [-0.5, -1.5, 1.5],
], dtype=np.float32)
y = np.array([[1.0], [2.0], [0.5], [3.0], [-1.0], [0.25], [1.5], [-0.5]], dtype=np.float32)
save_file({"x": x, "y": y}, "batch.safetensors")
```

```bash
python make_batch.py
```

`batch.safetensors` now holds eight samples: `x`, an `[8, 3]` matrix of features, and `y`, an
`[8, 1]` column of targets, both `float32`. The format is the one PyTorch and the Hugging Face
tools use.

## Step 2: Write the Training Program

Create `train.nr`:

```neuro
const RATE: f32 = 0.1
const STEPS: i32 = 5

@grad(wrt: [w, b])
func loss(
    w: &mut Tensor<f32, [3, 1]>,
    b: &mut Tensor<f32, [1]>,
    x: &Tensor<f32, [8, 3]>,
    y: &Tensor<f32, [8, 1]>
) -> Tensor<f32, []> {
    val error = x @ w + b - y
    return Tensor::scalar((&error * &error).mean())
}

func step(
    w: &mut Tensor<f32, [3, 1]>,
    b: &mut Tensor<f32, [1]>,
    x: &Tensor<f32, [8, 3]>,
    y: &Tensor<f32, [8, 1]>
) -> f32 {
    val l = loss(w, b, x, y)
    val value = l.item()
    l.backward()
    *w -= RATE * w.grad()
    *b -= RATE * b.grad()
    w.zero_grad()
    b.zero_grad()
    value
}

func save(path: &string, w: &Tensor<f32, [3, 1]>, b: &Tensor<f32, [1]>, done: i32) -> Result<(), IoError> {
    mut out = std::safetensors::Writer::new()
    out.add("w", w)
    out.add("b", b)
    val text = "{done}"
    out.add_metadata("step", &text)
    out.save(path)
}

func train() -> Result<(), IoError> {
    val batch = std::safetensors::open("batch.safetensors")?
    val x: Tensor<f32, [8, 3]> = batch.tensor("x")?
    val y: Tensor<f32, [8, 1]> = batch.tensor("y")?

    mut w: Tensor<f32, [3, 1]> = Tensor::zeros()
    mut b: Tensor<f32, [1]> = Tensor::zeros()
    for i in 0..STEPS {
        println("host step {i}: loss {step(&mut w, &mut b, &x, &y):.6}")
    }
    save("run.safetensors", &w, &b, STEPS)?

    val run = std::safetensors::open("run.safetensors")?
    val done = (run.metadata("step") ?? "0").parse::<i32>() ?? 0
    val gpu = Device::GPU(0)
    val w_saved: Tensor<f32, [3, 1]> = run.tensor("w")?
    val b_saved: Tensor<f32, [1]> = run.tensor("b")?
    mut w = w_saved.to(gpu)
    mut b = b_saved.to(gpu)
    val x = x.to(gpu)
    val y = y.to(gpu)
    for i in done..done + STEPS {
        println("gpu step {i}: loss {step(&mut w, &mut b, &x, &y):.6}")
    }
    save("run.safetensors", &w, &b, done + STEPS)
}

func main() -> i32 {
    match train() {
        Ok(_) => 0,
        Err(e) => {
            println("error: {e.message}")
            1
        }
    }
}
```

**The loss.** `loss` is the mean squared error of the prediction `x @ w + b`. `@grad(wrt: [w, b])`
has the compiler write the derivative of `loss` with respect to `w` and `b`, so the parameters
are passed as `&mut`: calling `.backward()` on the result stores each gradient beside its tensor.
The error is squared with `*` rather than `.pow(2.0)`. Multiplication is exact on the CPU and the
GPU alike, while `.pow` calls each side's math library, which may round differently in the last
bit. With `*`, the GPU steps give the same bits the CPU would.

**One step.** `step` calls the loss, keeps its value with `.item()`, fills the gradients with
`.backward()`, moves each parameter against its gradient, and clears the gradients with
`.zero_grad()` for the next step. It does not care which device the tensors are on.

**Reading the batch.** `std::safetensors::open` reads the file and checks its header.
`batch.tensor("x")` takes its type from the annotation: the read fails if the stored tensor is not
`F32` with shape `[8, 3]`. Every failure is an `IoError`, and `?` returns it from `train`.

**Saving and resuming.** `Writer` collects tensors and text metadata and `save` writes the file.
The checkpoint records the step count under `"step"`, and the resumed run reads it back with
`.metadata` and `.parse::<i32>()`.

**Moving to the GPU.** `.to(gpu)` copies a tensor into GPU memory. Operators, reductions, the
gradient and the update then run on the GPU, and the gradient stays there with its parameter.
The second `save` reads the weights back from the GPU before writing them.

[Standard Library](../language-reference/standard-library.md) describes the file API, and
[Automatic Differentiation](../language-reference/autodiff.md) describes `@grad`.

## Step 3: Run It

```bash
neuroc run train.nr
```

```text
host step 0: loss 2.226562
host step 1: loss 1.101490
host step 2: loss 0.692905
host step 3: loss 0.530029
host step 4: loss 0.454766
gpu step 5: loss 0.413121
gpu step 6: loss 0.386054
gpu step 7: loss 0.366436
gpu step 8: loss 0.351311
gpu step 9: loss 0.339256
```

Each printed loss is the one computed before that step's update. `run.safetensors` now holds the
weights after all ten steps, with `"step": "10"` in its metadata.

### Without a GPU

The first transfer stops the program. The CPU half has already run and saved its checkpoint:

```text
host step 0: loss 2.226562
host step 1: loss 1.101490
host step 2: loss 0.692905
host step 3: loss 0.530029
host step 4: loss 0.454766
panic: `Device::GPU` needs an NVIDIA GPU, and none is usable: CUDA_ERROR_NO_DEVICE
```

The reason after the colon comes from the CUDA driver and depends on the machine. To train on the
CPU only, delete the four `.to(gpu)` calls and the `gpu` binding.

## Step 4: Check It Against NumPy

Create `check.py`. It repeats the ten steps in NumPy `float32`, writing the gradient of the mean
squared error out by hand, and compares its weights with the ones Neuro saved:

```python
import numpy as np
from safetensors.numpy import load_file

batch = load_file("batch.safetensors")
x, y = batch["x"], batch["y"]
w = np.zeros((3, 1), dtype=np.float32)
b = np.zeros(1, dtype=np.float32)
rate = np.float32(0.1)
for i in range(10):
    error = x @ w + b - y
    print(f"step {i}: loss {np.mean(error * error):.6f}")
    slope = np.float32(2 / len(x)) * error
    w -= rate * (x.T @ slope)
    b -= rate * slope.sum(axis=0)

run = load_file("run.safetensors")
print("largest weight difference:", max(np.abs(run["w"] - w).max(), np.abs(run["b"] - b).max()))
```

```bash
python check.py
```

```text
step 0: loss 2.226562
step 1: loss 1.101490
step 2: loss 0.692905
step 3: loss 0.530029
step 4: loss 0.454766
step 5: loss 0.413121
step 6: loss 0.386054
step 7: loss 0.366436
step 8: loss 0.351311
step 9: loss 0.339256
largest weight difference: 0.0
```

The losses match Neuro's, and the weights match to the bit. `check.py` read the checkpoint with
Python's own `safetensors`, so the file Neuro wrote is one PyTorch and NumPy can load.

## See also

- [`examples/integration/gpu/file_training.nr`](../../examples/integration/gpu/file_training.nr): the same run,
  with the GPU half checked step by step against the CPU and the result scored by a `@gpu`
  function
- [Tensors](../language-reference/tensors.md): devices, operators and reductions
- [CLI Usage](../guides/cli-usage.md): `neuroc run`, `neuroc compile` and their flags
