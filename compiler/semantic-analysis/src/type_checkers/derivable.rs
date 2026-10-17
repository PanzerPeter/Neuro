//! The derivable traits: `Copy`, `Clone`, `Debug`, `PartialEq` and `Hashable`.
//!
//! Each is an ordinary trait a program may implement by hand and name as a bound, but a
//! scalar, `string` and a struct carrying the matching `@derive` implement it with no
//! `impl` block: the compiler generates what the impl would have done. A bound has to
//! accept both kinds of implementor alike, so this module answers "implements" for the
//! five names, and types the methods the compiler supplies in place of an impl.

use ast_types::Expr;
use shared_types::Span;

use super::TypeChecker;
use crate::types::{CollectionKind, Type};

pub(crate) const COPY_TRAIT: &str = "Copy";
pub(crate) const CLONE_TRAIT: &str = "Clone";
pub(crate) const DEBUG_TRAIT: &str = "Debug";
pub(crate) const PARTIAL_EQ_TRAIT: &str = "PartialEq";
pub(crate) const HASHABLE_TRAIT: &str = "Hashable";

/// The method `Debug` declares: it appends the value's debug form to a builder.
pub(crate) const FMT_DEBUG_METHOD: &str = "fmt_debug";
/// The method `Hashable` declares.
pub(crate) const HASH_METHOD: &str = "hash";
/// The method `PartialEq` declares: `eq(&self, rhs: &Self) -> bool`.
pub(crate) const EQ_METHOD: &str = "eq";

impl TypeChecker {
    /// Whether type parameter `param` carries a `Copy` bound in the scope being checked.
    pub(crate) fn has_copy_bound(&self, param: &str) -> bool {
        self.generic_bounds
            .get(param)
            .is_some_and(|bounds| bounds.iter().any(|b| b.trait_name == COPY_TRAIT))
    }

    /// Whether `concrete` meets a `Trait` bound: a type parameter through a bound of its
    /// own, any other type through what the compiler supplies or through an `impl`.
    pub(crate) fn satisfies_bound(&self, concrete: &Type, trait_name: &str) -> bool {
        if let Type::Generic(param) = concrete {
            // `Copy: Clone`, so a `Copy` bound answers for `Clone` as well.
            return self.generic_bounds.get(param).is_some_and(|bounds| {
                bounds.iter().any(|b| {
                    b.trait_name == trait_name
                        || (trait_name == CLONE_TRAIT && b.trait_name == COPY_TRAIT)
                })
            });
        }
        self.implements_without_impl(concrete, trait_name)
            || self.type_implements_trait(concrete, trait_name)
    }

    /// Whether `ty` implements a derivable trait with no `impl` block: a builtin type the
    /// compiler covers, or a struct that derives it.
    ///
    /// A reference implements none of them here although it is `Copy`. A method call
    /// reads through a borrow to its referent, so in an instance where `T` is `&S` the
    /// body's `v.clone()` would reach `S`'s `clone`, not the reference's copy.
    pub(crate) fn implements_without_impl(&self, ty: &Type, trait_name: &str) -> bool {
        if matches!(
            ty,
            Type::Reference { .. } | Type::Generic(_) | Type::Unknown
        ) {
            return false;
        }
        match trait_name {
            COPY_TRAIT => self.is_type_copy(ty),
            CLONE_TRAIT => match ty {
                Type::String => true,
                Type::Struct(name) => self.clone_structs.contains(name),
                other => self.is_type_copy(other),
            },
            DEBUG_TRAIT => match ty {
                Type::Struct(name) => self.debug_structs.contains(name),
                Type::Bool | Type::Char | Type::String => true,
                other => other.is_numeric() || other.is_half_float(),
            },
            HASHABLE_TRAIT => match ty {
                Type::Struct(name) => self.hashable_structs.contains(name),
                Type::Bool | Type::Char | Type::String => true,
                other => other.is_integer(),
            },
            PARTIAL_EQ_TRAIT => self.is_derived_comparable(ty),
            _ => false,
        }
    }

    /// Whether a `@derive(Hashable)` struct may hold a field of type `ty`: the derived
    /// hash folds each field's own hash, so the field must have one. A struct field must
    /// derive it too, as for `PartialEq`: the rule runs with the derives, before any impl
    /// is registered.
    pub(crate) fn is_derived_hashable(&self, ty: &Type) -> bool {
        match ty {
            // A template defers to its instances, which are validated where they are made.
            Type::Generic(_) | Type::Unknown => true,
            Type::Newtype(name) => self
                .lookup_newtype_inner(name)
                .cloned()
                .is_some_and(|inner| self.is_derived_hashable(&inner)),
            other => self.implements_without_impl(other, HASHABLE_TRAIT),
        }
    }

    /// Type a call of a derivable trait's method on a receiver that implements the trait
    /// without an `impl`: `x.clone()` on a `Copy` value, `s.hash()`, `s.eq(&t)` and
    /// `s.fmt_debug(out)` on a derived struct or a builtin. `None` when `method` is not one of those here,
    /// so the caller reports the method as missing.
    pub(crate) fn check_supplied_trait_method(
        &mut self,
        receiver: &Type,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        let receiver = receiver.referent();
        let (trait_name, params, ret) = match method {
            super::expressions::CLONE_METHOD => (CLONE_TRAIT, Vec::new(), receiver.clone()),
            HASH_METHOD => (HASHABLE_TRAIT, Vec::new(), Type::U64),
            EQ_METHOD => (
                PARTIAL_EQ_TRAIT,
                vec![Type::Reference {
                    inner: Box::new(receiver.clone()),
                    mutable: false,
                }],
                Type::Bool,
            ),
            FMT_DEBUG_METHOD => (
                DEBUG_TRAIT,
                vec![Type::Reference {
                    inner: Box::new(Type::Collection {
                        kind: CollectionKind::StringBuilder,
                        args: Vec::new(),
                    }),
                    mutable: true,
                }],
                Type::Void,
            ),
            _ => return None,
        };
        if !self.implements_without_impl(receiver, trait_name) {
            return None;
        }
        self.check_call_args(args, &params, span);
        Some(ret)
    }
}
