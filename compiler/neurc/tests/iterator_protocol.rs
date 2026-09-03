// End-to-end tests for the `IntoIterator` / `Iterator` protocol.
//
// `for x in e` calls `e.into_iter()` once and then `.next()` until it answers `None`.
// Each program compiles to a native binary and runs; the exit code encodes the result,
// so a desugar that dropped, repeated, or mis-ordered an element would fail here rather
// than merely type-check.

mod common;
use common::CompileTest;

/// A hand-written cursor over the integers `[start, end)`, plus the container that
/// hands one out. Every program below builds on these.
const SOURCE_TYPES: &str = r#"
@derive(Copy, Clone)
struct CountIter { at: i32, end: i32 }

impl Iterator for CountIter {
    type Item = i32
    func next(&mut self) -> Option<i32> {
        if self.at >= self.end { return Option::None }
        val current = self.at
        self.at = self.at + 1
        Option::Some(current)
    }
}

@derive(Copy, Clone)
struct Count { end: i32 }

impl IntoIterator for Count {
    type Item = i32
    type Iter = CountIter
    func into_iter(self) -> CountIter {
        CountIter { at: 0, end: self.end }
    }
}
"#;

fn program(body: &str) -> String {
    format!("{SOURCE_TYPES}\nfunc main() -> i32 {{\n{body}\n}}\n")
}

#[test]
fn for_over_an_into_iterator_container_walks_its_iterator() {
    let test = CompileTest::new();
    let source = program(
        r#"
    val c = Count { end: 5 }
    mut total = 0
    for v in c {
        total = total + v
    }
    total
"#,
    );
    let exit = test
        .compile_and_run("iter_container.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 10, "0 + 1 + 2 + 3 + 4");
}

/// A type implementing `Iterator` is its own iterator: the loop uses it directly rather
/// than demanding a second, identical `IntoIterator` impl.
#[test]
fn for_over_an_iterator_needs_no_into_iterator_impl() {
    let test = CompileTest::new();
    let source = program(
        r#"
    val it = CountIter { at: 10, end: 14 }
    mut total = 0
    for v in it {
        total = total + v
    }
    total
"#,
    );
    let exit = test
        .compile_and_run("iter_direct.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 46, "10 + 11 + 12 + 13");
}

/// An empty iterator runs the body zero times — the `None` on the very first `next()`
/// has to leave the loop, not fall into the body with an undefined binding.
#[test]
fn an_empty_iterator_runs_the_body_zero_times() {
    let test = CompileTest::new();
    let source = program(
        r#"
    val c = Count { end: 0 }
    mut runs = 7
    for v in c {
        runs = runs + v + 100
    }
    runs
"#,
    );
    let exit = test
        .compile_and_run("iter_empty.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

/// The container holds no cursor, so each `for` head asks it for a fresh iterator and
/// the second walk starts over.
#[test]
fn a_container_may_be_iterated_more_than_once() {
    let test = CompileTest::new();
    let source = program(
        r#"
    val c = Count { end: 4 }
    mut total = 0
    for v in c {
        total = total + v
    }
    for v in c {
        total = total + v
    }
    total
"#,
    );
    let exit = test
        .compile_and_run("iter_twice.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 12, "(0+1+2+3) twice");
}

#[test]
fn break_and_continue_target_the_protocol_loop() {
    let test = CompileTest::new();
    let source = program(
        r#"
    val c = Count { end: 10 }
    mut total = 0
    for v in c {
        if v == 2 { continue }
        if v == 5 { break }
        total = total + v
    }
    total
"#,
    );
    let exit = test
        .compile_and_run("iter_break.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 8, "0 + 1 + 3 + 4");
}

/// A label on a protocol `for` reaches out of a nested one, the same as on a counted loop.
#[test]
fn a_labeled_break_leaves_an_outer_protocol_loop() {
    let test = CompileTest::new();
    let source = program(
        r#"
    val outer_src = Count { end: 4 }
    mut pairs = 0
    outer: for p in outer_src {
        val inner_src = Count { end: 4 }
        for q in inner_src {
            if p * q > 2 { break outer }
            pairs = pairs + 1
        }
    }
    pairs
"#,
    );
    let exit = test
        .compile_and_run("iter_labeled.nr", &source)
        .expect("compile/run failed");
    assert_eq!(
        exit, 7,
        "p=0 counts all four q; p=1 counts q=0,1,2 and leaves both loops at q=3"
    );
}

/// The position an enumerated head binds is the loop's own count. A `continue` must not
/// skip the advance, or the next element would repeat the index the skipped one had.
#[test]
fn an_enumerated_protocol_head_counts_every_step() {
    let test = CompileTest::new();
    let source = program(
        r#"
    val c = Count { end: 4 }
    mut acc = 0
    for (i, v) in c.enumerate() {
        if v == 1 { continue }
        acc = acc + (i as i32) * 10 + v
    }
    acc
"#,
    );
    let exit = test
        .compile_and_run("iter_enumerate.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 55, "(0,0) + (2,2) + (3,3) = 0 + 22 + 33");
}

/// An adapter wraps another iterator and is one itself, so it stands in a `for` head
/// exactly like the source it wraps. This is the shape `.map()` would take.
#[test]
fn a_generic_adapter_composes_in_a_for_head() {
    let test = CompileTest::new();
    let source = format!(
        r#"{SOURCE_TYPES}
struct Scaled<S> {{
    inner: S,
    rule: (i32) -> i32
}}

impl<S: Iterator<Item = i32>> Iterator for Scaled<S> {{
    type Item = i32
    func next(&mut self) -> Option<i32> {{
        match self.inner.next() {{
            Option::Some(v) => {{
                val apply = self.rule
                Option::Some(apply(v))
            }}
            Option::None => Option::None
        }}
    }}
}}

func main() -> i32 {{
    val doubled = Scaled {{
        inner: CountIter {{ at: 1, end: 5 }},
        rule: |x: i32| -> i32 {{ x * 2 }}
    }}
    mut total = 0
    for v in doubled {{
        total = total + v
    }}
    total
}}
"#
    );
    let exit = test
        .compile_and_run("iter_adapter.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 20, "2 + 4 + 6 + 8");
}

/// Two adapters over one source, driven by a single `for` head: nothing between the
/// source and the loop is materialized.
#[test]
fn stacked_adapters_pull_through_one_element_at_a_time() {
    let test = CompileTest::new();
    let source = format!(
        r#"{SOURCE_TYPES}
@derive(Copy, Clone)
struct Scaled<S> {{ inner: S, factor: i32 }}

impl<S: Iterator<Item = i32>> Iterator for Scaled<S> {{
    type Item = i32
    func next(&mut self) -> Option<i32> {{
        match self.inner.next() {{
            Option::Some(v) => Option::Some(v * self.factor),
            Option::None => Option::None
        }}
    }}
}}

@derive(Copy, Clone)
struct Above<S> {{ inner: S, floor: i32 }}

impl<S: Iterator<Item = i32>> Iterator for Above<S> {{
    type Item = i32
    func next(&mut self) -> Option<i32> {{
        loop {{
            match self.inner.next() {{
                Option::Some(v) => {{
                    if v > self.floor {{ break Option::Some(v) }}
                }}
                Option::None => {{ break Option::None }}
            }}
        }}
    }}
}}

func main() -> i32 {{
    val c = Count {{ end: 6 }}
    val pipeline = Above {{
        inner: Scaled {{ inner: c.into_iter(), factor: 3 }},
        floor: 6
    }}
    mut total = 0
    for v in pipeline {{
        total = total + v
    }}
    total
}}
"#
    );
    let exit = test
        .compile_and_run("iter_stacked.nr", &source)
        .expect("compile/run failed");
    assert_eq!(
        exit, 36,
        "0..5 scaled by 3 is 0,3,6,9,12,15; above 6 keeps 9+12+15"
    );
}

/// A `for` head over a type implementing neither trait names the protocol it is missing.
#[test]
fn a_non_iterable_head_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Plain { n: i32 }

func main() -> i32 {
    val p = Plain { n: 1 }
    for v in p {
        return v
    }
    0
}
"#;
    let path = test.write_source("iter_not_iterable.nr", source);
    let error = test
        .compile(&path)
        .expect_err("a non-iterable `for` head must be rejected");
    assert!(
        error.contains("cannot iterate") && error.contains("IntoIterator"),
        "the diagnostic must name the protocol; got {error}"
    );
}
