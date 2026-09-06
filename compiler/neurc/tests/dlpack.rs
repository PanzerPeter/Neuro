// A tensor value is a DLPack handle. The structure's field values are asserted on
// the emitted IR inside the backend; what these check is that the representation carries a
// real program end to end — every element type allocated and released, a clone producing a
// second handle, and a handle crossing every ownership boundary the language has without a
// double free or a leak.
mod common;

use common::CompileTest;

fn run_program(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

/// Every element type with a DLPack dtype allocates its buffer and releases it through
/// the handle's deleter. Widths differ, so this also exercises the buffer sizing that the
/// 64-byte-aligned allocation is rounded up from.
#[test]
fn every_dlpack_dtype_allocates_and_releases() {
    let source = r#"
func main() -> i32 {
    val a = Tensor::<i8, [4]>::zeros()
    val b = Tensor::<i16, [4]>::zeros()
    val c = Tensor::<i32, [4]>::zeros()
    val d = Tensor::<i64, [4]>::zeros()
    val e = Tensor::<u8, [4]>::zeros()
    val f = Tensor::<u16, [4]>::zeros()
    val g = Tensor::<u32, [4]>::zeros()
    val h = Tensor::<u64, [4]>::zeros()
    val i = Tensor::<f16, [4]>::zeros()
    val j = Tensor::<bf16, [4]>::zeros()
    val k = Tensor::<f32, [4]>::zeros()
    val l = Tensor::<f64, [4]>::zeros()
    return 9
}
"#;
    assert_eq!(run_program("dlpack_dtypes.nr", source), 9);
}

/// A tensor smaller than one alignment unit still gets a whole 64-byte allocation, and
/// only its own elements are copied into it.
#[test]
fn a_tensor_below_the_alignment_unit_round_trips() {
    let source = r#"
func main() -> i32 {
    val scalar: Tensor<f32, []> = Tensor::scalar(42.0)
    val tiny: Tensor<i8, [3]> = [1, 2, 3]
    return 4
}
"#;
    assert_eq!(run_program("dlpack_small.nr", source), 4);
}

/// `.clone()` is the one operation that produces a second handle and a second buffer, so
/// both are released independently and neither release touches the other's memory.
#[test]
fn a_clone_produces_a_second_handle_released_on_its_own() {
    let source = r#"
func main() -> i32 {
    val original = Tensor::<f32, [8, 8]>::identity()
    val copy = original.clone()
    val third = copy.clone()
    return 5
}
"#;
    assert_eq!(run_program("dlpack_clone.nr", source), 5);
}

/// The handle crosses every ownership boundary the language has — a call, a return, a
/// struct field, and a device transfer that hands the same handle on — and is released
/// exactly once at the end of all of it.
#[test]
fn a_handle_survives_every_ownership_boundary() {
    let source = r#"
struct Layer {
    weights: Tensor<f32, [4, 4]>
}

func build() -> Tensor<f32, [4, 4]> {
    return Tensor::<f32, [4, 4]>::identity()
}

func consume(t: Tensor<f32, [4, 4]>) -> i32 {
    val moved = t
    return 1
}

func main() -> i32 {
    val made = build()
    val layer = Layer { weights: made }
    val hosted = Tensor::<f32, [4, 4]>::ones().to(Device::CPU)
    return consume(hosted) + 5
}
"#;
    assert_eq!(run_program("dlpack_ownership.nr", source), 6);
}
