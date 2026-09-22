// End-to-end coverage for `pool { }` arena blocks.
//
// The arena is not directly observable from a Neuro program: it has no surface, and a
// correct run and a leaking one print the same thing. What these tests pin is that the
// values a pool body builds are correct while the block runs and stay correct across a
// sibling and a nested block, which is what breaks first if the mark, the restore, or
// the release wrapper is wrong. The arena's shape is asserted on the IR, in
// `llvm-backend`'s own tests.
//
// The `PoolAware` sweep is the exception: it IS observable, because the order the arena
// calls `bulk_release` in is the order the program prints in. Those tests read stdout.
use crate::compile_harness::CompileTest;
use std::process::Command;

/// Compile and run `source`, returning its standard output with line endings
/// normalized (fd 1 is a text-mode descriptor on Windows, and these tests assert which
/// lines were written rather than the platform's line-ending policy).
fn stdout_of(test: &CompileTest, filename: &str, source: &str) -> String {
    let source_path = test.write_source(filename, source);
    let exe = test.compile(&source_path).expect("compile failed");
    let output = Command::new(&exe).output().expect("run failed");
    String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n")
}

/// A type that is both `Drop` and `PoolAware`, printing from all three hooks so a test
/// can tell which of them the arena actually ran.
const TRACED_HANDLE: &str = r#"
struct Handle {
    id: i32
}

impl Drop for Handle {
    func drop(&mut self) {
        println("drop {self.id}")
    }
}

impl PoolAware for Handle {
    func register_with_pool(&self, arena: &PoolHandle) {
        println("register {self.id}")
    }
    func bulk_release(&mut self) {
        println("release {self.id}")
    }
}
"#;

#[test]
fn a_pool_block_runs_its_body() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total: i32 = 0
    pool {
        val label = "batch " + "one"
        total = total + label.len() as i32
    }
    total
}
"#;
    let exit = test
        .compile_and_run("pool_body.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}

#[test]
fn a_labeled_pool_runs_the_same() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total: i32 = 0
    pool training {
        val note = "abcd"
        total = note.len() as i32
    }
    total
}
"#;
    let exit = test
        .compile_and_run("pool_label.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn sibling_pools_reuse_the_arena_without_corrupting_each_other() {
    // The second block starts from the mark the first restored, so it allocates over
    // the first block's bytes. Its own values must still read back correctly.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total: i32 = 0
    pool first {
        val a = "aaaa" + "aaaa"
        total = total + a.len() as i32
    }
    pool second {
        val b = "bb" + "bb"
        total = total + b.len() as i32
    }
    total
}
"#;
    let exit = test
        .compile_and_run("pool_siblings.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 12);
}

#[test]
fn an_outer_pools_values_survive_a_nested_pool() {
    // The inner block's restore must not reclaim what the outer block allocated
    // before it: an inner mark above the outer one is the whole nesting rule.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total: i32 = 0
    pool outer {
        val kept = "keep" + "me"
        pool inner {
            val scratch = "scratchscratch"
            total = total + scratch.len() as i32
        }
        total = total + kept.len() as i32
    }
    total
}
"#;
    let exit = test
        .compile_and_run("pool_nested.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 20);
}

#[test]
fn a_pool_inside_a_loop_releases_every_iteration() {
    // Each iteration allocates a tensor larger than the arena would hold a hundred
    // times over, so the run only completes on the arena if the mark is restored.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut rounds: i32 = 0
    for epoch in 0..100 {
        pool batch {
            val weights = Tensor::<f32, [512, 512]>::zeros()
            rounds = rounds + 1
        }
    }
    rounds
}
"#;
    let exit = test
        .compile_and_run("pool_loop.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 100);
}

#[test]
fn a_collection_built_inside_a_pool_reads_back_correctly() {
    // A map's table comes from the arena inside a block, and its growth path frees the
    // old table: that release has to recognize arena memory and do nothing.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total: i32 = 0
    pool {
        mut counts: HashMap<i32, i32> = HashMap::new()
        for i in 0..32 {
            counts.insert(i, i * 2)
        }
        total = counts.len() as i32
    }
    total
}
"#;
    let exit = test
        .compile_and_run("pool_map.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 32);
}

#[test]
fn a_value_built_into_an_outer_binding_outlives_the_arena() {
    // The routing rule end to end. `kept` is declared before the block, so the
    // concatenation written inside it is emitted with the arena switched off. The
    // second pool then reuses the very bytes the first released: if the buffer had
    // come from the arena, the line printed last would be the filler's, not the
    // survivor's.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut kept: string = "none"
    pool first {
        kept = "survivor-" + "{42}"
    }
    pool second {
        mut i: i32 = 0
        while i < 32 {
            val filler = "clobber-clobber-clobber-{i}"
            println("{filler.len()}")
            i = i + 1
        }
    }
    println("kept: {kept}")
    0
}
"#;
    let stdout = stdout_of(&test, "pool_routed_store.nr", source);
    let last = stdout.lines().next_back().unwrap_or_default();
    assert_eq!(last, "kept: survivor-42", "unexpected stdout: {stdout}");
}

#[test]
fn storing_the_blocks_own_allocation_into_an_outer_binding_is_rejected() {
    // The limit of the routing: a store can be emitted off the arena, but it cannot
    // move a buffer the block already took from it.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut out: string = ""
    pool {
        val local = "a" + "b"
        out = local
    }
    0
}
"#;
    let error = test
        .check("pool_escape.nr", source)
        .expect_err("carrying arena memory past the block must not compile");
    assert!(error.contains("outlives"), "unexpected diagnostic: {error}");
}

#[test]
fn a_value_a_callee_built_survives_the_pool() {
    // The counterpart to the test above, and the reason the escape rule reads the VALUE
    // and not only the place. `render` is emitted as its own function, outside every
    // arena, so what it returns is heap memory that the block's release does not touch.
    // Reading it back after the closing brace is what proves the routing is real.
    let test = CompileTest::new();
    let source = r#"
func render(n: i32) -> string {
    "row {n}"
}

func main() -> i32 {
    mut out: string = ""
    pool scratch {
        out = render(7)
    }
    println("{out}")
    out.len() as i32
}
"#;
    let stdout = stdout_of(&test, "pool_callee_value.nr", source);
    assert_eq!(stdout, "row 7\n", "unexpected stdout: {stdout}");
}

#[test]
fn a_value_from_a_dyn_call_may_not_cross_the_pool_boundary() {
    // The named case for the conservative fallback: behind a trait object the callee is
    // not known at compile time, so neither is what it allocates or who owns it.
    let test = CompileTest::new();
    let source = r#"
trait Namer {
    func name(&self) -> string
}

struct Plain {
    tag: i32
}

impl Namer for Plain {
    func name(&self) -> string {
        "plain"
    }
}

func main() -> i32 {
    val p = Plain { tag: 1 }
    val d: &dyn Namer = &p
    mut out: string = ""
    pool scratch {
        out = d.name()
    }
    0
}
"#;
    let error = test
        .check("pool_dyn_escape.nr", source)
        .expect_err("a dyn call's result has no provable owner");
    assert!(error.contains("outlives"), "unexpected diagnostic: {error}");
}

#[test]
fn the_same_call_on_a_concrete_receiver_compiles() {
    // The pair to the test above. Both call the same method body; only the dispatch
    // differs, which is what makes the rejection a statement about `dyn` rather than
    // about the method.
    let test = CompileTest::new();
    let source = r#"
trait Namer {
    func name(&self) -> string
}

struct Plain {
    tag: i32
}

impl Namer for Plain {
    func name(&self) -> string {
        "plain"
    }
}

func main() -> i32 {
    val p = Plain { tag: 1 }
    mut out: string = ""
    pool scratch {
        out = p.name()
    }
    println("{out}")
    0
}
"#;
    let stdout = stdout_of(&test, "pool_static_ok.nr", source);
    assert_eq!(stdout, "plain\n", "unexpected stdout: {stdout}");
}

#[test]
fn a_drop_only_value_reaching_a_pool_through_dyn_is_rejected() {
    // The other half of the same fallback: the arena cannot prove it does not own what
    // a vtable call handed back, so a `Drop`-only value arriving that way is refused at
    // the call site rather than silently registered for a per-object destructor.
    let test = CompileTest::new();
    let source = r#"
struct Handle {
    id: i32
}

impl Drop for Handle {
    func drop(&mut self) {
        println("released {self.id}")
    }
}

trait Factory {
    func make(&self) -> Handle
}

struct RealFactory {
    seed: i32
}

impl Factory for RealFactory {
    func make(&self) -> Handle {
        Handle { id: self.seed }
    }
}

func main() -> i32 {
    val f = RealFactory { seed: 2 }
    val d: &dyn Factory = &f
    pool scratch {
        val h = d.make()
    }
    0
}
"#;
    let error = test
        .check("pool_dyn_drop_only.nr", source)
        .expect_err("a Drop-only value through a vtable is still owned by the block");
    assert!(
        error.contains("implements 'Drop'") && error.contains("'scratch'"),
        "unexpected diagnostic: {error}"
    );
}

#[test]
fn a_return_inside_a_pool_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    pool {
        return 1
    }
}
"#;
    let error = test
        .check("pool_return.nr", source)
        .expect_err("a return jumps past the arena release");
    assert!(
        error.contains("may not leave"),
        "unexpected diagnostic: {error}"
    );
}

#[test]
fn a_drop_only_value_owned_by_a_pool_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Handle {
    id: i32
}

impl Drop for Handle {
    func drop(&mut self) {
        println("released {self.id}")
    }
}

func main() -> i32 {
    pool scratch {
        val h = Handle { id: 1 }
    }
    0
}
"#;
    let error = test
        .check("pool_drop_only.nr", source)
        .expect_err("an arbitrary destructor cannot run under a single-store release");
    assert!(
        error.contains("'PoolAware'") && error.contains("'scratch'"),
        "unexpected diagnostic: {error}"
    );
}

#[test]
fn a_drop_only_value_returned_by_a_call_names_that_function() {
    // The rejection has to reach a value the block owns wherever it was built, and to
    // say which function built it.
    let test = CompileTest::new();
    let source = r#"
struct Handle {
    id: i32
}

impl Drop for Handle {
    func drop(&mut self) {
        println("released {self.id}")
    }
}

func open(id: i32) -> Handle {
    Handle { id: id }
}

func main() -> i32 {
    pool {
        val h = open(4)
    }
    0
}
"#;
    let error = test
        .check("pool_drop_only_call.nr", source)
        .expect_err("a value a callee built is still owned by the block");
    assert!(
        error.contains("returned by 'open'"),
        "unexpected diagnostic: {error}"
    );
}

#[test]
fn a_pool_aware_type_runs_inside_a_pool() {
    let test = CompileTest::new();
    let source = r#"
struct Handle {
    id: i32
}

impl Drop for Handle {
    func drop(&mut self) {
        println("released {self.id}")
    }
}

impl PoolAware for Handle {
    func register_with_pool(&self, arena: &PoolHandle) {
    }
    func bulk_release(&mut self) {
    }
}

func main() -> i32 {
    mut seen: i32 = 0
    pool scratch {
        val h = Handle { id: 7 }
        seen = h.id
    }
    seen
}
"#;
    let exit = test
        .compile_and_run("pool_aware.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn a_drop_only_value_built_before_the_pool_is_untouched() {
    // The rule is about what the block owns. Something built outside it never comes
    // from the arena, so its ordinary destructor still applies.
    let test = CompileTest::new();
    let source = r#"
struct Handle {
    id: i32
}

impl Drop for Handle {
    func drop(&mut self) {
        println("released {self.id}")
    }
}

func main() -> i32 {
    val outer = Handle { id: 5 }
    mut seen: i32 = 0
    pool {
        seen = outer.id
    }
    seen
}
"#;
    let exit = test
        .compile_and_run("pool_drop_outside.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}

#[test]
fn a_pool_registers_at_construction_and_sweeps_in_reverse() {
    // The whole guarantee in one program: `register_with_pool` runs where the value is
    // built, `bulk_release` runs at the closing brace in reverse registration order,
    // and the ordinary destructor does not run at all inside the block.
    let test = CompileTest::new();
    let source = format!(
        "{TRACED_HANDLE}
func main() -> i32 {{
    pool scratch {{
        val first = Handle {{ id: 1 }}
        val second = Handle {{ id: 2 }}
        val third = Handle {{ id: 3 }}
        println(\"body\")
    }}
    println(\"after\")
    0
}}"
    );
    let printed = stdout_of(&test, "pool_sweep_order.nr", &source);
    assert_eq!(
        printed,
        "register 1\nregister 2\nregister 3\nbody\nrelease 3\nrelease 2\nrelease 1\nafter\n"
    );
}

#[test]
fn a_pool_aware_value_outside_a_pool_still_runs_its_destructor() {
    // The trait is an arena opt-in, not a replacement for `Drop`: with no pool open
    // there is no registration list to join and the destructor is all that is owed.
    let test = CompileTest::new();
    let source = format!(
        "{TRACED_HANDLE}
func main() -> i32 {{
    val lone = Handle {{ id: 9 }}
    println(\"body\")
    0
}}"
    );
    let printed = stdout_of(&test, "pool_aware_outside.nr", &source);
    assert_eq!(printed, "body\ndrop 9\n");
}

#[test]
fn a_nested_pool_sweeps_only_what_it_registered() {
    // The inner block stops at the head the outer block saved, so the outer block's
    // registrations survive it and are swept, still newest first, one brace later.
    let test = CompileTest::new();
    let source = format!(
        "{TRACED_HANDLE}
func main() -> i32 {{
    pool outer {{
        val kept = Handle {{ id: 1 }}
        pool inner {{
            val scratch = Handle {{ id: 2 }}
        }}
        println(\"between\")
    }}
    0
}}"
    );
    let printed = stdout_of(&test, "pool_sweep_nested.nr", &source);
    assert_eq!(
        printed,
        "register 1\nregister 2\nrelease 2\nbetween\nrelease 1\n"
    );
}

#[test]
fn a_registered_value_moved_into_another_binding_is_released_once() {
    // Registration reuses the binding's drop flag, so a move clears the source exactly
    // as it does on the ordinary drop path and the sweep passes over it.
    let test = CompileTest::new();
    let source = format!(
        "{TRACED_HANDLE}
func main() -> i32 {{
    pool scratch {{
        val original = Handle {{ id: 4 }}
        val moved = original
        println(\"moved {{moved.id}}\")
    }}
    0
}}"
    );
    let printed = stdout_of(&test, "pool_sweep_moved.nr", &source);
    assert_eq!(printed, "register 4\nmoved 4\nrelease 4\n");
}

/// A `&mut self` method is a channel into memory the caller holds. When the receiver
/// was declared before the block, handing it something the arena allocated leaves that
/// store dangling at the closing brace, and no rule reading the block's own text can
/// see it: the assignment is written in the callee.
#[test]
fn a_callee_may_not_retain_a_value_the_pool_allocated() {
    let test = CompileTest::new();
    let source = r#"
struct Box {
    s: string
}

impl Box {
    func stash(&mut self, v: string) {
        self.s = v
    }
}

func main() -> i32 {
    mut b = Box { s: "" }
    pool scratch {
        b.stash("a" + "b")
    }
    println(b.s)
    0
}
"#;
    let err = test
        .check("pool_callee_retains.nr", source)
        .expect_err("storing an arena value through a callee should be rejected");
    assert!(
        err.contains("'stash'") && err.contains("'b'") && err.contains("'scratch'"),
        "diagnostic should name the callee, the place and the pool, got: {err}"
    );
}

/// The same call with text the compiler can trace to the heap: a declared function's
/// body is emitted outside every arena, so what it returns survives the block and the
/// receiver may keep it.
#[test]
fn a_callee_may_retain_a_value_the_pool_did_not_allocate() {
    let test = CompileTest::new();
    let source = r#"
struct Box {
    s: string
}

impl Box {
    func stash(&mut self, v: string) {
        self.s = v
    }
}

func label(n: i32) -> string {
    return "row {n}"
}

func main() -> i32 {
    mut b = Box { s: "" }
    pool scratch {
        b.stash(label(7))
    }
    println(b.s)
    0
}
"#;
    let code = test
        .compile_and_run("pool_callee_keeps_heap.nr", source)
        .expect("a heap-traced value should still cross the boundary");
    assert_eq!(code, 0);
}

/// Place expressions made a plain `&mut` parameter assignable, so `&mut self` stopped
/// being the only spelling that reaches the caller's memory. Both are the same channel.
#[test]
fn a_mut_reference_parameter_may_not_retain_a_pool_value_either() {
    let test = CompileTest::new();
    let source = r#"
struct Box {
    s: string
}

func stash_into(target: &mut Box, v: string) {
    target.s = v
}

func main() -> i32 {
    mut b = Box { s: "" }
    pool scratch {
        stash_into(&mut b, "a" + "b")
    }
    println(b.s)
    0
}
"#;
    let err = test
        .check("pool_param_retains.nr", source)
        .expect_err("a &mut parameter should be refused the same value");
    assert!(
        err.contains("'stash_into'") && err.contains("'b'"),
        "diagnostic should name the callee and the place, got: {err}"
    );
}

/// A second pool that fills the arena with fresh text, so a buffer the first pool left
/// dangling reads back as filler instead of what was stored in it.
const CLOBBER_POOL: &str = r#"
    pool second {
        mut i: i32 = 0
        while i < 32 {
            val filler = "clobber-clobber-clobber-{i}"
            if filler.len() == 0 { println("x") }
            i = i + 1
        }
    }
"#;

/// An enum payload and a newtype's inner value are positions that can hold a `string`,
/// so an enum or newtype is not pointerless just because its name is nominal.
#[test]
fn regression_bug_041_an_enum_payload_may_not_carry_a_pool_value_out() {
    let test = CompileTest::new();
    let stores = [
        ("mut o: Option<string> = None", "o = Some(local)"),
        ("mut o: Result<string, i32> = Err(0)", "o = Ok(local)"),
        ("mut o = Msg::Empty", "o = Msg::Text(local)"),
        ("mut o = Name(\"none\")", "o = Name(local)"),
    ];
    for (i, (decl, store)) in stores.iter().enumerate() {
        let source = format!(
            r#"
enum Msg {{ Text(string), Empty }}
newtype Name = string
func main() -> i32 {{
    {decl}
    pool first {{
        val local = "survivor-" + "{{42}}"
        {store}
    }}
    0
}}
"#
        );
        let err = test
            .check(&format!("pool_enum_escape_{i}.nr"), &source)
            .expect_err("a payload the block allocated must not outlive it");
        assert!(
            err.contains("outlives"),
            "`{store}`: unexpected diagnostic: {err}"
        );
    }
}

/// The other half of the enum rule: a unit variant carries nothing, and a payload built
/// in a routed store comes from the heap, so both still cross the boundary.
#[test]
fn an_enum_value_built_off_the_arena_still_crosses_the_pool() {
    let test = CompileTest::new();
    let source = format!(
        r#"
enum Msg {{ Text(string), Empty }}
func main() -> i32 {{
    mut o: Option<string> = Some("none")
    mut m = Msg::Text("none")
    pool first {{
        o = None
        m = Msg::Empty
        o = Some("a" + "b")
    }}
{CLOBBER_POOL}
    match o {{ Some(s) => println("{{s}}"), None => println("none") }}
    match m {{ Msg::Text(s) => println("{{s}}"), Msg::Empty => println("empty") }}
    0
}}
"#
    );
    let stdout = stdout_of(&test, "pool_enum_routed.nr", &source);
    assert_eq!(stdout, "ab\nempty\n", "unexpected stdout: {stdout}");
}

/// `push` and `insert` store their argument into the receiver exactly as a `&mut self`
/// method would, so an outliving collection is the same channel `stash` is.
#[test]
fn regression_bug_042_a_collection_may_not_keep_a_pool_value() {
    let test = CompileTest::new();
    let stores = [
        ("mut v: Vec<string> = Vec::new()", "v.push(local)"),
        ("mut v: Vec<string> = Vec::new()", "v.push(\"a\" + \"b\")"),
        (
            "mut m: HashMap<i32, string> = HashMap::new()",
            "m.insert(1, local)",
        ),
    ];
    for (i, (decl, store)) in stores.iter().enumerate() {
        let source = format!(
            r#"
func main() -> i32 {{
    {decl}
    pool scratch {{
        val local = "survivor-" + "{{42}}"
        {store}
    }}
    0
}}
"#
        );
        let err = test
            .check(&format!("pool_collection_retains_{i}.nr"), &source)
            .expect_err("an outliving collection must not keep arena memory");
        assert!(
            err.contains("'scratch'") && (err.contains("'v'") || err.contains("'m'")),
            "`{store}`: diagnostic should name the place and the pool, got: {err}"
        );
    }
}

/// An `i32` has no address to leave behind, whatever expression computed it: the bound
/// form `val x = i + 1` was always accepted, so the inline one must be too.
#[test]
fn regression_bug_043_a_pointerless_argument_is_not_arena_memory() {
    let test = CompileTest::new();
    let source = r#"
func add_to(t: &mut i32, x: i32) { *t = *t + x }
func main() -> i32 {
    mut total: i32 = 0
    mut v: Vec<i32> = Vec::new()
    pool first {
        mut i: i32 = 0
        while i < 4 {
            add_to(&mut total, i + 1)
            v.push((i * 10) as i32)
            i = i + 1
        }
    }
    mut s: i32 = 0
    for x in v { s = s + x }
    total + s
}
"#;
    let code = test
        .compile_and_run("pool_scalar_argument.nr", source)
        .expect("a scalar argument carries no arena memory");
    assert_eq!(code, 70);
}

/// A generic callee's `&mut T` is the same channel a concrete one's `&mut string` is;
/// the template not being in the function table does not make it any less of one.
#[test]
fn regression_bug_044_a_generic_callee_may_not_retain_a_pool_value() {
    let test = CompileTest::new();
    let source = r#"
func put<T>(slot: &mut T, x: T) { *slot = x }
func main() -> i32 {
    mut out: string = "none"
    pool scratch {
        val local = "survivor-" + "{42}"
        put(&mut out, local)
    }
    0
}
"#;
    let err = test
        .check("pool_generic_retains.nr", source)
        .expect_err("a generic callee must not keep arena memory either");
    assert!(
        err.contains("'put'") && err.contains("'out'"),
        "diagnostic should name the callee and the place, got: {err}"
    );
}

/// A map's table is the map's own buffer. Growing an outliving map inside a pool must
/// not take the new table from the arena, or the entries vanish with the block.
#[test]
fn regression_bug_045_a_map_grown_inside_a_pool_keeps_its_entries() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    mut m: HashMap<i32, i32> = HashMap::new()
    pool first {{
        mut i: i32 = 1
        while i <= 20 {{
            m.insert(i, i)
            i = i + 1
        }}
    }}
{CLOBBER_POOL}
    match m.get(7) {{ Some(x) => x, None => 99 }}
}}
"#
    );
    let code = test
        .compile_and_run("pool_map_growth.nr", &source)
        .expect("compile/run failed");
    assert_eq!(code, 7);
}

/// `Vec` growth reallocates its buffer with libc, so a `Vec` whose buffer came from the
/// arena would hand `realloc` a pointer libc never gave out. `keys()` is such a `Vec`.
/// The second pool writes past the arena's first page, which is where the damage that
/// `realloc` did to the chunk becomes a fault.
#[test]
fn regression_bug_046_a_key_vec_built_inside_a_pool_can_grow() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut m: HashMap<i32, i32> = HashMap::new()
    m.insert(1, 10)
    m.insert(2, 20)
    mut total: i32 = 0
    pool first {
        mut k = m.keys()
        mut i: i32 = 0
        while i < 40 {
            k.push(i)
            i = i + 1
        }
        for x in k { total = total + x }
    }
    pool second {
        mut j: i32 = 0
        while j < 200 {
            val s = "after-after-after-after-{j}"
            if s.len() == 0 { println("x") }
            j = j + 1
        }
    }
    total
}
"#;
    let code = test
        .compile_and_run("pool_keys_growth.nr", source)
        .expect("compile/run failed");
    // 1 + 2 + (0 + 1 + ... + 39) = 783, and an exit code keeps the low byte.
    assert_eq!(code, 783 % 256);
}

/// A receiver reached through a field is the same channel as one named directly: the
/// write lands in `o`, which outlives the block, however many fields sit in between.
#[test]
fn regression_bug_051_a_nested_receiver_may_not_retain_a_pool_value() {
    let test = CompileTest::new();
    let source = r#"
struct Box { s: string }
impl Box {
    func stash(&mut self, v: string) { self.s = v }
}
struct Outer { inner: Box }
func main() -> i32 {
    mut o = Outer { inner: Box { s: "none" } }
    pool scratch {
        val local = "survivor-" + "{42}"
        o.inner.stash(local)
    }
    0
}
"#;
    let err = test
        .check("pool_nested_receiver.nr", source)
        .expect_err("a nested receiver must not keep arena memory");
    assert!(
        err.contains("'stash'") && err.contains("'o'"),
        "diagnostic should name the callee and the root binding, got: {err}"
    );
}
