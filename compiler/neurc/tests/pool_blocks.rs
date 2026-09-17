// End-to-end coverage for `pool { }` arena blocks.
//
// The arena is not directly observable from a Neuro program: it has no surface, and a
// correct run and a leaking one print the same thing. What these tests pin is that the
// values a pool body builds are correct while the block runs and stay correct across a
// sibling and a nested block, which is what breaks first if the mark, the restore, or
// the release wrapper is wrong. The arena's shape is asserted on the IR, in
// `llvm-backend`'s own tests.
mod common;
use common::CompileTest;

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
fn storing_a_heap_value_into_an_outer_binding_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut out: string = ""
    pool {
        out = "a" + "b"
    }
    0
}
"#;
    let error = test
        .check("pool_escape.nr", source)
        .expect_err("storing arena memory past the block must not compile");
    assert!(error.contains("outlives"), "unexpected diagnostic: {error}");
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
