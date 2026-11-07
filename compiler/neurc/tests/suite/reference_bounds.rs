// A `&T` meets `Copy` and `Clone`: `.clone()` on a receiver declared in a type parameter
// copies the reference, while one declared as a concrete borrow still reads through it.
use crate::compile_harness::{compile_rejected, run, stdout_of};

/// `Mark::clone` adds 100, so a result tells which `clone` ran.
const MARK: &str = r#"
struct Mark {
    n: i32
}

impl Clone for Mark {
    func clone(&self) -> Mark {
        Mark { n: self.n + 100 }
    }
}
"#;

fn run_with_mark(name: &str, source: &str) -> i32 {
    run(name, &format!("{MARK}{source}"))
}

#[test]
fn a_reference_meets_a_copy_bound_inferred_or_named() {
    let source = r#"
func dup<T: Copy>(v: T) -> (T, T) {
    (v, v)
}

func main() -> i32 {
    val n = 4
    val a = dup::<&i32>(&n)
    val b = dup(&n)
    *a.0 + *a.1 + *b.0
}
"#;
    assert_eq!(run("copy_bound.nr", source), 12);
}

#[test]
fn a_referent_with_no_clone_of_its_own_is_no_obstacle() {
    let source = r#"
struct Plain {
    n: i32
}

func twice<T: Clone>(v: T) -> (T, T) {
    (v.clone(), v.clone())
}

func main() -> i32 {
    val p = Plain { n: 3 }
    val r = &p
    val a = twice(&p)
    val b = twice(&r)
    a.0.n + a.1.n + b.1.n
}
"#;
    assert_eq!(run("no_clone.nr", source), 9);
}

#[test]
fn a_declared_parameter_copies_and_a_concrete_borrow_reads_through() {
    let source = r#"
func both<T: Clone>(v: T, s: &Mark) -> i32 {
    val concrete = s.clone()
    val declared = v.clone()
    concrete.n
}

func keep<T: Clone>(v: T) -> T {
    v.clone()
}

func main() -> i32 {
    val m = Mark { n: 5 }
    both(&m, &m) + keep(&m).n
}
"#;
    assert_eq!(run_with_mark("both.nr", source), 110);
}

#[test]
fn a_borrowed_parameter_peels_only_its_written_borrows() {
    let source = r#"
func one<T: Clone>(x: &T) -> T {
    x.clone()
}

func two<T: Clone>(x: &&T) -> T {
    x.clone()
}

func exclusive<T: Clone>(x: &mut T) -> T {
    x.clone()
}

func main() -> i32 {
    val m = Mark { n: 1 }
    val r = &m
    val rr = &r
    mut w = &m
    val reached = one(&m)
    val a = one(&r)
    val b = two(&rr)
    val c = exclusive(&mut w)
    reached.n + a.n + b.n + c.n
}
"#;
    assert_eq!(run_with_mark("peel.nr", source), 104);
}

#[test]
fn a_value_inferred_from_a_parameter_stays_declared_in_it() {
    let source = r#"
func chain<T: Clone>(v: T) -> T {
    val a = v.clone()
    val b = (a, 1)
    val c = b.0
    val f = |x: &T| x.clone()
    f(&c).clone()
}

func main() -> i32 {
    val m = Mark { n: 7 }
    chain(&m).n
}
"#;
    assert_eq!(run_with_mark("chain.nr", source), 7);
}

#[test]
fn generic_impls_and_methods_copy_a_reference_field() {
    let source = r#"
struct Slot<T> {
    held: T
}

impl<T: Clone> Slot<T> {
    func read(&self) -> T {
        self.held.clone()
    }

    func of(v: &T) -> Slot<T> {
        Slot { held: v.clone() }
    }

    func pair<U: Clone>(&self, other: U) -> (T, U) {
        (self.held.clone(), other.clone())
    }
}

func main() -> i32 {
    val m = Mark { n: 2 }
    val r = &m
    val slot = Slot { held: &m }
    val built = Slot::of(&r)
    val p = slot.pair(&m)
    slot.read().n + built.read().n + p.0.n + p.1.n
}
"#;
    assert_eq!(run_with_mark("slot.nr", source), 8);
}

#[test]
fn a_generic_enum_impl_copies_a_reference_payload() {
    let source = r#"
enum Either<T> {
    Held(T),
    Empty
}

impl<T: Copy> Either<T> {
    func or(&self, fallback: &T) -> T {
        if val Either::Held(x) = self { x.clone() } else { fallback.clone() }
    }
}

func main() -> i32 {
    val a = Mark { n: 1 }
    val b = Mark { n: 2 }
    val r = &b
    val held = Either::Held(&a)
    val empty: Either<&Mark> = Either::Empty
    held.or(&r).n * 10 + empty.or(&r).n
}
"#;
    assert_eq!(run_with_mark("either.nr", source), 12);
}

#[test]
fn a_struct_holding_a_reference_derives_clone_and_copy_per_instance() {
    let source = r#"
@derive(Copy, Clone)
struct Pin<T> {
    at: T
}

func dup<T: Copy>(v: T) -> [T; 2] {
    [v, v]
}

func keep<T: Clone>(v: T) -> T {
    v.clone()
}

func main() -> i32 {
    val m = Mark { n: 6 }
    val pin = Pin { at: &m }
    val both = dup(pin)
    keep(pin).at.n + both[1].at.n
}
"#;
    assert_eq!(run_with_mark("pin.nr", source), 12);
}

#[test]
fn slices_strings_and_trait_objects_are_references_too() {
    let source = r#"
trait Named {
    func id(&self) -> i32
}

struct Thing {
    n: i32
}

impl Named for Thing {
    func id(&self) -> i32 {
        self.n
    }
}

func twice<T: Clone>(v: T) -> (T, T) {
    (v.clone(), v.clone())
}

func main() -> i32 {
    val thing = Thing { n: 6 }
    val object: &dyn Named = &thing
    val arr = [1, 2, 3]
    val slice: &[i32] = &arr
    val text = "hello"
    val a = twice(object)
    val b = twice(slice)
    val c = twice(&text)
    a.1.id() + b.0[2] + c.1.len() as i32
}
"#;
    assert_eq!(run("unsized.nr", source), 14);
}

#[test]
fn an_owned_argument_still_reaches_its_own_clone() {
    let source = r#"
func keep<T: Clone>(v: &T) -> T {
    v.clone()
}

func main() -> i32 {
    val text = "owned"
    println(keep(&text))
    keep(&40)
}
"#;
    assert_eq!(stdout_of("owned.nr", source), "owned\n");
    assert_eq!(run("owned_exit.nr", source), 40);
}

#[test]
fn a_mutable_reference_meets_neither_bound() {
    let copy = r#"
func dup<T: Copy>(v: T) -> (T, T) {
    (v, v)
}

func main() -> i32 {
    mut n = 1
    val a = dup(&mut n)
    0
}
"#;
    let clone = r#"
func twice<T: Clone>(v: T) -> (T, T) {
    (v.clone(), v.clone())
}

func main() -> i32 {
    mut n = 1
    val a = twice::<&mut i32>(&mut n)
    0
}
"#;
    assert!(
        compile_rejected("mut_copy.nr", copy).contains("does not implement required trait 'Copy'")
    );
    assert!(
        compile_rejected("mut_clone.nr", clone)
            .contains("does not implement required trait 'Clone'")
    );
}

#[test]
fn a_result_still_holds_the_borrow_it_was_built_from() {
    let frozen = r#"
struct S {
    n: i32
}

func twice<T: Clone>(v: T) -> (T, T) {
    (v.clone(), v.clone())
}

func main() -> i32 {
    mut s = S { n: 1 }
    val a = twice(&s)
    s = S { n: 2 }
    a.0.n
}
"#;
    let escaped = r#"
struct S {
    n: i32
}

func twice<T: Clone>(v: T) -> (T, T) {
    (v.clone(), v.clone())
}

func leak() -> (&S, &S) {
    val s = S { n: 1 }
    twice(&s)
}

func main() -> i32 {
    0
}
"#;
    assert!(compile_rejected("frozen.nr", frozen).contains("while it is borrowed"));
    assert!(compile_rejected("escaped.nr", escaped).contains("cannot return a reference"));
}

#[test]
fn a_collection_built_from_a_reference_holds_its_borrow() {
    let source = r#"
func vec_of<T: Clone>(v: T) -> Vec<T> {
    mut out: Vec<T> = Vec::new()
    out.push(v.clone())
    out
}

func main() -> i32 {
    val outer = 0
    mut held = vec_of(&outer)
    {
        val inner = 3
        held = vec_of(&inner)
    }
    *held[0]
}
"#;
    assert!(compile_rejected("held.nr", source).contains("does not live long enough"));
}

#[test]
fn a_generic_parameter_binds_through_a_collection_argument() {
    let source = r#"
func last<T: Clone>(items: &Vec<T>) -> T {
    mut seen = items[0].clone()
    for x in items {
        seen = x.clone()
    }
    seen
}

func main() -> i32 {
    mut numbers: Vec<i32> = Vec::new()
    numbers.push(1)
    numbers.push(9)
    last(&numbers)
}
"#;
    assert_eq!(run("collection_arg.nr", source), 9);
}

#[test]
fn a_reference_chain_built_by_recursion_stays_a_bounded_error() {
    let source = r#"
func grow<T: Clone>(x: T, n: i32) -> i32 {
    if n == 0 { 0 } else { grow(&x, n - 1) }
}

func main() -> i32 {
    grow(1, 3)
}
"#;
    assert!(compile_rejected("grow.nr", source).contains("nests generic instances"));
}
