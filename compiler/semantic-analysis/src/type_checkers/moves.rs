//! Move-by-default ownership analysis.
//!
//! A non-`Copy` value is *moved* out of its source binding when it is placed
//! into a new owner: the initializer of a `val`/`mut`, the right-hand side of an
//! assignment, a `return` value, or a by-value call argument. After a move the
//! source binding is invalid, and reading it is a `UseOfMovedValue` error
//! (emitted from the `Expr::Identifier` arm in `expressions.rs`).
//!
//! The analysis is intentionally conservative: it flags only direct place
//! expressions in a consuming position, and conditional regions snapshot/restore
//! their move state (see `SymbolTable::snapshot_moves`). It may therefore miss
//! some moves, but it never rejects a valid program.

use ast_types::Expr;

use super::TypeChecker;

impl TypeChecker {
    /// Record the move that occurs when `expr` appears in a consuming position.
    ///
    /// Moves apply only to a bare place expression — an identifier, possibly
    /// wrapped in parentheses — whose binding has a move-tracked type. A literal,
    /// a `.clone()` call, or any compound expression produces a fresh value and
    /// moves nothing here; nested consuming positions (e.g. an argument inside a
    /// call) are handled where that call's arguments are checked.
    pub(crate) fn record_move(&mut self, expr: &Expr) {
        let mut place = expr;
        while let Expr::Paren(inner, _) = place {
            place = inner;
        }

        let Expr::Identifier(ident) = place else {
            return;
        };

        // A constant is a value, not an owner, so it cannot be moved from.
        let binding_ty = self.symbols.lookup(&ident.name).map(|info| info.ty.clone());

        if binding_ty.is_some_and(|ty| self.is_type_move_tracked(&ty)) {
            self.symbols.mark_moved(&ident.name, ident.span);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::type_check;
    use syntax_parsing::parse;

    fn errors(source: &str) -> Vec<String> {
        let ast = parse(source).expect("source should parse");
        match type_check(&ast) {
            Ok(_) => Vec::new(),
            Err(errs) => errs.iter().map(|e| e.to_string()).collect(),
        }
    }

    #[test]
    fn move_then_use_is_rejected() {
        let errs = errors(
            r#"
            func main() -> i32 {
                val s1: string = "Hello"
                val s2: string = s1
                val n: u64 = s1.len()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("use of moved value 's1'")),
            "expected use-of-moved diagnostic, got {errs:?}"
        );
    }

    #[test]
    fn move_into_call_then_use_is_rejected() {
        let errs = errors(
            r#"
            func consume(s: string) -> i32 { 0 }
            func main() -> i32 {
                val greeting: string = "Hi"
                val r: i32 = consume(greeting)
                val n: u64 = greeting.len()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("use of moved value 'greeting'")),
            "expected use-of-moved diagnostic, got {errs:?}"
        );
    }

    #[test]
    fn clone_does_not_move_receiver() {
        let errs = errors(
            r#"
            func main() -> i32 {
                val a: string = "hello"
                val b: string = a.clone()
                if a == b {
                    return 1
                }
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "clone must not move; got {errs:?}");
    }

    #[test]
    fn copy_scalars_do_not_move() {
        let errs = errors(
            r#"
            func main() -> i32 {
                val a: i32 = 5
                val b: i32 = a
                val c: i32 = a + b
                return c
            }
            "#,
        );
        assert!(errs.is_empty(), "scalars are Copy; got {errs:?}");
    }

    #[test]
    fn conditional_move_does_not_leak_out_of_branch() {
        // `s` is moved only on the `if` path; the later read is on a path that
        // may not have executed the move, so it must not be rejected.
        let errs = errors(
            r#"
            func consume(s: string) -> i32 { 0 }
            func main() -> i32 {
                val s: string = "hi"
                if true {
                    val r: i32 = consume(s)
                }
                val n: u64 = s.len()
                return 0
            }
            "#,
        );
        assert!(
            errs.is_empty(),
            "conditional move must not leak past the branch; got {errs:?}"
        );
    }

    #[test]
    fn non_copy_struct_move_then_use_is_rejected() {
        let errs = errors(
            r#"
            struct Point { x: i32, y: i32 }
            func main() -> i32 {
                val a = Point { x: 1, y: 2 }
                val b = a
                val r = a.x
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("use of moved value 'a'")),
            "a non-Copy struct must move; got {errs:?}"
        );
    }

    #[test]
    fn copy_struct_does_not_move() {
        let errs = errors(
            r#"
            @derive(Copy, Clone)
            struct Point { x: i32, y: i32 }
            func main() -> i32 {
                val a = Point { x: 1, y: 2 }
                val b = a
                val r = a.x
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "a Copy struct must not move; got {errs:?}");
    }

    #[test]
    fn derive_copy_with_non_copy_field_is_rejected() {
        let errs = errors(
            r#"
            @derive(Copy)
            struct Holder { name: string }
            func main() -> i32 { 0 }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("cannot derive Copy")),
            "Copy with a string field must be rejected; got {errs:?}"
        );
    }

    #[test]
    fn struct_clone_does_not_move_receiver() {
        let errs = errors(
            r#"
            @derive(Clone)
            struct Point { x: i32, y: i32 }
            func main() -> i32 {
                val a = Point { x: 1, y: 2 }
                val b = a.clone()
                val r = a.x
                return r
            }
            "#,
        );
        assert!(
            errs.is_empty(),
            "clone must not move the receiver; got {errs:?}"
        );
    }

    #[test]
    fn clone_on_non_clone_struct_is_rejected() {
        let errs = errors(
            r#"
            struct Point { x: i32, y: i32 }
            func main() -> i32 {
                val a = Point { x: 1, y: 2 }
                val b = a.clone()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("method") || e.contains("clone")),
            "clone on a non-Clone struct must be rejected; got {errs:?}"
        );
    }

    #[test]
    fn borrowing_a_string_does_not_move_it() {
        // Passing `&s` borrows rather than moves, so `s` stays usable afterward.
        let errs = errors(
            r#"
            func describe(s: &string) -> u64 { s.len() }
            func main() -> i32 {
                val s: string = "hello"
                val n: u64 = describe(&s)
                val m: u64 = describe(&s)
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "borrowing must not move; got {errs:?}");
    }

    #[test]
    fn borrowing_a_struct_does_not_move_it() {
        let errs = errors(
            r#"
            struct Point { x: i32, y: i32 }
            func read(p: &Point) -> i32 { p.x }
            func main() -> i32 {
                val pt = Point { x: 1, y: 2 }
                val a: i32 = read(&pt)
                val b: i32 = read(&pt)
                return 0
            }
            "#,
        );
        assert!(
            errs.is_empty(),
            "borrowing a struct must not move it; got {errs:?}"
        );
    }

    #[test]
    fn borrowing_a_temporary_is_rejected() {
        let errs = errors(
            r#"
            func main() -> i32 {
                val r = &5
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("cannot borrow")),
            "borrowing a literal must be rejected; got {errs:?}"
        );
    }

    #[test]
    fn mutable_borrow_of_a_mut_binding_is_accepted() {
        // `&mut` of a `mut` binding type-checks, and `*r` reads/writes through it.
        let errs = errors(
            r#"
            func main() -> i32 {
                mut x: i32 = 5
                val r: &mut i32 = &mut x
                *r = 9
                return *r
            }
            "#,
        );
        assert!(
            errs.is_empty(),
            "&mut of a mut binding is valid; got {errs:?}"
        );
    }

    #[test]
    fn mutable_borrow_of_a_val_binding_is_rejected() {
        let errs = errors(
            r#"
            func main() -> i32 {
                val x: i32 = 5
                val r: &mut i32 = &mut x
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("cannot mutably borrow")),
            "&mut of a val must be rejected; got {errs:?}"
        );
    }

    #[test]
    fn dereferencing_a_non_reference_is_rejected() {
        let errs = errors(
            r#"
            func main() -> i32 {
                val x: i32 = 5
                val y: i32 = *x
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("cannot dereference")),
            "deref of a non-reference must be rejected; got {errs:?}"
        );
    }

    #[test]
    fn writing_through_an_immutable_reference_is_rejected() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut x: i32 = 5
                val r: &i32 = &x
                *r = 9
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("immutable reference")),
            "writing through &T must be rejected; got {errs:?}"
        );
    }

    #[test]
    fn mut_self_method_type_checks_and_allows_field_write() {
        // A `&mut self` method may assign to `self.field`, and calling it on
        // a `mut` binding is sound.
        let errs = errors(
            r#"
            struct Counter { value: i32 }
            impl Counter {
                func increment(&mut self) { self.value = self.value + 1 }
            }
            func main() -> i32 {
                mut c = Counter { value: 0 }
                c.increment()
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "&mut self must type-check; got {errs:?}");
    }

    #[test]
    fn mut_self_field_write_through_self_in_ref_self_is_rejected() {
        // A `&self` method is read-only: assigning to `self.field` must fail.
        let errs = errors(
            r#"
            struct Counter { value: i32 }
            impl Counter {
                func bad(&self) { self.value = 1 }
            }
            func main() -> i32 { 0 }
            "#,
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("immutable") || e.contains("cannot assign")),
            "writing self.field in a &self method must be rejected; got {errs:?}"
        );
    }

    #[test]
    fn mut_self_on_immutable_binding_is_rejected() {
        let errs = errors(
            r#"
            struct Counter { value: i32 }
            impl Counter {
                func increment(&mut self) { self.value = self.value + 1 }
            }
            func main() -> i32 {
                val c = Counter { value: 0 }
                c.increment()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("cannot mutably borrow 'c'")),
            "calling &mut self on a val must be rejected; got {errs:?}"
        );
    }

    #[test]
    fn mut_self_call_while_shared_borrowed_is_rejected() {
        let errs = errors(
            r#"
            struct Counter { value: i32 }
            impl Counter {
                func increment(&mut self) { self.value = self.value + 1 }
            }
            func main() -> i32 {
                mut c = Counter { value: 0 }
                val r = &c
                c.increment()
                return r.value
            }
            "#,
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("cannot borrow 'c' as mutable")),
            "calling &mut self while shared-borrowed must conflict; got {errs:?}"
        );
    }

    #[test]
    fn mut_self_on_field_of_immutable_binding_is_rejected() {
        // Mutating `o.inner` through a `&mut self` method needs the *root* binding
        // `o` to be mutable; a `val` root is rejected. Semantic-only — nested
        // struct fields are not lowered yet, so this exercises the check in isolation.
        let errs = errors(
            r#"
            struct Inner { v: i32 }
            struct Outer { inner: Inner }
            impl Inner {
                func bump(&mut self) { self.v = self.v + 1 }
            }
            func main() -> i32 {
                val o = Outer { inner: Inner { v: 0 } }
                o.inner.bump()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("cannot mutably borrow 'o'")),
            "a &mut self call rooted in a val binding must be rejected; got {errs:?}"
        );
    }

    #[test]
    fn mut_self_on_temporary_receiver_is_rejected() {
        // A call-result receiver has no place to borrow, so a `&mut self` call on it
        // is rejected like any `&mut` of a temporary value.
        let errs = errors(
            r#"
            struct C { v: i32 }
            impl C {
                func new() -> C { C { v: 0 } }
                func bump(&mut self) { self.v = self.v + 1 }
            }
            func main() -> i32 {
                C::new().bump()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("cannot borrow")),
            "a &mut self call on a temporary must be rejected; got {errs:?}"
        );
    }

    #[test]
    fn consuming_self_is_still_rejected() {
        let errs = errors(
            r#"
            struct Wrapper { value: i32 }
            impl Wrapper {
                func unwrap(self) -> i32 { self.value }
            }
            func main() -> i32 { 0 }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("not yet supported")),
            "consuming self must still be rejected; got {errs:?}"
        );
    }

    #[test]
    fn reassigning_a_mut_revives_the_binding() {
        let errs = errors(
            r#"
            func consume(s: string) -> i32 { 0 }
            func main() -> i32 {
                mut s: string = "a"
                val r: i32 = consume(s)
                s = "b"
                val n: u64 = s.len()
                return 0
            }
            "#,
        );
        assert!(
            errs.is_empty(),
            "reassignment should clear the moved state; got {errs:?}"
        );
    }
}
