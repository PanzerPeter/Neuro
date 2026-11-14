// `std::prelude`: the module every Neuro module imports implicitly, compiled into every
// program.
//
// These declarations are ordinary Neuro source, parsed by the same parser as user code and
// loaded as a module of the standard library. A module's own declaration of the same name
// shadows the entry here inside that module, so a program may define its own `Option` or
// `Result` beside these.

// The absence-or-presence type: `Option::Some(v)` carries a value, `Option::None`
// carries nothing.
export enum Option<T> {
    Some(T),
    None
}

// The success-or-failure type: `Result::Ok(v)` carries the produced value,
// `Result::Err(e)` the reason it failed.
export enum Result<T, E> {
    Ok(T),
    Err(E)
}

// The method surface that keeps a `match` out of the common cases. Each method consumes
// its receiver except the `is_*` tests, which only look at the tag. A callback is
// `once` because each is called at most one time, so any closure is accepted.
impl<T> Option<T> {
    func map<U>(self, f: once (T) -> U) -> Option<U> {
        match self {
            Option::Some(v) => Option::Some(f(v)),
            Option::None => Option::None
        }
    }
    func ok_or<E>(self, e: E) -> Result<T, E> {
        match self {
            Option::Some(v) => Result::Ok(v),
            Option::None => Result::Err(e)
        }
    }
    func is_some(&self) -> bool {
        match self {
            Option::Some(_) => true,
            Option::None => false
        }
    }
    func is_none(&self) -> bool {
        match self {
            Option::Some(_) => false,
            Option::None => true
        }
    }
}

impl<T, E> Result<T, E> {
    func map<U>(self, f: once (T) -> U) -> Result<U, E> {
        match self {
            Result::Ok(v) => Result::Ok(f(v)),
            Result::Err(e) => Result::Err(e)
        }
    }
    func map_err<F>(self, f: once (E) -> F) -> Result<T, F> {
        match self {
            Result::Ok(v) => Result::Ok(v),
            Result::Err(e) => Result::Err(f(e))
        }
    }
    func ok(self) -> Option<T> {
        match self {
            Result::Ok(v) => Option::Some(v),
            Result::Err(_) => Option::None
        }
    }
    func is_ok(&self) -> bool {
        match self {
            Result::Ok(_) => true,
            Result::Err(_) => false
        }
    }
    func is_err(&self) -> bool {
        match self {
            Result::Ok(_) => false,
            Result::Err(_) => true
        }
    }
}

// Why `string.parse::<T>()` refused its text. The backend builds these by name, and the
// driver leaves them out of the names a module may write bare: a word as plain as `Empty`
// stays the program's own.
export enum ParseError {
    Empty,
    Invalid,
    OutOfRange
}

// The derivable traits. `@derive` writes each of these impls field by field; a program
// may write one by hand instead and name any of them as a bound. The scalars, `string`,
// and a struct carrying the derive implement them with no impl block here.
//
// `Copy` has no methods: it is the promise that a value duplicates instead of moving.
export trait Copy: Clone {}

export trait Clone {
    func clone(&self) -> Self
}

// `{x:?}` renders through this: it appends the value's debug form to `out`.
export trait Debug {
    func fmt_debug(&self, out: &mut StringBuilder)
}

// What a `HashMap` key hashes with. Two values `==` treats as equal must hash alike.
export trait Hashable {
    func hash(&self) -> u64
}

impl Debug for ParseError {
    func fmt_debug(&self, out: &mut StringBuilder) {
        match self {
            ParseError::Empty => out.push_str("Empty"),
            ParseError::Invalid => out.push_str("Invalid"),
            ParseError::OutOfRange => out.push_str("OutOfRange")
        }
    }
}

// Total-order wrappers over the floating-point types.
//
// `<` on a raw float is IEEE-754, which is a partial order: NaN compares false against
// everything, including itself. An ordered map keyed on a raw float would therefore be
// able to hold a key it can never find again. These wrappers restore a total order by
// excluding NaN at construction, which is why they are validating structs rather than
// newtypes: a newtype has no construction-time hook. They implement `Comparable`, so
// they are ordered-map keys; hashing a float has no representation-independent answer,
// so they deliberately do not implement `Hashable`.
@derive(Copy, Clone)
export struct OrderedF32 {
    export value: f32
}

impl OrderedF32 {
    func new(value: f32) -> OrderedF32 {
        if value.is_nan() {
            panic("OrderedF32 cannot hold NaN")
        }
        OrderedF32 { value: value }
    }
}

impl PartialEq for OrderedF32 {
    func eq(&self, rhs: &OrderedF32) -> bool {
        self.value == rhs.value
    }
    func ne(&self, rhs: &OrderedF32) -> bool {
        self.value != rhs.value
    }
}

impl Comparable for OrderedF32 {
    func lt(&self, rhs: &OrderedF32) -> bool {
        self.value < rhs.value
    }
    func le(&self, rhs: &OrderedF32) -> bool {
        self.value <= rhs.value
    }
    func gt(&self, rhs: &OrderedF32) -> bool {
        self.value > rhs.value
    }
    func ge(&self, rhs: &OrderedF32) -> bool {
        self.value >= rhs.value
    }
}

@derive(Copy, Clone)
export struct OrderedF64 {
    export value: f64
}

impl OrderedF64 {
    func new(value: f64) -> OrderedF64 {
        if value.is_nan() {
            panic("OrderedF64 cannot hold NaN")
        }
        OrderedF64 { value: value }
    }
}

impl PartialEq for OrderedF64 {
    func eq(&self, rhs: &OrderedF64) -> bool {
        self.value == rhs.value
    }
    func ne(&self, rhs: &OrderedF64) -> bool {
        self.value != rhs.value
    }
}

impl Comparable for OrderedF64 {
    func lt(&self, rhs: &OrderedF64) -> bool {
        self.value < rhs.value
    }
    func le(&self, rhs: &OrderedF64) -> bool {
        self.value <= rhs.value
    }
    func gt(&self, rhs: &OrderedF64) -> bool {
        self.value > rhs.value
    }
    func ge(&self, rhs: &OrderedF64) -> bool {
        self.value >= rhs.value
    }
}

// The iteration protocol `for` desugars against.
//
// `for x in e` calls `e.into_iter()` once and then `.next()` until it answers
// `None`. A type implementing `Iterator` is its own iterator: the compiler uses it
// directly rather than demanding a second, identical `IntoIterator` impl, which is
// the blanket `impl<I: Iterator> IntoIterator for I` written as a rule because a
// blanket impl is not yet expressible here.
//
// `type Iter` carries no `: Iterator` bound: a bound on an associated-type
// DECLARATION has no syntax yet, so the constraint is enforced where the loop is
// built, by requiring the returned type to have a `next`.
export trait Iterator {
    type Item
    func next(&mut self) -> Option<Self::Item>
}

export trait IntoIterator {
    type Item
    type Iter
    func into_iter(self) -> Self::Iter
}

// The adapters `.map(f)`, `.filter(p)` and `.enumerate()` build over any `Iterator`.
//
// The compiler builds these literals itself when an iterator has no method of that name, so
// the fields stay private. They are not default methods on `Iterator`: a default method
// returning `Filter<Self, ...>` would be registered for `Filter<Self, ...>` too, and that
// registration has no end. Each type carries its element type `A` as a parameter, because a
// field cannot name the associated type `I::Item`.
//
// The function is `mut`, so a closure that writes what it captured can drive the adapter.
// It is called through `&mut self` and never through a shared borrow.
export struct Map<I, A, B> {
    inner: I,
    f: mut (A) -> B
}

impl<I: Iterator<Item = A>, A, B> Iterator for Map<I, A, B> {
    type Item = B
    func next(&mut self) -> Option<B> {
        match self.inner.next() {
            Option::Some(value) => Option::Some((self.f)(value)),
            Option::None => Option::None
        }
    }
}

// The predicate takes a borrow: the element it accepts is still the one `next` returns.
export struct Filter<I, A> {
    inner: I,
    keep: mut (&A) -> bool
}

impl<I: Iterator<Item = A>, A> Iterator for Filter<I, A> {
    type Item = A
    func next(&mut self) -> Option<A> {
        loop {
            match self.inner.next() {
                Option::Some(value) => {
                    if (self.keep)(&value) {
                        break Option::Some(value)
                    }
                }
                Option::None => { break Option::None }
            }
        }
    }
}

// The position counts what this adapter has yielded, from zero.
export struct Enumerate<I, A> {
    inner: I,
    position: u64
}

impl<I: Iterator<Item = A>, A> Iterator for Enumerate<I, A> {
    type Item = (u64, A)
    func next(&mut self) -> Option<(u64, A)> {
        match self.inner.next() {
            Option::Some(value) => {
                val at = self.position
                self.position = self.position + 1
                Option::Some((at, value))
            }
            Option::None => Option::None
        }
    }
}

// The value `start..end` and `start..=end` build outside a `for` head: the integers
// from `start` up to `end`, which `inclusive` says whether to reach.
//
// The walk is written against two prelude-private intrinsics on the element type,
// because a generic body has no integer arithmetic yet: `x.__to_u64()` is `x as u64`
// (sign-extended), and `x.__offset(k)` adds `k` to `x` modulo the type's width. The
// distance between two values of any integer type fits a `u64` that way, so a walk to
// the type's last value never steps past it. Only integer bounds build a `Range`.
//
// `stride` and `reversed` are what `.step(n)` and `.rev()` set before handing the walk
// on as a `Step` or `Rev`; `done` marks an inclusive walk that has yielded its last
// value, which no bound can say once `start` has reached the type's maximum.
export struct Range<T> {
    start: T,
    end: T,
    inclusive: bool,
    stride: u64,
    reversed: bool,
    done: bool
}

impl<T: Comparable + Copy> Iterator for Range<T> {
    type Item = T
    func next(&mut self) -> Option<T> {
        if self.done || self.end < self.start || (!self.inclusive && !(self.start < self.end)) {
            self.done = true
            return Option::None
        }
        // The offset of the walk's last value from `start`.
        mut last = self.end.__to_u64().wrapping_sub(self.start.__to_u64())
        if !self.inclusive {
            last = last - 1
        }
        val value = if self.reversed { self.start.__offset(last) } else { self.start }
        if last < self.stride {
            self.done = true
        } else if self.reversed {
            self.end = self.start.__offset(last - self.stride)
            self.inclusive = true
        } else {
            self.start = self.start.__offset(self.stride)
        }
        Option::Some(value)
    }
}

impl<T: Comparable + Copy> Range<T> {
    // The same values from the top down. What has been walked stays walked.
    func rev(self) -> Rev<T> {
        mut range = self
        range.reversed = true
        Rev { range: range }
    }
    // Every `n`th value, counting from the first. `n` has the element type, as the
    // `for`-head form's has, so its zero is reached through the intrinsics: `n` plus
    // its own negation. The check is the one a head's `.step(n)` makes where its loop
    // starts.
    func step(self, n: T) -> Step<T> {
        val zero = n.__offset((0 as u64).wrapping_sub(n.__to_u64()))
        if n <= zero {
            panic("range step must be positive")
        }
        mut range = self
        range.stride = n.__to_u64()
        Step { range: range }
    }
}

// `range.rev()`: a `Range` walked from its last value down.
export struct Rev<T> {
    range: Range<T>
}

impl<T: Comparable + Copy> Iterator for Rev<T> {
    type Item = T
    func next(&mut self) -> Option<T> {
        self.range.next()
    }
}

impl<T: Comparable + Copy> Rev<T> {
    // Every `n`th value of the downward walk, from the last one.
    func step(self, n: T) -> Step<T> {
        self.range.step(n)
    }
}

// `range.step(n)` and `range.rev().step(n)`. It has no `.rev()`: striding down from the
// top visits different values than reversing an upward stride, and only the first is
// written `.rev().step(n)`.
export struct Step<T> {
    range: Range<T>
}

impl<T: Comparable + Copy> Iterator for Step<T> {
    type Item = T
    func next(&mut self) -> Option<T> {
        self.range.next()
    }
}

// The iterator `string.chars()` hands out: a cursor over one borrowed string's
// Unicode scalar values.
//
// The text is borrowed, never owned, so walking a string neither copies nor consumes
// it. `.chars()` is the only way to build one: the fields are private, and the
// compiler fills them in.
export struct Chars {
    source: &string,
    offset: u64
}

impl Iterator for Chars {
    type Item = char
    func next(&mut self) -> Option<char> {
        if self.offset >= self.source.len() {
            return Option::None
        }
        val scalar = self.source.__char_at(self.offset)
        // UTF-8 spends as many bytes on a scalar as its magnitude needs, so the step
        // to the next code point follows from the value just decoded and costs no
        // second pass over the text. This is what keeps a step O(1).
        val code = scalar as u32
        mut width: u64 = 1
        if code >= 128 {
            width = 2
        }
        if code >= 2048 {
            width = 3
        }
        if code >= 65536 {
            width = 4
        }
        self.offset = self.offset + width
        Option::Some(scalar)
    }
}

// The device a tensor's buffer lives on, and the argument `tensor.to(device)` takes.
//
// The backend finds the host variant by its name and treats every other variant as a
// GPU whose index is its one payload field, so renaming `CPU` or reshaping `GPU`
// breaks every transfer.
export enum Device {
    CPU,
    GPU(i32)
}

// The arena a `pool` block allocates from, as a type a pool-aware constructor can
// name. It carries the arena's mark and nothing else: a program never builds one,
// and the only thing it is useful for is being handed to `register_with_pool`.
export struct PoolHandle {
    id: u64
}

// The opt-in that lets a type holding an external resource live in a `pool`.
//
// A pool releases its arena in one store at the closing brace, so it cannot run an
// arbitrary destructor per object without giving up the O(1) free. A type that owns
// only memory needs no destructor and is allowed in a pool as it stands; a type that
// implements `Drop` and nothing else is rejected from one. Implementing `PoolAware` is
// how such a type says its release can be batched: it registers its handle with the
// arena at construction, and the arena releases every registered handle in one sweep
// at pool exit, in reverse registration order so a resource is never released before
// something queued on it.
export trait PoolAware {
    func register_with_pool(&self, arena: &PoolHandle)
    func bulk_release(&mut self)
}
