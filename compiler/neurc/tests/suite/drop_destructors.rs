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

/// Regression test for BUG-060: a binding moved into an enum payload (`Slot::Full(p)`,
/// `Some(xs)`) belongs to the enum, whose drop releases it. The payload was never disowned
/// at the construction, so the binding's own scope released it a second time: the probe
/// counted two drops and a `Vec` payload aborted in `free`.
#[test]
fn test_bug_060_a_binding_moved_into_an_enum_payload_is_released_once() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
enum Slot {{ Full(Probe), Empty }}

func main() -> i32 {{
    mut count: i32 = 0
    {{
        val p = Probe {{ sink: &mut count }}
        val s = Slot::Full(p)
    }}
    mut xs: Vec<i32> = Vec::new()
    xs.push(4)
    val o = Some(xs)
    val n = match o {{
        Some(v) => v.len() as i32,
        None => 0
    }}
    return count * 10 + n
}}
"#
    );
    let exit = test
        .compile_and_run("enum_payload_moved_binding.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 11, "one drop of the probe, and the Vec is freed once");
}

/// An arm that binds nothing leaves the scrutinee whole, so the scrutinee still owns
/// every payload when that arm runs. The match disowns the scrutinee up front whenever
/// SOME arm binds, which left a `B(_)` or `_` arm's payload owned by nobody.
#[test]
fn a_match_arm_that_binds_nothing_leaves_the_payload_to_the_scrutinee() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
enum Two {{ A(Probe), B(Probe), C }}

func main() -> i32 {{
    mut count: i32 = 0
    {{
        val t = Two::B(Probe {{ sink: &mut count }})
        val n = match t {{
            Two::A(x) => 0,
            Two::B(_) => 1,
            Two::C => 2
        }}
        val u = Two::B(Probe {{ sink: &mut count }})
        val m = match u {{
            Two::A(x) => 0,
            _ => 1
        }}
        val v = Two::A(Probe {{ sink: &mut count }})
        val k = match v {{
            Two::A(x) => 0,
            _ => 1
        }}
    }}
    return count
}}
"#
    );
    let exit = test
        .compile_and_run("match_arm_unbound_payload.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 3, "each payload is destroyed exactly once");
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

/// Reading a `Copy` value through an index the compiler cannot evaluate moves nothing, so
/// the holder keeps every owner it holds. A run-time index used to disown the whole
/// binding at any move site, as if the element itself had left, so neither element's
/// destructor ran; the same read at a literal index released both.
#[test]
fn test_bug_061_a_copy_read_through_a_runtime_index_keeps_the_holder_armed() {
    let test = CompileTest::new();
    let source = r#"
struct Tagged { id: i32, sink: &mut i32 }

impl Drop for Tagged {
    func drop(&mut self) { *self.sink = *self.sink + 1 }
}

func twice(x: i32) -> i32 { x * 2 }

func main() -> i32 {
    mut a: i32 = 0
    mut b: i32 = 0
    mut read: i32 = 0
    {
        val hs = [Tagged { id: 3, sink: &mut a }, Tagged { id: 4, sink: &mut b }]
        mut k = 0
        k = k + 1
        val first = hs[k].id
        read = hs[k].id
        read = read + first + twice(hs[k].id)
    }
    return a * 100 + b * 10 + read
}
"#;
    let exit = test
        .compile_and_run("runtime_index_copy_read.nr", source)
        .expect("compile/run failed");
    assert_eq!(
        exit, 126,
        "both elements destroyed once, and 4 + 4 + 8 read"
    );
}

/// A store into an array element or a nested field destroys the value it displaces, as a
/// store into a binding's own field always did. Only that one shape consulted the
/// holder's drop flags, so every other position lost its old value without a destructor.
#[test]
fn test_bug_075_a_displaced_element_or_nested_field_is_dropped() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
struct Pair {{ a: Probe }}
struct Outer {{ p: Pair, xs: [Probe; 2] }}

func main() -> i32 {{
    mut c0: i32 = 0
    mut c1: i32 = 0
    mut c2: i32 = 0
    mut c3: i32 = 0
    mut c4: i32 = 0
    mut c5: i32 = 0
    mut c6: i32 = 0
    mut c7: i32 = 0
    {{
        mut o = Outer {{
            p: Pair {{ a: Probe {{ sink: &mut c0 }} }},
            xs: [Probe {{ sink: &mut c1 }}, Probe {{ sink: &mut c2 }}]
        }}
        mut k = 0
        k = k + 1
        o.p.a = Probe {{ sink: &mut c3 }}
        o.xs[0] = Probe {{ sink: &mut c4 }}
        o.xs[k] = Probe {{ sink: &mut c5 }}
        mut arr: [Probe; 1] = [Probe {{ sink: &mut c6 }}]
        arr[0] = Probe {{ sink: &mut c7 }}
    }}
    if c0 == 1 && c1 == 1 && c2 == 1 && c3 == 1 && c4 == 1 && c5 == 1 && c6 == 1 && c7 == 1 {{
        return 8
    }}
    return c0 + c1 + c2 + c3 + c4 + c5 + c6 + c7
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_displaced_element.nr", &source)
        .expect("Drop program should compile and run");
    assert_eq!(
        exit_code, 8,
        "every displaced value and every replacement is destroyed exactly once"
    );
}

/// A binding that owns nothing (here a `&mut`) shadowing an owner of the same name must
/// hide it from the drop pass. The lookup matched on the name alone, so a field store
/// through the inner borrow released the OUTER owner's field, which was then released
/// again at its own scope exit.
#[test]
fn test_bug_076_a_borrow_shadowing_an_owner_leaves_the_owner_alone() {
    let test = CompileTest::new();
    let source = format!(
        r#"{PROBE}
struct Pair {{ a: Probe }}

func main() -> i32 {{
    mut outer: i32 = 0
    mut other: i32 = 0
    mut fresh: i32 = 0
    {{
        mut p = Pair {{ a: Probe {{ sink: &mut outer }} }}
        mut q = Pair {{ a: Probe {{ sink: &mut other }} }}
        if true {{
            val p = &mut q
            p.a = Probe {{ sink: &mut fresh }}
        }}
    }}
    return outer * 100 + fresh * 10 + other
}}
"#
    );
    let exit_code = test
        .compile_and_run("drop_shadowed_owner.nr", &source)
        .expect("Drop program should compile and run");
    // `other` is displaced through a borrow, which no pass destroys yet (BUG-077), so it
    // stays 0 here; what this test pins is that `outer` is released once, not twice.
    assert_eq!(
        exit_code, 110,
        "the outer owner is released once, at its scope exit"
    );
}

/// A `Drop` value no binding ever owns is destroyed once it has been read: a call whose
/// value is discarded, a temporary a field is read from, a struct literal read the same
/// way, and a temporary `&self` receiver. None of them had an owner a scope exit could
/// reach, so their destructors never ran.
#[test]
fn test_bug_047_an_unbound_drop_temporary_is_destroyed() {
    let test = CompileTest::new();
    let source = r#"
struct Tagged { id: i32, sink: &mut i32 }

impl Drop for Tagged {
    func drop(&mut self) { *self.sink = *self.sink + 1 }
}

impl Tagged {
    func get(&self) -> i32 { self.id }
}

func make(id: i32, sink: &mut i32) -> Tagged { Tagged { id: id, sink: sink } }

func main() -> i32 {
    mut a: i32 = 0
    mut b: i32 = 0
    mut c: i32 = 0
    mut d: i32 = 0
    mut read: i32 = 0
    {
        make(1, &mut a)
        read = read + make(2, &mut b).id
        read = read + Tagged { id: 3, sink: &mut c }.id
        read = read + make(4, &mut d).get()
    }
    if read != 9 { return 100 }
    return a * 8 + b * 4 + c * 2 + d
}
"#;
    let exit = test
        .compile_and_run("unbound_drop_temporary.nr", source)
        .expect("compile/run failed");
    assert_eq!(
        exit, 15,
        "each temporary destroyed exactly once, and 2 + 3 + 4 read"
    );
}
