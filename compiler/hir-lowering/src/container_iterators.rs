//! `.iter()` on an array, a slice or a `Vec`, and `m.keys()` / `m.values()` on a map,
//! built as literals of the prelude structs that walk them, so no backend learns they
//! exist:
//!
//! ```text
//! v.iter()    =>  Iter<T> { items: &v as &[T], position: 0 }
//! m.keys()    =>  HashKeys<K, V> { map: &m, slot: 0 }       (BTreeKeys for a BTreeMap)
//! m.values()  =>  HashValues<K, V> { map: &m, slot: 0 }     (BTreeValues likewise)
//! ```
//!
//! The four map intrinsics the views step with are ordinary collection methods from here
//! on; `collections.rs` types them and the backend emits them.

use neuro_hir::{HirCollectionKind, HirExpr, HirExprKind, HirFieldInit, HirType};
use shared_types::{Literal, Span};

use crate::{Lowerer, LoweringError, MonoArg};

const ITER_METHOD: &str = "iter";
const ITER_STRUCT: &str = "Iter";
const KEYS_METHOD: &str = "keys";
const VALUES_METHOD: &str = "values";

/// The prelude struct `method` builds over a receiver of type `recv`, the field holding
/// the borrow, the cursor field, and the struct's type arguments.
fn iterator_shape(
    recv: &HirType,
    method: &str,
) -> Option<(&'static str, &'static str, &'static str, Vec<HirType>)> {
    match (recv, method) {
        (HirType::Array { element, .. } | HirType::Slice(element), ITER_METHOD) => {
            Some((ITER_STRUCT, "items", "position", vec![(**element).clone()]))
        }
        (
            HirType::Collection {
                kind: HirCollectionKind::Vec,
                args,
            },
            ITER_METHOD,
        ) => Some((ITER_STRUCT, "items", "position", args.clone())),
        (HirType::Collection { kind, args }, KEYS_METHOD | VALUES_METHOD) => {
            let keys = method == KEYS_METHOD;
            let base = match (kind, keys) {
                (HirCollectionKind::HashMap, true) => "HashKeys",
                (HirCollectionKind::HashMap, false) => "HashValues",
                (HirCollectionKind::BTreeMap, true) => "BTreeKeys",
                (HirCollectionKind::BTreeMap, false) => "BTreeValues",
                _ => return None,
            };
            Some((base, "map", "slot", args.clone()))
        }
        _ => None,
    }
}

impl Lowerer {
    /// Build the iterator `object.method()` yields over a container, or answer `None`
    /// when the call is not one. `object` is the receiver read down to one `&`.
    ///
    /// The checker refused a receiver that is neither a place nor a borrow, so the borrow
    /// taken here always points at storage that outlives the call.
    pub(crate) fn lower_container_iterator(
        &mut self,
        object: &HirExpr,
        method: &str,
        span: Span,
    ) -> Result<Option<HirExpr>, LoweringError> {
        let Some((base, borrow_field, cursor_field, args)) =
            iterator_shape(object.ty.referent(), method)
        else {
            return Ok(None);
        };
        let borrow = shared_borrow(object.clone(), span);
        let name = self.instantiate_generic_struct(
            base,
            &args.into_iter().map(MonoArg::Type).collect::<Vec<_>>(),
        )?;
        let cursor = HirExpr::new(
            HirExprKind::Literal(Literal::Integer(0, None)),
            HirType::U64,
            span,
        );
        let fields = [(borrow_field, borrow), (cursor_field, cursor)]
            .into_iter()
            .map(|(field, value)| HirFieldInit {
                name: field.to_string(),
                value: Box::new(value),
                span,
            })
            .collect();
        Ok(Some(HirExpr::new(
            HirExprKind::StructLiteral {
                name: name.clone(),
                fields,
                base: None,
            },
            HirType::Struct(name),
            span,
        )))
    }
}

/// A shared borrow of the container `object`: `&object` for a place, the borrow itself
/// for a `&`, and a `&mut` read as the `&` it implies. An array or a `Vec` is then
/// unsized to the `&[T]` view `Iter` holds.
fn shared_borrow(object: HirExpr, span: Span) -> HirExpr {
    let referent = object.ty.referent().clone();
    let borrowed = match object.ty {
        HirType::Reference { .. } => HirExpr {
            ty: HirType::Reference {
                inner: Box::new(referent.clone()),
                mutable: false,
            },
            ..object
        },
        _ => HirExpr::new(
            HirExprKind::Reference {
                operand: Box::new(object),
                mutable: false,
            },
            HirType::Reference {
                inner: Box::new(referent.clone()),
                mutable: false,
            },
            span,
        ),
    };
    let element = match &referent {
        HirType::Array { element, .. } => element.clone(),
        HirType::Collection {
            kind: HirCollectionKind::Vec,
            args,
        } => Box::new(args.first().cloned().unwrap_or(HirType::Void)),
        _ => return borrowed,
    };
    HirExpr::new(
        HirExprKind::SliceCoerce {
            value: Box::new(borrowed),
        },
        HirType::Reference {
            inner: Box::new(HirType::Slice(element)),
            mutable: false,
        },
        span,
    )
}

#[cfg(test)]
mod tests {
    use super::iterator_shape;
    use neuro_hir::{HirCollectionKind, HirType};

    #[test]
    fn each_container_names_the_struct_it_builds() {
        let vec = HirType::Collection {
            kind: HirCollectionKind::Vec,
            args: vec![HirType::I32],
        };
        let slice = HirType::Slice(Box::new(HirType::String));
        let ordered = HirType::Collection {
            kind: HirCollectionKind::BTreeMap,
            args: vec![HirType::String, HirType::I32],
        };
        let shape = |ty: &HirType, method| iterator_shape(ty, method).map(|(base, ..)| base);
        assert_eq!(shape(&vec, "iter"), Some("Iter"));
        assert_eq!(shape(&slice, "iter"), Some("Iter"));
        assert_eq!(shape(&ordered, "keys"), Some("BTreeKeys"));
        assert_eq!(shape(&ordered, "values"), Some("BTreeValues"));
        assert_eq!(shape(&vec, "keys"), None);
        assert_eq!(shape(&ordered, "iter"), None);
        let (_, field, cursor, args) = iterator_shape(&slice, "iter").expect("a slice walks");
        assert_eq!((field, cursor), ("items", "position"));
        assert_eq!(args, vec![HirType::String]);
    }
}
