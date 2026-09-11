//! Move-by-default ownership analysis.
//!
//! A non-`Copy` value is *moved* out of its source binding when it is placed
//! into a new owner: the initializer of a `val`/`mut`, the right-hand side of an
//! assignment, a `return` value, or a by-value call argument. After a move the
//! source binding is invalid, and reading it is a `UseOfMovedValue` error
//! (emitted from the `Expr::Identifier` arm in `expressions.rs`).
//!
//! The analysis is intentionally conservative: it flags only place expressions
//! in a consuming position, and conditional regions snapshot/restore their move
//! state (see `SymbolTable::snapshot_moves`). It may therefore miss some moves,
//! but it never rejects a valid program.
//!
//! A move out of a *sub-place* — `l.w`, `(o).inner.w` — is recorded against the
//! place's ROOT binding rather than the one field, because the language leaves a
//! struct whose field has been moved out partially moved and unusable as a whole.
//! Reaching the place through a borrow is not a move at all but an error: a
//! `&self` method that consumes `self.w` would release a buffer its caller still
//! owns, once per call.
//!
//! A loop body is the one region where restoring the state is not the whole
//! story. The restore is right for what follows the loop — it may run zero times,
//! so the binding may still own its value there — but a move that is still
//! outstanding when the body *ends* is one the next iteration performs again on a
//! binding that owns nothing. `report_loop_body_moves` reports those before the
//! restore, which is what keeps the second iteration from freeing the same buffer
//! twice.

use ast_types::{Expr, Stmt};
use shared_types::Span;

use crate::errors::TypeError;
use crate::types::Type;

use super::TypeChecker;

/// The method receiver. It is bound as the struct type rather than `&Struct`, so
/// that a field read and a `&mut self` field write stay ordinary field access —
/// but every receiver the language admits is a borrow, `SelfParam::Owned` being
/// rejected until the by-value struct ABI exists. A field of it therefore cannot
/// be moved out, and `self` is a keyword, so no other binding can wear the name.
const SELF_RECEIVER: &str = "self";

impl TypeChecker {
    /// Record the move that occurs when `expr` appears in a consuming position.
    ///
    /// Moves apply to a place expression — an identifier or a field path rooted
    /// in one, either possibly wrapped in parentheses — whose value has a
    /// move-tracked type. A literal, a `.clone()` call, or any compound
    /// expression produces a fresh value and moves nothing here; nested
    /// consuming positions (e.g. an argument inside a call) are handled where
    /// that call's arguments are checked.
    pub(crate) fn record_move(&mut self, expr: &Expr) {
        let mut place = expr;
        while let Expr::Paren(inner, _) = place {
            place = inner;
        }

        // A constant is a value, not an owner, so it cannot be moved from.
        if let Expr::Identifier(ident) = place {
            let binding_ty = self.symbols.lookup(&ident.name).map(|info| info.ty.clone());

            if binding_ty.is_some_and(|ty| self.is_type_move_tracked(&ty)) {
                self.symbols.mark_moved(&ident.name, ident.span);
            }
            return;
        }

        let Some((place_ty, behind_borrow)) = self.place_origin(place) else {
            return;
        };
        if !self.is_type_move_tracked(&place_ty) {
            return;
        }
        let Some(root) = Self::place_root_name(place) else {
            return;
        };

        if behind_borrow {
            self.record_error(TypeError::CannotMoveOutOfBorrow {
                name: root,
                span: place.span(),
            });
            return;
        }

        self.symbols.mark_moved(&root, place.span());
    }

    /// The type a place expression denotes, paired with whether reaching it
    /// crossed a reference.
    ///
    /// `None` means the expression is not a place rooted in a binding — a call
    /// result or a literal — which owns nothing a caller could move out of.
    fn place_origin(&self, place: &Expr) -> Option<(Type, bool)> {
        match place {
            Expr::Paren(inner, _) => self.place_origin(inner),
            Expr::Identifier(ident) => self
                .symbols
                .lookup(&ident.name)
                .map(|info| (info.ty.clone(), ident.name == SELF_RECEIVER)),
            Expr::Deref { operand, .. } => match self.place_origin(operand)?.0 {
                Type::Reference { inner, .. } => Some((*inner, true)),
                _ => None,
            },
            Expr::FieldAccess { object, field, .. } => {
                let (object_ty, behind_borrow) = self.place_origin(object)?;
                let behind_borrow = behind_borrow || matches!(object_ty, Type::Reference { .. });
                let Type::Struct(name) = object_ty.referent() else {
                    return None;
                };
                let field_ty = self
                    .struct_defs
                    .get(name)?
                    .iter()
                    .find(|(n, _)| n == &field.name)
                    .map(|(_, ty)| ty.clone())?;
                Some((field_ty, behind_borrow))
            }
            _ => None,
        }
    }

    /// Report every binding the just-checked loop body moved out of and did not
    /// replace, given the move state captured before the body ran.
    ///
    /// A loop body's moves are restored afterwards because the loop may run zero
    /// times, but a move that is still outstanding when the body ends is a move the
    /// *next* iteration performs again on a binding that no longer owns anything —
    /// a double free at run time rather than a diagnostic. Call this with the scope
    /// stack the snapshot was taken on: bindings declared inside the body have gone
    /// with its scope, which is what leaves only the outer ones.
    pub(crate) fn report_loop_body_moves(&mut self, snapshot: &[Option<Span>], body: &[Stmt]) {
        // A body that always leaves the loop runs its move once, so there is no
        // second iteration to catch.
        if stmts_exit_loop(body) {
            return;
        }

        for (name, span) in self.symbols.moves_since(snapshot) {
            self.record_error(TypeError::MovedInLoopBody { name, span });
        }
    }
}

/// Whether a statement list always leaves the enclosing loop before falling off its
/// end, via `break` or `return`.
///
/// `continue` is deliberately not an exit: it starts the next iteration, which is
/// exactly the repetition this predicate exists to detect. Nested loops are not
/// descended into either — a `break` inside one targets that loop, not this one.
fn stmts_exit_loop(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_exits_loop)
}

fn stmt_exits_loop(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Break { .. } | Stmt::Return { .. } => true,
        Stmt::If {
            then_block,
            else_if_blocks,
            else_block: Some(else_block),
            ..
        } => {
            stmts_exit_loop(then_block)
                && else_if_blocks
                    .iter()
                    .all(|(_, block)| stmts_exit_loop(block))
                && stmts_exit_loop(else_block)
        }
        _ => false,
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
    fn move_in_a_tail_expression_is_counted_once() {
        // The tail expression is the implicit return, and it is the last use of `p`.
        // Checking it twice would see the move it performed itself.
        let errs = errors(
            r#"
            struct Point { x: i32, y: i32 }
            func total(p: Point) -> i32 { p.x + p.y }
            func main() -> i32 {
                val p = Point { x: 1, y: 2 }
                total(p)
            }
            "#,
        );
        assert!(
            errs.is_empty(),
            "a move in the tail expression must not report itself; got {errs:?}"
        );
    }

    #[test]
    fn move_in_a_tail_method_expression_is_counted_once() {
        let errs = errors(
            r#"
            struct Sink { n: i32 }
            impl Sink {
                func swallow(&self, s: string) -> i32 { self.n }
            }
            func main() -> i32 {
                val s: Sink = Sink { n: 1 }
                val text: string = "hi"
                s.swallow(text)
            }
            "#,
        );
        assert!(
            errs.is_empty(),
            "a move in a tail method call must not report itself; got {errs:?}"
        );
    }

    #[test]
    fn a_real_use_after_move_in_a_tail_expression_is_reported_once() {
        // The genuine error still fires, and only once, since the tail is no
        // longer checked twice.
        let errs = errors(
            r#"
            func consume(s: string) -> i32 { 0 }
            func main() -> i32 {
                val s: string = "hi"
                val first: i32 = consume(s)
                consume(s)
            }
            "#,
        );
        let moved: Vec<_> = errs
            .iter()
            .filter(|e| e.contains("use of moved value 's'"))
            .collect();
        assert_eq!(
            moved.len(),
            1,
            "expected exactly one use-of-moved diagnostic, got {errs:?}"
        );
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
        // `o` to be mutable; a `val` root is rejected. Semantic-only: nested
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
    fn moving_a_field_marks_the_whole_binding() {
        // A struct with a field moved out is partially moved and unusable as a
        // whole, so the root binding is what the diagnostic names.
        let errs = errors(
            r#"
            struct Holder { name: string }
            func consume(s: string) -> i32 { 0 }
            func main() -> i32 {
                val h = Holder { name: "hi" }
                val r: i32 = consume(h.name)
                val n: u64 = h.name.len()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("use of moved value 'h'")),
            "a moved-out field must move its struct; got {errs:?}"
        );
    }

    #[test]
    fn moving_a_copy_field_moves_nothing() {
        let errs = errors(
            r#"
            struct Point { x: i32, y: i32 }
            func consume(v: i32) -> i32 { v }
            func main() -> i32 {
                val p = Point { x: 1, y: 2 }
                val r: i32 = consume(p.x)
                return p.y
            }
            "#,
        );
        assert!(errs.is_empty(), "a Copy field moves nothing; got {errs:?}");
    }

    #[test]
    fn moving_a_field_out_of_a_borrowed_receiver_is_rejected() {
        let errs = errors(
            r#"
            struct Holder { name: string }
            func consume(s: string) -> i32 { 0 }
            impl Holder {
                func give(&self) -> i32 { consume(self.name) }
            }
            func main() -> i32 { 0 }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("cannot move out of 'self'")),
            "a &self receiver owns nothing to give away; got {errs:?}"
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
