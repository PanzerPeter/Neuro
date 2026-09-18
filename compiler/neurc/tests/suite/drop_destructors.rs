// Drop trait + deterministic destruction tests (Phase 1.7).
//
// Each Drop type below holds a `&mut i32` "sink" and increments it in `drop`, so
// the number of destructor calls is observable through the program's exit code
// once the dropping scope has closed. This exercises scope-exit insertion, LIFO
// order, move elision (a moved value is not double-dropped), and the Copy/Drop
// conflict rule end-to-end.
use crate::compile_harness::CompileTest;

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()`. That assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

fn check_source(source: &str) -> (bool, String) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_path = temp_dir.path().join("test.nr");
    fs::write(&source_path, source).expect("Failed to write source file");

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc check");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stderr)
}

const PROBE: &str = r#"
struct Probe { sink: &mut i32 }

impl Drop for Probe {
    func drop(&mut self) { *self.sink = *self.sink + 1 }
}
"#;

#[test]
fn destructor_runs_once_at_scope_exit() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    {{
        val p = Probe {{ sink: &mut count }}
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_once.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(exit_code, 1, "the destructor must run exactly once");
}

#[test]
fn two_owned_values_drop_twice() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    {{
        val a = Probe {{ sink: &mut count }}
        val b = Probe {{ sink: &mut count }}
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_twice.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(exit_code, 2, "both owned values must be dropped");
}

#[test]
fn moved_value_is_not_double_dropped() {
    // `val q = p` moves `p`; only the new owner `q` is dropped. Without the
    // drop flag this would run the destructor twice.
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    {{
        val p = Probe {{ sink: &mut count }}
        val q = p
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_move.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(exit_code, 1, "a moved value must be dropped exactly once");
}

#[test]
fn a_consumed_receiver_is_dropped_once_by_the_callee() {
    // `p.finish()` hands the receiver to the method, which owns it and destroys it at
    // its own exit. The caller must not drop it again once the method returns.
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
impl Probe {{
    func finish(self) -> i32 {{ 0 }}
}}

func main() -> i32 {{
    mut count: i32 = 0
    {{
        val p = Probe {{ sink: &mut count }}
        val done = p.finish()
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_consumed.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 1,
        "a consumed receiver is dropped exactly once, by the callee"
    );
}

#[test]
fn loop_body_value_drops_each_iteration() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    for i in 0..5 {{
        val p = Probe {{ sink: &mut count }}
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_loop.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 5,
        "a loop-body value is dropped on each iteration"
    );
}

#[test]
fn reassignment_drops_the_prior_value() {
    // The binding's first value loses its owner at the assignment, so its destructor
    // runs there rather than being skipped; the replacement is still dropped at exit.
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    {{
        mut p = Probe {{ sink: &mut count }}
        p = Probe {{ sink: &mut count }}
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_reassign.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 2,
        "the displaced value is dropped, and so is the one that replaced it"
    );
}

#[test]
fn reassignment_after_a_move_drops_only_the_new_value() {
    // `val q = p` already handed the first value away, so the reassignment has nothing
    // to release. The runtime drop flag is what tells the two cases apart.
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    {{
        mut p = Probe {{ sink: &mut count }}
        val q = p
        p = Probe {{ sink: &mut count }}
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_reassign_moved.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 2,
        "a value moved out before the reassignment must not be dropped twice"
    );
}

#[test]
fn reassignment_in_a_loop_drops_each_prior_value() {
    // Four reassignments displace four values, and the fifth leaves at scope exit.
    // Without the per-assignment release this is an unbounded leak in a loop.
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    {{
        mut p = Probe {{ sink: &mut count }}
        for i in 0..4 {{
            p = Probe {{ sink: &mut count }}
        }}
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_reassign_loop.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 5,
        "every displaced value is dropped, plus the last one at scope exit"
    );
}

#[test]
fn a_binding_assigned_from_itself_is_dropped_once() {
    // `p = p` leaves the storage holding what it already held, so releasing the "prior"
    // value would leave the binding pointing at freed memory.
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
func main() -> i32 {{
    mut count: i32 = 0
    {{
        mut p = Probe {{ sink: &mut count }}
        p = p
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_reassign_self.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 1,
        "a self-assignment drops the value exactly once"
    );
}

#[test]
fn copy_and_drop_conflict_is_rejected() {
    let source = r#"
@derive(Copy)
struct Bad { x: i32 }

impl Drop for Bad {
    func drop(&mut self) { }
}

func main() -> i32 { 0 }
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "a Copy type implementing Drop must be rejected");
    assert!(
        stderr.contains("cannot be Copy"),
        "expected the Copy/Drop conflict diagnostic, got: {stderr}"
    );
}

/// Holder types over [`PROBE`], one per position a value can be held in. Each field
/// takes its own sink because a `&mut` borrow is exclusive: two probes counting into
/// one binding would be two live mutable borrows of it.
const HOLDERS: &str = r#"
struct Pair { a: Probe, b: Probe }
struct One { a: Probe }
struct Nest { inner: One }
newtype Boxed = One
enum Slot { Filled(One), Empty }
"#;

#[test]
fn struct_fields_are_dropped_with_their_holder() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}{HOLDERS}
func main() -> i32 {{
    mut a: i32 = 0
    mut b: i32 = 0
    {{
        val h = Pair {{ a: Probe {{ sink: &mut a }}, b: Probe {{ sink: &mut b }} }}
    }}
    return a + b
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_struct_fields.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(exit_code, 2, "both fields are destroyed with the struct");
}

#[test]
fn array_and_tuple_elements_are_dropped_with_their_holder() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}{HOLDERS}
func main() -> i32 {{
    mut a: i32 = 0
    mut b: i32 = 0
    mut t: i32 = 0
    {{
        val arr = [Probe {{ sink: &mut a }}, Probe {{ sink: &mut b }}]
        val pair = (Probe {{ sink: &mut t }}, 7)
    }}
    return a + b + t
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_array_tuple.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 3,
        "every element of both aggregates is destroyed"
    );
}

#[test]
fn an_enum_payload_is_dropped_with_the_active_variant() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}{HOLDERS}
func main() -> i32 {{
    mut filled: i32 = 0
    {{
        val held = Slot::Filled(One {{ a: Probe {{ sink: &mut filled }} }})
        val empty = Slot::Empty
    }}
    return filled
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_enum_payload.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 1,
        "the filled variant's payload is destroyed and the empty one has none"
    );
}

#[test]
fn a_newtype_inner_value_and_a_nested_holder_are_dropped() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}{HOLDERS}
func main() -> i32 {{
    mut boxed: i32 = 0
    mut nested: i32 = 0
    {{
        val b = Boxed(One {{ a: Probe {{ sink: &mut boxed }} }})
        val n = Nest {{ inner: One {{ a: Probe {{ sink: &mut nested }} }} }}
    }}
    return boxed + nested
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_newtype_nested.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 2,
        "a newtype is transparent and a holder's holder is reached too"
    );
}

#[test]
fn a_field_moved_out_is_dropped_once_and_its_sibling_still_dropped() {
    // The partial-move case: giving up `h.a` disowns that position alone, so the
    // binding it moved into destroys it and the holder destroys what it still owns.
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}{HOLDERS}
func main() -> i32 {{
    mut moved: i32 = 0
    mut kept: i32 = 0
    {{
        val h = Pair {{ a: Probe {{ sink: &mut moved }}, b: Probe {{ sink: &mut kept }} }}
        val taken = h.a
    }}
    return moved + kept
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_partial_move.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(exit_code, 2, "each value is destroyed exactly once");
}

#[test]
fn a_displaced_field_value_is_dropped_at_the_field_assignment() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}{HOLDERS}
func main() -> i32 {{
    mut first: i32 = 0
    mut second: i32 = 0
    {{
        mut h = One {{ a: Probe {{ sink: &mut first }} }}
        h.a = Probe {{ sink: &mut second }}
    }}
    return first + second
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_field_assign.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 2,
        "the displaced field goes at the assignment and its replacement at scope exit"
    );
}

#[test]
fn reassigning_a_holder_releases_what_it_held() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}{HOLDERS}
func main() -> i32 {{
    mut first: i32 = 0
    mut second: i32 = 0
    {{
        mut h = One {{ a: Probe {{ sink: &mut first }} }}
        h = One {{ a: Probe {{ sink: &mut second }} }}
    }}
    return first + second
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_holder_reassign.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 2,
        "the displaced holder's field goes at the assignment, the new one at scope exit"
    );
}

#[test]
fn a_holder_moved_into_a_callee_is_dropped_once() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}{HOLDERS}
func swallow(h: One) -> i32 {{ return 0 }}

func main() -> i32 {{
    mut count: i32 = 0
    {{
        val h = One {{ a: Probe {{ sink: &mut count }} }}
        val ignored = swallow(h)
    }}
    return count
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_holder_moved.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 1,
        "the callee owns the holder, so the caller must not release its field too"
    );
}

#[test]
fn a_collection_read_out_of_a_field_is_released_once() {
    // A collection copied out of a field to be read aliases the holder's buffer, so
    // only the holder's drop may free it.
    let test = CompileTest::new();
    let source = r#"
struct Bag { items: Vec<i32> }

func build() -> Bag {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    v.push(2)
    return Bag { items: v }
}

func main() -> i32 {
    val b = build()
    return b.items.len() as i32
}
"#;
    let exit_code = test
        .compile_and_run("drop_field_collection.nr", source)
        .expect("Drop program should compile and run");
    assert_eq!(exit_code, 2, "the field's buffer is read, not double-freed");
}

/// A `match` arm that binds an enum payload takes ownership of it: the scrutinee is
/// disowned at the match, so the arm's binding is the only thing left that can release
/// the payload. Before this was registered, the payload was simply never destroyed.
#[test]
fn a_match_arm_binding_destroys_the_payload_it_takes() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
enum Slot {{ Full(Probe), Empty }}

func main() -> i32 {{
    mut count: i32 = 0
    {{
        val s = Slot::Full(Probe {{ sink: &mut count }})
        val n = match s {{
            Slot::Full(p) => 0,
            Slot::Empty => 1
        }}
    }}
    return count
}}
"#
    );
    let exit = test
        .compile_and_run("match_arm_payload_drop.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 1, "the bound payload is destroyed exactly once");
}

/// Every owning variant whose arm binds is released, and exactly once: the arm that ran
/// is the only one whose binding exists.
#[test]
fn each_owning_variant_is_released_by_the_arm_that_binds_it() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
enum Two {{ A(Probe), B(Probe) }}

func main() -> i32 {{
    mut count: i32 = 0
    {{
        val t = Two::B(Probe {{ sink: &mut count }})
        val n = match t {{
            Two::A(x) => 0,
            Two::B(y) => 1
        }}
    }}
    return count
}}
"#
    );
    let exit = test
        .compile_and_run("match_arm_two_owners.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 1, "the taken arm releases its payload once");
}

/// An arm that MOVES the payload out hands ownership on, so the arm must not release it
/// as well: the binding it flows into is what destroys it, once.
#[test]
fn a_match_arm_that_moves_the_payload_out_does_not_release_it() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
enum Slot {{ Full(Probe), Empty }}

func main() -> i32 {{
    mut count: i32 = 0
    {{
        val s = Slot::Full(Probe {{ sink: &mut count }})
        val kept = match s {{
            Slot::Full(p) => p,
            Slot::Empty => Probe {{ sink: &mut count }}
        }}
    }}
    return count
}}
"#
    );
    let exit = test
        .compile_and_run("match_arm_moved_payload.nr", &source)
        .expect("compile/run failed");
    assert_eq!(
        exit, 1,
        "a moved-out payload is destroyed once, by its new owner"
    );
}
