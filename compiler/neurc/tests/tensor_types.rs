// Static tensor type syntax (Phase 2B): `Tensor<T, [d0, ...]>` in every type position.
// Tensor *values* cannot be built yet, so these drive `neurc check` and pin the
// diagnostic a compile attempt produces.
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()` — that assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

fn run(command: &str, source: &str) -> (bool, String) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_path = temp_dir.path().join("test.nr");
    fs::write(&source_path, source).expect("Failed to write source file");

    let output = Command::new(neurc_path())
        .arg(command)
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc");

    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), text)
}

#[test]
fn tensor_annotations_check_in_every_type_position() {
    let source = r#"
type Weights = Tensor<f32, [784, 128]>

struct Layer {
    bias: Tensor<f32, [128]>
}

func forward(w: Weights, x: Tensor<f32, [128]>) -> Tensor<f32, [128]> {
    return x
}

func scalar_loss(l: Tensor<f32, []>) { }

func image(pixels: Tensor<u8, [3, 224, 224]>) { }

func main() -> i32 {
    return 0
}
"#;
    let (ok, out) = run("check", source);
    assert!(ok, "tensor annotations should check; got: {out}");
}

#[test]
fn a_shape_mismatch_is_reported_with_both_shapes() {
    let source = r#"
func takes_square(t: Tensor<f32, [3, 3]>) { }

func pass_through(t: Tensor<f32, [2, 2]>) {
    takes_square(t)
}

func main() -> i32 {
    return 0
}
"#;
    let (ok, out) = run("check", source);
    assert!(!ok, "a shape mismatch must fail; got: {out}");
    assert!(
        out.contains("Tensor<f32, [3, 3]>") && out.contains("Tensor<f32, [2, 2]>"),
        "diagnostic should spell both tensor types; got: {out}"
    );
}

#[test]
fn a_tensor_moves_when_passed() {
    let source = r#"
func consume(t: Tensor<f32, [2, 2]>) { }

func twice(t: Tensor<f32, [2, 2]>) {
    consume(t)
    consume(t)
}

func main() -> i32 {
    return 0
}
"#;
    let (ok, out) = run("check", source);
    assert!(!ok, "a tensor is not Copy; got: {out}");
    assert!(
        out.contains("moved"),
        "diagnostic should name the move; got: {out}"
    );
}

#[test]
fn a_shape_argument_outside_a_tensor_is_a_parse_error() {
    let source = r#"
func f(g: Grid<f32, [2, 2]>) { }

func main() -> i32 {
    return 0
}
"#;
    let (ok, out) = run("check", source);
    assert!(!ok, "a shape argument is tensor-only; got: {out}");
    assert!(
        out.contains("shape argument"),
        "diagnostic should name the shape argument; got: {out}"
    );
}

/// A program using tensor types type-checks but has no runtime form yet. The compile
/// path must say that rather than reporting the type as unresolved.
#[test]
fn compiling_a_tensor_program_reports_the_missing_runtime_form() {
    let source = r#"
func forward(x: Tensor<f32, [4]>) { }

func main() -> i32 {
    return 0
}
"#;
    let (ok, out) = run("compile", source);
    assert!(!ok, "a tensor program cannot be compiled yet; got: {out}");
    assert!(
        out.contains("no runtime representation yet"),
        "diagnostic should explain the limitation; got: {out}"
    );
}
