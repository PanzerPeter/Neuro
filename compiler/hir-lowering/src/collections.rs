//! Lowering for the standard collections `Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`.
//!
//! The type checker has already validated element/key types and method arity, so this
//! only re-derives the resolved types the HIR must carry. The method table is
//! duplicated from the checker's rather than shared: the two slices stay independent,
//! and a divergence surfaces as a lowering error instead of a silent miscompile.

use ast_types::Expr;
use neuro_hir::{HirCollectionKind, HirExpr, HirExprKind, HirType};

use crate::{Lowerer, LoweringError, MonoArg};

/// The prelude enum the fallible readers (`Vec::pop`, `Map::get`) return.
const OPTION_ENUM: &str = "Option";

/// The associated function that builds an empty collection.
pub(crate) const COLLECTION_CTOR: &str = "new";

/// The collection named by `name`, or `None` when it is not a standard collection.
pub(crate) fn collection_kind(name: &str) -> Option<HirCollectionKind> {
    match name {
        "Vec" => Some(HirCollectionKind::Vec),
        "HashMap" => Some(HirCollectionKind::HashMap),
        "BTreeMap" => Some(HirCollectionKind::BTreeMap),
        _ => None,
    }
}

impl Lowerer {
    /// Lower `Vec::new()` / `HashMap::new()` / `BTreeMap::new()`. The collection's
    /// element types come from the annotated target, which the checker required.
    pub(crate) fn lower_collection_new(
        &mut self,
        kind: HirCollectionKind,
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        match expected {
            Some(ty @ HirType::Collection { kind: k, .. }) if *k == kind => {
                Ok(HirExpr::new(HirExprKind::CollectionNew, ty.clone(), span))
            }
            _ => Err(LoweringError::Malformed {
                detail: format!("`{}::new()` reached lowering without a target type", {
                    kind.name()
                }),
            }),
        }
    }

    /// Lower a method call on a collection receiver, returning the lowered arguments
    /// and the call's result type.
    pub(crate) fn lower_collection_method(
        &mut self,
        recv: &HirType,
        method: &str,
        args: &[Expr],
    ) -> Result<(Vec<HirExpr>, HirType), LoweringError> {
        let HirType::Collection { kind, args: params } = recv.referent().clone() else {
            return Err(LoweringError::UnresolvedCall {
                target: format!("{}.{}", recv, method),
            });
        };
        let key = params.first().cloned().unwrap_or(HirType::Void);
        let value = params.last().cloned().unwrap_or(HirType::Void);

        let (param_tys, result): (Vec<HirType>, HirType) = match (kind, method) {
            (_, "len") => (vec![], HirType::U64),
            (_, "clear") => (vec![], HirType::Void),
            (HirCollectionKind::Vec, "push") => (vec![value], HirType::Void),
            (HirCollectionKind::Vec, "pop") => (vec![], self.option_of(value)?),
            (HirCollectionKind::Vec, "get") => (vec![HirType::U64], self.option_of(value)?),
            (HirCollectionKind::HashMap | HirCollectionKind::BTreeMap, "insert") => {
                (vec![key, value], HirType::Void)
            }
            (HirCollectionKind::HashMap | HirCollectionKind::BTreeMap, "get") => {
                (vec![key], self.option_of(value)?)
            }
            (HirCollectionKind::HashMap | HirCollectionKind::BTreeMap, "contains_key") => {
                (vec![key], HirType::Bool)
            }
            (HirCollectionKind::HashMap | HirCollectionKind::BTreeMap, "remove") => {
                (vec![key], HirType::Bool)
            }
            (HirCollectionKind::HashMap | HirCollectionKind::BTreeMap, "keys") => (
                vec![],
                HirType::Collection {
                    kind: HirCollectionKind::Vec,
                    args: vec![key],
                },
            ),
            _ => {
                return Err(LoweringError::UnresolvedCall {
                    target: format!("{}.{}", recv, method),
                })
            }
        };

        Ok((self.lower_args(args, &param_tys)?, result))
    }

    /// The element type yielded by `v[i]` and `for x in v`, or `None` for a receiver
    /// that is not an indexable collection.
    pub(crate) fn collection_element(ty: &HirType) -> Option<HirType> {
        match ty.referent() {
            HirType::Collection {
                kind: HirCollectionKind::Vec,
                args,
            } => args.first().cloned(),
            _ => None,
        }
    }

    /// Monomorphize `Option<T>` for a fallible reader's result, materializing the
    /// instance so the backend sees an ordinary enum item.
    fn option_of(&mut self, inner: HirType) -> Result<HirType, LoweringError> {
        let mangled = self.instantiate_generic_enum(OPTION_ENUM, &[MonoArg::Type(inner)])?;
        Ok(HirType::Enum(mangled))
    }
}
