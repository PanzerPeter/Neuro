// `@kernel` form rules, and the names only a kernel body can see.

use super::semantic_errors;
use crate::errors::TypeError;

const KERNEL: &str = "@kernel(threads: [16, 16])
func add(a: Tensor<f32, [37, 45]>, s: f32, on: bool, out: KernelOut<Tensor<f32, [37, 45]>>) {
    val row = thread_id.x
    val col = thread_id.y
    val block = block_id.z
    if on && row < 37 && col < 45 {
        out[row, col] = a[row, col] * s + (block as f32)
    }
}
";

/// The one `KernelForm` error `src` raises, as its problem and where it points.
fn kernel_error(src: &str) -> (String, usize) {
    let errors = semantic_errors(src);
    let [TypeError::KernelForm { problem, span }] = errors.as_slice() else {
        panic!("expected one KernelForm error for {src:?}, got {errors:?}");
    };
    (problem.clone(), span.start)
}

#[test]
fn a_kernel_reads_its_grid_position_and_writes_its_output() {
    let errors = semantic_errors(KERNEL);
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_malformed_threads_argument_is_refused_at_the_attribute() {
    for attribute in [
        "@kernel",
        "@kernel(threads: 16)",
        "@kernel(threads: [])",
        "@kernel(threads: [0, 16])",
        "@kernel(threads: [2, 2, 2, 2])",
        "@kernel(threads: [n, 16])",
        "@kernel(blocks: [16, 16])",
        "@kernel(threads: [16, 16], threads: [16, 16])",
        "@kernel(threads: [64, 32])",
    ] {
        let src = KERNEL.replace("@kernel(threads: [16, 16])", attribute);
        let (_, at) = kernel_error(&src);
        assert_eq!(at, 0, "{attribute}");
    }
}

#[test]
fn threads_needs_one_entry_per_axis_of_the_grid_tensor() {
    let src = KERNEL.replace("[16, 16]", "[256]");
    let (problem, at) = kernel_error(&src);
    assert_eq!(at, 0);
    assert!(problem.contains("rank-2"), "{problem}");
}

#[test]
fn a_kernel_signature_is_checked_where_it_goes_wrong() {
    for (src, needle) in [
        (
            "@kernel(threads: [4])\nfunc k(out: KernelOut<Tensor<f32, [4]>>) -> i32 {\n    0\n}\n",
            "k(",
        ),
        (
            "@kernel(threads: [4])\nfunc k(a: &Tensor<f32, [4]>, out: KernelOut<Tensor<f32, [4]>>) {}\n",
            "a:",
        ),
        (
            "@kernel(threads: [4])\nfunc k(out: &mut Tensor<f32, [4]>) {}\n",
            "out:",
        ),
        (
            "@kernel(threads: [4])\nfunc k(a: Tensor<f32, [?]>, out: KernelOut<Tensor<f32, [4]>>) {}\n",
            "a:",
        ),
        (
            "@kernel(threads: [4])\nfunc k(o: KernelOut<f32>, out: KernelOut<Tensor<f32, [4]>>) {}\n",
            "KernelOut<f32>",
        ),
        (
            "@kernel(threads: [4])\nfunc k(t: string, out: KernelOut<Tensor<f32, [4]>>) {}\n",
            "t:",
        ),
        (
            "@kernel(threads: [4])\nfunc k(a: Tensor<f32, [4]>) {}\n",
            "k(",
        ),
    ] {
        let (_, at) = kernel_error(src);
        assert_eq!(at, src.find(needle).expect("needle in source"), "{src}");
    }
}

#[test]
fn a_kernel_is_refused_on_a_method_and_beside_gpu_or_grad() {
    let src =
        "struct S { x: f32 }\nimpl S {\n    @kernel(threads: [4])\n    func get(&self) {}\n}\n";
    let (problem, at) = kernel_error(src);
    assert!(problem.starts_with("on a method"), "{problem}");
    assert_eq!(at, src.find("@kernel").expect("attribute"));

    let src = format!("@gpu\n{KERNEL}");
    let (problem, at) = kernel_error(&src);
    assert!(problem.contains("`@gpu`"), "{problem}");
    assert_eq!(at, src.find("@kernel").expect("attribute"));

    let src = format!("@grad\n{KERNEL}");
    let errors = semantic_errors(&src);
    assert!(
        errors.iter().any(|error| matches!(
            error,
            TypeError::KernelForm { problem, .. } if problem.contains("`@grad`")
        )),
        "got {errors:?}"
    );
}

#[test]
fn grid_positions_are_read_one_axis_at_a_time() {
    let src = KERNEL.replace("block_id.z", "block_id");
    let (_, at) = kernel_error(&src);
    assert_eq!(at, src.find("block_id").expect("name"));

    let src = KERNEL.replace("block_id.z", "block_id.w");
    let (_, at) = kernel_error(&src);
    assert_eq!(at, src.find(".w").expect("field") + 1);
}

#[test]
fn grid_positions_exist_only_in_a_kernel_body_and_yield_to_a_local() {
    let errors = semantic_errors("func host() -> u32 {\n    thread_id.x\n}\n");
    assert!(
        matches!(errors.as_slice(), [TypeError::UndefinedVariable { name, .. }] if name == "thread_id"),
        "got {errors:?}"
    );

    // A local of the same name is an ordinary binding, read whole like any other.
    let src = KERNEL.replace(
        "    val block = block_id.z\n",
        "    val block_id = 3u32\n    val block = block_id\n",
    );
    let errors = semantic_errors(&src);
    assert!(errors.is_empty(), "got {errors:?}");
}

/// `KernelOut` names a kernel parameter's whole type and nothing else.
#[test]
fn kernel_out_is_only_a_kernel_parameter_type() {
    for src in [
        "func host(out: KernelOut<Tensor<f32, [4]>>) {}\n",
        "struct S { out: KernelOut<Tensor<f32, [4]>> }\n",
        "@kernel(threads: [4])\nfunc k(out: KernelOut<Tensor<f32, [4]>>, o: [KernelOut<Tensor<f32, [4]>>; 1]) {}\n",
    ] {
        let errors = semantic_errors(src);
        assert!(
            errors.iter().any(|error| matches!(
                error,
                TypeError::KernelForm { problem, .. } if problem.contains("only a kernel parameter's type")
            )),
            "{src}: got {errors:?}"
        );
    }
}

/// An output is written element by element; the handle itself never leaves the body.
#[test]
fn a_kernel_out_handle_is_only_indexed() {
    let with = |line: &str| {
        KERNEL.replace(
            "    val block = block_id.z\n",
            &format!("    val block = block_id.z\n{line}\n"),
        )
    };
    let fine = with("    out[0, 0] += out[1, 1]");
    let errors = semantic_errors(&fine);
    assert!(errors.is_empty(), "got {errors:?}");

    for line in [
        "    val o = out",
        "    val o = &out",
        "    sink(out)",
        "    val f = |i: i64| out[0, 0]",
    ] {
        let src = format!(
            "func sink(t: &mut Tensor<f32, [37, 45]>) {{}}\n{}",
            with(line)
        );
        let errors = semantic_errors(&src);
        assert!(!errors.is_empty(), "{line}: accepted");
        assert!(
            errors.iter().any(|error| matches!(
                error,
                TypeError::KernelForm { problem, span } if problem.contains("output 'out'")
                    && span.start == src.find(line).expect("line") + line.rfind("out").expect("use")
            )),
            "{line}: got {errors:?}"
        );
    }
}

/// A bare `Tensor` input is borrowed by the call: the caller keeps it, may not pass it
/// with `&` too, and may not lend it while the same call writes it.
#[test]
fn a_kernel_call_borrows_its_inputs() {
    let call = |args: &str| {
        format!(
            "{KERNEL}func main() -> i32 {{\n    mut a: Tensor<f32, [37, 45]> = Tensor::zeros()\n    mut r: Tensor<f32, [37, 45]> = Tensor::zeros()\n    add({args})\n    add({args})\n    return 0\n}}\n"
        )
    };
    let errors = semantic_errors(&call("a, 2.0, true, &mut r"));
    assert!(errors.is_empty(), "got {errors:?}");

    let src = call("a, 2.0, true, &mut r").replacen("    add(a", "    val f = add\n    add(a", 1);
    let (problem, at) = kernel_error(&src);
    assert!(problem.contains("called by name"), "{problem}");
    assert_eq!(at, src.find("= add").expect("value") + 2);

    let src = call("&a, 2.0, true, &mut r");
    let errors = semantic_errors(&src);
    assert!(
        errors.iter().any(|error| matches!(
            error,
            TypeError::KernelForm { problem, .. } if problem.contains("pass the tensor, not `&`")
        )),
        "got {errors:?}"
    );

    let errors = semantic_errors(&call("r, 2.0, true, &mut r"));
    assert!(
        errors.iter().any(|error| matches!(
            error,
            TypeError::CannotMutablyBorrowWhileBorrowed { .. }
                | TypeError::CannotBorrowWhileMutablyBorrowed { .. }
        )),
        "got {errors:?}"
    );
}
