//! Stores into places: assignment and compound assignment, resolving a place to its
//! root binding and projections, and whether the store is allowed there.

use ast_types::{BinaryOp, Expr, Place, TensorIndexArg};
use shared_types::{Identifier, Span};

use crate::errors::TypeError;
use crate::type_checkers::TypeChecker;
use crate::type_checkers::backward::gradient_view_root;
use crate::types::Type;

use super::borrow_target_of;

impl TypeChecker {
    /// Check `place = value` and `place OP= value`.
    ///
    /// The place is resolved first: its type decides between the in-place update a
    /// tensor takes and the `place = place OP value` desugaring everything else
    /// takes, and its root binding is what mutability, borrow and pool residency are
    /// all keyed by.
    pub(super) fn check_assign(
        &mut self,
        place: &Place,
        op: Option<BinaryOp>,
        value: &Expr,
        span: Span,
    ) -> Option<()> {
        let place_ty = self.resolve_place(place, span)?;
        match op {
            // The operator-trait dispatch rule: a type implementing the matching
            // `*Assign` trait updates in place; everything else desugars and allocates
            // a fresh value. Tensors are the one type on the first path today, and the
            // one where the difference is observable: the desugaring would move the
            // tensor out of its own place and reallocate its buffer.
            Some(op) if matches!(place_ty, Type::Tensor { .. }) => {
                self.check_tensor_compound_assign(place, &place_ty, op, value, span)
            }
            Some(op) => {
                let desugared = Expr::Binary {
                    left: Box::new(place.to_expr()),
                    op,
                    right: Box::new(value.clone()),
                    span,
                };
                self.check_place_store(place, &place_ty, &desugared, span)
            }
            None => self.check_place_store(place, &place_ty, value, span),
        }
    }

    /// The type of the storage an assignment writes to, reporting the place's own
    /// errors: an undefined root, an immutable one, a field that does not exist, a
    /// non-indexable base.
    pub(super) fn resolve_place(&mut self, place: &Place, span: Span) -> Option<Type> {
        match place {
            Place::Var(target) => {
                let Some(symbol) = self.symbols.lookup(&target.name) else {
                    self.record_error(TypeError::UndefinedVariable {
                        name: target.name.clone(),
                        span: target.span,
                    });
                    return None;
                };
                let ty = symbol.ty.clone();
                let mutable = symbol.mutable;
                if !mutable {
                    self.record_error(TypeError::AssignToImmutable {
                        name: target.name.clone(),
                        span: target.span,
                    });
                    return None;
                }
                Some(ty)
            }

            Place::Field {
                object,
                field,
                span: field_span,
            } => {
                let obj_ty = self.check_expr(object, None)?;
                let Type::Struct(struct_name) = obj_ty.referent().clone() else {
                    self.record_error(TypeError::UnknownField {
                        struct_name: obj_ty.to_string(),
                        field_name: field.name.clone(),
                        span: field.span,
                    });
                    return None;
                };
                if !self.place_is_writable(&obj_ty, place) {
                    let Some(root) = place.root().filter(|_| !self.root_is_mutable(place)) else {
                        self.report_immutable_place(place);
                        return None;
                    };
                    self.record_error(TypeError::AssignToImmutableField {
                        var_name: root.name.clone(),
                        field_name: field.name.clone(),
                        span: *field_span,
                    });
                    return None;
                }
                let field_ty = self
                    .struct_defs
                    .get(&struct_name)
                    .and_then(|def| def.iter().find(|(n, _)| n == &field.name))
                    .map(|(_, t)| t.clone());
                let Some(field_ty) = field_ty else {
                    self.record_error(TypeError::UnknownField {
                        struct_name,
                        field_name: field.name.clone(),
                        span: field.span,
                    });
                    return None;
                };
                self.reject_private_field(&struct_name, &field.name, field.span);
                Some(field_ty)
            }

            Place::Index {
                object,
                index,
                span: index_span,
            } => {
                let obj_ty = self.check_index_base(object)?;
                if !self.place_is_writable(&obj_ty, place) {
                    self.report_immutable_place(place);
                    return None;
                }
                // A rank-1 tensor is indexed with one argument, which parses as the
                // ordinary index form; the axis rules are the tensor's either way.
                if let Type::Tensor { element, shape } = obj_ty.referent().clone() {
                    let axes = [TensorIndexArg::Position((**index).clone())];
                    return self.resolve_tensor_element(&element, &shape, &axes, *index_span);
                }
                let idx_ty = self.check_expr(index, None).unwrap_or(Type::Unknown);
                if !matches!(idx_ty, Type::Unknown) && !idx_ty.is_integer() {
                    self.record_error(TypeError::IndexNotInteger {
                        found: idx_ty,
                        span: index.span(),
                    });
                }
                if let Some(element) = self.collection_element(&obj_ty) {
                    return Some(element);
                }
                match obj_ty.referent() {
                    Type::Array { element, .. } | Type::Slice(element) => Some((**element).clone()),
                    other => {
                        self.record_error(TypeError::NotIndexable {
                            found: other.clone(),
                            span,
                        });
                        None
                    }
                }
            }

            Place::TensorIndex {
                object,
                indices,
                span: index_span,
            } => {
                let obj_ty = self.check_index_base(object)?;
                if !self.place_is_writable(&obj_ty, place) {
                    self.report_immutable_place(place);
                    return None;
                }
                let Type::Tensor { element, shape } = obj_ty.referent().clone() else {
                    self.record_error(TypeError::TensorIndexOnNonTensor {
                        found: obj_ty,
                        span: *index_span,
                    });
                    return None;
                };
                self.resolve_tensor_element(&element, &shape, indices, *index_span)
            }

            Place::Deref {
                pointer,
                span: deref_span,
            } => {
                let pointer_ty = self.check_expr(pointer, None).unwrap_or(Type::Unknown);
                match &pointer_ty {
                    Type::Unknown => None,
                    Type::Reference {
                        inner,
                        mutable: true,
                    } => Some((**inner).clone()),
                    Type::Reference {
                        inner,
                        mutable: false,
                    } => {
                        self.record_error(TypeError::CannotAssignThroughRef {
                            inner: (**inner).clone(),
                            span: *deref_span,
                        });
                        None
                    }
                    other => {
                        self.record_error(TypeError::CannotDereference {
                            found: other.clone(),
                            span: pointer.span(),
                        });
                        None
                    }
                }
            }
        }
    }

    /// The element a tensor index names, rejecting an index that leaves an axis
    /// standing: a slice is a fresh tensor, so there is no storage to write into.
    pub(super) fn resolve_tensor_element(
        &mut self,
        element: &Type,
        shape: &[crate::types::TensorAxis],
        indices: &[TensorIndexArg],
        span: Span,
    ) -> Option<Type> {
        let indexed = self.check_tensor_index(element, shape, indices, span);
        if matches!(indexed, Type::Tensor { .. }) {
            self.record_error(TypeError::AssignToTensorSlice { span });
            return None;
        }
        Some(indexed)
    }

    /// Whether a sub-place may be written through.
    ///
    /// Write permission through a borrow comes from the borrow, not the binding:
    /// `xs: &mut [T]` is an immutable binding holding a mutable view, and a `&[T]`
    /// binding declared `mut` still may not write. The reference NEAREST the place
    /// decides, however many projections lie between them: `r[0][1]` through
    /// `r: &mut [[T; 2]]` may write, and through a `mut r: &[[T; 2]]` may not.
    /// Only a path that crosses no reference inherits the mutability of the binding
    /// the place is rooted at.
    pub(super) fn place_is_writable(&self, object_ty: &Type, place: &Place) -> bool {
        let through = match place {
            Place::Field { object, .. }
            | Place::Index { object, .. }
            | Place::TensorIndex { object, .. } => self.nearest_reference(object, object_ty),
            Place::Var(_) | Place::Deref { .. } => None,
        };
        if let Some(mutable) = through {
            return mutable;
        }
        self.root_is_mutable(place)
    }

    /// Whether the binding a place is rooted at was declared `mut`.
    pub(super) fn root_is_mutable(&self, place: &Place) -> bool {
        place
            .root()
            .and_then(|root| self.symbols.lookup(&root.name))
            .is_some_and(|info| info.mutable)
    }

    /// The mutability of the first reference met walking a place's object chain from
    /// `expr` (of type `ty`) toward its root. `None` when the chain crosses none, or
    /// passes through a projection whose type cannot be read without re-checking it.
    pub(crate) fn nearest_reference(&self, expr: &Expr, ty: &Type) -> Option<bool> {
        if let Type::Reference { mutable, .. } = ty {
            return Some(*mutable);
        }
        let object = match expr {
            Expr::Paren(inner, _) => return self.nearest_reference(inner, ty),
            Expr::FieldAccess { object, .. }
            | Expr::Index { object, .. }
            | Expr::TupleIndex { object, .. } => object,
            Expr::Deref { operand, .. } => operand,
            _ => return None,
        };
        let object_ty = self.projection_type(object)?;
        self.nearest_reference(object, &object_ty)
    }

    /// The type of one link of a place's object chain, read from the symbol table and
    /// the declarations alone. `check_expr` would type it too, but it records moves and
    /// borrows, and the chain has already been checked once as the place's object.
    pub(crate) fn projection_type(&self, expr: &Expr) -> Option<Type> {
        match expr {
            Expr::Identifier(ident) => self.symbols.lookup(&ident.name).map(|s| s.ty.clone()),
            Expr::Paren(inner, _) => self.projection_type(inner),
            Expr::FieldAccess { object, field, .. } => {
                let Type::Struct(name) = self.projection_type(object)?.referent().clone() else {
                    return None;
                };
                self.struct_defs
                    .get(&name)?
                    .iter()
                    .find(|(n, _)| n == &field.name)
                    .map(|(_, t)| t.clone())
            }
            Expr::Index { object, .. } => {
                let object_ty = self.projection_type(object)?;
                if let Some(element) = self.collection_element(&object_ty) {
                    return Some(element);
                }
                match object_ty.referent() {
                    Type::Array { element, .. } | Type::Slice(element) => Some((**element).clone()),
                    _ => None,
                }
            }
            Expr::TupleIndex { object, index, .. } => {
                match self.projection_type(object)?.referent() {
                    Type::Tuple(elements) => elements.get(*index).cloned(),
                    _ => None,
                }
            }
            Expr::Deref { operand, .. } => match self.projection_type(operand)? {
                Type::Reference { inner, .. } => Some(*inner),
                _ => None,
            },
            _ => None,
        }
    }

    /// A refused write. A `mut` root that still may not write is reached through a
    /// shared borrow, and saying the binding is immutable would contradict its `mut`.
    pub(super) fn report_immutable_place(&mut self, place: &Place) {
        if let (true, Some(root)) = (self.root_is_mutable(place), place.root()) {
            self.record_error(TypeError::AssignThroughSharedBorrow {
                name: root.name.clone(),
                span: place.span(),
            });
            return;
        }
        let Some(root) = place.root() else {
            self.record_error(TypeError::AssignToTemporary { span: place.span() });
            return;
        };
        self.record_error(TypeError::AssignToImmutable {
            name: root.name.clone(),
            span: root.span,
        });
    }

    /// Store `value` into an already-resolved place.
    pub(super) fn check_place_store(
        &mut self,
        place: &Place,
        place_ty: &Type,
        value: &Expr,
        span: Span,
    ) -> Option<()> {
        if let Place::Var(target) = place {
            return self.check_binding_store(target, place_ty, value, span);
        }

        // A borrow of the binding the place is rooted at sees this write, and a store that
        // displaces an owned value frees what the borrow points into. Tested before the RHS,
        // like a whole-binding store.
        self.refuse_store_while_borrowed(place);
        if self.holds_function_value(place_ty) && self.reached_through_reference(place) {
            self.record_error(TypeError::FunctionValueEscapes {
                problem: "be stored through a reference".to_string(),
                span,
            });
        }

        match place {
            Place::Deref { .. } => self.check_pool_ref_store(place_ty, value, span),
            _ => {
                if let Some(root) = place.root() {
                    let root = root.name.clone();
                    let through_reference = self.reached_through_reference(place);
                    self.check_pool_store(&root, place_ty, value, span, through_reference);
                }
            }
        }

        let value_ty = self
            .check_expr(value, Some(place_ty))
            .unwrap_or(Type::Unknown);
        // Storing the value into the place moves it out of its source.
        self.record_move(value);
        if !matches!(value_ty, Type::Unknown) && !value_ty.is_compatible_with(place_ty) {
            self.record_error(TypeError::Mismatch {
                expected: place_ty.clone(),
                found: value_ty,
                span,
            });
        }
        Some(())
    }

    /// Refuse a write into any part of a binding while a borrow of it is live. It is as
    /// coarse as the whole-binding rule: a borrow of one field freezes the whole value,
    /// because the borrow counts are kept per binding.
    pub(crate) fn refuse_store_while_borrowed(&mut self, place: &Place) {
        let Some(root) = place.root() else {
            return;
        };
        let Some((shared, exclusive)) = self.symbols.borrow_counts(&root.name) else {
            return;
        };
        if shared > 0 || exclusive > 0 {
            self.record_error(TypeError::CannotAssignWhileBorrowed {
                name: root.name.clone(),
                span: root.span,
            });
        }
    }

    /// Whether a store into `place` writes memory another frame owns: through a `*r`, a
    /// reference met on the way to the root, or a borrowed `self`.
    pub(super) fn reached_through_reference(&self, place: &Place) -> bool {
        let object = match place {
            Place::Var(_) => return false,
            Place::Deref { .. } => return true,
            Place::Field { object, .. }
            | Place::Index { object, .. }
            | Place::TensorIndex { object, .. } => object,
        };
        if place
            .root()
            .is_some_and(|root| root.name == "self" && !self.self_is_owned)
        {
            return true;
        }
        self.projection_type(object)
            .is_some_and(|ty| self.nearest_reference(object, &ty).is_some())
    }

    /// Whether a value of `ty` is, or holds, a function value. A closure's captures live
    /// in the frame that wrote it, so the language keeps such a value inside that frame.
    pub(crate) fn holds_function_value(&self, ty: &Type) -> bool {
        self.holds_function_within(ty, &mut std::collections::HashSet::new())
    }

    pub(super) fn holds_function_within(
        &self,
        ty: &Type,
        seen: &mut std::collections::HashSet<std::string::String>,
    ) -> bool {
        match ty {
            Type::Function { .. } => true,
            Type::Struct(name) => {
                seen.insert(name.clone())
                    && self.struct_defs.get(name).is_some_and(|fields| {
                        fields
                            .iter()
                            .any(|(_, field)| self.holds_function_within(field, seen))
                    })
            }
            Type::Enum(name) => {
                seen.insert(name.clone())
                    && self.enum_defs.get(name).is_some_and(|variants| {
                        variants
                            .iter()
                            .flat_map(|variant| variant.fields.iter())
                            .any(|(_, payload)| self.holds_function_within(payload, seen))
                    })
            }
            Type::Newtype(name) => {
                seen.insert(name.clone())
                    && self
                        .newtype_defs
                        .get(name)
                        .is_some_and(|inner| self.holds_function_within(inner, seen))
            }
            Type::Array { element, .. } => self.holds_function_within(element, seen),
            Type::Tuple(elements) => elements
                .iter()
                .any(|element| self.holds_function_within(element, seen)),
            _ => false,
        }
    }

    /// Refuse a declared return type that is or holds a function value.
    pub(crate) fn refuse_function_valued_return(&mut self, return_type: &Type, span: Span) {
        if self.holds_function_value(return_type) {
            self.record_error(TypeError::FunctionValueEscapes {
                problem: "be returned".to_string(),
                span,
            });
        }
    }

    /// Store into a whole binding: the one place form that also replaces what the
    /// binding held, so borrow, move and mutability state on the name itself change.
    pub(super) fn check_binding_store(
        &mut self,
        target: &Identifier,
        expected_ty: &Type,
        value: &Expr,
        span: Span,
    ) -> Option<()> {
        let expected_ty = Some(expected_ty.clone());

        // Replacing the value destroys what every live borrow of the target points at,
        // so the borrowee rules apply to the write as much as to a read or a move.
        // Tested before the RHS is checked: a `&target` appearing in the RHS is a borrow
        // this assignment does not conflict with.
        if let Some((shared, exclusive)) = self.symbols.borrow_counts(&target.name)
            && (shared > 0 || exclusive > 0)
        {
            self.record_error(TypeError::CannotAssignWhileBorrowed {
                name: target.name.clone(),
                span: target.span,
            });
        }

        // If the target was a reference binding, its previous borrow ends
        // here: release it before the new value is checked so that
        // re-borrowing the same place (`r = &mut x`) is not a false
        // conflict against the borrow being overwritten.
        self.symbols.release_borrow_of(&target.name);

        let value_ty = self
            .check_expr(value, expected_ty.as_ref())
            .unwrap_or(Type::Unknown);

        // The RHS is moved into the target, and the target now owns a
        // fresh value, clearing any prior moved-out state on it.
        self.record_move(value);
        self.symbols.clear_moved(&target.name);

        self.check_pool_store(&target.name, &value_ty, value, span, false);

        // A direct `&place` / `&mut place` RHS makes the target hold a new
        // persistent borrow of that place.
        if let Some((place, exclusive)) = borrow_target_of(value) {
            self.symbols.attach_borrow(&target.name, &place, exclusive);
        } else {
            self.hold_carried_borrows(&target.name, value, &value_ty);
        }
        if let Some(place) = gradient_view_root(value, &value_ty) {
            self.symbols.attach_borrow(&target.name, &place, false);
        }
        self.hold_returned_borrows(&target.name, value, &value_ty);

        let symbol_info = self.symbols.lookup(&target.name)?;
        if !matches!(value_ty, Type::Unknown) && !value_ty.is_compatible_with(&symbol_info.ty) {
            self.record_error(TypeError::Mismatch {
                expected: symbol_info.ty.clone(),
                found: value_ty,
                span,
            });
        }
        Some(())
    }
}
