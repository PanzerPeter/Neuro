// The derivable traits `Copy`, `Clone`, `Debug`, `PartialEq` and `Hashable` as ordinary
// traits: written by hand, named as bounds, and met alike by a derive, a hand-written
// impl and a builtin type, so one generic body instantiates over all three.
use crate::compile_harness::CompileTest;

fn run(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

fn stdout_of(name: &str, source: &str) -> String {
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source(name, source))
        .unwrap_or_else(|e| panic!("{name} should compile: {e}"));
    let out = std::process::Command::new(&exe)
        .output()
        .expect("run executable");
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

fn rejected(name: &str, source: &str) -> String {
    let test = CompileTest::new();
    let path = test.write_source(name, source);
    test.compile(&path)
        .expect_err("the program must be rejected")
}

#[test]
fn a_copy_bound_lets_the_body_use_a_value_twice() {
    let source = r#"
@derive(Copy)
struct Cell {
    v: i32
}

func dup<T: Copy>(v: T) -> (T, T) {
    val a = v
    val b = v
    (a, b)
}

func main() -> i32 {
    val (a, b) = dup(20)
    val (c, d) = dup(Cell { v: 1 })
    a + b + c.v + d.v
}
"#;
    assert_eq!(run("dup.nr", source), 42);
}

#[test]
fn a_clone_bound_clones_scalars_strings_derives_and_hand_written_impls() {
    let source = r#"
@derive(Clone)
struct Derived {
    x: i32
}

struct Hand {
    x: i32
}

impl Clone for Hand {
    func clone(&self) -> Self {
        Hand { x: self.x + 100 }
    }
}

func twice<T: Clone>(v: &T) -> T {
    v.clone()
}

func main() -> i32 {
    val n = 1
    val s = "text"
    val d = Derived { x: 2 }
    val h = Hand { x: 3 }
    println("{twice(&n)}")
    println(twice(&s))
    println("{twice(&d).x}")
    println("{twice(&h).x}")
    0
}
"#;
    assert_eq!(stdout_of("twice.nr", source), "1\ntext\n2\n103\n");
}

#[test]
fn a_copy_bound_reaches_clone_and_a_copy_value_clones_to_itself() {
    let source = r#"
func again<T: Copy>(v: &T) -> T {
    v.clone()
}

func main() -> i32 {
    val n = 40
    val pair = (1, 1).clone()
    again(&n) + pair.0 + pair.1
}
"#;
    assert_eq!(run("again.nr", source), 42);
}

#[test]
fn a_hand_written_copy_makes_its_struct_copy() {
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

impl Clone for Point {
    func clone(&self) -> Self {
        Point { x: self.x, y: self.y }
    }
}

impl Copy for Point {}

func main() -> i32 {
    val p = Point { x: 20, y: 1 }
    val a = p
    val b = p
    a.x + b.x + p.y + b.y
}
"#;
    assert_eq!(run("hand_copy.nr", source), 42);
}

#[test]
fn a_debug_bound_renders_derived_hand_written_and_builtin_values() {
    let source = r#"
@derive(Debug)
struct Derived {
    x: i32,
    tag: string
}

struct Hand {
    x: i32
}

impl Debug for Hand {
    func fmt_debug(&self, out: &mut StringBuilder) {
        out.push_str("Hand<{self.x}>")
    }
}

func show<T: Debug>(v: &T) -> string {
    "[{v:?}]"
}

func render<T: Debug>(v: &T) -> string {
    mut out = StringBuilder::new()
    v.fmt_debug(&mut out)
    out.to_string()
}

func main() -> i32 {
    val d = Derived { x: 1, tag: "t" }
    val h = Hand { x: 2 }
    val n = 3
    println(show(&d))
    println(show(&h))
    println(show(&n))
    println(render(&d))
    println(render(&h))
    println("{h:?} and {h:>9?}")
    0
}
"#;
    assert_eq!(
        stdout_of("show.nr", source),
        "[Derived { x: 1, tag: \"t\" }]\n[Hand<2>]\n[3]\nDerived { x: 1, tag: \"t\" }\nHand<2>\nHand<2> and   Hand<2>\n"
    );
}

#[test]
fn a_derived_partial_eq_and_hashable_struct_is_a_hash_map_key() {
    let source = r#"
@derive(PartialEq, Hashable)
struct Key {
    id: i32,
    name: string,
    live: bool
}

func main() -> i32 {
    mut m: HashMap<Key, i32> = HashMap::new()
    m.insert(Key { id: 1, name: "a", live: true }, 40)
    m.insert(Key { id: 1, name: "a", live: false }, 2)
    m.insert(Key { id: 1, name: "a", live: true }, 41)
    val hit = m.get(Key { id: 1, name: "a", live: true }) ?? 0
    val other = m.get(Key { id: 1, name: "a", live: false }) ?? 0
    val miss = m.get(Key { id: 2, name: "a", live: true }) ?? 100
    if m.len() == 2 && miss == 100 { hit + other - 1 } else { 0 }
}
"#;
    assert_eq!(run("derived_key.nr", source), 42);
}

#[test]
fn hash_agrees_between_a_map_a_bound_and_a_direct_call() {
    let source = r#"
@derive(PartialEq, Hashable)
struct Key {
    id: u8,
    c: char
}

func hash_of<T: Hashable>(v: &T) -> u64 {
    v.hash()
}

func main() -> i32 {
    val a = Key { id: 1, c: 'x' }
    val b = Key { id: 1, c: 'x' }
    val c = Key { id: 2, c: 'x' }
    val n = 7
    val s = "text"
    mut score = 0
    if hash_of(&a) == b.hash() { score = score + 10 }
    if a.hash() != c.hash() { score = score + 10 }
    if hash_of(&n) == n.hash() { score = score + 10 }
    if hash_of(&s) == "te".hash() { score = score - 100 }
    if hash_of(&s) == ("te" + "xt").hash() { score = score + 12 }
    score
}
"#;
    assert_eq!(run("hash_agree.nr", source), 42);
}

#[test]
fn a_bool_key_hashes_and_compares() {
    let source = r#"
func main() -> i32 {
    mut m: HashMap<bool, i32> = HashMap::new()
    m.insert(true, 40)
    m.insert(false, 2)
    mut ordered: BTreeMap<bool, i32> = BTreeMap::new()
    ordered.insert(true, 1)
    ordered.insert(false, 0)
    (m.get(true) ?? 0) + (m.get(false) ?? 0) + (ordered.get(false) ?? 9)
}
"#;
    assert_eq!(run("bool_key.nr", source), 42);
}

#[test]
fn a_hand_written_hashable_keys_a_map_through_its_own_hash() {
    let source = r#"
struct Mod {
    n: i32
}

impl PartialEq for Mod {
    func eq(&self, rhs: &Mod) -> bool {
        self.n % 10 == rhs.n % 10
    }
}

impl Hashable for Mod {
    func hash(&self) -> u64 {
        (self.n % 10) as u64
    }
}

impl Clone for Mod {
    func clone(&self) -> Self {
        Mod { n: self.n }
    }
}

impl Copy for Mod {}

func main() -> i32 {
    mut m: HashMap<Mod, i32> = HashMap::new()
    m.insert(Mod { n: 3 }, 41)
    m.get(Mod { n: 13 }) ?? 0
}
"#;
    assert_eq!(run("hand_key.nr", source), 41);
}

#[test]
fn a_vec_of_a_copy_bounded_parameter_is_built_in_a_generic_body() {
    let source = r#"
func filled<T: Copy>(v: T, n: i32) -> Vec<T> {
    mut out: Vec<T> = Vec::new()
    mut i = 0
    while i < n {
        out.push(v)
        i = i + 1
    }
    out
}

func main() -> i32 {
    val ints = filled(6, 7)
    val flags = filled(true, 2)
    mut total = 0
    for x in ints {
        total = total + x
    }
    total + flags.len() as i32 - 2
}
"#;
    assert_eq!(run("filled.nr", source), 42);
}

#[test]
fn a_bound_the_argument_does_not_meet_is_refused() {
    let err = rejected(
        "unmet.nr",
        r#"
struct Plain {
    x: i32
}

func twice<T: Clone>(v: &T) -> T {
    v.clone()
}

func dup<T: Copy>(v: T) -> (T, T) {
    (v, v)
}

func main() -> i32 {
    val p = Plain { x: 1 }
    val q = twice(&p)
    val pair = dup("owned")
    0
}
"#,
    );
    assert!(
        err.contains("does not implement required trait 'Clone'"),
        "got: {err}"
    );
    assert!(
        err.contains("does not implement required trait 'Copy'"),
        "got: {err}"
    );
}

#[test]
fn a_hand_written_copy_is_held_to_what_the_derive_accepts() {
    let err = rejected(
        "bad_copy.nr",
        r#"
struct Owns {
    s: string
}

impl Clone for Owns {
    func clone(&self) -> Self {
        Owns { s: "copy" }
    }
}

impl Copy for Owns {}

struct Lone {
    x: i32
}

impl Copy for Lone {}

enum Mode {
    On,
    Off
}

impl Copy for Mode {}

func main() -> i32 { 0 }
"#,
    );
    assert!(err.contains("struct 'Owns' cannot be `Copy`"), "got: {err}");
    assert!(
        err.contains("`impl Copy for Lone` requires `impl Clone for Lone`"),
        "got: {err}"
    );
    assert!(
        err.contains("`impl Copy for Mode` is not allowed"),
        "got: {err}"
    );
}

#[test]
fn deriving_a_trait_and_writing_its_impl_conflict() {
    let err = rejected(
        "conflict.nr",
        r#"
@derive(Hashable)
struct K {
    x: i32
}

impl Hashable for K {
    func hash(&self) -> u64 {
        0
    }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        err.contains("both derives `Hashable` and declares `impl Hashable for K`"),
        "got: {err}"
    );
}

#[test]
fn a_derived_hashable_field_must_hash() {
    let err = rejected(
        "float_field.nr",
        r#"
@derive(Hashable)
struct F {
    x: f32
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        err.contains("struct 'F' cannot derive `Hashable`: field 'x'"),
        "got: {err}"
    );
}

#[test]
fn an_unbounded_parameter_neither_renders_nor_fills_a_vec() {
    let err = rejected(
        "unbounded.nr",
        r#"
func show<T>(v: &T) -> string {
    "{v:?}"
}

func bag<T>(v: T) -> Vec<T> {
    mut out: Vec<T> = Vec::new()
    out
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        err.contains("a value of type T cannot be interpolated"),
        "got: {err}"
    );
    assert!(err.contains("T cannot be stored in a `Vec`"), "got: {err}");
}

#[test]
fn eq_is_called_through_a_partial_eq_bound_and_on_a_derived_struct() {
    // `.eq` was `MethodNotFound` on everything without a hand-written impl.
    let out = stdout_of(
        "eq_method.nr",
        r#"
@derive(PartialEq)
struct P { a: i32, s: string }

@derive(Copy, Clone)
struct H { v: i32 }

impl PartialEq for H {
    func eq(&self, rhs: &H) -> bool { self.v % 10 == rhs.v % 10 }
}

func same<T: PartialEq>(v: T, w: T) -> bool { v.eq(&w) }
func same_ref<T: PartialEq>(v: &T, w: &T) -> bool { v.eq(w) }
func ordered<T: Comparable>(v: T, w: T) -> bool { v.eq(&w) }

func main() -> i32 {
    val p = P { a: 1, s: "q" + "r" }
    val q = P { a: 1, s: "qr" }
    val x = 7
    val seven = 7
    val strings = same("ab", "a" + "b")
    println("{same(3, 3)} {same(3, 4)} {strings} {same(1.5, 2.5)}")
    println("{p.eq(&q)} {same_ref(&p, &q)} {same(H { v: 13 }, H { v: 23 })}")
    println("{ordered(5u8, 5u8)} {x.eq(&seven)}")
    0
}
"#,
    );
    assert_eq!(
        out, "true false true false\ntrue true true\ntrue true\n",
        "got: {out}"
    );
}

#[test]
fn eq_needs_a_partial_eq_bound_or_implementor() {
    let err = rejected(
        "eq_unbounded.nr",
        r#"
struct Plain { a: i32 }

func same<T>(v: T, w: T) -> bool { v.eq(&w) }

func main() -> i32 {
    val a = Plain { a: 1 }
    val b = Plain { a: 1 }
    if a.eq(&b) { 1 } else { 0 }
}
"#,
    );
    assert!(err.contains("'T' has no method 'eq'"), "got: {err}");
    assert!(err.contains("'Plain' has no method 'eq'"), "got: {err}");
}

#[test]
fn a_map_is_keyed_by_a_type_parameter_whose_bounds_supply_the_key_traits() {
    let source = r#"
@derive(Copy, Clone, PartialEq, Hashable)
struct Pt { x: i32, y: i32 }

func count<K: Hashable + PartialEq + Copy>(keys: [K; 4]) -> i32 {
    mut m: HashMap<K, i32> = HashMap::new()
    for k in keys {
        val c = m.get(k) ?? 0
        m.insert(k, c + 1)
    }
    m.len() as i32
}

func distinct<K: Comparable + PartialEq + Copy>(a: K, b: K) -> i32 {
    mut m: BTreeMap<K, i32> = BTreeMap::new()
    m.insert(b, 2)
    m.insert(a, 1)
    m.insert(b, 3)
    m.len() as i32
}

func main() -> i32 {
    val a = count([1, 2, 2, 3])
    val b = count([Pt { x: 1, y: 1 }, Pt { x: 1, y: 1 }, Pt { x: 2, y: 1 }, Pt { x: 1, y: 1 }])
    val c = count(['a', 'b', 'c', 'd'])
    val d = distinct(5u8, 9u8)
    a * 1000 + b * 100 + c * 10 + d - 3000
}
"#;
    assert_eq!(run("generic_key.nr", source), 242);
}

#[test]
fn a_type_parameter_key_needs_every_key_trait_as_a_bound() {
    let err = rejected(
        "generic_key_bounds.nr",
        r#"
func a<K: PartialEq + Copy>(k: K) -> i32 {
    val m: HashMap<K, i32> = HashMap::new()
    m.len() as i32
}

func b<K: Hashable + PartialEq>(k: K) -> i32 {
    val m: HashMap<K, i32> = HashMap::new()
    m.len() as i32
}

func c<K: Hashable + PartialEq + Copy>(k: K) -> i32 {
    val m: BTreeMap<K, i32> = BTreeMap::new()
    m.len() as i32
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        err.contains("map keys require `Hashable` on K"),
        "got: {err}"
    );
    assert!(err.contains("map keys require `Copy` on K"), "got: {err}");
    assert!(
        err.contains("map keys require `Comparable` on K"),
        "got: {err}"
    );
}

/// An impl bounded by a derivable trait applies to every instance whose argument meets the
/// bound, a builtin type included. The bounded impls were left out of a builtin instance,
/// so the methods the checker had accepted were undefined at code generation.
#[test]
fn regression_bounded_impl_reaches_builtin_arguments() {
    let source = r#"
@derive(Debug, PartialEq, Hashable, Clone)
struct D { n: i32 }
struct Cell<T> { v: T }
impl<T: Debug> Cell<T> {
    func dbg(&self) -> i32 {
        println("{self.v:?}")
        1
    }
}
impl<T: PartialEq> Cell<T> { func same(&self, o: &Cell<T>) -> i32 { if self.v == o.v { 10 } else { 0 } } }
impl<T: Hashable> Cell<T> { func h(&self) -> i32 { 100 } }
impl<T: Copy> Cell<T> { func get(&self) -> T { self.v } }
func main() -> i32 {
    val a = Cell { v: 3 }
    val s = Cell { v: "xy" }
    val d = Cell { v: D { n: 2 } }
    val f = Cell { v: 1.5f16 }
    (a.dbg() + a.same(&a) + a.h()) + (s.dbg() + s.same(&s) + s.h())
        + (d.dbg() + d.same(&d) + d.h()) + (f.dbg() + f.same(&f)) + a.get() - 100
}
"#;
    assert_eq!(run("bounded_builtin.nr", source), 111 * 3 + 11 + 3 - 100);
    assert_eq!(
        stdout_of("bounded_builtin_out.nr", source),
        "3\n\"xy\"\nD { n: 2 }\n1.5\n"
    );
}
