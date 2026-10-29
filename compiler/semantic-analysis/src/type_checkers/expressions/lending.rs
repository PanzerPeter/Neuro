// Lending function values: the borrow a call takes of the function value it calls, and of
// each one it hands to a function-typed parameter.
//
// A callee can only call a function-typed parameter (it is scope-bound), so the caller
// keeps the value: a plain parameter borrows it shared, a `mut` one exclusively, for as
// long as the call runs. Calling a value borrows it the same way. The borrows begin once
// the arguments are evaluated, so `f(f(x))` is two calls one after the other, while a
// closure argument that captures `f` holds its borrow into the call and conflicts. They
// end with the call, not the statement, so `f(1) + f(2)` holds one at a time.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{FunctionMode, Type};
use ast_types::Expr;

/// The borrow a call holds of one function value until it returns.
pub(super) struct Lend {
    name: String,
    exclusive: bool,
}

impl TypeChecker {
    /// Pass `arg` to a parameter of type `param`: lent for the call when the parameter is a
    /// plain or `mut` function type, so it joins `lent` to be borrowed once every argument
    /// is checked; moved into the callee otherwise (a `once` parameter consumes it).
    pub(super) fn pass_argument<'a>(
        &mut self,
        arg: &'a Expr,
        param: &Type,
        lent: &mut Vec<(&'a Expr, bool)>,
    ) {
        match param {
            Type::Function { mode, .. } if *mode != FunctionMode::Once => {
                lent.push((arg, *mode == FunctionMode::Mut));
            }
            _ => self.record_move(arg),
        }
    }

    /// Borrow every argument [`Self::pass_argument`] set aside, in order, so two lends of
    /// one value to the same call meet.
    pub(super) fn lend_arguments(&mut self, lent: Vec<(&Expr, bool)>) -> Vec<Lend> {
        lent.into_iter()
            .filter_map(|(arg, exclusive)| self.lend_function_value(arg, exclusive))
            .collect()
    }

    /// End the borrows a call took once it returns.
    pub(super) fn end_lends(&mut self, lends: Vec<Lend>) {
        for lend in lends {
            self.symbols
                .release_transient_borrow(&lend.name, lend.exclusive);
        }
    }

    /// Borrow the function value at `place` for a call: exclusively for a `mut` value,
    /// whose call may write its environment, shared otherwise. Nothing is borrowed when
    /// `place` names no binding: a closure literal or a call's result is the call's own.
    pub(super) fn lend_function_value(&mut self, place: &Expr, exclusive: bool) -> Option<Lend> {
        let mut place = place;
        while let Expr::Paren(inner, _) = place {
            place = inner;
        }
        let name = match place {
            Expr::Identifier(ident) => ident.name.clone(),
            _ if self.is_borrowable_sub_place(place) => Self::place_root_name(place)?,
            _ => return None,
        };
        let (shared, held_exclusive) = self.symbols.borrow_counts(&name)?;
        if exclusive {
            // A borrowed `self` carries its borrow in the binding's mutability, not its type.
            let through_shared = match self
                .projection_type(place)
                .and_then(|ty| self.nearest_reference(place, &ty))
            {
                Some(mutable) => !mutable,
                None => {
                    name == "self"
                        && !self.self_is_owned
                        && self.symbols.lookup(&name).is_some_and(|info| !info.mutable)
                }
            };
            if through_shared {
                self.record_error(TypeError::MutFunctionThroughSharedBorrow {
                    name: name.clone(),
                    span: place.span(),
                });
            } else if shared + held_exclusive > 0 {
                self.record_error(TypeError::CannotMutablyBorrowWhileBorrowed {
                    name: name.clone(),
                    span: place.span(),
                });
            }
        } else if held_exclusive > 0 {
            self.record_error(TypeError::CannotBorrowWhileMutablyBorrowed {
                name: name.clone(),
                span: place.span(),
            });
        }
        self.symbols.add_transient_borrow(&name, exclusive);
        Some(Lend { name, exclusive })
    }
}

#[cfg(test)]
mod tests {
    use crate::errors::TypeError;
    use crate::type_checkers::tests::semantic_errors;

    /// `main` with a `mut` closure `f` over `n` in scope, running `body`.
    fn with_counter(helpers: &str, body: &str) -> Vec<TypeError> {
        semantic_errors(&format!(
            "{helpers}
func main() -> i32 {{
    mut n = 0
    {{
        val f = |x: i32| -> i32 {{
            n += x
            n
        }}
        {body}
    }}
    n
}}"
        ))
    }

    fn conflicts(errors: &[TypeError]) -> usize {
        errors
            .iter()
            .filter(|e| matches!(e, TypeError::CannotMutablyBorrowWhileBorrowed { name, .. } if name == "f"))
            .count()
    }

    #[test]
    fn a_mut_function_value_moves_and_a_shared_one_copies() {
        let errors = with_counter("", "val g = f\n        g(1)\n        f(2)");
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeError::UseOfMovedValue { name, .. } if name == "f")),
            "{errors:?}"
        );
        let copied = semantic_errors(
            "func main() -> i32 {
    val k = 2
    val f = |x: i32| -> i32 { x * k }
    val g = f
    f(1) + g(2)
}",
        );
        assert!(copied.is_empty(), "{copied:?}");
    }

    #[test]
    fn a_function_typed_parameter_borrows_its_argument_for_the_call() {
        let lent = with_counter(
            "func apply(f: mut (i32) -> i32, x: i32) -> i32 { f(x) }
func last(f: once (i32) -> i32) -> i32 { f(1) }",
            "apply(f, 1)\n        f(2)\n        apply(f, 3)\n        last(f)",
        );
        assert!(lent.is_empty(), "{lent:?}");
        let consumed = with_counter(
            "func last(f: once (i32) -> i32) -> i32 { f(1) }",
            "last(f)\n        f(2)",
        );
        assert!(
            consumed
                .iter()
                .any(|e| matches!(e, TypeError::UseOfMovedValue { name, .. } if name == "f")),
            "{consumed:?}"
        );
    }

    #[test]
    fn a_mut_lend_excludes_every_other_borrow_in_the_call() {
        let helpers = "func two(a: mut (i32) -> i32, b: mut (i32) -> i32) { a(1)\n b(2) }";
        for body in [
            "two(f, f)",
            "two(f, |x: i32| f(x))",
            "two(|x: i32| f(x), f)",
        ] {
            let errors = with_counter(helpers, body);
            assert_eq!(conflicts(&errors), 1, "{body}: {errors:?}");
        }
    }

    /// An argument's call returns before the call it feeds starts, so it borrows nothing
    /// the outer call holds; two calls in one statement hold one borrow at a time.
    #[test]
    fn a_lend_begins_after_the_arguments_and_ends_with_the_call() {
        let errors = with_counter(
            "func apply(f: mut (i32) -> i32, x: i32) -> i32 { f(x) }",
            "val a = f(f(1))\n        val b = apply(f, f(2)) + f(1) + f(1)",
        );
        assert!(errors.is_empty(), "{errors:?}");
    }

    /// A call of a `mut` value borrows it exclusively, so a closure argument that calls
    /// it again would write the environment under the running call.
    #[test]
    fn a_call_conflicts_with_a_closure_argument_over_the_same_value() {
        let errors = semantic_errors(
            "func main() -> i32 {
    mut n = 0
    {
        val f = |g: mut () -> void| -> void {
            n += 1
            g()
        }
        f(|| -> void { f(|| -> void {}) })
    }
    n
}",
        );
        assert_eq!(
            errors
                .iter()
                .filter(|e| matches!(e, TypeError::CannotMutablyBorrowWhileBorrowed { name, .. } if name == "f"))
                .count(),
            1,
            "{errors:?}"
        );
    }

    #[test]
    fn a_mut_value_behind_a_shared_borrow_is_neither_called_nor_lent() {
        let errors = semantic_errors(
            "struct Holder { step: mut (i32) -> void }
impl Holder {
    func run(&self) { (self.step)(1) }
    func run_mut(&mut self) { (self.step)(1) }
}
func each(f: mut (i32) -> void) { f(3) }
func lend(h: &Holder) { each(h.step) }
func lend_mut(h: &mut Holder) { each(h.step) }
func main() -> i32 { 0 }",
        );
        let refused: Vec<&str> = errors
            .iter()
            .filter_map(|e| match e {
                TypeError::MutFunctionThroughSharedBorrow { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(refused, ["self", "h"], "{errors:?}");
        assert_eq!(errors.len(), 2, "{errors:?}");
    }

    #[test]
    fn two_shared_lends_coexist() {
        let errors = semantic_errors(
            "func two(a: (i32) -> i32, b: (i32) -> i32) -> i32 { a(1) + b(2) }
func main() -> i32 {
    val k = 3
    val f = |x: i32| -> i32 { x + k }
    two(f, f) + two(f, |x: i32| f(x))
}",
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(conflicts(&errors), 0);
    }
}
