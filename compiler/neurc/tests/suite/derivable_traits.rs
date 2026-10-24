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
fn an_unbounded_parameter_does_not_render_but_fills_a_vec() {
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
    assert!(!err.contains("cannot be stored"), "got: {err}");
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

/// A derive writes the impl a programmer would have written, field by field, so a field
/// whose trait is written by hand is rendered, compared, hashed and cloned by its own
/// method, through a nested derive, an enum and a generic instance alike. Each hand-written
/// impl here does something a field-wise one would not, so the output shows it was called.
#[test]
fn a_derive_calls_the_hand_written_impl_of_a_field() {
    let source = r#"
@derive(Debug, Clone)
struct Wrap<T> {
    v: T
}

struct Hand {
    x: i32,
    tag: string
}

enum Color {
    Red,
    Green
}

@derive(Debug, Clone)
struct Mid {
    h: Hand,
    c: Color,
    n: i32
}

@derive(Debug, Clone)
struct Top {
    m: Mid,
    w: Wrap<Hand>
}

impl Debug for Hand {
    func fmt_debug(&self, out: &mut StringBuilder) {
        out.push_str("hand {self.x}")
    }
}

impl Clone for Hand {
    func clone(&self) -> Self {
        print("[clone]")
        Hand { x: self.x + 1, tag: self.tag.clone() }
    }
}

impl Debug for Color {
    func fmt_debug(&self, out: &mut StringBuilder) {
        match self {
            Color::Red => out.push_str("red"),
            Color::Green => out.push_str("green")
        }
    }
}

impl Clone for Color {
    func clone(&self) -> Self {
        print("[color]")
        Color::Green
    }
}

struct Key {
    x: i32,
    junk: i32
}

impl Clone for Key {
    func clone(&self) -> Self {
        Key { x: self.x, junk: self.junk }
    }
}

impl Copy for Key {}

impl PartialEq for Key {
    func eq(&self, rhs: &Key) -> bool {
        self.x == rhs.x
    }
}

impl Hashable for Key {
    func hash(&self) -> u64 {
        self.x as u64
    }
}

@derive(PartialEq, Hashable)
struct KeyMid {
    k: Key,
    n: i32
}

@derive(PartialEq, Hashable)
struct KeyTop {
    m: KeyMid
}

func key(x: i32, junk: i32) -> KeyTop {
    KeyTop { m: KeyMid { k: Key { x: x, junk: junk }, n: 2 } }
}

func main() -> i32 {
    val t = Top {
        m: Mid { h: Hand { x: 1, tag: "a" }, c: Color::Red, n: 2 },
        w: Wrap { v: Hand { x: 10, tag: "w" } }
    }
    println("{t:?}")
    val u = t.clone()
    println("")
    println("{u:?} {u.m.h.tag} {t.m.h.tag}")
    mut m: HashMap<KeyTop, i32> = HashMap::new()
    m.insert(key(1, 5), 41)
    val found = m.get(key(1, 6)) ?? 0
    println("{key(1, 5) == key(1, 6)} {key(1, 5) == key(2, 5)} {key(1, 5).hash() == key(1, 6).hash()} {found}")
    0
}
"#;
    assert_eq!(
        stdout_of("field_impls.nr", source),
        "Top { m: Mid { h: hand 1, c: red, n: 2 }, w: Wrap { v: hand 10 } }\n\
         [clone][color][clone]\n\
         Top { m: Mid { h: hand 2, c: green, n: 2 }, w: Wrap { v: hand 11 } } a a\n\
         true false true 41\n"
    );
}

/// A hand-written impl that does what the derive would is indistinguishable from it.
#[test]
fn a_hand_written_field_impl_matches_the_derive_it_replaces() {
    let program = |inner: &str| {
        format!(
            r#"
{inner}

@derive(Debug, Clone, PartialEq, Hashable)
struct Outer {{
    i: Inner,
    n: i32
}}

func main() -> i32 {{
    val a = Outer {{ i: Inner {{ x: 1, y: 2 }}, n: 3 }}
    val b = a.clone()
    val c = Outer {{ i: Inner {{ x: 1, y: 9 }}, n: 3 }}
    println("{{a:?}} {{b:?}} {{a == b}} {{a == c}} {{a.hash() == b.hash()}}")
    0
}}
"#
        )
    };
    let derived =
        program("@derive(Debug, Clone, PartialEq, Hashable)\nstruct Inner { x: i32, y: i32 }");
    let by_hand = program(
        r#"struct Inner { x: i32, y: i32 }

impl Debug for Inner {
    func fmt_debug(&self, out: &mut StringBuilder) {
        out.push_str("Inner \{ x: {self.x}, y: {self.y} \}")
    }
}

impl Clone for Inner {
    func clone(&self) -> Self {
        Inner { x: self.x, y: self.y }
    }
}

impl Copy for Inner {}

impl PartialEq for Inner {
    func eq(&self, rhs: &Inner) -> bool {
        self.x == rhs.x && self.y == rhs.y
    }
}

impl Hashable for Inner {
    func hash(&self) -> u64 {
        (self.x * 31 + self.y) as u64
    }
}"#,
    );
    let expected = "Outer { i: Inner { x: 1, y: 2 }, n: 3 } Outer { i: Inner { x: 1, y: 2 }, n: 3 } true false true\n";
    assert_eq!(stdout_of("derived_inner.nr", &derived), expected);
    assert_eq!(stdout_of("hand_inner.nr", &by_hand), expected);
}

/// A `Copy` value's `.clone()` is a copy, so cloning a `Copy` struct calls no field's
/// hand-written `clone`, at the top or nested inside a non-`Copy` struct, while that
/// struct's own non-`Copy` field still has its `clone` called. The receiver may be a
/// binding, a temporary or a borrow.
#[test]
fn a_copy_struct_clone_is_a_copy_whatever_its_fields_clone_does() {
    let source = r#"
struct P {
    x: i32
}

impl Clone for P {
    func clone(&self) -> Self {
        print("[p]")
        P { x: self.x + 100 }
    }
}

impl Copy for P {}

@derive(Copy, Clone)
struct Q {
    p: P,
    n: i32
}

struct Hand {
    x: i32,
    tag: string
}

impl Clone for Hand {
    func clone(&self) -> Self {
        print("[h]")
        Hand { x: self.x + 1, tag: self.tag.clone() }
    }
}

@derive(Clone)
struct Outer {
    q: Q,
    h: Hand
}

func make() -> Outer {
    Outer { q: Q { p: P { x: 1 }, n: 2 }, h: Hand { x: 10, tag: "t" } }
}

func through(r: &Outer) -> Outer {
    r.clone()
}

func main() -> i32 {
    val q = Q { p: P { x: 1 }, n: 2 }
    val q2 = q.clone()
    println("|{q2.p.x}")
    val o = make()
    val o2 = o.clone()
    println("|{o2.q.p.x} {o2.h.x}")
    val o3 = make().clone()
    println("|{o3.h.x} {o3.h.tag}")
    val o4 = through(&o)
    println("|{o4.h.x} {o.h.tag}")
    q2.p.x + o2.q.p.x + o2.h.x + o3.h.x + o4.h.x
}
"#;
    assert_eq!(
        stdout_of("copy_clone_fields.nr", source),
        "|1\n[h]|1 11\n[h]|11 t\n[h]|11 t\n"
    );
    assert_eq!(run("copy_clone_fields_exit.nr", source), 35);
}

/// A derived clone lends its receiver to the field-by-field impl as a hand-written `clone`
/// is lent it: a borrow returned by a call, an array element and a temporary are read,
/// never taken over, so nothing is freed or dropped twice and each `Drop` runs once, at
/// its owner's scope exit.
#[test]
fn a_derived_clone_borrows_its_receiver_like_a_hand_written_one() {
    let borrowed = r#"
struct Hand {
    v: Vec<i32>
}

impl Clone for Hand {
    func clone(&self) -> Self {
        print("c")
        mut w: Vec<i32> = Vec::new()
        w.push(self.v[0] + 1)
        Hand { v: w }
    }
}

@derive(Clone)
struct Outer {
    h: Hand,
    k: i32
}

func get(o: &Outer) -> &Outer {
    o
}

func run() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(10)
    val o = Outer { h: Hand { v: v }, k: 2 }
    val a = get(&o).clone()
    print("|")
    a.h.v[0] + o.h.v[0]
}

func main() -> i32 {
    val r = run()
    println("")
    r
}
"#;
    assert_eq!(stdout_of("borrowed_receiver.nr", borrowed), "c|\n");
    assert_eq!(run("borrowed_receiver_exit.nr", borrowed), 21);
    let dropped = r#"
struct Hand {
    n: i32
}

impl Clone for Hand {
    func clone(&self) -> Self {
        print("c")
        Hand { n: self.n + 1 }
    }
}

impl Drop for Hand {
    func drop(&mut self) {
        print("d{self.n}")
    }
}

@derive(Clone)
struct Outer {
    h: Hand,
    k: i32
}

func get(o: &Outer) -> &Outer {
    o
}

func make(n: i32) -> Outer {
    Outer { h: Hand { n: n }, k: 1 }
}

func run() -> i32 {
    val o = Outer { h: Hand { n: 10 }, k: 2 }
    val arr = [Outer { h: Hand { n: 30 }, k: 3 }]
    val a = arr[0].clone()
    print("|")
    val b = make(20).clone()
    print("|")
    a.h.n + b.h.n
}

func main() -> i32 {
    val r = run()
    println("")
    r
}
"#;
    assert_eq!(
        stdout_of("dropped_receiver.nr", dropped),
        "c|cd20|d21d31d30d10\n"
    );
    assert_eq!(run("dropped_receiver_exit.nr", dropped), 52);
    let indexed = r#"
struct Hand {
    v: Vec<i32>
}

impl Clone for Hand {
    func clone(&self) -> Self {
        print("c")
        mut w: Vec<i32> = Vec::new()
        w.push(self.v[0] + 1)
        Hand { v: w }
    }
}

@derive(Clone)
struct Mid {
    h: Hand
}

func one(n: i32) -> Vec<i32> {
    mut v: Vec<i32> = Vec::new()
    v.push(n)
    v
}

func run() -> i32 {
    val arr = [Mid { h: Hand { v: one(30) } }]
    val q = arr[0].clone()
    print("|")
    q.h.v[0] + arr[0].h.v[0]
}

func main() -> i32 {
    val r = run()
    println("")
    r
}
"#;
    assert_eq!(stdout_of("indexed_receiver.nr", indexed), "c|\n");
    assert_eq!(run("indexed_receiver_exit.nr", indexed), 61);
}

/// A tuple, an array and a newtype are cloned by their bytes, so a struct inside one
/// whose derived clone calls a field's `clone` is refused there, as a hand-written one is.
#[test]
fn a_struct_cloned_by_its_fields_cannot_sit_in_a_tuple_or_array() {
    let err = rejected(
        "held_by_bytes.nr",
        r#"
struct Hand {
    n: i32,
    tag: string
}

impl Clone for Hand {
    func clone(&self) -> Self {
        print("c")
        Hand { n: self.n + 1, tag: self.tag.clone() }
    }
}

@derive(Clone)
struct Mid {
    h: Hand
}

@derive(Clone)
struct Outer {
    arr: [Mid; 2],
    t: (Mid, i32)
}

func main() -> i32 {
    val o = Outer { arr: [Mid { h: Hand { n: 1, tag: "a" + "b" } }, Mid { h: Hand { n: 2, tag: "c" + "d" } }], t: (Mid { h: Hand { n: 3, tag: "e" + "f" } }, 4) }
    val p = o.clone()
    println("")
    println(p.arr[0].h.tag)
    p.arr[0].h.n + p.arr[1].h.n + p.t.0.h.n
}
"#,
    );
    assert!(
        err.contains("field 't' has type (Mid, i32), which is copied by its bytes"),
        "got: {err}"
    );
    assert!(
        err.contains("field 'arr' has type [Mid; 2], which is copied by its bytes"),
        "got: {err}"
    );
}

/// The derived `==` calls a field's `eq` only through its `PartialEq` impl: an inherent
/// method of that name on a field that derives the trait is not its comparison.
#[test]
fn a_derived_eq_ignores_an_inherent_eq_on_a_field() {
    let same_shape = r#"
@derive(PartialEq)
struct Inner {
    x: i32
}

impl Inner {
    func eq(&self, rhs: &Inner) -> bool {
        false
    }
}

@derive(PartialEq)
struct Outer {
    i: Inner,
    k: i32
}

func main() -> i32 {
    val a = Outer { i: Inner { x: 1 }, k: 2 }
    val b = Outer { i: Inner { x: 1 }, k: 2 }
    mut r = 0
    if a.i == b.i {
        r = r + 1
    }
    if a == b {
        r = r + 10
    }
    r
}
"#;
    assert_eq!(run("inherent_eq.nr", same_shape), 11);
    let other_shape = r#"
@derive(PartialEq)
struct Inner {
    x: i32
}

impl Inner {
    func eq(&self, rhs: i64) -> i32 {
        7
    }
}

@derive(PartialEq)
struct Outer {
    i: Inner,
    k: i32
}

func main() -> i32 {
    val a = Outer { i: Inner { x: 1 }, k: 2 }
    val b = Outer { i: Inner { x: 1 }, k: 2 }
    mut r = 0
    if a.i.x == b.i.x {
        r = r + 1
    }
    if a == b {
        r = r + 10
    }
    r
}
"#;
    assert_eq!(run("inherent_eq_shape.nr", other_shape), 11);
}

/// A refused operator impl is reported once: the derive holding the type is not told to
/// give it a derive as well.
#[test]
fn a_refused_field_impl_is_not_reported_missing_again() {
    let err = rejected(
        "refused_field_impl.nr",
        r#"
struct Hand {
    tag: string
}

impl PartialEq for Hand {
    func eq(&self, rhs: &Hand) -> bool {
        true
    }
}

@derive(PartialEq)
struct Outer {
    h: Hand
}

func main() -> i32 {
    0
}
"#,
    );
    assert!(
        err.contains("operator trait 'PartialEq' can only be implemented for a `Copy` type"),
        "got: {err}"
    );
    assert!(!err.contains("cannot derive"), "got: {err}");
}

/// A derive calls a field's `clone` only through its `impl Clone`, the rule the checker
/// and lowering share: an inherent method of that name on a field that derives `Clone` is
/// not its clone, so the struct holding it, alone or inside a tuple, is copied.
#[test]
fn an_inherent_clone_on_a_field_is_not_its_clone_impl() {
    let source = r#"
@derive(Clone)
struct H { x: i32 }
impl H {
    func clone(&self) -> H {
        print("i")
        H { x: self.x + 1 }
    }
}
@derive(Clone)
struct M { h: H }
@derive(Clone)
struct O { t: (M, i32) }
func main() -> i32 {
    val m = M { h: H { x: 1 } }
    val m2 = m.clone()
    val o = O { t: (M { h: H { x: 5 } }, 2) }
    val o2 = o.clone()
    println("")
    m2.h.x + o2.t.0.h.x
}
"#;
    assert_eq!(stdout_of("inherent_clone.nr", source), "\n");
    assert_eq!(run("inherent_clone_exit.nr", source), 6);
}

/// A newtype field's hand-written `Clone` is called by the derive, so a struct holding one
/// cannot sit in a tuple, which is copied by its bytes; the refusal says so rather than
/// blaming a buffer.
#[test]
fn a_newtype_field_clone_is_called_and_a_byte_copy_of_it_refused() {
    let called = r#"
newtype N = i32

impl Clone for N {
    func clone(&self) -> Self {
        print("n")
        N(self.0 + 1)
    }
}

@derive(Clone)
struct S {
    n: N,
    k: i32
}

func main() -> i32 {
    val s = S { n: N(1), k: 2 }
    val c = s.clone()
    println("")
    c.n.0 + c.k
}
"#;
    assert_eq!(stdout_of("newtype_clone.nr", called), "n\n");
    assert_eq!(run("newtype_clone_exit.nr", called), 4);
    let err = rejected(
        "newtype_in_tuple.nr",
        r#"
newtype N = i32
impl Clone for N {
    func clone(&self) -> Self {
        print("n")
        N(self.0 + 1)
    }
}
@derive(Clone)
struct S { n: N }
@derive(Clone)
struct O { t: (S, i32) }
func main() -> i32 {
    val o = O { t: (S { n: N(1) }, 2) }
    val c = o.clone()
    println("")
    c.t.0.n.0 + c.t.1
}
"#,
    );
    assert!(
        err.contains(
            "is copied by its bytes, so the `clone` a struct inside it calls would be skipped"
        ),
        "got: {err}"
    );
    assert!(!err.contains("owns a buffer"), "got: {err}");
}

/// A derived or builtin implementor coerces to `&dyn Debug` / `&dyn Hashable` like a
/// hand-written one: the vtable's methods are the ones the compiler supplies, and a
/// generic body's `&T` coerces per instance.
#[test]
fn regression_bug_119_a_derived_or_builtin_implementor_is_a_trait_object() {
    let source = r#"
@derive(Debug, Hashable, PartialEq, Copy, Clone)
struct P {
    x: i32,
    y: i32
}

struct Name {
    s: string
}

impl Debug for Name {
    func fmt_debug(&self, out: &mut StringBuilder) { out.push_str("Name<{self.s}>") }
}

@derive(Debug)
struct Outer {
    n: Name,
    k: bool
}

func render(d: &dyn Debug) {
    mut out = StringBuilder::new()
    d.fmt_debug(&mut out)
    println(out.to_string())
}

func via<T: Debug>(x: &T) { render(x) }

func hd(d: &dyn Hashable) -> u64 { d.hash() }

func main() -> i32 {
    val p = P { x: 1, y: 2 }
    val o = Outer { n: Name { s: "z" }, k: true }
    val n = 42
    val c = 'q'
    render(&p)
    render(&o)
    render(&n)
    render(&c)
    via(&p)
    via(&n)
    mut m: HashMap<P, i32> = HashMap::new()
    m.insert(p, 5)
    val same = hd(&p) == p.hash() && hd(&n) == n.hash()
    val marker: &dyn Copy = &p
    (m.get(p) ?? 0) + same as i32
}
"#;
    assert_eq!(
        stdout_of("dyn_supplied.nr", source),
        "P { x: 1, y: 2 }\nOuter { n: Name<z>, k: true }\n42\n'q'\nP { x: 1, y: 2 }\n42\n"
    );
    assert_eq!(run("dyn_supplied_exit.nr", source), 6);
}

/// `fmt_debug` called on a `&dyn Debug` goes through the vtable. It was lowered as the
/// derive's `{x:?}` rendering of the trait object itself, an internal compiler error.
#[test]
fn regression_bug_120_fmt_debug_on_a_trait_object_dispatches() {
    let source = r#"
struct Name {
    s: string
}

impl Debug for Name {
    func fmt_debug(&self, out: &mut StringBuilder) { out.push_str("Name<{self.s}>") }
}

func render(d: &dyn Debug) -> i32 {
    mut out = StringBuilder::new()
    d.fmt_debug(&mut out)
    out.to_string().len() as i32
}

func main() -> i32 {
    val n = Name { s: "z" }
    render(&n)
}
"#;
    assert_eq!(run("dyn_fmt_debug.nr", source), 7);
}

/// A `&string` is a fat pointer, so it is no trait object's data pointer: refused
/// written directly, and reported (not an internal error) through a generic instance.
#[test]
fn a_string_is_not_a_trait_object() {
    let direct = rejected(
        "dyn_string.nr",
        r#"
func render(d: &dyn Debug) {}
func main() -> i32 {
    val s = "hi"
    render(&s)
    0
}
"#,
    );
    assert!(
        direct.contains("expected &dyn Debug, found &string"),
        "got: {direct}"
    );
    let generic = rejected(
        "dyn_string_generic.nr",
        r#"
func render(d: &dyn Debug) {}
func via<T: Debug>(x: &T) { render(x) }
func main() -> i32 {
    val s = "hi"
    via(&s)
    0
}
"#,
    );
    assert!(
        generic.contains("a `&string` cannot become a `&dyn Debug`"),
        "got: {generic}"
    );
}

/// A generic struct's instance named first by a struct field (`w: Wrap<i32>`) is made
/// before the generic impls are registered. It received none of them, so the derive
/// refused the field and a bound refused every later use of the same instance.
#[test]
fn regression_bug_123_an_instance_named_by_a_field_gets_the_generic_impls() {
    let source = r#"
struct Wrap<T> {
    v: T
}

impl<T: Debug> Debug for Wrap<T> {
    func fmt_debug(&self, out: &mut StringBuilder) { out.push_str("W{self.v:?}") }
}

func show<T: Debug>(x: &T) -> i32 {
    mut out = StringBuilder::new()
    x.fmt_debug(&mut out)
    out.to_string().len() as i32
}

@derive(Debug)
struct S {
    w: Wrap<i32>,
    k: i32
}

func main() -> i32 {
    val s = S { w: Wrap { v: 4 }, k: 5 }
    println("{s:?}")
    val w = Wrap { v: 30 }
    show(&w)
}
"#;
    assert_eq!(
        stdout_of("field_instance.nr", source),
        "S { w: W4, k: 5 }\n"
    );
    assert_eq!(run("field_instance_exit.nr", source), 3);
}

/// A newtype field with a `Debug`, `PartialEq` or `Hashable` of its own is rendered,
/// compared and hashed by it inside a derive, as a struct field is. The backend erases a
/// newtype to its inner value, so the derive read the inner value instead.
#[test]
fn a_newtype_field_impl_is_called_by_the_derive() {
    let source = r#"
newtype Celsius = i32

impl Debug for Celsius {
    func fmt_debug(&self, out: &mut StringBuilder) { out.push_str("{self.0}C") }
}

impl PartialEq for Celsius {
    func eq(&self, rhs: &Celsius) -> bool { self.0 / 10 == rhs.0 / 10 }
}

impl Hashable for Celsius {
    func hash(&self) -> u64 { (self.0 / 10) as u64 }
}

newtype Plain = i32

@derive(Debug, PartialEq, Hashable)
struct Reading {
    t: Celsius,
    p: Plain,
    k: i32
}

func main() -> i32 {
    val a = Reading { t: Celsius(21), p: Plain(1), k: 3 }
    val b = Reading { t: Celsius(25), p: Plain(1), k: 3 }
    val c = Reading { t: Celsius(31), p: Plain(1), k: 3 }
    println("{a:?}")
    val score = (a == b) as i32 + (a == c) as i32 * 2 + (a.hash() == b.hash()) as i32 * 4
    mut m: HashMap<Reading, i32> = HashMap::new()
    m.insert(a, 7)
    val hit = m.get(b) ?? 0
    val miss = m.get(c) ?? 0
    score + hit * 8 + miss
}
"#;
    assert_eq!(
        stdout_of("newtype_field.nr", source),
        "Reading { t: 21C, p: 1, k: 3 }\n"
    );
    assert_eq!(run("newtype_field_exit.nr", source), 61);
}
