// Tensor operations on a device tensor outside `@gpu` run on its device: element reads and
// writes and `.clone()` on every build, the operators, reductions and sorts where the MLIR
// backend can build their kernels.
//
// Only a machine with an NVIDIA GPU runs a transfer to the end. CI has none, so each test
// that needs one also accepts the transfer's own diagnostic, as `device_management.rs` does.

use std::process::{Command, Output};

use crate::compile_harness::CompileTest;

const NO_GPU: &str = "panic: `Device::GPU` needs an NVIDIA GPU, and none is usable: ";
const ON_DEVICE: &str = "panic: this tensor lives on a GPU, where host code cannot read it: move it back with `.to(Device::CPU)` first at ";

fn run(test: &CompileTest, name: &str, source: &str, hide_devices: bool) -> Output {
    let exe = test
        .compile(&test.write_source(name, source))
        .expect("the program compiles");
    let mut command = Command::new(exe);
    if hide_devices {
        command.env("CUDA_VISIBLE_DEVICES", "");
    }
    command.output().expect("the program should start")
}

fn without_gpu(output: &Output) -> bool {
    String::from_utf8_lossy(&output.stderr).starts_with(NO_GPU)
}

fn assert_ran(output: &Output, code: i32, stdout: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(code), "stderr: {stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), stdout);
}

/// An element is copied to or from the GPU on its own, and a clone is a second device
/// buffer: writing the original after the clone leaves the clone as it was.
#[cfg(unix)]
#[test]
fn elements_and_clones_are_read_and_written_on_the_device() {
    const SOURCE: &str = r#"
func main() -> i32 {
    val m: Tensor<f32, [3, 4]> = Tensor::ones()
    mut g = m.to(Device::GPU(0))
    g[1, 2] = 7.0f32
    g[1, 2] += 0.5f32
    val c = g.clone()
    g[0, 0] = -1.0f32
    println("{g[1, 2]} {c[1, 2]} {c[0, 0]} {g[0, 0]}")
    val back = c.to(Device::CPU)
    return back.sum() as i32
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "elements.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    assert_ran(&output, 18, "7.5 7.5 1.0 -1.0\n");
}

/// Every operator and reduction a device tensor takes gives the host's answer bit for bit,
/// odd extents included: a mismatch count of zero.
#[cfg(all(feature = "mlir", unix))]
#[test]
fn operators_and_reductions_on_the_device_match_the_host() {
    const SOURCE: &str = r#"
func main() -> i32 {
    mut m: Tensor<f32, [37, 19]> = Tensor::zeros()
    mut i = 0
    while i < 37 {
        mut j = 0
        while j < 19 {
            m[i, j] = ((i * 19 + j) % 23) as f32 - 7.5
            j += 1
        }
        i += 1
    }
    val w = Tensor::<f32, [19, 5]>::ones() * 0.25f32
    val g = m.clone().to(Device::GPU(0))
    val gw = w.clone().to(Device::GPU(0))

    val fused = ((&g + &g) * 2.0f32 - &m).to(Device::CPU)
    val product = (&g @ &gw).to(Device::CPU)
    val rows = g.sum(axis: 1).to(Device::CPU)
    val columns = g.mean(axis: 0).to(Device::CPU)
    val least = g.min(axis: 1).to(Device::CPU)
    val total = g.sum()
    val peak = (&g + &g).max()
    val average = g.mean()

    mut wrong = 0
    i = 0
    while i < 37 {
        if rows[i] != m.sum(axis: 1)[i] { wrong += 1 }
        if least[i] != m.min(axis: 1)[i] { wrong += 1 }
        mut j = 0
        while j < 19 {
            if fused[i, j] != ((&m + &m) * 2.0f32 - &m)[i, j] { wrong += 1 }
            j += 1
        }
        j = 0
        while j < 5 {
            if product[i, j] != (&m @ &w)[i, j] { wrong += 1 }
            j += 1
        }
        i += 1
    }
    i = 0
    while i < 19 {
        if columns[i] != m.mean(axis: 0)[i] { wrong += 1 }
        i += 1
    }
    if total != m.sum() { wrong += 1 }
    if peak != (&m + &m).max() { wrong += 1 }
    if average != m.mean() { wrong += 1 }
    println("{total} {peak} {product[36, 4]}")
    return wrong
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "match.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    assert_ran(&output, 0, "2395.5 29.0 13.125\n");
}

/// A result stays on the GPU its operands live on, so host code that has no device form
/// refuses it. A host operand beside a device one is copied over rather than refused.
#[cfg(all(feature = "mlir", unix))]
#[test]
fn a_result_stays_on_the_device() {
    for (operation, column) in [
        ("val s = &g + &h", 13),
        ("val s = g.sum(axis: 0)", 13),
        ("val s = g.sort()", 13),
        ("val (s, t) = g.topk(k: 2)", 13),
    ] {
        let source = format!(
            "func main() -> i32 {{\n    val h = Tensor::<f32, [2, 3]>::ones()\n    val g = h.clone().to(Device::GPU(0))\n    {operation}\n    val r = s.map(|v: f32| v * 2.0f32)\n    return 0\n}}\n"
        );
        let test = CompileTest::new();
        let output = run(&test, "resident.nr", &source, false);
        if without_gpu(&output) {
            continue;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with(ON_DEVICE) && stderr.contains(&format!("resident.nr:5:{column}")),
            "`{operation}`: {stderr}"
        );
    }
}

/// A program that transfers but computes on host tensors runs as it did, GPU or not: the
/// kernels it carries are loaded at startup without making a missing GPU fatal.
#[cfg(all(feature = "mlir", unix))]
#[test]
fn host_operands_stay_on_the_host_without_a_gpu() {
    const SOURCE: &str = r#"
func main() -> i32 {
    val a: Tensor<f32, [4, 4]> = Tensor::ones()
    val b = a.to(Device::CPU)
    val c = (&b + &b) * 3.0f32
    println("{c[1, 2]} {c.sum()} {c.max(axis: 0)[3]}")
    return c.sum() as i32
}
"#;
    let test = CompileTest::new();
    for hide_devices in [false, true] {
        assert_ran(
            &run(&test, "host.nr", SOURCE, hide_devices),
            96,
            "6.0 96.0 6.0\n",
        );
    }
}

/// An integer tensor has no device form (the kernels carry no overflow or zero-divisor
/// guard), so its operator refuses a device operand at the operation.
#[cfg(unix)]
#[test]
fn an_integer_operator_refuses_a_device_operand() {
    const SOURCE: &str = r#"
func main() -> i32 {
    val k = Tensor::<i32, [4]>::ones().to(Device::GPU(0))
    val r = &k + &k
    return 0
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "integer.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with(ON_DEVICE) && stderr.contains("integer.nr:4:13"),
        "{stderr}"
    );
}

/// `.sort()`, `.argsort()` and `.topk()` on a device tensor give the host's order exactly:
/// ties keep their positions, a NaN sorts last either way, and an inner axis or an `f64`
/// tensor changes nothing. A mismatch count of zero.
#[cfg(all(feature = "mlir", unix))]
#[test]
fn sorts_on_the_device_match_the_host() {
    const SOURCE: &str = r#"
func same(a: f32, b: f32) -> bool {
    a.is_nan() == b.is_nan() && (a.is_nan() || a == b)
}

func main() -> i32 {
    mut m: Tensor<f32, [9, 7]> = Tensor::zeros()
    mut i = 0
    while i < 9 {
        mut j = 0
        while j < 7 {
            m[i, j] = ((i * 7 + j * 3) % 4) as f32 - 1.5
            if (i + j) % 5 == 0 { m[i, j] = 0.0f32 / 0.0f32 }
            j += 1
        }
        i += 1
    }
    val d: Tensor<f64, [3, 4, 2]> = Tensor::ones()
    val g = m.clone().to(Device::GPU(0))

    val sorted = g.sort().to(Device::CPU)
    val down = g.sort(axis: 0, descending: true).to(Device::CPU)
    val order = (&g + &g).argsort(axis: 0).to(Device::CPU)
    val (gtop, gat) = g.topk(k: 3)
    val top = gtop.to(Device::CPU)
    val at = gat.to(Device::CPU)
    val (_, gdat) = d.clone().to(Device::GPU(0)).topk(k: 2, axis: 1)
    val dat = gdat.to(Device::CPU)

    val (htop, hat) = m.topk(k: 3)
    val (_, hdat) = d.topk(k: 2, axis: 1)
    mut wrong = 0
    i = 0
    while i < 9 {
        mut j = 0
        while j < 7 {
            if !same(sorted[i, j], m.sort()[i, j]) { wrong += 1 }
            if !same(down[i, j], m.sort(axis: 0, descending: true)[i, j]) { wrong += 1 }
            if order[i, j] != (&m + &m).argsort(axis: 0)[i, j] { wrong += 1 }
            if j < 3 && (!same(top[i, j], htop[i, j]) || at[i, j] != hat[i, j]) { wrong += 1 }
            j += 1
        }
        i += 1
    }
    i = 0
    while i < 3 {
        if dat[i, 0, 1] != hdat[i, 0, 1] || dat[i, 1, 0] != hdat[i, 1, 0] { wrong += 1 }
        i += 1
    }
    println("{sorted[0, 0]} {sorted[0, 6]} {order[0, 0]} {top[1, 0]} {at[1, 0]} {dat[2, 1, 1]}")
    return wrong
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "sorts.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    assert_ran(&output, 0, "-1.5 nan 4 1.5 0 1\n");
}

/// A program that sorts only host tensors sorts them on the host, GPU or not.
#[cfg(all(feature = "mlir", unix))]
#[test]
fn a_host_sort_stays_on_the_host_without_a_gpu() {
    const SOURCE: &str = r#"
func main() -> i32 {
    val a: Tensor<f32, [2, 3]> = [[3.0, 1.0, 2.0], [0.5, 0.5, -1.0]]
    val b = a.to(Device::CPU)
    val (top, at) = b.topk(k: 1)
    println("{b.sort()[0, 0]} {b.argsort()[1, 0]} {top[0, 0]} {at[1, 0]}")
    return at[0, 0]
}
"#;
    let test = CompileTest::new();
    for hide_devices in [false, true] {
        assert_ran(
            &run(&test, "host_sort.nr", SOURCE, hide_devices),
            0,
            "1.0 2 3.0 0\n",
        );
    }
}

/// An integer tensor has no device form, a sort included, so it refuses a device receiver.
#[cfg(unix)]
#[test]
fn an_integer_sort_refuses_a_device_operand() {
    const SOURCE: &str = r#"
func main() -> i32 {
    val k = Tensor::<i32, [4]>::ones().to(Device::GPU(0))
    val r = k.sort()
    return 0
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "integer_sort.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with(ON_DEVICE) && stderr.contains("integer_sort.nr:4:13"),
        "{stderr}"
    );
}
