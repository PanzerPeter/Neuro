//! Expression lowering with resolved-type re-derivation.

use ast_types::{BinaryOp, Expr, FieldInit, UnaryOp};
use neuro_hir::{HirExpr, HirExprKind, HirFieldInit, HirStmt, HirType};
use shared_types::Literal;

use crate::types::{float_suffix_type, int_suffix_type};
use crate::{is_full_float, is_integer, peels_to_string, LoopCtx, Lowerer, LoweringError};

/// The divergent panic-family builtins (§1.2). Each aborts and never returns, so a
/// call takes on whatever type its context demands.
const PANIC_BUILTINS: &[&str] = &["panic", "assert", "unreachable"];

/// The deep-copy method shared by `string` and `Clone`-deriving structs (§2.3, §2.7).
const CLONE_METHOD: &str = "clone";

/// An enum variant's ordered payload fields: each `(optional field name, type)`.
/// `Some` name marks a struct-variant field; `None` a tuple-variant element (§3.5).
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
        match expr {
            // Grouping is encoded by tree structure in the HIR; drop the node.
            Expr::Paren(inner, _) => self.lower_expr(inner, expected),

            Expr::Literal(lit, span) => {
                let ty = literal_type(lit, expected);
                Ok(HirExpr::new(HirExprKind::Literal(lit.clone()), ty, *span))
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
                let left = self.lower_expr(left, None)?;
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

            Expr::Call { func, args, span } => self.lower_call(func, args, expected, *span),

            // A bare path is a unit-variant enum construction `E::V` (§3.5) when the
            // type names an enum, else an associated-function reference.
            Expr::Path {
                type_name,
                member,
                span,
            } if self.enums.contains_key(&type_name.name) => {
                self.lower_enum_construct(&type_name.name, &member.name, Vec::new(), *span)
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
            } => self.lower_enum_struct_literal(&enum_name.name, &variant.name, fields, *span),

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
                let HirType::Tuple(element_tys) = object.ty.referent().clone() else {
                    return Err(LoweringError::Malformed {
                        detail: format!("tuple index into non-tuple type '{}'", object.ty),
                    });
                };
                let element_ty =
                    element_tys
                        .get(*index)
                        .cloned()
                        .ok_or_else(|| LoweringError::Malformed {
                            detail: format!(
                                "tuple index {} out of range for arity {}",
                                index,
                                element_tys.len()
                            ),
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

            Expr::Index {
                object,
                index,
                span,
            } => {
                let object = self.lower_expr(object, None)?;
                let index = self.lower_expr(index, None)?;
                let HirType::Array { element, .. } = object.ty.referent().clone() else {
                    return Err(LoweringError::Malformed {
                        detail: format!("index into non-array type '{}'", object.ty),
                    });
                };
                Ok(HirExpr::new(
                    HirExprKind::Index {
                        object: Box::new(object),
                        index: Box::new(index),
                    },
                    *element,
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
            } => {
                let condition = self.lower_expr(condition, Some(&HirType::Bool))?;
                let (then_stmts, then_ty) = self.lower_block_value(then_block)?;
                let mut elifs = Vec::with_capacity(else_if_blocks.len());
                for (cond, block) in else_if_blocks {
                    let cond = self.lower_expr(cond, Some(&HirType::Bool))?;
                    let (block, _) = self.lower_block_value(block)?;
                    elifs.push((cond, block));
                }
                // An `if` is a value only with an `else`; otherwise it yields unit.
                let (else_block, ty) = match else_block {
                    Some(block) => {
                        let (block, _) = self.lower_block_value(block)?;
                        (Some(block), then_ty)
                    }
                    None => (None, HirType::Void),
                };
                Ok(HirExpr::new(
                    HirExprKind::If {
                        condition: Box::new(condition),
                        then_block: then_stmts,
                        else_if_blocks: elifs,
                        else_block,
                    },
                    ty,
                    *span,
                ))
            }

            Expr::Block { stmts, span } => {
                let (stmts, ty) = self.lower_block_value(stmts)?;
                Ok(HirExpr::new(HirExprKind::Block { stmts }, ty, *span))
            }

            Expr::Unsafe { stmts, span } => {
                let (stmts, ty) = self.lower_block_value(stmts)?;
                Ok(HirExpr::new(HirExprKind::Unsafe { stmts }, ty, *span))
            }

            Expr::Loop { label, body, span } => {
                let label_name = label.as_ref().map(|l| l.name.clone());
                self.loop_stack.push(LoopCtx {
                    label: label_name.clone(),
                    is_value: true,
                    value_ty: None,
                });
                self.push_scope();
                let body = self.lower_stmt_list(body);
                self.pop_scope();
                let ctx = self.loop_stack.pop();
                let body = body?;
                let ty = ctx.and_then(|c| c.value_ty).unwrap_or(HirType::Void);
                Ok(HirExpr::new(
                    HirExprKind::Loop {
                        label: label_name,
                        body,
                    },
                    ty,
                    *span,
                ))
            }
        }
    }

    /// Lower a call, dispatching on the callee shape: free/builtin function,
    /// instance method, or associated function.
    fn lower_call(
        &mut self,
        func: &Expr,
        args: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        match func {
            Expr::Identifier(ident) => self.lower_ident_call(&ident.name, args, expected, span),
            Expr::FieldAccess { object, field, .. } => {
                self.lower_method_call(object, &field.name, args, span)
            }
            // `Enum::Variant(args)` is a tuple-variant construction (§3.5) when the
            // type names an enum; otherwise an associated-function call.
            Expr::Path {
                type_name, member, ..
            } if self.enums.contains_key(&type_name.name) => {
                self.lower_enum_tuple_call(&type_name.name, &member.name, args, span)
            }
            Expr::Path {
                type_name, member, ..
            } => self.lower_assoc_call(&type_name.name, &member.name, args, span),
            other => Err(LoweringError::Malformed {
                detail: format!(
                    "call of non-callable expression {:?}",
                    std::mem::discriminant(other)
                ),
            }),
        }
    }

    /// Lower a plain identifier call: a registered free function, or one of the
    /// divergent panic-family builtins (which take their context's type).
    fn lower_ident_call(
        &mut self,
        name: &str,
        args: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        if let Some((params, ret)) = self.functions.get(name).cloned() {
            let args = self.lower_args(args, &params)?;
            let callee = HirExpr::new(
                HirExprKind::Variable(name.to_string()),
                HirType::Function {
                    params,
                    ret: Box::new(ret.clone()),
                },
                span,
            );
            return Ok(HirExpr::new(
                HirExprKind::Call {
                    callee: Box::new(callee),
                    args,
                },
                ret,
                span,
            ));
        }

        if PANIC_BUILTINS.contains(&name) {
            let param = match name {
                "panic" => vec![HirType::String],
                "assert" => vec![HirType::Bool],
                _ => vec![],
            };
            let args = self.lower_args(args, &param)?;
            // Divergent: adopt the expected context type, or unit in statement position.
            let ret = expected.cloned().unwrap_or(HirType::Void);
            let callee = HirExpr::new(
                HirExprKind::Variable(name.to_string()),
                HirType::Function {
                    params: param,
                    ret: Box::new(HirType::Void),
                },
                span,
            );
            return Ok(HirExpr::new(
                HirExprKind::Call {
                    callee: Box::new(callee),
                    args,
                },
                ret,
                span,
            ));
        }

        Err(LoweringError::UnresolvedCall {
            target: name.to_string(),
        })
    }

    /// Lower `instance.method(args)`: a struct method (or the `.clone()` builtin on a
    /// `Clone` struct), or a compiler-known intrinsic on a builtin receiver.
    fn lower_method_call(
        &mut self,
        object: &Expr,
        method: &str,
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let object = self.lower_expr(object, None)?;
        let recv = object.ty.clone();

        let (lowered_args, result_ty) = if let HirType::Struct(struct_name) = recv.referent() {
            let struct_name = struct_name.clone();
            if let Some(mangled) = self
                .impl_methods
                .get(&struct_name)
                .and_then(|m| m.get(method))
                .cloned()
            {
                let (params, ret) = self.functions.get(&mangled).cloned().ok_or_else(|| {
                    LoweringError::UnresolvedCall {
                        target: mangled.clone(),
                    }
                })?;
                // params[0] is the implicit `self`; callers pass only the rest.
                let visible = if params.is_empty() {
                    &params[..]
                } else {
                    &params[1..]
                };
                (self.lower_args(args, visible)?, ret)
            } else if method == CLONE_METHOD && self.clone_structs.contains(&struct_name) {
                (self.lower_args(args, &[])?, HirType::Struct(struct_name))
            } else {
                return Err(LoweringError::UnresolvedCall {
                    target: format!("{}.{}", struct_name, method),
                });
            }
        } else {
            self.lower_builtin_method(&recv, method, args)?
        };

        // The method-name callee is a synthetic node (the language has no first-class
        // method value); it carries the call's result type as a convenience for
        // backends that key dispatch off the field name and receiver type.
        let callee = HirExpr::new(
            HirExprKind::FieldAccess {
                object: Box::new(object),
                field: method.to_string(),
            },
            result_ty.clone(),
            span,
        );
        Ok(HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args: lowered_args,
            },
            result_ty,
            span,
        ))
    }

    /// Resolve a compiler-known intrinsic on a builtin (non-struct) receiver,
    /// returning the lowered arguments and the result type (§1.2, §2.7, §3.1).
    fn lower_builtin_method(
        &mut self,
        recv: &HirType,
        method: &str,
        args: &[Expr],
    ) -> Result<(Vec<HirExpr>, HirType), LoweringError> {
        // String intrinsics auto-deref through `&string`, so match on the referent.
        match (recv.referent(), method) {
            (HirType::String, "len") => Ok((self.lower_args(args, &[])?, HirType::U64)),
            (HirType::String, "clone") => Ok((self.lower_args(args, &[])?, HirType::String)),
            (HirType::String, "slice") => {
                let arg = args.first().ok_or_else(|| LoweringError::Malformed {
                    detail: "string.slice expects a range argument".to_string(),
                })?;
                let range = self.lower_expr(arg, None)?;
                let slice_ty = HirType::Reference {
                    inner: Box::new(HirType::String),
                    mutable: false,
                };
                Ok((vec![range], slice_ty))
            }
            (HirType::Array { .. }, "len") => Ok((self.lower_args(args, &[])?, HirType::U64)),
            (
                _,
                "wrapping_add" | "wrapping_sub" | "wrapping_mul" | "saturating_add"
                | "saturating_sub" | "saturating_mul" | "shr",
            ) if is_integer(recv) => {
                let args = self.lower_args(args, std::slice::from_ref(recv))?;
                Ok((args, recv.clone()))
            }
            _ => Err(LoweringError::UnresolvedCall {
                target: format!("{}.{}", recv, method),
            }),
        }
    }

    /// Look up an enum variant by name, returning its discriminant tag (declaration
    /// index) and a clone of its ordered payload fields. The clone frees the enum
    /// table's immutable borrow before the mutable argument lowering that follows.
    fn enum_variant(
        &self,
        enum_name: &str,
        variant: &str,
    ) -> Result<(u32, PayloadFields), LoweringError> {
        let variants = self
            .enums
            .get(enum_name)
            .ok_or_else(|| LoweringError::UnresolvedType {
                name: enum_name.to_string(),
            })?;
        variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.name == variant)
            .map(|(i, v)| (i as u32, v.fields.clone()))
            .ok_or_else(|| LoweringError::UnresolvedCall {
                target: format!("{}::{}", enum_name, variant),
            })
    }

    /// Assemble the single [`HirExprKind::EnumConstruct`] node every surface form
    /// lowers to. `payload` is already in the variant's declared field order.
    fn build_enum_construct(
        &self,
        enum_name: &str,
        variant: &str,
        tag: u32,
        payload: Vec<HirExpr>,
        span: shared_types::Span,
    ) -> HirExpr {
        HirExpr::new(
            HirExprKind::EnumConstruct {
                enum_name: enum_name.to_string(),
                variant: variant.to_string(),
                tag,
                payload,
            },
            HirType::Enum(enum_name.to_string()),
            span,
        )
    }

    /// Lower a unit-variant construction `E::V` — an empty payload (§3.5).
    fn lower_enum_construct(
        &mut self,
        enum_name: &str,
        variant: &str,
        payload: Vec<HirExpr>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let (tag, _) = self.enum_variant(enum_name, variant)?;
        Ok(self.build_enum_construct(enum_name, variant, tag, payload, span))
    }

    /// Lower a tuple-variant construction `E::V(args)`: arguments are positional, so
    /// they are the payload as-is, lowered against the declared field types (§3.5).
    fn lower_enum_tuple_call(
        &mut self,
        enum_name: &str,
        variant: &str,
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let (tag, fields) = self.enum_variant(enum_name, variant)?;
        let field_tys: Vec<HirType> = fields.into_iter().map(|(_, t)| t).collect();
        let payload = self.lower_args(args, &field_tys)?;
        Ok(self.build_enum_construct(enum_name, variant, tag, payload, span))
    }

    /// Lower a struct-variant construction `E::V { field: expr, ... }`: the provided
    /// fields are reordered into the variant's declared field order before becoming
    /// the payload, so codegen sees a single positional layout (§3.5).
    fn lower_enum_struct_literal(
        &mut self,
        enum_name: &str,
        variant: &str,
        fields: &[FieldInit],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let (tag, declared) = self.enum_variant(enum_name, variant)?;
        let mut payload = Vec::with_capacity(declared.len());
        for (field_name, field_ty) in &declared {
            let Some(field_name) = field_name else {
                return Err(LoweringError::Malformed {
                    detail: format!(
                        "tuple variant '{}::{}' constructed with field names",
                        enum_name, variant
                    ),
                });
            };
            let provided = fields
                .iter()
                .find(|f| &f.name.name == field_name)
                .ok_or_else(|| LoweringError::Malformed {
                    detail: format!(
                        "missing field '{}' for enum variant '{}::{}'",
                        field_name, enum_name, variant
                    ),
                })?;
            payload.push(self.lower_expr(&provided.value, Some(field_ty))?);
        }
        Ok(self.build_enum_construct(enum_name, variant, tag, payload, span))
    }

    /// Lower an associated-function call `TypeName::func(args)`.
    fn lower_assoc_call(
        &mut self,
        type_name: &str,
        member: &str,
        args: &[Expr],
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let (params, ret) = self.assoc_signature(type_name, member)?;
        let args = self.lower_args(args, &params)?;
        let callee = HirExpr::new(
            HirExprKind::Path {
                type_name: type_name.to_string(),
                member: member.to_string(),
            },
            HirType::Function {
                params,
                ret: Box::new(ret.clone()),
            },
            span,
        );
        Ok(HirExpr::new(
            HirExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            ret,
            span,
        ))
    }

    /// Lower each argument against its corresponding parameter type (the contextual
    /// hint a callee imposes on its arguments). Extra arguments lower with no hint.
    fn lower_args(
        &mut self,
        args: &[Expr],
        params: &[HirType],
    ) -> Result<Vec<HirExpr>, LoweringError> {
        let mut out = Vec::with_capacity(args.len());
        for (i, arg) in args.iter().enumerate() {
            out.push(self.lower_expr(arg, params.get(i))?);
        }
        Ok(out)
    }

    fn lower_struct_literal(
        &mut self,
        name: &shared_types::Identifier,
        fields: &[FieldInit],
        base: &Option<Box<Expr>>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let def =
            self.structs
                .get(&name.name)
                .cloned()
                .ok_or_else(|| LoweringError::UnresolvedType {
                    name: name.name.clone(),
                })?;

        let mut lowered_fields = Vec::with_capacity(fields.len());
        for FieldInit {
            name: fname,
            value,
            span: fspan,
        } in fields
        {
            let expected = def
                .iter()
                .find(|(n, _)| n == &fname.name)
                .map(|(_, t)| t.clone());
            let value = self.lower_expr(value, expected.as_ref())?;
            lowered_fields.push(HirFieldInit {
                name: fname.name.clone(),
                value: Box::new(value),
                span: *fspan,
            });
        }

        let struct_ty = HirType::Struct(name.name.clone());
        let base = match base {
            Some(b) => Some(Box::new(self.lower_expr(b, Some(&struct_ty))?)),
            None => None,
        };

        Ok(HirExpr::new(
            HirExprKind::StructLiteral {
                name: name.name.clone(),
                fields: lowered_fields,
                base,
            },
            struct_ty,
            span,
        ))
    }

    /// Lower a `start..end` / `start..=end` range. Ranges are not first-class values
    /// (§2.7) — only valid as a `string.slice` argument — so the node carries
    /// `void`; the slice lowering reads the bounds directly. Bounds are `u64`-typed.
    fn lower_range(
        &mut self,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let start = self.lower_expr(start, Some(&HirType::U64))?;
        let end = self.lower_expr(end, Some(&HirType::U64))?;
        Ok(HirExpr::new(
            HirExprKind::Range {
                start: Box::new(start),
                end: Box::new(end),
                inclusive,
            },
            HirType::Void,
            span,
        ))
    }

    fn lower_array_literal(
        &mut self,
        elements: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let expected_element = match expected {
            Some(HirType::Array { element, .. }) => Some((**element).clone()),
            _ => None,
        };

        if elements.is_empty() {
            let ty = match expected {
                Some(HirType::Array { element, size }) => HirType::Array {
                    element: element.clone(),
                    size: *size,
                },
                _ => {
                    return Err(LoweringError::Malformed {
                        detail: "cannot infer element type of empty array literal".to_string(),
                    })
                }
            };
            return Ok(HirExpr::new(
                HirExprKind::ArrayLiteral { elements: vec![] },
                ty,
                span,
            ));
        }

        let first = self.lower_expr(&elements[0], expected_element.as_ref())?;
        let element_ty = first.ty.clone();
        let mut lowered = Vec::with_capacity(elements.len());
        lowered.push(first);
        for el in &elements[1..] {
            lowered.push(self.lower_expr(el, Some(&element_ty))?);
        }

        Ok(HirExpr::new(
            HirExprKind::ArrayLiteral { elements: lowered },
            HirType::Array {
                element: Box::new(element_ty),
                size: elements.len(),
            },
            span,
        ))
    }

    fn lower_tuple_literal(
        &mut self,
        elements: &[Expr],
        expected: Option<&HirType>,
        span: shared_types::Span,
    ) -> Result<HirExpr, LoweringError> {
        let expected_elems = match expected {
            Some(HirType::Tuple(es)) if es.len() == elements.len() => Some(es.clone()),
            _ => None,
        };
        let mut lowered = Vec::with_capacity(elements.len());
        let mut tys = Vec::with_capacity(elements.len());
        for (i, el) in elements.iter().enumerate() {
            let hint = expected_elems.as_ref().map(|es| &es[i]);
            let el = self.lower_expr(el, hint)?;
            tys.push(el.ty.clone());
            lowered.push(el);
        }
        Ok(HirExpr::new(
            HirExprKind::TupleLiteral { elements: lowered },
            HirType::Tuple(tys),
            span,
        ))
    }

    /// Lower a block in value position (a bare/`unsafe` block or an `if` arm),
    /// returning the lowered statements and the block's value type — the type of the
    /// trailing expression, or `void`. The tail is typed with no contextual hint,
    /// matching the checker's `check_block_expr_type`.
    fn lower_block_value(
        &mut self,
        stmts: &[ast_types::Stmt],
    ) -> Result<(Vec<HirStmt>, HirType), LoweringError> {
        self.push_scope();
        let result = self.lower_block_value_inner(stmts);
        self.pop_scope();
        result
    }

    fn lower_block_value_inner(
        &mut self,
        stmts: &[ast_types::Stmt],
    ) -> Result<(Vec<HirStmt>, HirType), LoweringError> {
        let mut out = Vec::with_capacity(stmts.len());
        let mut ty = HirType::Void;
        let last = stmts.len().saturating_sub(1);
        for (i, stmt) in stmts.iter().enumerate() {
            if i == last {
                if let ast_types::Stmt::Expr(expr) = stmt {
                    let tail = self.lower_expr(expr, None)?;
                    ty = tail.ty.clone();
                    out.push(HirStmt::Expr(tail));
                    return Ok((out, ty));
                }
            }
            out.push(self.lower_stmt(stmt)?);
        }
        Ok((out, ty))
    }

    /// The declared type of `field` on `struct_name`.
    fn struct_field_type(&self, struct_name: &str, field: &str) -> Result<HirType, LoweringError> {
        self.structs
            .get(struct_name)
            .and_then(|fields| fields.iter().find(|(n, _)| n == field))
            .map(|(_, t)| t.clone())
            .ok_or_else(|| LoweringError::Malformed {
                detail: format!("unknown field '{}' on struct '{}'", field, struct_name),
            })
    }

    /// The `(parameter types, return type)` of an associated function `Type::member`.
    fn assoc_signature(
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

/// The resolved type of a literal under an optional contextual `expected` type,
/// mirroring the checker's literal inference (suffix wins; else the expected type
/// when it fits the literal's family; else the default `i32` / `f64`).
fn literal_type(lit: &Literal, expected: Option<&HirType>) -> HirType {
    match lit {
        Literal::Integer(_, Some(suffix)) => int_suffix_type(suffix),
        Literal::Integer(_, None) => match expected {
            Some(t) if is_integer(t) => t.clone(),
            _ => HirType::I32,
        },
        Literal::Float(_, Some(suffix)) => float_suffix_type(suffix),
        Literal::Float(_, None) => match expected {
            Some(t) if is_full_float(t) => t.clone(),
            _ => HirType::F64,
        },
        Literal::Boolean(_) => HirType::Bool,
        Literal::Char(_) => HirType::Char,
        Literal::String(_) => HirType::String,
    }
}

/// Whether `t` is a numeric type usable with `-` / arithmetic (integer or
/// full-precision float). Half-precision is excluded (§1.2).
fn is_numeric(t: &HirType) -> bool {
    is_integer(t) || is_full_float(t)
}

/// The result type of a binary operator given its operand types. Comparisons and
/// logical operators yield `bool`; `+` on two strings yields a new owned `string`
/// (§2.7); other arithmetic and bitwise operators yield the left operand's type.
fn binary_result_type(
    op: BinaryOp,
    left: &HirType,
    right: &HirType,
) -> Result<HirType, LoweringError> {
    Ok(match op {
        BinaryOp::Equal
        | BinaryOp::NotEqual
        | BinaryOp::Less
        | BinaryOp::Greater
        | BinaryOp::LessEqual
        | BinaryOp::GreaterEqual
        | BinaryOp::And
        | BinaryOp::Or => HirType::Bool,
        BinaryOp::Add if peels_to_string(left) && peels_to_string(right) => HirType::String,
        BinaryOp::Add
        | BinaryOp::Subtract
        | BinaryOp::Multiply
        | BinaryOp::Divide
        | BinaryOp::Modulo
        | BinaryOp::BitAnd
        | BinaryOp::BitOr
        | BinaryOp::BitXor
        | BinaryOp::Shl => left.clone(),
        // `??` is rejected by the checker (Option/Result arrive in Phase 2), so it
        // never reaches a well-typed program's HIR.
        BinaryOp::NullCoalesce => {
            return Err(LoweringError::Malformed {
                detail: "`??` operator is not supported until Phase 2".to_string(),
            })
        }
    })
}
