// A first training run from a file: a batch written the way Python's `safetensors` writes it, a
// Neuro program that reads it, trains on the host, saves, reloads and continues on the GPU, and
// NumPy's float32 run of the same steps as the reference. The program is the one the
// getting-started tutorial walks through.
//
// The reference numbers come from NumPy 2 in float32: `error = x @ w + b - y`, the loss
// `mean(error * error)`, and `w -= 0.1 * x.T @ (2 / 8 * error)`, `b` alike. NumPy sums a mean
// pairwise, so a loss can differ from Neuro's in its last bits and is compared to six places;
// the weights agree bit for bit.
//
// A machine with no usable GPU runs the host half and stops at the first transfer with the
// runtime's diagnostic, which these tests accept and check in full. Device operations outside
// `@gpu` compile on Linux only, so the two GPU tests run there.

use crate::compile_harness::CompileTest;
use std::path::PathBuf;
use std::process::{Command, Output};

#[cfg(target_os = "linux")]
const NO_GPU: &str = "panic: `Device::GPU` needs an NVIDIA GPU, and none is usable: ";

const X: [[f32; 3]; 8] = [
    [1.0, 2.0, 0.5],
    [0.5, -1.0, 2.0],
    [1.5, 0.25, -0.5],
    [2.0, 1.0, 1.0],
    [-1.0, 0.5, 0.75],
    [0.0, 1.5, -1.0],
    [1.25, -0.5, 0.0],
    [-0.5, -1.5, 1.5],
];
const Y: [f32; 8] = [1.0, 2.0, 0.5, 3.0, -1.0, 0.25, 1.5, -0.5];

/// NumPy's losses for the first ten steps, to six places.
#[cfg(target_os = "linux")]
const NUMPY_LOSSES: [&str; 10] = [
    "2.226562", "1.101490", "0.692905", "0.530029", "0.454766", "0.413121", "0.386054", "0.366436",
    "0.351311", "0.339256",
];

/// NumPy's `w` and `b` after ten steps, as the bits of each `f32`.
const NUMPY_WEIGHTS: [u32; 4] = [1064130086, 1033691052, 1053188873, 1043106504];

/// `batch.safetensors` byte for byte as `safetensors.numpy.save_file({"x": x, "y": y}, ...)`
/// writes it (safetensors 0.8): compact JSON, the header padded with spaces to 8 bytes.
fn python_batch() -> Vec<u8> {
    let header = r#"{"x":{"dtype":"F32","shape":[8,3],"data_offsets":[0,96]},"y":{"dtype":"F32","shape":[8,1],"data_offsets":[96,128]}}     "#;
    let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
    bytes.extend_from_slice(header.as_bytes());
    for value in X.iter().flatten().chain(Y.iter()) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// The tutorial's program.
const TRAIN: &str = r#"
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
"#;

/// Compile `source` with `DIR` replaced by its directory, write `batch.safetensors` there as
/// Python would, and run the program there.
fn run_beside_batch(source: &str) -> (Output, PathBuf, CompileTest) {
    let test = CompileTest::new();
    let probe = test.write_source("probe.txt", "");
    let dir = probe
        .parent()
        .expect("the source has a directory")
        .to_path_buf();
    std::fs::write(dir.join("batch.safetensors"), python_batch()).expect("write the batch");
    let dir_text = dir.display().to_string().replace('\\', "/");
    let exe = test
        .compile(&test.write_source("main.nr", &source.replace("DIR", &dir_text)))
        .unwrap_or_else(|e| panic!("should compile: {e}"));
    let output = Command::new(&exe)
        .current_dir(&dir)
        .output()
        .expect("run the program");
    (output, dir, test)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

/// The `f32` values `Writer` stored for `w` then `b`: the last 16 bytes, as it adds them in order.
#[cfg(target_os = "linux")]
fn saved_weights(dir: &std::path::Path) -> Vec<f32> {
    let file = std::fs::read(dir.join("run.safetensors")).expect("the program saved its run");
    file[file.len() - 16..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect()
}

fn numpy_weights() -> Vec<f32> {
    NUMPY_WEIGHTS
        .iter()
        .map(|bits| f32::from_bits(*bits))
        .collect()
}

/// Python's batch read, five steps on the host, a checkpoint saved and read back, and five
/// more on the GPU: every loss NumPy's to six places, and the saved weights NumPy's bits.
#[cfg(target_os = "linux")]
#[test]
fn a_python_batch_trains_saves_reloads_and_continues_on_the_gpu() {
    let (output, dir, _test) = run_beside_batch(TRAIN);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<String> = NUMPY_LOSSES
        .iter()
        .enumerate()
        .map(|(i, loss)| {
            let place = if i < 5 { "host" } else { "gpu" };
            format!("{place} step {i}: loss {loss}\n")
        })
        .collect();
    if stderr.starts_with(NO_GPU) {
        assert_eq!(stdout(&output), lines[..5].concat());
        assert!(
            !output.status.success(),
            "a failed transfer must not exit 0"
        );
        let file = std::fs::read(dir.join("run.safetensors")).expect("the host half saved");
        let text = String::from_utf8_lossy(&file);
        assert!(text.contains(r#""__metadata__":{"step":"5"}"#), "{text}");
        return;
    }
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert_eq!(stdout(&output), lines.concat());
    assert_eq!(saved_weights(&dir), numpy_weights());
    let file = std::fs::read(dir.join("run.safetensors")).expect("the run was saved");
    assert!(String::from_utf8_lossy(&file).contains(r#""__metadata__":{"step":"10"}"#));
}

/// The same thirty steps on the host and on the GPU, side by side. Every operation in the loss
/// and its gradient is exact or folds in the order the host and the GPU share, so both runs
/// stay on the same bits for as long as they go, losses and weights alike.
#[cfg(target_os = "linux")]
#[test]
fn host_and_gpu_training_agree_bit_for_bit_over_many_steps() {
    let source = r#"
@grad(wrt: [w, b])
func loss(w: &mut Tensor<f32, [3, 1]>, b: &mut Tensor<f32, [1]>, x: &Tensor<f32, [8, 3]>, y: &Tensor<f32, [8, 1]>) -> Tensor<f32, []> {
    val error = x @ w + b - y
    return Tensor::scalar((&error * &error).mean())
}

func step(w: &mut Tensor<f32, [3, 1]>, b: &mut Tensor<f32, [1]>, x: &Tensor<f32, [8, 3]>, y: &Tensor<f32, [8, 1]>) -> f32 {
    val l = loss(w, b, x, y)
    val value = l.item()
    l.backward()
    *w -= 0.1f32 * w.grad()
    *b -= 0.1f32 * b.grad()
    w.zero_grad()
    b.zero_grad()
    value
}

func main() -> i32 {
    val batch = std::safetensors::open("batch.safetensors") ?? panic("no batch")
    val x: Tensor<f32, [8, 3]> = batch.tensor("x") ?? panic("no x")
    val y: Tensor<f32, [8, 1]> = batch.tensor("y") ?? panic("no y")
    mut w: Tensor<f32, [3, 1]> = Tensor::zeros()
    mut b: Tensor<f32, [1]> = Tensor::zeros()
    val gpu = Device::GPU(0)
    mut w_gpu = w.clone().to(gpu)
    mut b_gpu = b.clone().to(gpu)
    val x_gpu = x.clone().to(gpu)
    val y_gpu = y.clone().to(gpu)
    mut differ = 0
    mut last = 0.0f32
    for i in 0..30 {
        last = step(&mut w, &mut b, &x, &y)
        if step(&mut w_gpu, &mut b_gpu, &x_gpu, &y_gpu) != last { differ += 1 }
        val w_back = w_gpu.clone().to(Device::CPU)
        val b_back = b_gpu.clone().to(Device::CPU)
        for j in 0..3 {
            if w_back[j, 0] != w[j, 0] { differ += 1 }
        }
        if b_back[0] != b[0] { differ += 1 }
    }
    println("step 29: loss {last:.6}")
    differ
}
"#;
    let (output, _dir, _test) = run_beside_batch(source);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.starts_with(NO_GPU) {
        assert_eq!(stdout(&output), "");
        return;
    }
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert_eq!(stdout(&output), "step 29: loss 0.279145\n");
}

/// Ten host steps written five ways, each ending on NumPy's weights: a stepping function in a
/// loop, the step inline, a function handing the weights back as its tail tuple, a stepping
/// closure, and five steps saved, read back through an annotated `val ... else` and resumed.
#[test]
fn every_way_of_writing_the_run_lands_on_numpys_weights() {
    let source = r#"
@grad(wrt: [w, b])
func loss(w: &mut Tensor<f32, [3, 1]>, b: &mut Tensor<f32, [1]>, x: &Tensor<f32, [8, 3]>, y: &Tensor<f32, [8, 1]>) -> Tensor<f32, []> {
    val error = x @ w + b - y
    return Tensor::scalar((&error * &error).mean())
}

func step(w: &mut Tensor<f32, [3, 1]>, b: &mut Tensor<f32, [1]>, x: &Tensor<f32, [8, 3]>, y: &Tensor<f32, [8, 1]>) {
    val l = loss(w, b, x, y)
    l.backward()
    *w -= 0.1f32 * w.grad()
    *b -= 0.1f32 * b.grad()
    w.zero_grad()
    b.zero_grad()
}

func trained(count: i32, x: &Tensor<f32, [8, 3]>, y: &Tensor<f32, [8, 1]>) -> (Tensor<f32, [3, 1]>, Tensor<f32, [1]>) {
    mut w: Tensor<f32, [3, 1]> = Tensor::zeros()
    mut b: Tensor<f32, [1]> = Tensor::zeros()
    for _ in 0..count { step(&mut w, &mut b, x, y) }
    (w, b)
}

func show(w: &Tensor<f32, [3, 1]>, b: &Tensor<f32, [1]>) {
    println("{w[0, 0]} {w[1, 0]} {w[2, 0]} {b[0]}")
}

func main() -> i32 {
    val batch = std::safetensors::open("batch.safetensors") ?? panic("no batch")
    val x: Tensor<f32, [8, 3]> = batch.tensor("x") ?? panic("no x")
    val y: Tensor<f32, [8, 1]> = batch.tensor("y") ?? panic("no y")

    mut w: Tensor<f32, [3, 1]> = Tensor::zeros()
    mut b: Tensor<f32, [1]> = Tensor::zeros()
    for _ in 0..10 { step(&mut w, &mut b, &x, &y) }
    show(&w, &b)

    mut w: Tensor<f32, [3, 1]> = Tensor::zeros()
    mut b: Tensor<f32, [1]> = Tensor::zeros()
    for _ in 0..10 {
        val l = loss(&mut w, &mut b, &x, &y)
        l.backward()
        w -= 0.1f32 * w.grad()
        b -= 0.1f32 * b.grad()
        w.zero_grad()
        b.zero_grad()
    }
    show(&w, &b)

    val (w, b) = trained(10, &x, &y)
    show(&w, &b)

    mut w: Tensor<f32, [3, 1]> = Tensor::zeros()
    mut b: Tensor<f32, [1]> = Tensor::zeros()
    val advance = |w: &mut Tensor<f32, [3, 1]>, b: &mut Tensor<f32, [1]>| step(w, b, &x, &y)
    for _ in 0..10 { advance(&mut w, &mut b) }
    show(&w, &b)

    val (w, b) = trained(5, &x, &y)
    mut out = std::safetensors::Writer::new()
    out.add("w", &w)
    out.add("b", &b)
    out.save("DIR/half.safetensors") ?? panic("cannot save")
    val half = std::safetensors::open("DIR/half.safetensors") ?? panic("cannot open")
    val Ok(w): Result<Tensor<f32, [3, 1]>, IoError> = half.tensor("w") else |e| {
        println(e.message)
        return 1
    }
    val b: Tensor<f32, [1]> = half.tensor("b") ?? panic("no b")
    mut w = w
    mut b = b
    for _ in 0..5 { step(&mut w, &mut b, &x, &y) }
    show(&w, &b)
    0
}
"#;
    let (output, _dir, _test) = run_beside_batch(source);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Each line prints an `f32` widened to `f64` with enough digits to name it exactly.
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 5, "{text}");
    for line in lines {
        let weights: Vec<f32> = line
            .split(' ')
            .map(|value| value.parse::<f64>().expect("a number") as f32)
            .collect();
        assert_eq!(weights, numpy_weights(), "{line}");
    }
}

/// Reading the batch, training, saving and reading the checkpoint back, many times over,
/// leave nothing behind.
#[test]
fn a_training_run_from_a_file_releases_everything() {
    let source = r#"
@grad(wrt: [w, b])
func loss(w: &mut Tensor<f32, [3, 1]>, b: &mut Tensor<f32, [1]>, x: &Tensor<f32, [8, 3]>, y: &Tensor<f32, [8, 1]>) -> Tensor<f32, []> {
    val error = x @ w + b - y
    return Tensor::scalar((&error * &error).mean())
}

func round(path: &string) -> Result<f32, IoError> {
    val batch = std::safetensors::open("DIR/batch.safetensors")?
    val x: Tensor<f32, [8, 3]> = batch.tensor("x")?
    val y: Tensor<f32, [8, 1]> = batch.tensor("y")?
    mut w: Tensor<f32, [3, 1]> = Tensor::zeros()
    mut b: Tensor<f32, [1]> = Tensor::zeros()
    mut last = 0.0f32
    for _ in 0..3 {
        val l = loss(&mut w, &mut b, &x, &y)
        last = l.item()
        l.backward()
        w -= 0.1f32 * w.grad()
        b -= 0.1f32 * b.grad()
        w.zero_grad()
        b.zero_grad()
    }
    mut out = std::safetensors::Writer::new()
    out.add("w", &w)
    out.add("b", &b)
    out.add_metadata("step", "3")
    out.save(path)?
    val saved = std::safetensors::open(path)?
    val back: Tensor<f32, [3, 1]> = saved.tensor("w")?
    val missing: Result<Tensor<f32, [3, 1]>, IoError> = saved.tensor("v")
    if missing.is_ok() { return Ok(-1.0) }
    Ok(last + back[0, 0] - w[0, 0])
}

func main() -> i32 {
    mut total = 0.0f32
    for _ in 0..40 {
        total += round("DIR/leak.safetensors") ?? -100.0f32
    }
    if total > 27.0f32 && total < 28.0f32 { 0 } else { 1 }
}
"#;
    let test = CompileTest::new();
    let probe = test.write_source("probe.txt", "");
    let dir = probe.parent().expect("a directory").to_path_buf();
    std::fs::write(dir.join("batch.safetensors"), python_batch()).expect("write the batch");
    let dir_text = dir.display().to_string().replace('\\', "/");
    assert_eq!(
        test.run_leak_checked("leak.nr", &source.replace("DIR", &dir_text)),
        0
    );
}

/// The tutorial shows the program these tests run, character for character.
#[test]
fn the_tutorial_shows_the_tested_program() {
    let tutorial = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/getting-started/first-training-run.md");
    let text = std::fs::read_to_string(&tutorial)
        .expect("read the tutorial")
        .replace("\r\n", "\n");
    let start = text.find("```neuro\n").expect("a Neuro listing") + "```neuro\n".len();
    let length = text[start..].find("```").expect("the listing ends");
    assert_eq!(&text[start..start + length], TRAIN.trim_start_matches('\n'));
}
