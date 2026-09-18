// End-to-end tests for the release of anonymous heap `string`s: the owned buffer a
// `+`, an interpolation, or `String::to_string` allocates in an expression that binds
// it to nothing.
//
// A leak has no exit code, so each program below runs its leaking shape a few hundred
// thousand times over a payload wide enough that the unreleased buffers add up to tens
// of megabytes: without the release the process's heap climbs, with it it stays flat.
// What is asserted is the arithmetic and that the run completes, the same way
// `string_builder.rs` catches a builder that never frees its buffer; the count of
// releases the backend actually emits is asserted directly in `llvm-backend`'s own
// tests, which is where a regression shows up as a failure rather than as memory use.
use crate::compile_harness::CompileTest;

/// Rounds and payload width chosen together so one unreleased buffer per iteration is
/// tens of megabytes: enough that a regression is a visible memory event, cheap enough
/// that the loop itself costs a fraction of a second.
const LEAK_ROUNDS: u32 = 200_000;

/// The concatenation payload, 64 bytes, repeated as both operands throughout.
const PAYLOAD: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// `PAYLOAD.len()`, as the byte arithmetic each program checks itself against.
const WIDTH: u64 = PAYLOAD.len() as u64;

#[test]
fn a_concatenation_chain_does_not_leak_its_intermediate_results() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        val s = a + a + a
        n = n + s.len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * WIDTH * 3
    );
    let exit = test
        .compile_and_run("concat_chain.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn an_operand_built_for_a_comparison_is_released_at_it() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut hits: u32 = 0
    while i < {LEAK_ROUNDS} {{
        if a + a == "{PAYLOAD}{PAYLOAD}" {{
            hits = hits + 1
        }}
        i = i + 1
    }}
    if hits != {LEAK_ROUNDS} {{
        return 91
    }}
    0
}}
"#
    );
    let exit = test
        .compile_and_run("compare_operand.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn a_receiver_and_a_push_str_argument_built_for_their_call_are_released_at_it() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        n = n + (a + a).len()
        mut b = String::new()
        b.push_str(a + a)
        n = n + b.len()
        i = i + 1
    }}
    if n != {} {{
        return 91
    }}
    0
}}
"#,
        u64::from(LEAK_ROUNDS) * WIDTH * 4
    );
    let exit = test
        .compile_and_run("call_temporaries.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn a_statement_whose_value_nothing_reads_releases_it() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    val a = "{PAYLOAD}"
    mut i: u32 = 0
    while i < {LEAK_ROUNDS} {{
        a + a
        "{{a}} and {{a}}"
        i = i + 1
    }}
    0
}}
"#
    );
    let exit = test
        .compile_and_run("discarded_value.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// `String::to_string` allocates on every call, so the `string` it hands back is owned
/// by whatever takes it: a binding releases it at scope exit, and a consumer that
/// discards it releases it there.
#[test]
fn the_builder_copy_out_is_owned_by_whatever_takes_it() {
    let test = CompileTest::new();
    let source = format!(
        r#"
func main() -> i32 {{
    mut i: u32 = 0
    mut n: u64 = 0
    while i < {LEAK_ROUNDS} {{
        mut b = String::new()
        b.push_str("{PAYLOAD}")
        val s = b.to_string()
        n = n + s.len()
        n = n + b.to_string().len()
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
        .compile_and_run("builder_copy_out.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// The release is a release, not a `free`: a concatenation inside a `pool` block draws
/// its buffer from the arena, and handing an arena pointer to `free` would abort. The
/// arena's own sweep at the closing brace is what reclaims it.
#[test]
fn a_temporary_allocated_in_a_pool_is_left_to_the_arena() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a = "0123456789abcdef"
    mut n: u64 = 0
    pool {
        mut i: u32 = 0
        while i < 1000 {
            n = n + (a + a + a).len()
            i = i + 1
        }
    }
    if n != 48000 {
        return 91
    }
    0
}
"#;
    let exit = test
        .compile_and_run("pooled_temporary.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// A buffer a binding still owns is not released by a consumer that merely reads it:
/// `s.len()` and `s == t` take a place expression, which `produces_owned_string` answers
/// `false` for, so the binding's own scope-exit release stays the only one.
#[test]
fn a_bound_buffer_is_not_released_twice_by_a_reader() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a = "abc"
    val s = a + "def"
    val t = a + "def"
    if s.len() != 6 {
        return 91
    }
    if s != t {
        return 92
    }
    mut b = String::new()
    b.push_str(s)
    b.push_str(t)
    b.len() as i32
}
"#;
    let exit = test
        .compile_and_run("bound_not_double_released.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 12);
}
