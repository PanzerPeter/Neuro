//! Move-by-default ownership analysis (§2.2).
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
        // §2.4: passing `&s` borrows rather than moves, so `s` stays usable afterward.
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
