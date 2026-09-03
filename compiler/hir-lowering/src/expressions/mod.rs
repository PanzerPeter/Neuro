//! Expression lowering with resolved-type re-derivation: the `lower_expr`
//! dispatch and the block-value tail rule it shares with every block-shaped
//! node. Each expression category lives in a sibling module here; all of them
//! add methods to the same `impl Lowerer` block.

mod calls;
mod coalesce;
mod coercion;
mod enums;
mod interpolation;
mod matches;
mod sequences;
mod structs;
mod try_op;

use ast_types::{Expr, UnaryOp};
use neuro_hir::{HirExpr, HirExprKind, HirStmt, HirType};
use shared_types::Literal;

use coercion::{
    apply_unsizing_coercion, binary_result_type, is_numeric, literal_scalar, literal_type,
};

use crate::{is_integer, LoopCtx, Lowerer, LoweringError};

/// The divergent panic-family builtins. Each aborts and never returns, so a
/// call takes on whatever type its context demands.
const PANIC_BUILTINS: &[&str] = &["panic", "assert", "unreachable"];

/// The standard-output builtins. Each takes one `string` and returns unit, so —
/// unlike the panic family — the call's type is fixed rather than taken from context.
const IO_BUILTINS: &[&str] = &["print", "println"];

/// The deep-copy method shared by `string` and `Clone`-deriving structs.
const CLONE_METHOD: &str = "clone";

/// The borrowing sub-range method on every contiguous container.
const SLICE_METHOD: &str = "slice";

/// The codepoint iterator `string.chars()` yields, declared in the prelude, and the
/// two fields the lowering fills in: the borrowed text and the byte cursor into it.
pub(crate) const CHARS_STRUCT: &str = "Chars";
pub(crate) const CHARS_SOURCE_FIELD: &str = "source";
pub(crate) const CHARS_OFFSET_FIELD: &str = "offset";
/// The method that produces one.
pub(crate) const CHARS_METHOD: &str = "chars";
/// The prelude-private decode step `Chars::next` is written against.
const CHAR_AT_METHOD: &str = "__char_at";

/// An enum variant's ordered payload fields: each `(optional field name, type)`.
/// `Some` name marks a struct-variant field; `None` a tuple-variant element.
type PayloadFields = Vec<(Option<String>, HirType)>;

impl Lowerer {
    /// Lower an expression to a typed [`HirExpr`], deriving its resolved type from
    /// the surrounding `expected` type where the language's contextual inference
    /// rules require it (literals, array elements, …).
    pub(crate) fn lower_expr(
        &mut self,
        expr: &Expr,
        expected: Option<&HirType>,
    ) -> Result<HirExpr, LoweringError> {
        let lowered = self.lower_expr_uncoerced(expr, expected)?;
        Ok(apply_unsizing_coercion(lowered, expected))
    }

    /// Lower an expression without applying the unsizing coercions. Every contextual
    /// typing rule lives here; [`Lowerer::lower_expr`] wraps the result so the two
    /// unsizing sites — `&T` → `&dyn Trait` and `&[T; N]` / `&Vec<T>` → `&[T]` — are
    /// applied uniformly wherever an expected type is supplied: call arguments,
    /// returns, and annotated bindings.
    fn lower_expr_uncoerced(
        &mut self,
        expr: &Expr,
        expected: Option<&HirType>,
    ) -> Result<HirExpr, LoweringError> {
        match expr {
            // Grouping is encoded by tree structure in the HIR; drop the node.
            Expr::Paren(inner, _) => self.lower_expr(inner, expected),

            // `?` desugars to a `match` whose failure arm returns; the expected type
            // describes the unwrapped payload, not the fallible operand.
            Expr::Try { operand, span } => self.lower_try(operand, *span),

            Expr::Literal(lit, span) => {
                let ty = literal_type(lit, expected);
                Ok(HirExpr::new(HirExprKind::Literal(lit.clone()), ty, *span))
            }

            Expr::InterpString { parts, span } => self.lower_interp_string(parts, *span),

            // A const generic parameter used as a value inside a monomorphized body
            // lowers to its concrete integer literal, typed by its declared int type.
            Expr::Identifier(ident) if self.const_subst.contains_key(&ident.name) => {
                let value = self.const_subst[&ident.name];
                let ty = self
                    .const_types
                    .get(&ident.name)
                    .cloned()
                    .unwrap_or(HirType::U64);
                Ok(HirExpr::new(
                    HirExprKind::Literal(Literal::Integer(value as i64, None)),
                    ty,
                    ident.span,
                ))
            }
            Expr::Identifier(ident) => match self.lookup(&ident.name) {
                Some(ty) => Ok(HirExpr::new(
                    HirExprKind::Variable(ident.name.clone()),
                    ty,
                    ident.span,
                )),
                None => Err(LoweringError::UnresolvedBinding {
                    name: ident.name.clone(),
                }),
            },

            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                // `??` is not an operand-symmetric operator: it desugars to a `match` on
                // the left side, which types the right side from the unwrapped payload.
                if matches!(op, ast_types::BinaryOp::NullCoalesce) {
                    return self.lower_null_coalesce(left, right, *span);
                }
                let left = self.lower_expr(left, None)?;
                // Operator-trait dispatch on a user type: desugar `a OP b` into the
                // impl method call `a.op(b)`. The checker validated the impl, so a lookup
                // hit means the call resolves.
                if let HirType::Struct(name) = left.ty.referent() {
                    if let Some(dispatch) = self.operator_binary_impls.get(&(name.clone(), *op)) {
                        let dispatch = crate::OpDispatch {
                            method: dispatch.method.clone(),
                            rhs_param: dispatch.rhs_param.clone(),
                            result: dispatch.result.clone(),
                        };
                        let right = self.lower_expr(right, None)?;
                        return self.build_operator_call(left, right, dispatch, *span);
                    }
                }
                let right = self.lower_expr(right, Some(&left.ty))?;
                let ty = binary_result_type(*op, &left.ty, &right.ty)?;
                Ok(HirExpr::new(
                    HirExprKind::Binary {
                        op: *op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Unary { op, operand, span } => {
                let operand_expected = match op {
                    UnaryOp::Negate => expected.filter(|t| is_numeric(t)),
                    UnaryOp::Not => None,
                    UnaryOp::BitNot => expected.filter(|t| is_integer(t)),
                };
                let operand = self.lower_expr(operand, operand_expected)?;
                // Operator-trait dispatch: `-a` → `a.neg()`, `~a` → `a.not()`.
                if let HirType::Struct(name) = operand.ty.referent() {
                    if let Some((method, result)) =
                        self.operator_unary_impls.get(&(name.clone(), *op))
                    {
                        let (method, result) = (method.clone(), result.clone());
                        return Ok(self.build_unary_operator_call(operand, method, result, *span));
                    }
                }
                let ty = match op {
                    UnaryOp::Negate | UnaryOp::BitNot => operand.ty.clone(),
                    UnaryOp::Not => HirType::Bool,
                };
                Ok(HirExpr::new(
                    HirExprKind::Unary {
                        op: *op,
                        operand: Box::new(operand),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Cast {
                expr,
                target_type,
                span,
            } => {
                let value = self.lower_expr(expr, None)?;
                let ty = self.resolve_type(target_type)?;
                Ok(HirExpr::new(
                    HirExprKind::Cast {
                        value: Box::new(value),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Call {
                func,
                type_args,
                args,
                arg_labels,
                span,
            } => {
                // Every named argument was bound to a parameter before type checking.
                // A label surviving to here means the binding pass never reached this
                // call, which would silently bind arguments in the order they were
                // written; refusing is the difference between a loud bug and a wrong
                // program.
                if !arg_labels.is_empty() {
                    return Err(LoweringError::Malformed {
                        detail: "a named argument reached lowering unbound".to_string(),
                    });
                }
                self.lower_call(func, type_args, args, expected, *span)
            }

            Expr::Closure {
                params,
                ret,
                body,
                span,
                ..
            } => self.lower_closure(params, ret.as_ref(), body, *span),

            // A bare path is a unit-variant enum construction `E::V` when the
            // type names an enum, else an associated-function reference.
            Expr::Path {
                type_name,
                member,
                span,
            } if self.enums.contains_key(&type_name.name)
                || self.is_generic_enum(&type_name.name) =>
            {
                self.lower_enum_construct(&type_name.name, &member.name, expected, *span)
            }

            Expr::Path {
                type_name,
                member,
                span,
            } => {
                let (params, ret) = self.assoc_signature(&type_name.name, &member.name)?;
                let ty = HirType::Function {
                    params,
                    ret: Box::new(ret),
                };
                Ok(HirExpr::new(
                    HirExprKind::Path {
                        type_name: type_name.name.clone(),
                        member: member.name.clone(),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::StructLiteral {
                name,
                fields,
                base,
                span,
            } => self.lower_struct_literal(name, fields, base, *span),

            Expr::EnumStructLiteral {
                enum_name,
                variant,
                fields,
                span,
            } => self.lower_enum_struct_literal(
                &enum_name.name,
                &variant.name,
                fields,
                expected,
                *span,
            ),

            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                let object = self.lower_expr(object, None)?;
                let HirType::Struct(struct_name) = object.ty.referent().clone() else {
                    return Err(LoweringError::Malformed {
                        detail: format!("field access on non-struct type '{}'", object.ty),
                    });
                };
                let ty = self.struct_field_type(&struct_name, &field.name)?;
                Ok(HirExpr::new(
                    HirExprKind::FieldAccess {
                        object: Box::new(object),
                        field: field.name.clone(),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Reference {
                operand,
                mutable,
                span,
            } => {
                let operand = self.lower_expr(operand, None)?;
                let ty = HirType::Reference {
                    inner: Box::new(operand.ty.clone()),
                    mutable: *mutable,
                };
                Ok(HirExpr::new(
                    HirExprKind::Reference {
                        operand: Box::new(operand),
                        mutable: *mutable,
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Deref { operand, span } => {
                let operand = self.lower_expr(operand, None)?;
                let HirType::Reference { inner, .. } = &operand.ty else {
                    return Err(LoweringError::Malformed {
                        detail: format!("dereference of non-reference type '{}'", operand.ty),
                    });
                };
                let ty = (**inner).clone();
                Ok(HirExpr::new(
                    HirExprKind::Deref {
                        operand: Box::new(operand),
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Range {
                start,
                end,
                inclusive,
                span,
            } => self.lower_range(start, end, *inclusive, *span),

            Expr::ArrayLiteral { elements, span } => {
                self.lower_array_literal(elements, expected, *span)
            }

            Expr::TupleLiteral { elements, span } => {
                self.lower_tuple_literal(elements, expected, *span)
            }

            Expr::TupleIndex {
                object,
                index,
                span,
            } => {
                let object = self.lower_expr(object, None)?;
                match object.ty.referent().clone() {
                    HirType::Tuple(element_tys) => {
                        let element_ty = element_tys.get(*index).cloned().ok_or_else(|| {
                            LoweringError::Malformed {
                                detail: format!(
                                    "tuple index {} out of range for arity {}",
                                    index,
                                    element_tys.len()
                                ),
                            }
                        })?;
                        Ok(HirExpr::new(
                            HirExprKind::TupleIndex {
                                object: Box::new(object),
                                index: *index,
                            },
                            element_ty,
                            *span,
                        ))
                    }
                    // `.0` on a newtype reads its transparent inner value. The
                    // checker guarantees the index is 0.
                    HirType::Newtype { inner, .. } => Ok(HirExpr::new(
                        HirExprKind::NewtypeAccess {
                            object: Box::new(object),
                        },
                        *inner,
                        *span,
                    )),
                    other => Err(LoweringError::Malformed {
                        detail: format!("tuple index into non-tuple type '{}'", other),
                    }),
                }
            }

            Expr::Index {
                object,
                index,
                span,
            } => {
                let object = self.lower_expr(object, None)?;
                let index = self.lower_expr(index, None)?;
                let element = match Self::collection_element(&object.ty) {
                    Some(element) => element,
                    None => match object.ty.referent().clone() {
                        HirType::Array { element, .. } | HirType::Slice(element) => *element,
                        other => {
                            return Err(LoweringError::Malformed {
                                detail: format!("index into non-indexable type '{}'", other),
                            })
                        }
                    },
                };
                Ok(HirExpr::new(
                    HirExprKind::Index {
                        object: Box::new(object),
                        index: Box::new(index),
                    },
                    element,
                    *span,
                ))
            }

            Expr::ArrayRest {
                array,
                start,
                exact,
                span,
            } => {
                let array = self.lower_expr(array, None)?;
                let HirType::Array { element, size } = array.ty.referent().clone() else {
                    return Err(LoweringError::Malformed {
                        detail: format!("array rest pattern on non-array type '{}'", array.ty),
                    });
                };
                // Arity is validated in semantic analysis; re-check here so a
                // malformed input surfaces as an error rather than a subtraction
                // underflow on `size - start`.
                if (*exact && size != *start) || (!*exact && *start > size) {
                    return Err(LoweringError::Malformed {
                        detail: format!(
                            "array destructuring binds {} element(s) but the array has {}",
                            start, size
                        ),
                    });
                }
                Ok(HirExpr::new(
                    HirExprKind::ArrayRest {
                        array: Box::new(array),
                        start: *start,
                    },
                    HirType::Array {
                        element,
                        size: size - *start,
                    },
                    *span,
                ))
            }

            Expr::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            } => self.lower_if_expr(
                condition,
                then_block,
                else_if_blocks,
                else_block,
                expected,
                *span,
            ),

            Expr::Block { stmts, span } => {
                let (stmts, ty) = self.lower_block_value(stmts, expected)?;
                Ok(HirExpr::new(HirExprKind::Block { stmts }, ty, *span))
            }

            Expr::Unsafe { stmts, span } => {
                let (stmts, ty) = self.lower_block_value(stmts, expected)?;
                Ok(HirExpr::new(HirExprKind::Unsafe { stmts }, ty, *span))
            }

            Expr::Loop { label, body, span } => {
                let label_name = label.as_ref().map(|l| l.name.clone());
                self.loop_stack.push(LoopCtx {
                    label: label_name.clone(),
                    is_value: true,
                    value_ty: None,
                    has_break: false,
                });
                self.push_scope();
                let body = self.lower_stmt_list(body);
                self.pop_scope();
                let ctx = self.loop_stack.pop();
                let body = body?;
                // A `loop` no `break` targets never reaches its exit block, so it
                // yields no value; it takes the expected type so the exit block's
                // (dead) result slot is still typed for the position it sits in.
                let ty = match ctx {
                    Some(LoopCtx {
                        value_ty: Some(ty), ..
                    }) => ty,
                    Some(LoopCtx {
                        has_break: false, ..
                    }) => expected.cloned().unwrap_or(HirType::Void),
                    _ => HirType::Void,
                };
                Ok(HirExpr::new(
                    HirExprKind::Loop {
                        label: label_name,
                        body,
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Match {
                scrutinee,
                arms,
                span,
            } => self.lower_match(scrutinee, arms, expected, *span),
        }
    }

    /// Lower an `if` in value position, from either `Expr::If` or a block's trailing
    /// `Stmt::If`. An `if` is a value only with an `else`; otherwise it yields unit.
    pub(super) fn lower_if_expr(
        &mut self,
        condition: &ast_types::Expr,
        then_block: &[ast_types::Stmt],
        else_if_blocks: &[(ast_types::Expr, Vec<ast_types::Stmt>)],
        else_block: &Option<Vec<ast_types::Stmt>>,
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let condition = self.lower_expr(condition, Some(&HirType::Bool))?;

        // Arm-type hint, mirroring `lower_match`: the expected type if any, else the
        // first arm's type, so a later arm carrying no type of its own resolves against
        // its siblings rather than against nothing.
        let mut hint: Option<HirType> = expected.cloned();
        let (then_stmts, then_ty) = self.lower_block_value(then_block, hint.as_ref())?;
        if hint.is_none() {
            hint = Some(then_ty.clone());
        }
        // The first arm that carries a type decides the `if`'s, mirroring the checker.
        // A divergent arm — a `panic` or an `unreachable` with no context type — lowers
        // to `void` and describes nothing, so taking the `then` arm unconditionally made
        // the whole `if` void purely because of the order the arms were written in.
        let mut ty = then_ty;
        let mut elifs = Vec::with_capacity(else_if_blocks.len());
        for (cond, block) in else_if_blocks {
            let cond = self.lower_expr(cond, Some(&HirType::Bool))?;
            let (block, block_ty) = self.lower_block_value(block, hint.as_ref())?;
            if matches!(ty, HirType::Void) {
                ty = block_ty;
            }
            elifs.push((cond, block));
        }
        let else_block = match else_block {
            Some(block) => {
                let (block, block_ty) = self.lower_block_value(block, hint.as_ref())?;
                if matches!(ty, HirType::Void) {
                    ty = block_ty;
                }
                Some(block)
            }
            None => {
                ty = HirType::Void;
                None
            }
        };
        Ok(HirExpr::new(
            HirExprKind::If {
                condition: Box::new(condition),
                then_block: then_stmts,
                else_if_blocks: elifs,
                else_block,
            },
            ty,
            span,
        ))
    }

    /// Lower a block in value position (a bare/`unsafe` block or an `if` arm),
    /// returning the lowered statements and the block's value type — the type of the
    /// trailing expression, or `void`. The tail is typed under `expected`, matching the
    /// checker's `check_block_expr_type`.
    pub(super) fn lower_block_value(
        &mut self,
        stmts: &[ast_types::Stmt],
        expected: Option<&HirType>,
    ) -> Result<(Vec<HirStmt>, HirType), LoweringError> {
        self.push_scope();
        let result = self.lower_block_value_inner(stmts, expected);
        self.pop_scope();
        result
    }

    pub(super) fn lower_block_value_inner(
        &mut self,
        stmts: &[ast_types::Stmt],
        expected: Option<&HirType>,
    ) -> Result<(Vec<HirStmt>, HirType), LoweringError> {
        let mut out = Vec::with_capacity(stmts.len());
        let mut ty = HirType::Void;
        let last = stmts.len().saturating_sub(1);
        for (i, stmt) in stmts.iter().enumerate() {
            if i == last {
                // An `if` written in statement position parses to `Stmt::If`, never
                // `Stmt::Expr(Expr::If)`, so a trailing `if/else` has to be recognized
                // here to carry the block's value.
                let tail = match stmt {
                    ast_types::Stmt::Expr(expr) => Some(self.lower_expr(expr, expected)?),
                    ast_types::Stmt::If {
                        condition,
                        then_block,
                        else_if_blocks,
                        else_block,
                        span,
                    } if else_block.is_some() => Some(self.lower_if_expr(
                        condition,
                        then_block,
                        else_if_blocks,
                        else_block,
                        expected,
                        *span,
                    )?),
                    _ => None,
                };
                if let Some(tail) = tail {
                    ty = tail.ty.clone();
                    out.push(HirStmt::Expr(tail));
                    return Ok((out, ty));
                }
            }
            out.push(self.lower_stmt(stmt)?);
        }
        Ok((out, ty))
    }

    /// The `(parameter types, return type)` of an associated function `Type::member`.
    pub(super) fn assoc_signature(
        &self,
        type_name: &str,
        member: &str,
    ) -> Result<(Vec<HirType>, HirType), LoweringError> {
        let mangled = format!("{}__{}", type_name, member);
        self.functions
            .get(&mangled)
            .cloned()
            .ok_or(LoweringError::UnresolvedCall { target: mangled })
    }
}
