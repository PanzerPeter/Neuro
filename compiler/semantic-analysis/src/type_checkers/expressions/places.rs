// Place expressions: borrows, dereferences, indexing, and ranges.
//
// Reached from the `check_expr` dispatch in this module's `mod.rs`. Every file
// here adds methods to the same `impl TypeChecker` block.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::Expr;
use shared_types::Span;

impl TypeChecker {
    /// The root binding name of a place expression, peeling parentheses, field
    /// access, and dereference (`(o).inner` and `*o` both root at `o`). A receiver
    /// with no place root — a call or literal temporary — yields `None`.
    pub(super) fn place_root_name(expr: &Expr) -> Option<String> {
        match expr {
            Expr::Identifier(ident) => Some(ident.name.clone()),
            Expr::Paren(inner, _) => Self::place_root_name(inner),
            Expr::FieldAccess { object, .. } => Self::place_root_name(object),
            Expr::Deref { operand, .. } => Self::place_root_name(operand),
            _ => None,
        }
    }

    /// Whether `expr` is exactly a binding (an identifier, possibly parenthesised),
    /// as opposed to a sub-place like a field access. Borrow tracking is keyed by
    /// binding, so only a bare binding registers a tracked borrow.
    pub(super) fn is_bare_binding(expr: &Expr) -> bool {
        match expr {
            Expr::Identifier(_) => true,
            Expr::Paren(inner, _) => Self::is_bare_binding(inner),
            _ => false,
        }
    }

    /// Borrow `&place` / `&mut place`. The result type is `&T`
    /// (or `&mut T`). Checking the operand reads its type without consuming it:
    /// a borrow never moves the borrowed value, which is the whole point of a
    /// reference.
    pub(super) fn check_reference_expr(
        &mut self,
        operand: &Expr,
        mutable: bool,
        span: &Span,
    ) -> Option<Type> {
        // Only a live binding (`val`/`mut`/parameter) is a borrowable place. A
        // `const` is an inlined value with no address, and temporaries
        // (literals, calls, operator results) are not places.
        let mut place = operand;
        while let Expr::Paren(inner, _) = place {
            place = inner;
        }
        let binding = match place {
            Expr::Identifier(ident) => self
                .symbols
                .lookup(&ident.name)
                .map(|info| (ident.name.clone(), info.mutable)),
            _ => None,
        };
        let Some((name, is_mut_binding)) = binding else {
            self.record_error(TypeError::CannotBorrowValue { span: *span });
            let _ = self.check_expr(operand, None);
            return Some(Type::Unknown);
        };
        // `&mut` demands a `mut` binding — you cannot acquire write access
        // through a reference to a value you may not write directly.
        if mutable && !is_mut_binding {
            self.record_error(TypeError::CannotBorrowMutably { name, span: *span });
            let _ = self.check_expr(operand, None);
            return Some(Type::Unknown);
        }
        let inner = self.check_expr(operand, None)?;
        if matches!(inner, Type::Unknown) {
            return Some(Type::Unknown);
        }

        // Aliasing exclusivity. A `&mut` borrow is exclusive:
        // no other borrow of the place may be live at the same time. A
        // shared `&` borrow tolerates other shared borrows but excludes an
        // active `&mut`. The counts sum persistent borrows (held by live
        // reference bindings) and transient borrows (taken earlier in this
        // same statement, e.g. another argument of the same call).
        if let Some((shared, exclusive)) = self.symbols.borrow_counts(&name) {
            if mutable {
                if shared > 0 || exclusive > 0 {
                    self.record_error(TypeError::CannotMutablyBorrowWhileBorrowed {
                        name: name.clone(),
                        span: *span,
                    });
                }
            } else if exclusive > 0 {
                self.record_error(TypeError::CannotBorrowWhileMutablyBorrowed {
                    name: name.clone(),
                    span: *span,
                });
            }
        }
        // Every fresh borrow starts transient; a `val r = &place` initializer
        // is promoted to a persistent borrow by the `VarDecl` handler.
        self.symbols.add_transient_borrow(&name, mutable);

        Some(Type::Reference {
            inner: Box::new(inner),
            mutable,
        })
    }

    /// Dereference `*operand`: the result is the referent type `T`. The
    /// operand must have a reference type; dereferencing anything else is an
    /// error. Reading through either `&T` or `&mut T` is permitted.
    pub(super) fn check_deref_expr(&mut self, operand: &Expr, span: &Span) -> Option<Type> {
        let operand_ty = self.check_expr(operand, None)?;
        if matches!(operand_ty, Type::Unknown) {
            return Some(Type::Unknown);
        }
        match operand_ty {
            Type::Reference { inner, .. } => Some(*inner),
            other => {
                self.record_error(TypeError::CannotDereference {
                    found: other,
                    span: *span,
                });
                Some(Type::Unknown)
            }
        }
    }

    /// A range `a..b` is not a first-class value: it is consumed directly
    /// by `string.slice` via `check_string_slice`, so reaching it through the
    /// general expression path means it was used somewhere a range is not allowed.
    /// Still check the bounds for cascaded diagnostics.
    pub(super) fn check_range_expr(
        &mut self,
        start: &Expr,
        end: &Expr,
        span: &Span,
    ) -> Option<Type> {
        let _ = self.check_expr(start, None);
        let _ = self.check_expr(end, None);
        self.record_error(TypeError::RangeNotAllowed { span: *span });
        Some(Type::Unknown)
    }

    /// Array indexing `object[index]`: the object is an array (or a
    /// borrow of one, auto-derefed per); the index is an integer; the
    /// result is the element type.
    pub(super) fn check_index_expr(
        &mut self,
        object: &Expr,
        index: &Expr,
        span: &Span,
    ) -> Option<Type> {
        let obj_ty = self.check_expr(object, None).unwrap_or(Type::Unknown);
        let idx_ty = self.check_expr(index, None).unwrap_or(Type::Unknown);

        if !matches!(idx_ty, Type::Unknown) && !idx_ty.is_integer() {
            self.record_error(TypeError::IndexNotInteger {
                found: idx_ty,
                span: index.span(),
            });
        }

        if matches!(obj_ty, Type::Unknown) {
            return Some(Type::Unknown);
        }

        if let Some(element) = self.collection_element(&obj_ty) {
            return Some(element);
        }
        match obj_ty.referent() {
            Type::Array { element, .. } => Some((**element).clone()),
            other => {
                self.record_error(TypeError::NotIndexable {
                    found: other.clone(),
                    span: *span,
                });
                Some(Type::Unknown)
            }
        }
    }
}
