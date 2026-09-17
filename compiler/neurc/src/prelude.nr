// Implicit standard library: compiled into every program.
//
// These declarations are ordinary Neuro source, parsed by the same parser as user
// code and prepended to the program's items. A user declaration of the same name
// replaces the entry here, so a program may define its own `Option` or `Result`.

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
        // NaN is the only value that differs from itself.
        if value != value {
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
        if value != value {
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
// `CPU` is declared first because it is the only device the compiler can lower to
// today: the backend reads its discriminant to decide whether a transfer is a no-op
// or an abort, so reordering these variants changes what `.to()` accepts.
export enum Device {
    CPU,
    GPU(i32)
}

// The arena a `pool` block allocates from, as a type a pool-aware constructor can
// name. It carries the arena's identity and nothing else: a program never builds one,
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
