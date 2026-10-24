// End-to-end tests for the `string` values a collection stores: the collection owns its
// slots, an insertion moves into them, and a read names the slot in place.
//
// A leak has no exit code, so the shapes that used to leak run a few hundred thousand
// times over a payload wide enough that the unreleased buffers would be tens of
// megabytes. What is asserted is the arithmetic and that the run completes; the IR the
// backend emits for each shape is asserted directly in `llvm-backend`'s own tests, which
// is where a regression reads as a failure rather than as memory use.
use crate::compile_harness::CompileTest;

/// Rounds and payload width chosen together so one unreleased buffer per iteration is
/// tens of megabytes, matching `string_temporaries.rs`.
const LEAK_ROUNDS: u32 = 200_000;

/// The payload every program below stores, 64 bytes.
const PAYLOAD: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// `PAYLOAD.len()`, as the byte arithmetic each program checks itself against.
const WIDTH: u64 = PAYLOAD.len() as u64;

#[test]
fn a_vec_releases_the_string_elements_it_holds() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        mut v: Vec<string> = Vec::new()
        v.push(a + a)
        n = n + v[0].len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * WIDTH * 2
    );
    let exit = test
        .compile_and_run("vec_elements.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn a_map_releases_the_keys_and_values_it_holds() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        mut m: HashMap<string, string> = HashMap::new()
        m.insert(a + a, a + a)
        mut b: BTreeMap<string, string> = BTreeMap::new()
        b.insert(a + a, a + a)
        n = n + m.len() + b.len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * 2
    );
    let exit = test
        .compile_and_run("map_entries.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// A `clear()` gives up every live slot, and `remove` gives up one, so both release what
/// they drop rather than keeping the buffer alive under a reset count.
#[test]
fn clearing_and_removing_release_the_slots_they_give_up() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut v: Vec<string> = Vec::new()
    mut m: HashMap<string, string> = HashMap::new()
    mut i: u32 = 0
    while i < {LEAK_ROUNDS} {{
        v.push(a + a)
        v.clear()
        m.insert(a + a, a + a)
        val gone = m.remove(a)
        m.clear()
        i = i + 1
    }}
    if v.len() != 0 {{
        return 91
    }}
    if m.len() != 0 {{
        return 92
    }}
    0
}}
"#
    );
    let exit = test
        .compile_and_run("clear_and_remove.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// A borrowed head reads each element in place, so an iteration over a long-lived
/// collection copies nothing and accumulates nothing.
#[test]
fn iterating_a_borrowed_string_collection_copies_nothing() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut v: Vec<string> = Vec::new()
    v.push(a + a)
    v.push(a + a)
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {} {{
        for s in &v {{
            n = n + s.len()
        }}
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        LEAK_ROUNDS / 2,
        u64::from(LEAK_ROUNDS / 2) * WIDTH * 4
    );
    let exit = test
        .compile_and_run("iterate_elements.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// A by-value head consumes the collection: each pass's binding owns the element it
/// holds, and a `break` releases the elements no pass reached along with the buffer.
#[test]
fn a_consuming_loop_releases_taken_and_untaken_elements() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        mut v: Vec<string> = Vec::new()
        v.push(a + a)
        v.push(a + a)
        v.push(a + a)
        for s in v {{
            n = n + s.len()
            if n % 3 == 0 {{ break }}
        }}
        i = i + 1
    }}
    if n == 0 {{
        return 91
    }}
    0
}}
"#
    );
    let exit = test
        .compile_and_run("consuming_loop.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// A value taken out of a slot is the taker's: it outlives the collection it came from,
/// whose release no longer reaches it.
#[test]
fn a_removed_element_outlives_the_collection_it_came_from() {
    let test = CompileTest::new();
    let source = r#"
func first(words: Vec<string>) -> string {
    mut words = words
    return words.remove(0)
}

func main() -> i32 {
    mut v: Vec<string> = Vec::new()
    v.push("alpha" + "")
    v.push("beta")
    val kept = first(v)
    if kept != "alpha" {
        return 91
    }
    kept.len() as i32
}
"#;
    let exit = test
        .compile_and_run("read_outlives.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}

/// An insertion moves its argument into the slot, so the binding it came from cannot be
/// read afterwards; a `.clone()` keeps an independent copy.
#[test]
fn an_insertion_moves_its_argument() {
    let test = CompileTest::new();
    let err = test
        .check(
            "argument_moved.nr",
            r#"
func main() -> i32 {
    val b = "one" + "!"
    mut v: Vec<string> = Vec::new()
    v.push(b)
    b.len() as i32
}
"#,
        )
        .expect_err("a pushed binding is moved");
    assert!(err.contains("use of moved value 'b'"), "got: {err}");

    let source = r#"
func main() -> i32 {
    val a = "one"
    val b = a + "!"
    mut v: Vec<string> = Vec::new()
    mut m: HashMap<string, i32> = HashMap::new()
    v.push(b.clone())
    m.insert(b, 7)
    if v[0] != "one!" {
        return 91
    }
    val found = m.get("one!") ?? -1
    found + (v[0].len() as i32)
}
"#;
    let exit = test
        .compile_and_run("argument_cloned.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 11);
}

/// Overwriting a slot releases what it held: an element assignment and an insertion onto
/// an existing key both displace an owner.
#[test]
fn overwriting_a_slot_releases_what_it_displaced() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut v: Vec<string> = Vec::new()
    mut m: BTreeMap<string, string> = BTreeMap::new()
    v.push(a + a)
    m.insert("k", a + a)
    mut i: u32 = 0
    while i < {LEAK_ROUNDS} {{
        v[0] = a + a
        m.insert("k", a + a)
        i = i + 1
    }}
    if v[0].len() != {} {{
        return 91
    }}
    0
}}
"#,
        WIDTH * 2
    );
    let exit = test
        .compile_and_run("displaced_slots.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// The canonical read: a `match` over a fallible reader. `get` lends the slot and `pop`
/// hands the slot's own buffer to the arm's binding, so a loop that reads does not
/// accumulate.
#[test]
fn a_match_over_a_fallible_reader_owns_the_payload_it_binds() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut m: HashMap<string, string> = HashMap::new()
    mut v: Vec<string> = Vec::new()
    m.insert("k", a + a)
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        match m.get("k") {{
            Option::Some(hit) => {{ n = n + hit.len() }}
            Option::None => {{ return 91 }}
        }}
        v.push(a + a)
        match v.pop() {{
            Option::Some(last) => {{ n = n + last.len() }}
            Option::None => {{ return 92 }}
        }}
        i = i + 1
    }}
    if n != {} {{
        return 93
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * WIDTH * 4
    );
    let exit = test
        .compile_and_run("fallible_readers.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// A collection built inside a `pool` draws its copies from the arena, whose release is
/// the mark restore; handing an arena pointer to `free` would abort instead.
#[test]
fn a_string_collection_inside_a_pool_releases_through_the_arena() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total: u64 = 0
    pool {
        mut v: Vec<string> = Vec::new()
        val a = "alpha"
        v.push(a + "!")
        v.push(a)
        total = v[0].len() + v[1].len()
    }
    total as i32
}
"#;
    let exit = test
        .compile_and_run("pooled_elements.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 11);
}

/// The `string` releases `main` emits for `source`, read off its textual IR. A
/// collection's own buffer is released under another name, and once per exit path, so
/// it is not counted.
fn main_string_releases(test: &CompileTest, filename: &str, source: &str) -> usize {
    let source_path = test.write_source(filename, source);
    let ir_path = source_path.with_extension("ll");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_neurc"))
        .args(["compile", "--emit", "llvm-ir", "-o"])
        .arg(&ir_path)
        .arg(&source_path)
        .output()
        .expect("run neurc");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ir = std::fs::read_to_string(&ir_path).expect("read IR");
    let start = ir.find("@main(").expect("main is defined");
    let body = &ir[start..];
    let end = body.find("\n}\n").unwrap_or(body.len());
    body[..end]
        .matches("call void @__neuro_release(ptr %str.drop.buf")
        .count()
}

/// `val Some(s) = v.pop() else { ... }` takes the same owned payload a `match` arm
/// does, and releases it the same way. The success binding registered no owner, and the
/// payload leaked once per evaluation.
#[test]
fn a_val_else_string_payload_is_released_like_a_match_arms() {
    let test = CompileTest::new();
    let val_else = r#"
func main() -> i32 {
    mut v: Vec<string> = Vec::new()
    v.push("x")
    val Some(s) = v.pop() else {
        return 2
    }
    return s.len() as i32
}
"#;
    let matched = r#"
func main() -> i32 {
    mut v: Vec<string> = Vec::new()
    v.push("x")
    return match v.pop() {
        Some(s) => s.len() as i32,
        None => 2
    }
}
"#;
    let by_val_else = main_string_releases(&test, "val_else_payload.nr", val_else);
    let by_match = main_string_releases(&test, "match_payload.nr", matched);
    assert!(by_val_else > 0, "the payload is released at all");
    assert_eq!(
        by_val_else, by_match,
        "the two forms release the payload alike"
    );
}
