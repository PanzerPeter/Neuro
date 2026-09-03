// Implicit standard library — compiled into every program.
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
// newtypes — a newtype has no construction-time hook. They implement `Comparable`, so
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
