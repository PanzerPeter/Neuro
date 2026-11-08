//! `.map(f)`, `.filter(p)` and `.enumerate()` called on an `Iterator` value.
//!
//! Each call becomes a literal of the prelude adapter it names, holding the receiver and
//! the function, so no backend learns the adapters exist:
//!
//! ```text
//! it.map(f)       =>  Map<I, A, B> { inner: it, f: f }
//! it.filter(p)    =>  Filter<I, A> { inner: it, keep: p }
//! it.enumerate()  =>  Enumerate<I, A> { inner: it, position: 0 }
//! ```
//!
//! `A` is what the receiver's `next` yields, and `B` what `f` returns. The checker has
//! already refused every call this cannot build.

use ast_types::Expr;
use neuro_hir::{HirExpr, HirExprKind, HirFieldInit, HirType};
use shared_types::{Literal, Span};

use crate::closures::ClosureHint;
use crate::{Lowerer, LoweringError, MonoArg};

const MAP_METHOD: &str = "map";
const FILTER_METHOD: &str = "filter";
const ENUMERATE_METHOD: &str = "enumerate";
const MAP_STRUCT: &str = "Map";
const FILTER_STRUCT: &str = "Filter";
const ENUMERATE_STRUCT: &str = "Enumerate";
/// The field every adapter keeps the iterator it wraps in.
const INNER_FIELD: &str = "inner";
const MAP_FUNCTION_FIELD: &str = "f";
const FILTER_PREDICATE_FIELD: &str = "keep";
const ENUMERATE_POSITION_FIELD: &str = "position";

/// Whether `method` names one of the adapters an `Iterator` value is given.
pub(crate) fn is_iterator_adapter(method: &str) -> bool {
    matches!(method, MAP_METHOD | FILTER_METHOD | ENUMERATE_METHOD)
}

impl Lowerer {
    /// Lower `receiver.method(args)` to the adapter literal wrapping `receiver`.
    pub(crate) fn lower_iterator_adapter(
        &mut self,
        receiver: HirExpr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let item = self.protocol_item_type(&receiver.ty)?;
        let iterator = MonoArg::Type(receiver.ty.clone());
        let (base, type_args, field) = match method {
            ENUMERATE_METHOD => {
                let zero = HirExpr::new(
                    HirExprKind::Literal(Literal::Integer(0, None)),
                    HirType::U64,
                    span,
                );
                let args = vec![iterator, MonoArg::Type(item)];
                (ENUMERATE_STRUCT, args, (ENUMERATE_POSITION_FIELD, zero))
            }
            FILTER_METHOD => {
                let borrowed = HirType::Reference {
                    inner: Box::new(item.clone()),
                    mutable: false,
                };
                let keep = self.adapter_function(args, borrowed, Some(HirType::Bool))?;
                let args = vec![iterator, MonoArg::Type(item)];
                (FILTER_STRUCT, args, (FILTER_PREDICATE_FIELD, keep))
            }
            _ => {
                let f = self.adapter_function(args, item.clone(), None)?;
                let HirType::Function { ret, .. } = &f.ty else {
                    return Err(LoweringError::Malformed {
                        detail: "`.map()` on an iterator is not given a function".to_string(),
                    });
                };
                let args = vec![
                    iterator,
                    MonoArg::Type(item),
                    MonoArg::Type((**ret).clone()),
                ];
                (MAP_STRUCT, args, (MAP_FUNCTION_FIELD, f))
            }
        };
        let name = self.instantiate_generic_struct(base, &type_args)?;
        let fields = [(INNER_FIELD, receiver), field]
            .into_iter()
            .map(|(field, value)| HirFieldInit {
                name: field.to_string(),
                value: Box::new(value),
                span,
            })
            .collect();
        Ok(HirExpr::new(
            HirExprKind::StructLiteral {
                name: name.clone(),
                fields,
                base: None,
            },
            HirType::Struct(name),
            span,
        ))
    }

    /// The one function argument of `.map` / `.filter`, typed by the element it is handed.
    fn adapter_function(
        &mut self,
        args: &[Expr],
        param: HirType,
        ret: Option<HirType>,
    ) -> Result<HirExpr, LoweringError> {
        let [arg] = args else {
            return Err(LoweringError::Malformed {
                detail: "an iterator adapter takes one function".to_string(),
            });
        };
        let hint = ClosureHint {
            params: vec![Some(param)],
            ret,
        };
        self.lower_function_arg(arg, &hint)
    }
}
