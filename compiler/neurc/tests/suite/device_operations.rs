// Tensor operations on a device tensor outside `@gpu` run on its device: element reads and
// writes and `.clone()` on every build, and where the MLIR backend can build their kernels the
// operators, compound assignment, reductions, sorts, slices, permutations, `einsum`,
// elementwise math and `.map` / `.zip` / `.reduce`.
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
#[cfg(target_os = "linux")]
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

/// A float `%` on device tensors is the host's exact `fmod`, through the vendor's device math
/// library. The quotients here are far past what a divide, truncate and subtract can recover,
/// which is how the GPU's own `frem` computes it. Without the library the operation stays on the
/// host and refuses the device operand at the `%`.
#[cfg(target_os = "linux")]
#[test]
fn a_float_remainder_on_the_device_is_the_hosts_fmod() {
    const SOURCE: &str = r#"
func main() -> i32 {
    val a: Tensor<f32, [4]> = [1e30, -3.4e38, 16777217.0, 7.5]
    val b: Tensor<f32, [4]> = [3.0, 0.7, 0.1, -2.0]
    val c: Tensor<f64, [3]> = [1e300, -7.25, 123456789.5]
    val d: Tensor<f64, [3]> = [0.3, 2.0, -0.001]
    val ga = a.clone().to(Device::GPU(0))
    val gb = b.clone().to(Device::GPU(0))
    val gc = c.clone().to(Device::GPU(0))
    val gd = d.clone().to(Device::GPU(0))
    val r = (&ga % &gb).to(Device::CPU)
    val s = (&gc % &gd).to(Device::CPU)
    mut wrong = 0
    for i in 0..4 {
        if r[i] != a[i] % b[i] || 1.0f32 / r[i] != 1.0f32 / (a[i] % b[i]) { wrong += 1 }
    }
    for i in 0..3 {
        if s[i] != c[i] % d[i] { wrong += 1 }
    }
    println("{r[3]} {s[1]}")
    return wrong
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "fmod.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.starts_with(ON_DEVICE) && stderr.contains("fmod.nr:11:") {
        return;
    }
    assert_ran(&output, 0, "1.5 -1.25\n");
}

/// A run longer than the 4096 reduction lanes folds in lanes on the device, one GPU thread
/// per lane, in the order the host's lanes take: whole-tensor and axis reductions over long
/// runs (along the last axis and across rows) match the host bit for bit. The values are
/// picked so a left-to-right fold would give a different answer.
#[cfg(target_os = "linux")]
#[test]
fn long_runs_reduce_in_lanes_on_the_device_as_on_the_host() {
    const SOURCE: &str = r#"
func main() -> i32 {
    mut m = Tensor::<f32, [3, 9001]>::zeros()
    mut i = 0
    while i < 3 {
        mut j = 0
        while j < 9001 {
            m[i, j] = (((i * 9001 + j) * 7919) % 10007) as f32 * 0.001f32 + 0.1f32
            j += 1
        }
        i += 1
    }
    m[0, 0] = 16777216.0f32
    val c = m.clone().t()
    mut k = m.clone()
    k[1, 4100] = 0.0f32 / 0.0f32
    k[2, 0] = 0.0f32 / 0.0f32
    val g = m.clone().to(Device::GPU(0))
    val gc = c.clone().to(Device::GPU(0))
    val gk = k.clone().to(Device::GPU(0))

    val rows = g.sum(axis: 1).to(Device::CPU)
    val means = g.mean(axis: 1).to(Device::CPU)
    val columns = gc.sum(axis: 0).to(Device::CPU)
    val peaks = gk.max(axis: 1).to(Device::CPU)
    val lows = gk.min(axis: 1).to(Device::CPU)
    val total = g.sum()
    val average = g.mean()
    val peak = gk.max()

    mut wrong = 0
    i = 0
    while i < 3 {
        if rows[i] != m.sum(axis: 1)[i] { wrong += 1 }
        if means[i] != m.mean(axis: 1)[i] { wrong += 1 }
        if columns[i] != c.sum(axis: 0)[i] { wrong += 1 }
        if peaks[i] != k.max(axis: 1)[i] { wrong += 1 }
        if lows[i] != k.min(axis: 1)[i] { wrong += 1 }
        i += 1
    }
    if total != m.sum() { wrong += 1 }
    if average != m.mean() { wrong += 1 }
    if peak != k.max() { wrong += 1 }
    println("{total} {rows[0]} {columns[2]}")
    return wrong
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "lanes.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    assert_ran(&output, 0, "16915028.0 16823168.0 45939.59375\n");
}

/// A result stays on the GPU its operands live on, so host code that has no device form
/// refuses it. A host operand beside a device one is copied over rather than refused.
#[cfg(target_os = "linux")]
#[test]
fn a_result_stays_on_the_device() {
    for (operation, column) in [
        ("val s = &g + &h", 13),
        ("val s = g.sum(axis: 0)", 13),
        ("val s = g.sort()", 13),
        ("val (s, t) = g.topk(k: 2)", 13),
    ] {
        // A closure reached through a local is a value only the run time knows, so its
        // traversal has no device form.
        let source = format!(
            "func main() -> i32 {{\n    val h = Tensor::<f32, [2, 3]>::ones()\n    val g = h.clone().to(Device::GPU(0))\n    {operation}\n    val r = s.map(f)\n    return 0\n}}\n"
        )
        .replace("func main() -> i32 {\n", "func main() -> i32 {\n    val f = |v: f32| v * 2.0f32\n");
        let test = CompileTest::new();
        let output = run(&test, "resident.nr", &source, false);
        if without_gpu(&output) {
            continue;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with(ON_DEVICE) && stderr.contains(&format!("resident.nr:6:{column}")),
            "`{operation}`: {stderr}"
        );
    }
}

/// A program that transfers but computes on host tensors runs as it did, GPU or not: the
/// kernels it carries are loaded at startup without making a missing GPU fatal.
#[cfg(target_os = "linux")]
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

/// Integer tensors take the device forms float ones do, checks included: operators, `@`,
/// reductions, a sort, `einsum`, a traversal whose closure does integer arithmetic and a
/// compound assignment give the host's answers at both tiers. A mismatch count of zero.
#[cfg(target_os = "linux")]
#[test]
fn integer_operations_on_the_device_match_the_host() {
    const SOURCE: &str = r#"
func mismatch(a: &Tensor<i32, [9, 7]>, b: &Tensor<i32, [9, 7]>) -> i32 {
    mut bad = 0
    mut i = 0
    while i < 9 {
        mut j = 0
        while j < 7 {
            if a[i, j] != b[i, j] { bad += 1 }
            j += 1
        }
        i += 1
    }
    bad
}

func main() -> i32 {
    mut m: Tensor<i32, [9, 7]> = Tensor::zeros()
    mut i = 0
    while i < 9 {
        mut j = 0
        while j < 7 {
            m[i, j] = ((i * 7 + j) % 11) - 5
            j += 1
        }
        i += 1
    }
    val ones: Tensor<i32, [7]> = Tensor::ones()
    val d = ones * 3
    val g = m.clone().to(Device::GPU(0))
    val gd = d.clone().to(Device::GPU(0))
    val host = (&m + &d) * &m - &m / &d
    val dev = ((&g + &gd) * &g - &g / &gd).to(Device::CPU)
    val w: Tensor<i32, [7, 2]> = Tensor::ones()
    val gw = w.clone().to(Device::GPU(0))
    val product = (&m @ &w).sum() - (&g @ &gw).to(Device::CPU).sum()
    val sorted = m.sort(axis: 1)
    val dsorted = g.sort(axis: 1).to(Device::CPU)
    val mapped = m.map(|x: i32| -> i32 { x * 3 - 1 })
    val dmapped = g.map(|x: i32| -> i32 { x * 3 - 1 }).to(Device::CPU)
    val e = einsum("ij,ij->i", &m, &m)
    val de = einsum("ij,ij->i", &g, &g).to(Device::CPU)
    mut hc = m.clone()
    hc *= 2
    mut dc = g.clone()
    dc *= 2
    val back = dc.to(Device::CPU)
    println("{mismatch(&host, &dev)} {product} {m.sum() - g.sum()} {m.max()} {g.min()}")
    println("{mismatch(&sorted, &dsorted)} {mismatch(&mapped, &dmapped)} {e[4] - de[4]} {mismatch(&hc, &back)}")
    return 0
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "integers.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    assert_ran(&output, 0, "0 0 0 5 -5\n0 0 0 0\n");
}

/// A check an integer operation fails on the device aborts at the operation with the host's
/// own diagnostic, as the same operation on host tensors does: an overflow on the debug tier,
/// a zero divisor, a remainder by zero in a traversal's closure.
#[cfg(target_os = "linux")]
#[test]
fn a_failed_integer_check_on_the_device_is_the_hosts_panic() {
    for (operation, message, at) in [
        (
            "val r = (&g + &one).to(Device::CPU)",
            "integer overflow",
            "&g",
        ),
        (
            "val r = (&g / &zeros).to(Device::CPU)",
            "division by zero",
            "&g",
        ),
        (
            "val r = g.map(|x: i32| -> i32 { x % z }).to(Device::CPU)",
            "remainder by zero",
            "x %",
        ),
    ] {
        let source = format!(
            "func main() -> i32 {{\n    val h: Tensor<i32, [3]> = [1, 2147483647, 3]\n    val one: Tensor<i32, [3]> = Tensor::ones()\n    val zeros: Tensor<i32, [3]> = Tensor::zeros()\n    val z = 0\n    val g = h.to(Device::GPU(0))\n    println(\"before\")\n    {operation}\n    return 0\n}}\n"
        );
        let test = CompileTest::new();
        let output = run(&test, "checked.nr", &source, false);
        if without_gpu(&output) {
            return;
        }
        let column = 5 + operation.find(at).expect("the operation's position");
        // Windows writes text-mode `\r\n` and names the source with `\` separators.
        let stderr = String::from_utf8_lossy(&output.stderr)
            .replace("\r\n", "\n")
            .replace('\\', "/");
        assert!(
            stderr.starts_with(&format!("panic: {message} at "))
                && stderr.ends_with(&format!("/checked.nr:8:{column}\n"))
                && stderr.lines().count() == 1,
            "`{operation}`: {stderr}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
            "before\n"
        );
        assert!(!output.status.success());
    }
}

/// `.sort()`, `.argsort()` and `.topk()` on a device tensor give the host's order exactly:
/// ties keep their positions, a NaN sorts last either way, and an inner axis or an `f64`
/// tensor changes nothing. A mismatch count of zero.
#[cfg(target_os = "linux")]
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
#[cfg(target_os = "linux")]
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

/// Slices (stepped, reversed, at a run-time position), a permutation and `einsum` (a
/// contraction, a diagonal, a full contraction) on a device tensor give the host's answer bit
/// for bit. A mismatch count of zero.
#[cfg(target_os = "linux")]
#[test]
fn layouts_and_contractions_on_the_device_match_the_host() {
    const SOURCE: &str = r#"
func main() -> i32 {
    mut m: Tensor<f32, [37, 19]> = Tensor::zeros()
    mut i = 0
    while i < 37 {
        mut j = 0
        while j < 19 {
            m[i, j] = ((i * 19 + j) % 23) as f32 * 0.37f32 - 3.5f32
            j += 1
        }
        i += 1
    }
    val g = m.clone().to(Device::GPU(0))
    val k = 5u64
    val stepped = g[(3..30).step(4), (0..19).rev()].to(Device::CPU)
    val row = g[k, 2..17].to(Device::CPU)
    val flipped = g.clone().t().to(Device::CPU)
    val gram = einsum("ij,kj->ik", &g, &g).to(Device::CPU)
    val diagonal = einsum("ii->i", g[0..19, ..]).to(Device::CPU)
    val total = einsum("ij,ij->", &g, &g)

    val host_gram = einsum("ij,kj->ik", &m, &m)
    val host_stepped = m[(3..30).step(4), (0..19).rev()]
    mut wrong = 0
    i = 0
    while i < 37 {
        mut j = 0
        while j < 19 {
            if flipped[j, i] != m[i, j] { wrong += 1 }
            j += 1
        }
        j = 0
        while j < 37 {
            if gram[i, j] != host_gram[i, j] { wrong += 1 }
            j += 1
        }
        i += 1
    }
    i = 0
    while i < 7 {
        mut j = 0
        while j < 19 {
            if stepped[i, j] != host_stepped[i, j] { wrong += 1 }
            j += 1
        }
        i += 1
    }
    i = 0
    while i < 15 {
        if row[i] != m[5, 2 + i] { wrong += 1 }
        i += 1
    }
    i = 0
    while i < 19 {
        if diagonal[i] != m[i, i] { wrong += 1 }
        i += 1
    }
    if total != einsum("ij,ij->", &m, &m) { wrong += 1 }
    println("{total} {stepped[6, 0]} {row[0]} {gram[36, 36]}")
    return wrong
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "layouts.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    assert_ran(
        &output,
        0,
        "4426.68115234375 -2.759999990463257 -1.649999976158142 131.4180908203125\n",
    );
}

/// A product the GPU computes in register blocks (36 x 44 in blocks of 4 x 4, 6 x 9 in
/// blocks of 3 x 3) gives the bits of the host's, which adds each element's products in
/// contracted order too: a block is independent accumulators, never a split sum.
#[cfg(target_os = "linux")]
#[test]
fn a_blocked_product_on_the_device_matches_the_host() {
    const SOURCE: &str = r#"
func main() -> i32 {
    mut a: Tensor<f32, [36, 20]> = Tensor::zeros()
    mut b: Tensor<f32, [20, 44]> = Tensor::zeros()
    for i in 0..36 {
        for k in 0..20 {
            a[i, k] = ((i * 7 + k * 13) % 23) as f32 * 0.37f32 - 3.1f32
        }
    }
    for k in 0..20 {
        for j in 0..44 {
            b[k, j] = ((k * 5 + j * 3) % 29) as f32 * 0.013f32 + 0.7f32
        }
    }
    val small_a = a[0..6, ..]
    val small_b = b[.., 0..9]
    val ga = a.clone().to(Device::GPU(0))
    val gb = b.clone().to(Device::GPU(0))
    val product = (&ga @ &gb).to(Device::CPU)
    val small = (small_a.clone().to(Device::GPU(0)) @ small_b.clone().to(Device::GPU(0))).to(Device::CPU)
    val host = &a @ &b
    mut wrong = 0
    for i in 0..36 {
        for j in 0..44 {
            if product[i, j] != host[i, j] { wrong += 1 }
        }
    }
    for i in 0..6 {
        for j in 0..9 {
            if small[i, j] != host[i, j] { wrong += 1 }
        }
    }
    println("{product[35, 43]}")
    return wrong
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "blocked_product.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
}

/// `sqrt` and `abs` on a device tensor are exact, as on the host. The transcendental functions
/// are the GPU vendor's device math library, which may differ from the host's C library in the
/// last bits, so they are held to a relative error. A compiler that found no device math
/// library leaves the math to the host, and the device tensor refuses it at the first call.
#[cfg(target_os = "linux")]
#[test]
fn elementwise_math_on_the_device_follows_the_device_library() {
    const SOURCE: &str = r#"
func near(a: f32, b: f32) -> bool {
    val d = (a - b).abs()
    d == 0.0f32 || d <= b.abs() * 0.000001f32
}

func main() -> i32 {
    mut m: Tensor<f32, [37, 19]> = Tensor::zeros()
    mut i = 0
    while i < 37 {
        mut j = 0
        while j < 19 {
            m[i, j] = ((i * 19 + j) % 23) as f32 * 0.37f32 - 3.5f32
            j += 1
        }
        i += 1
    }
    val g = m.clone().to(Device::GPU(0))
    val root = (&g * &g).sqrt().to(Device::CPU)
    val size = g.abs().to(Device::CPU)
    val grown = g.exp().to(Device::CPU)
    val logs = (g.abs() + 1.0f32).log().to(Device::CPU)
    val squashed = g.tanh().to(Device::CPU)
    val power = g.abs().pow(1.5f32).to(Device::CPU)
    mut wrong = 0
    i = 0
    while i < 37 {
        mut j = 0
        while j < 19 {
            val x = m[i, j]
            if root[i, j] != (x * x).sqrt() || size[i, j] != x.abs() { wrong += 1 }
            if !near(grown[i, j], x.exp()) || !near(logs[i, j], (x.abs() + 1.0f32).log()) { wrong += 1 }
            if !near(squashed[i, j], x.tanh()) || !near(power[i, j], x.abs().pow(1.5f32)) { wrong += 1 }
            j += 1
        }
        i += 1
    }
    println("{root[2, 3]} {size[0, 0]}")
    return wrong
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "math.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.starts_with(ON_DEVICE) && stderr.contains("math.nr:19:16") {
        return;
    }
    assert_ran(&output, 0, "3.159999847412109 3.5\n");
}

/// `.map` and `.zip` with capturing closures (one with an early `return` and a loop),
/// `.reduce` in the host's order, and compound assignment into a device target, a host target
/// beside a device value, and a `&mut` parameter, all give the host's answer bit for bit.
#[cfg(target_os = "linux")]
#[test]
fn traversals_and_compound_assignment_on_the_device_match_the_host() {
    const SOURCE: &str = r#"
func step(w: &mut Tensor<f32, [37, 19]>, g: &Tensor<f32, [19]>) {
    *w -= g
}

func main() -> i32 {
    mut m: Tensor<f32, [37, 19]> = Tensor::zeros()
    mut i = 0
    while i < 37 {
        mut j = 0
        while j < 19 {
            m[i, j] = ((i * 19 + j) % 23) as f32 * 0.37f32 - 3.5f32
            j += 1
        }
        i += 1
    }
    val b = Tensor::<f32, [19]>::ones() * 0.3f32
    val g = m.clone().to(Device::GPU(0))
    val gb = b.clone().to(Device::GPU(0))
    val scale = 1.7f32
    val limit = 2u32
    val mapped = g.map(|x: f32| -> f32 {
        mut y = x * scale
        if y > 1.0f32 { return y - 1.0f32 }
        for k in 0u32..limit { y = y * 0.5f32 }
        y
    }).to(Device::CPU)
    val zipped = g.zip(&g, |a: f32, c: f32| -> f32 { a * c - scale }).to(Device::CPU)
    val folded = g.reduce(0.0f32, |acc: f32, x: f32| -> f32 { acc * 0.5f32 + x })

    mut w = m.clone().to(Device::GPU(0))
    w -= &gb
    w *= 2.0f32
    w += &g
    step(&mut w, &gb)
    w /= &g
    mut expected = m.clone()
    expected -= &b
    expected *= 2.0f32
    expected += &m
    expected -= &b
    expected /= &m
    mut beside = m.clone()
    beside += &g
    val updated = w.to(Device::CPU)

    mut wrong = 0
    i = 0
    while i < 37 {
        mut j = 0
        while j < 19 {
            val x = m[i, j]
            mut y = x * scale
            if y > 1.0f32 { y = y - 1.0f32 } else { y = y * 0.5f32 * 0.5f32 }
            if mapped[i, j] != y { wrong += 1 }
            if zipped[i, j] != x * x - scale { wrong += 1 }
            if updated[i, j] != expected[i, j] { wrong += 1 }
            if beside[i, j] != x + x { wrong += 1 }
            j += 1
        }
        i += 1
    }
    if folded != m.reduce(0.0f32, |acc: f32, x: f32| -> f32 { acc * 0.5f32 + x }) { wrong += 1 }
    println("{folded} {mapped[3, 4]} {updated[36, 18]}")
    return wrong
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "traversals.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    assert_ran(
        &output,
        0,
        "1.142077803611755 2.485000371932983 2.042553424835205\n",
    );
}

/// A slice position is checked where the call evaluates it, with the host's guard, so a
/// position past its axis on a device tensor panics exactly as on a host one, on every build.
#[cfg(unix)]
#[test]
fn a_device_slice_past_its_axis_panics_as_on_the_host() {
    const SOURCE: &str = r#"
func main() -> i32 {
    val m: Tensor<f32, [4, 3]> = Tensor::ones()
    val g = m.clone().to(Device::GPU(0))
    val k = -1
    val row = g[k, ..]
    return 0
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "past.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("panic: tensor index out of bounds at ")
            && stderr.contains("past.nr:6:15"),
        "{stderr}"
    );
}

/// What has no device form refuses a device tensor at the operation: a traversal whose
/// function shifts an integer (no device form) or is reached through a local.
#[cfg(unix)]
#[test]
fn what_has_no_device_form_refuses_a_device_tensor() {
    for (operation, column) in [
        (
            "val r = g.map(|x: f32| -> f32 { ((x as i32) << 1) as f32 })",
            13,
        ),
        ("val r = g.map(f)", 13),
    ] {
        let source = format!(
            "struct Layer {{\n    w: Tensor<f32, [2, 3]>\n}}\n\nfunc main() -> i32 {{\n    val f = |v: f32| v\n    val g = Tensor::<f32, [2, 3]>::ones().to(Device::GPU(0))\n    mut layer = Layer {{ w: Tensor::<f32, [2, 3]>::ones().to(Device::GPU(0)) }}\n    {operation}\n    return 0\n}}\n"
        );
        let test = CompileTest::new();
        let output = run(&test, "refused.nr", &source, false);
        if without_gpu(&output) {
            continue;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with(ON_DEVICE) && stderr.contains(&format!("refused.nr:9:{column}")),
            "`{operation}`: {stderr}"
        );
    }
}

/// A tensor inside a struct is reached through its field and updated, reduced and mapped on
/// the device it lives on.
#[cfg(unix)]
#[test]
fn a_field_is_computed_on_its_device() {
    const SOURCE: &str = r#"
struct Layer {
    w: Tensor<f32, [2, 3]>
}

func main() -> i32 {
    val g = Tensor::<f32, [2, 3]>::ones().to(Device::GPU(0))
    mut layer = Layer { w: Tensor::<f32, [2, 3]>::ones().to(Device::GPU(0)) }
    layer.w -= &g
    layer.w += 2.5f32
    val total = layer.w.sum()
    val doubled = layer.w.map(|x: f32| -> f32 { x * 2.0f32 }).to(Device::CPU)
    println("{total} {doubled[1, 2]}")
    return 0
}
"#;
    let test = CompileTest::new();
    let output = run(&test, "field.nr", SOURCE, false);
    if without_gpu(&output) {
        return;
    }
    assert_ran(&output, 0, "15.0 5.0\n");
}

/// A program that transfers but slices, permutes, contracts, maps, folds and updates host
/// tensors only computes them on the host, GPU or not, as it did before any of them had a
/// device form.
#[cfg(unix)]
#[test]
fn host_layouts_traversals_and_updates_stay_on_the_host() {
    const SOURCE: &str = r#"
func main() -> i32 {
    val a: Tensor<f32, [2, 3]> = [[3.0, 1.0, 2.0], [0.5, 0.5, -1.0]]
    val b = a.to(Device::CPU)
    val k = 1u64
    val scale = 2.0f32
    mut w = b.clone()
    w -= b.clone().t().t()
    w += b.exp().log()
    val total = einsum("ij,ij->", &b, &b)
    val doubled = b.map(|x: f32| -> f32 { x * scale })
    val sum = b.reduce(0.0f32, |s: f32, x: f32| -> f32 { s + x })
    println("{b[k, 1..3][1]} {b.clone().t()[2, 1]} {total} {doubled[0, 0]} {sum} {w[0, 2]}")
    return 0
}
"#;
    let test = CompileTest::new();
    for hide_devices in [false, true] {
        assert_ran(
            &run(&test, "host_layouts.nr", SOURCE, hide_devices),
            0,
            "-1.0 -1.0 15.5 6.0 6.0 2.0\n",
        );
    }
}
