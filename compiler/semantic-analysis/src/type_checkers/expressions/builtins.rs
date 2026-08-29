// Intrinsic methods on builtin (non-struct) receivers, and the divergent panic family.
//
// Reached from the `check_expr` dispatch in this module's `mod.rs`. Every file
// here adds methods to the same `impl TypeChecker` block.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::Expr;
use shared_types::Span;

impl TypeChecker {
    /// Resolve a compiler-known intrinsic method on a builtin (non-struct) receiver.
    ///
    /// Returns `Some(return_type)` when `method` names an intrinsic for `recv` — recording
    /// an arity diagnostic when the argument count is wrong — and `None` when no such
    /// intrinsic exists, so the caller falls through to the standard `MethodNotFound` error.
    pub(super) fn resolve_builtin_method(
        &mut self,
        recv: &Type,
        method: &str,
        args: &[Expr],
        call_span: Span,
    ) -> Option<Type> {
        // String methods auto-deref through an immutable borrow `&string`, so the
        // referent drives the string match below.
        match (recv.referent(), method) {
            // O(1) byte length read from the string fat pointer's stored `len`.
            (Type::String, "len") => {
                if !args.is_empty() {
                    self.record_error(TypeError::ArgumentCountMismatch {
                        expected: 0,
                        found: args.len(),
                        span: call_span,
                    });
                }
                Some(Type::U64)
            }
            // Explicit deep copy of an owned string. Takes no arguments and yields a
            // fresh `string`. The canonical opt-out of move-by-default for non-`Copy` types.
            (Type::String, "clone") => {
                if !args.is_empty() {
                    self.record_error(TypeError::ArgumentCountMismatch {
                        expected: 0,
                        found: args.len(),
                        span: call_span,
                    });
                }
                Some(Type::String)
            }
            // Borrowed sub-slice. Takes a single range argument `a..b` / `a..=b`
            // and yields a `&string` view into the receiver's UTF-8 data (zero copy).
            (Type::String, "slice") => Some(self.check_string_slice(args, call_span)),
            // Array length, the compile-time `N` of `[T; N]`. Auto-derefs through
            // a borrow of an array (`&[T; N]`). Takes no arguments and yields `u64`.
            (Type::Array { .. }, "len") => {
                if !args.is_empty() {
                    self.record_error(TypeError::ArgumentCountMismatch {
                        expected: 0,
                        found: args.len(),
                        span: call_span,
                    });
                }
                Some(Type::U64)
            }
            // Wrapping/saturating arithmetic and the right-shift method.
            // Each takes one same-typed argument and returns the receiver's integer type.
            // Matched on `recv` (not the referent): integer intrinsics require a value
            // receiver, since reading a scalar through `&T` needs the deref operator.
            (
                _,
                "wrapping_add" | "wrapping_sub" | "wrapping_mul" | "saturating_add"
                | "saturating_sub" | "saturating_mul" | "shr",
            ) if recv.is_integer() => {
                self.check_unary_int_intrinsic_arg(recv, args, call_span);
                Some(recv.clone())
            }
            // Overflow-reporting arithmetic. Same argument contract as the intrinsics
            // above, but the result is `Option<T>` over the receiver's type: `None` is
            // the overflow answer, so the caller must deconstruct before using the value.
            (_, "checked_add" | "checked_sub" | "checked_mul") if recv.is_integer() => {
                self.check_unary_int_intrinsic_arg(recv, args, call_span);
                Some(self.option_of(recv.clone(), call_span))
            }
            _ => None,
        }
    }

    /// Validate the single argument of an integer intrinsic (`wrapping_*`, `saturating_*`,
    /// `.shr`): exactly one argument whose type matches the receiver's integer type. Records
    /// an arity or mismatch diagnostic on violation; the call's result type is unaffected.
    pub(super) fn check_unary_int_intrinsic_arg(
        &mut self,
        recv: &Type,
        args: &[Expr],
        call_span: Span,
    ) {
        if args.len() != 1 {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 1,
                found: args.len(),
                span: call_span,
            });
            return;
        }

        if let Some(arg_ty) = self.check_expr(&args[0], Some(recv)) {
            if !arg_ty.is_compatible_with(recv) {
                self.record_error(TypeError::Mismatch {
                    expected: recv.clone(),
                    found: arg_ty,
                    span: args[0].span(),
                });
            }
        }
    }

    /// Type-check `string.slice(range)`: exactly one `a..b` / `a..=b` argument
    /// whose bounds are integers. Returns the `&string` slice type; on any violation a
    /// diagnostic is recorded and the `&string` type is still returned so checking
    /// continues with the documented result type.
    pub(super) fn check_string_slice(&mut self, args: &[Expr], call_span: Span) -> Type {
        let slice_ty = Type::Reference {
            inner: Box::new(Type::String),
            mutable: false,
        };

        if args.len() != 1 {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 1,
                found: args.len(),
                span: call_span,
            });
            return slice_ty;
        }

        let Expr::Range { start, end, .. } = &args[0] else {
            self.record_error(TypeError::SliceExpectsRange {
                span: args[0].span(),
            });
            return slice_ty;
        };

        for bound in [start.as_ref(), end.as_ref()] {
            if let Some(bound_ty) = self.check_expr(bound, Some(&Type::U64)) {
                if !matches!(bound_ty, Type::Unknown) && !bound_ty.is_integer() {
                    self.record_error(TypeError::Mismatch {
                        expected: Type::U64,
                        found: bound_ty,
                        span: bound.span(),
                    });
                }
            }
        }

        slice_ty
    }

    /// Type-check a call to a compiler-known panic-family builtin:
    /// `panic(msg: string)`, `assert(cond: bool)`, or `unreachable()`.
    ///
    /// Returns `Some(ty)` when `func_name` names a builtin — recording an arity or
    /// argument-type diagnostic on violation — and `None` otherwise, so the caller falls
    /// through to ordinary function resolution. The result type is `Type::Unknown`: these
    /// builtins **diverge** (they abort and never return), so the call must satisfy any
    /// context — a unit statement, a non-`void` tail return (`func f() -> i32 { panic(..) }`),
    /// or a value binding. `Type::Unknown` is the type system's "compatible with everything"
    /// type, which is exactly the divergent (`never`) contract until a dedicated `!` type lands.
    pub(super) fn resolve_panic_builtin(
        &mut self,
        func_name: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        // Each builtin's single fixed parameter type, or `None` for the nullary `unreachable`.
        let expected_param = match func_name {
            "panic" => Some(Type::String),
            "assert" => Some(Type::Bool),
            "unreachable" => None,
            _ => return None,
        };

        let expected_arity = if expected_param.is_some() { 1 } else { 0 };
        if args.len() != expected_arity {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: expected_arity,
                found: args.len(),
                span,
            });
            return Some(Type::Unknown);
        }

        if let (Some(expected), Some(arg)) = (expected_param, args.first()) {
            if let Some(arg_ty) = self.check_expr(arg, Some(&expected)) {
                if !arg_ty.is_compatible_with(&expected) {
                    self.record_error(TypeError::Mismatch {
                        expected,
                        found: arg_ty,
                        span: arg.span(),
                    });
                }
            }
        }

        Some(Type::Unknown)
    }

    /// Type-check a call to a compiler-known standard-output builtin:
    /// `print(text: string)` or `println(text: string)`.
    ///
    /// Returns `Some(Type::Void)` when `func_name` names one — recording an arity or
    /// argument-type diagnostic on violation — and `None` otherwise, so the caller falls
    /// through to ordinary function resolution. Unlike the panic family these **return**,
    /// so the result is the real unit type rather than the divergent `Type::Unknown`.
    ///
    /// The argument may be an owned `string` or an immutable `&string` slice: both are
    /// the same `{ ptr, len }` pair, and `.slice(range)` yields the latter. It is read,
    /// not consumed, so no move is recorded and the caller may keep using the value.
    pub(super) fn resolve_io_builtin(
        &mut self,
        func_name: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        if !matches!(func_name, "print" | "println") {
            return None;
        }

        if args.len() != 1 {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 1,
                found: args.len(),
                span,
            });
            return Some(Type::Void);
        }

        if let Some(arg_ty) = self.check_expr(&args[0], Some(&Type::String)) {
            // A `&mut string` is a pointer to the fat pointer, not the fat pointer
            // itself, so it is rejected here rather than peeled with the shared borrows.
            let accepted = match &arg_ty {
                Type::Unknown => true,
                Type::Reference { inner, mutable } => !*mutable && matches!(**inner, Type::String),
                other => matches!(other, Type::String),
            };
            if !accepted {
                self.record_error(TypeError::Mismatch {
                    expected: Type::String,
                    found: arg_ty,
                    span: args[0].span(),
                });
            }
        }

        Some(Type::Void)
    }

    /// Enforce the rules for the receiver of a `&mut self` method call, which
    /// borrows the receiver mutably for the call's duration.
    ///
    /// A receiver reached through a `&mut T` borrow is already write-capable and
    /// passes; a `&T` receiver cannot yield write access, so it is rejected. An
    /// owned receiver must root in a `mut` binding — mutating `o.inner` needs `o`
    /// itself mutable. A receiver with no place root (a call or literal temporary)
    /// is not assignable, so it is rejected like any `&mut` of a value. Exclusivity
    /// is tracked at binding granularity (matching `&place` borrows), so only a
    /// receiver that *is* the binding registers the call's transient exclusive
    /// borrow and checks for a coexisting borrow; both clear at statement end.
    pub(crate) fn check_mut_self_receiver(
        &mut self,
        object: &Expr,
        obj_ty: &Type,
        span: shared_types::Span,
    ) {
        if let Type::Reference { mutable, .. } = obj_ty {
            if !mutable {
                let name = Self::place_root_name(object).unwrap_or_else(|| "value".to_string());
                self.record_error(TypeError::CannotBorrowMutably { name, span });
            }
            return;
        }

        let Some(name) = Self::place_root_name(object) else {
            self.record_error(TypeError::CannotBorrowValue { span });
            return;
        };
        let Some(info) = self.symbols.lookup(&name) else {
            return;
        };
        if !info.mutable {
            self.record_error(TypeError::CannotBorrowMutably {
                name: name.clone(),
                span,
            });
            return;
        }
        if Self::is_bare_binding(object) {
            if let Some((shared, exclusive)) = self.symbols.borrow_counts(&name) {
                if shared > 0 || exclusive > 0 {
                    self.record_error(TypeError::CannotMutablyBorrowWhileBorrowed {
                        name: name.clone(),
                        span,
                    });
                }
            }
            self.symbols.add_transient_borrow(&name, true);
        }
    }
}
