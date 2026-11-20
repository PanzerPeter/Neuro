//! Named dynamic extents `?B`: where a name is bound, the run-time checks that agree
//! its bindings, and the result a call takes from the extents its arguments carry.
//!
//! A name lives in a hidden `u64` local, `__extent_B`. The checker's binding rules (R6, R7,
//! R10, R11) are re-derived here as ordinary HIR: an assignment where a name is first bound,
//! and a comparison that panics where it is already bound. HIR has no names, so once this
//! module has run the rest of lowering sees only the locals and checks it emitted.
//!
//! Where the local is declared follows the name's scope. A signature's names are declared in
//! the body's prologue. A name first written in a statement (an annotation, a constructor,
//! `.as_shape`) is declared right before that statement, in the block the statement sits in,
//! so it stays in scope to the end of that block as the checker's rule says. A struct literal's
//! names are local to the literal, since they are fresh per literal.

use std::collections::HashMap;

use ast_types::{BinaryOp, Parameter, TensorDim, TensorExtent, Type};
use neuro_hir::{AxisNames, HirExpr, HirExprKind, HirInterpPart, HirPlace, HirStmt, HirType};
use shared_types::{FormatSpec, Literal, Span};

use crate::{Lowerer, LoweringError};

/// The prefix of the hidden local a dynamic extent name `B` lives in. The `__` prefix is
/// reserved for generated names, so the local cannot clash with a user binding.
const LOCAL_PREFIX: &str = "__extent_";

/// What an out-of-range position, slice or `topk` reports: the same words the backends and
/// the tensor outliner use.
pub(crate) const INDEX_OUT_OF_BOUNDS: &str = "tensor index out of bounds";

/// The hidden local a dynamic extent name is held in.
pub(crate) fn local_name(name: &str) -> String {
    format!("{LOCAL_PREFIX}{name}")
}

/// A dynamic extent name in scope, with the depth of the scope its hidden local lives in.
#[derive(Clone)]
pub(crate) struct ExtentName {
    pub(crate) name: String,
    pub(crate) depth: usize,
}

/// The statements a single statement needs around it: declarations of the names it first
/// binds (before it) and the checks on what it bound (after it, once the bound value exists).
#[derive(Clone, Default)]
pub(crate) struct StatementExtents {
    pub(crate) before: Vec<HirStmt>,
    pub(crate) after: Vec<HirStmt>,
}

/// A user function's signature, as far as dynamic extents go. Present only when a parameter
/// names one, since then a call's result may name the same extents.
#[derive(Clone)]
pub(crate) struct ExtentSignature {
    /// For each parameter, the `(axis, name)` pairs its top-level tensor type spells.
    pub(crate) params: Vec<Vec<(usize, String)>>,
    /// The return type as written, resolved again at each call under the extents the
    /// arguments bind. `None` for an `impl Trait` return, which has no written form to re-read.
    pub(crate) ret: Option<Type>,
}

impl ExtentSignature {
    /// The signature of a function or method whose parameters name a dynamic extent.
    pub(crate) fn of(params: &[Parameter], ret: Option<&Type>) -> Option<Self> {
        let params: Vec<Vec<(usize, String)>> =
            params.iter().map(|param| tensor_names(&param.ty)).collect();
        if params.iter().all(Vec::is_empty) {
            return None;
        }
        let ret = match ret {
            None | Some(Type::ImplTrait { .. }) => None,
            Some(ty) => Some(ty.clone()),
        };
        Some(Self { params, ret })
    }
}

/// One named dynamic extent a struct field spells, at `axis` of the field's tensor.
#[derive(Clone)]
pub(crate) struct FieldExtent {
    pub(crate) field: String,
    pub(crate) axis: usize,
    pub(crate) name: String,
}

/// The `(axis, name)` pairs a top-level tensor type spells, through one borrow at most. A
/// name nested deeper is refused by the checker, so it never reaches here.
pub(crate) fn tensor_names(ty: &Type) -> Vec<(usize, String)> {
    let tensor = match ty {
        Type::Reference { inner, .. } => inner.as_ref(),
        other => other,
    };
    let Type::Tensor { shape, .. } = tensor else {
        return Vec::new();
    };
    shape
        .iter()
        .enumerate()
        .filter_map(|(axis, dim)| match &dim.extent {
            TensorExtent::NamedDynamic(id) => Some((axis, id.name.clone())),
            _ => None,
        })
        .collect()
}

/// The name each axis of a written shape spells, when it spells a dynamic one.
pub(crate) fn written_names(shape: &[TensorDim]) -> Vec<Option<String>> {
    shape
        .iter()
        .map(|dim| match &dim.extent {
            TensorExtent::NamedDynamic(id) => Some(id.name.clone()),
            _ => None,
        })
        .collect()
}

pub(crate) fn u64_value(value: u64, span: Span) -> HirExpr {
    HirExpr::new(
        HirExprKind::Literal(Literal::Integer(i128::from(value), None)),
        HirType::U64,
        span,
    )
}

pub(crate) fn bool_value(value: bool, span: Span) -> HirExpr {
    HirExpr::new(
        HirExprKind::Literal(Literal::Boolean(value)),
        HirType::Bool,
        span,
    )
}

pub(crate) fn variable(name: &str, ty: HirType, span: Span) -> HirExpr {
    HirExpr::new(HirExprKind::Variable(name.to_string()), ty, span)
}

/// `expr` as a borrow: a value already borrowed is returned as it is, and any other is lent
/// through `&`, which consumes nothing.
pub(crate) fn borrow(expr: HirExpr) -> HirExpr {
    if matches!(expr.ty, HirType::Reference { .. }) {
        return expr;
    }
    let span = expr.span;
    let ty = HirType::Reference {
        inner: Box::new(expr.ty.clone()),
        mutable: false,
    };
    HirExpr::new(
        HirExprKind::Reference {
            operand: Box::new(expr),
            mutable: false,
        },
        ty,
        span,
    )
}

/// The run-time extent of `axis` of the tensor `tensor` borrows.
pub(crate) fn extent_read(tensor: HirExpr, axis: usize, span: Span) -> HirExpr {
    HirExpr::new(
        HirExprKind::TensorExtent {
            tensor: Box::new(tensor),
            axis,
        },
        HirType::U64,
        span,
    )
}

pub(crate) fn binary(
    op: BinaryOp,
    left: HirExpr,
    right: HirExpr,
    ty: HirType,
    span: Span,
) -> HirExpr {
    HirExpr::new(
        HirExprKind::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        },
        ty,
        span,
    )
}

/// The conjunction of `parts`, `true` for none.
pub(crate) fn all_of(parts: Vec<HirExpr>, span: Span) -> HirExpr {
    parts
        .into_iter()
        .reduce(|left, right| binary(BinaryOp::And, left, right, HirType::Bool, span))
        .unwrap_or_else(|| bool_value(true, span))
}

/// A value in a panic message: the run-time value of `expr`, rendered as the language does.
pub(crate) fn shown(expr: HirExpr) -> HirInterpPart {
    HirInterpPart::Formatted {
        expr,
        spec: FormatSpec::default(),
    }
}

pub(crate) fn text(part: impl Into<String>) -> HirInterpPart {
    HirInterpPart::Text(part.into())
}

/// `panic(message)` over the parts of a message.
pub(crate) fn panic_with(parts: Vec<HirInterpPart>, span: Span) -> HirExpr {
    let message = HirExpr::new(HirExprKind::InterpString { parts }, HirType::String, span);
    panic_call(message, span)
}

/// `panic(message)` over a constant message.
pub(crate) fn panic_text(message: &str, span: Span) -> HirExpr {
    let message = HirExpr::new(
        HirExprKind::Literal(Literal::String(message.to_string())),
        HirType::String,
        span,
    );
    panic_call(message, span)
}

fn panic_call(message: HirExpr, span: Span) -> HirExpr {
    let callee = HirExpr::new(
        HirExprKind::Variable("panic".to_string()),
        HirType::Function {
            params: vec![HirType::String],
            ret: Box::new(HirType::Void),
        },
        span,
    );
    HirExpr::new(
        HirExprKind::Call {
            callee: Box::new(callee),
            args: vec![message],
        },
        HirType::Void,
        span,
    )
}

/// `if !holds { failure }`: a run-time check asserts `holds`, the condition that must be true.
pub(crate) fn guard(condition: HirExpr, failure: HirExpr, span: Span) -> HirStmt {
    let not = HirExpr::new(
        HirExprKind::Unary {
            op: ast_types::UnaryOp::Not,
            operand: Box::new(condition),
        },
        HirType::Bool,
        span,
    );
    HirStmt::If {
        condition: not,
        then_block: vec![HirStmt::Expr(failure)],
        else_if_blocks: Vec::new(),
        else_block: None,
        span,
    }
}

/// The declaration of a hidden extent local, `mut __extent_B = 0`, which a later statement
/// assigns once the name's value is known.
pub(crate) fn local_declaration(name: &str, span: Span) -> HirStmt {
    HirStmt::VarDecl {
        name: local_name(name),
        ty: HirType::U64,
        init: Some(u64_value(0, span)),
        mutable: true,
        span,
    }
}

pub(crate) fn assign_local(name: &str, value: HirExpr, span: Span) -> HirStmt {
    HirStmt::Assign {
        place: HirPlace::Var {
            name: local_name(name),
            ty: HirType::U64,
        },
        value,
        span,
    }
}

/// The value of a written extent that is known at this instance, for a comparison.
fn written_value(extent: &TensorExtent, resolved: Option<usize>) -> Option<u64> {
    match extent {
        TensorExtent::Dynamic(_) | TensorExtent::NamedDynamic(_) => None,
        _ => resolved.map(|value| value as u64),
    }
}

impl Lowerer {
    /// Whether `name` is a dynamic extent name in scope at this point.
    pub(crate) fn extent_in_scope(&self, name: &str) -> bool {
        self.extent_names.iter().any(|entry| entry.name == name)
    }

    /// Bring `name` into scope, its hidden local living in the scope at `depth`.
    pub(crate) fn bind_extent(&mut self, name: &str, depth: usize) {
        self.extent_names.push(ExtentName {
            name: name.to_string(),
            depth,
        });
    }

    /// Lower one statement with the declarations and checks its expressions need placed
    /// around it: the declarations before, the checks after, both in the block it sits in.
    pub(crate) fn in_extent_statement(
        &mut self,
        out: &mut Vec<HirStmt>,
        lower: impl FnOnce(&mut Self, &mut Vec<HirStmt>) -> Result<(), LoweringError>,
    ) -> Result<(), LoweringError> {
        let outer = std::mem::take(&mut self.extent_statement);
        let depth = std::mem::replace(&mut self.extent_depth, self.scopes.len());
        let mark = out.len();
        let lowered = lower(self, out);
        let mine = std::mem::replace(&mut self.extent_statement, outer);
        self.extent_depth = depth;
        lowered?;
        if !mine.before.is_empty() {
            let statement = out.split_off(mark);
            out.extend(mine.before);
            out.extend(statement);
        }
        out.extend(mine.after);
        Ok(())
    }

    /// The prologue of a function or method body: each name its parameters spell is
    /// declared, and bound to the first extent a parameter carries, while every later
    /// occurrence is checked against it at run time. The checks run before the body does.
    pub(crate) fn bind_parameter_extents(
        &mut self,
        params: &[Parameter],
        span: Span,
    ) -> Result<Vec<HirStmt>, LoweringError> {
        let mut stmts = Vec::new();
        let mut first_seen: Vec<(String, String)> = Vec::new();
        let depth = self.scopes.len();
        for param in params {
            let names = tensor_names(&param.ty);
            if names.is_empty() {
                continue;
            }
            let ty = self.lookup_local(&param.name.name).ok_or_else(|| {
                LoweringError::UnresolvedBinding {
                    name: param.name.name.clone(),
                }
            })?;
            for (axis, name) in names {
                let found = extent_read(
                    borrow(variable(&param.name.name, ty.clone(), span)),
                    axis,
                    span,
                );
                if let Some((_, first)) = first_seen.iter().find(|(seen, _)| *seen == name) {
                    let bound = variable(&local_name(&name), HirType::U64, span);
                    let message = vec![
                        text(format!("dynamic extent `{name}` is ")),
                        shown(bound.clone()),
                        text(format!(" from `{first}` but ")),
                        shown(found.clone()),
                        text(format!(" from `{}`", param.name.name)),
                    ];
                    let agreed = binary(BinaryOp::Equal, found, bound, HirType::Bool, span);
                    stmts.push(guard(agreed, panic_with(message, span), span));
                } else {
                    stmts.push(local_declaration(&name, span));
                    stmts.push(assign_local(&name, found, span));
                    first_seen.push((name.clone(), param.name.name.clone()));
                    self.bind_extent(&name, depth);
                }
            }
        }
        Ok(stmts)
    }

    /// The names a `val` or `mut` annotation binds to the value it initializes: a fresh name
    /// takes the value's extent, a bound one is checked against it. Queued on the statement
    /// being lowered.
    pub(crate) fn bind_annotation_extents(
        &mut self,
        binding: &str,
        annotation: &Type,
        ty: &HirType,
        span: Span,
    ) {
        let names = tensor_names(annotation);
        if names.is_empty() {
            return;
        }
        let place = variable(binding, ty.clone(), span);
        let tensor = borrow(place);
        for (axis, name) in names {
            let found = extent_read(tensor.clone(), axis, span);
            if self.extent_in_scope(&name) {
                let bound = variable(&local_name(&name), HirType::U64, span);
                let message = vec![
                    text(format!("dynamic extent `{name}` is ")),
                    shown(bound.clone()),
                    text(format!(" but `{binding}` has ")),
                    shown(found.clone()),
                ];
                let agreed = binary(BinaryOp::Equal, found, bound, HirType::Bool, span);
                let failure = guard(agreed, panic_with(message, span), span);
                self.extent_statement.after.push(failure);
            } else {
                self.extent_statement
                    .before
                    .push(local_declaration(&name, span));
                self.extent_statement
                    .after
                    .push(assign_local(&name, found, span));
                let depth = self.extent_depth;
                self.bind_extent(&name, depth);
            }
        }
    }

    /// The `dims:` a constructor takes for the axes its type leaves open, one `u64` per open
    /// axis in axis order, and the statements that bind the names it introduces. A bound name
    /// reads its local; a fresh one takes its entry; an anonymous axis takes its entry.
    pub(crate) fn constructor_dims(
        &mut self,
        annotation: Option<&Type>,
        shape: &[Option<usize>],
        dims: Option<&[ast_types::Expr]>,
        span: Span,
    ) -> Result<(Vec<HirStmt>, Vec<HirExpr>), LoweringError> {
        let open: Vec<usize> = shape
            .iter()
            .enumerate()
            .filter(|(_, extent)| extent.is_none())
            .map(|(axis, _)| axis)
            .collect();
        if open.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let written = match annotation {
            Some(Type::Tensor { shape: written, .. }) if written.len() == shape.len() => {
                written_names(written)
            }
            _ => vec![None; shape.len()],
        };
        let entries = dims.unwrap_or_default();
        let mut next = 0;
        let mut stmts = Vec::new();
        let mut out = Vec::with_capacity(open.len());
        for axis in open {
            let name = written[axis].clone();
            if let Some(name) = name.as_deref().filter(|name| self.extent_in_scope(name)) {
                out.push(variable(&local_name(name), HirType::U64, span));
                continue;
            }
            let entry = entries.get(next).ok_or_else(|| LoweringError::Malformed {
                detail: "a tensor constructor has fewer `dims:` entries than open axes".to_string(),
            })?;
            next += 1;
            let value = self.lower_expr(entry, Some(&HirType::U64))?;
            let temp = self.fresh_extent_temp();
            stmts.push(HirStmt::VarDecl {
                name: temp.clone(),
                ty: HirType::U64,
                init: Some(value),
                mutable: false,
                span,
            });
            match name {
                Some(name) => {
                    self.extent_statement
                        .before
                        .push(local_declaration(&name, span));
                    stmts.push(assign_local(
                        &name,
                        variable(&temp, HirType::U64, span),
                        span,
                    ));
                    let depth = self.extent_depth;
                    self.bind_extent(&name, depth);
                    out.push(variable(&local_name(&name), HirType::U64, span));
                }
                None => out.push(variable(&temp, HirType::U64, span)),
            }
        }
        if next != entries.len() {
            return Err(LoweringError::Malformed {
                detail: "a tensor constructor has more `dims:` entries than open axes".to_string(),
            });
        }
        Ok((stmts, out))
    }

    pub(crate) fn fresh_extent_temp(&mut self) -> String {
        self.extent_counter += 1;
        format!("__extent_tmp_{}", self.extent_counter)
    }

    /// `.as_shape::<[...]>()` on an owned tensor: the receiver is bound to a temporary, each
    /// fresh name takes its axis's extent, and the result is `Some(tensor)` when every
    /// literal, parameter and bound name agrees with the tensor, else `None`.
    pub(crate) fn lower_as_shape(
        &mut self,
        receiver: HirExpr,
        dims: &[TensorDim],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let HirType::Tensor {
            element,
            shape: received,
            ..
        } = receiver.ty.clone()
        else {
            return Err(LoweringError::Malformed {
                detail: "`.as_shape` reached lowering on a non-tensor receiver".to_string(),
            });
        };
        if dims.len() != received.len() {
            return Err(LoweringError::Malformed {
                detail: format!(
                    "`.as_shape` names {} axes for a rank-{} tensor",
                    dims.len(),
                    received.len()
                ),
            });
        }
        let written = self.resolve_written_shape(dims)?;
        if written.len() != received.len() {
            return Err(LoweringError::Malformed {
                detail: "`.as_shape` reached lowering with a shape splice".to_string(),
            });
        }
        self.extent_counter += 1;
        let source = format!("__as_shape_{}", self.extent_counter);
        let receiver_ty = receiver.ty.clone();
        let mut stmts = vec![HirStmt::VarDecl {
            name: source.clone(),
            ty: receiver_ty.clone(),
            init: Some(receiver),
            mutable: false,
            span,
        }];
        let mut checks = Vec::new();
        for (axis, dim) in dims.iter().enumerate() {
            let extent_of = || {
                extent_read(
                    borrow(variable(&source, receiver_ty.clone(), span)),
                    axis,
                    span,
                )
            };
            // A static axis was agreed by the checker, so only a dynamic one is compared here.
            let received_dynamic = received[axis].is_none();
            match &dim.extent {
                TensorExtent::Dynamic(_) => {}
                TensorExtent::NamedDynamic(id) => {
                    if self.extent_in_scope(&id.name) {
                        let bound = variable(&local_name(&id.name), HirType::U64, span);
                        checks.push(binary(
                            BinaryOp::Equal,
                            extent_of(),
                            bound,
                            HirType::Bool,
                            span,
                        ));
                    } else {
                        self.extent_statement
                            .before
                            .push(local_declaration(&id.name, span));
                        stmts.push(assign_local(&id.name, extent_of(), span));
                        let depth = self.extent_depth;
                        self.bind_extent(&id.name, depth);
                    }
                }
                extent => {
                    if let Some(value) = written_value(extent, written[axis])
                        && received_dynamic
                    {
                        let agrees = binary(
                            BinaryOp::Equal,
                            extent_of(),
                            u64_value(value, span),
                            HirType::Bool,
                            span,
                        );
                        checks.push(agrees);
                    }
                }
            }
        }
        let tensor_ty = HirType::Tensor {
            element,
            shape: written,
            names: AxisNames(written_names(dims)),
        };
        let option = self.option_of(tensor_ty.clone())?;
        let (some_tag, _) = self.enum_variant(&option_name(&option), "Some")?;
        let (none_tag, _) = self.enum_variant(&option_name(&option), "None")?;
        let payload = variable(&source, tensor_ty, span);
        let some =
            self.build_enum_construct(&option_name(&option), "Some", some_tag, vec![payload], span);
        let none =
            self.build_enum_construct(&option_name(&option), "None", none_tag, Vec::new(), span);
        let condition = all_of(checks, span);
        stmts.push(HirStmt::Expr(HirExpr::new(
            HirExprKind::If {
                condition: Box::new(condition),
                then_block: vec![HirStmt::Expr(some)],
                else_if_blocks: Vec::new(),
                else_block: Some(vec![HirStmt::Expr(none)]),
            },
            option.clone(),
            span,
        )));
        Ok(HirExpr::new(HirExprKind::Block { stmts }, option, span))
    }

    /// The extents a written shape resolves to at this instance, with a dynamic axis `None`.
    fn resolve_written_shape(
        &mut self,
        dims: &[TensorDim],
    ) -> Result<Vec<Option<usize>>, LoweringError> {
        let (extents, _) = self.resolve_shape(dims)?;
        Ok(extents)
    }
}

/// The name of an enum instance, as the lowered type spells it.
fn option_name(ty: &HirType) -> String {
    match ty {
        HirType::Enum(name) => name.clone(),
        _ => String::new(),
    }
}

/// A map from dynamic extent name to the value a call bound it to, `None` where no argument
/// carries a value for it.
type Bindings = HashMap<String, Option<usize>>;

/// The type a call yields: the callee's declared result, with each dynamic extent its written
/// return type names replaced by the first extent an argument carries for it (R6). An axis no
/// argument fixes stays `?`, since every bound extent is equal at run time once the checks pass.
pub(crate) fn instantiate_result(
    signature: Option<&ExtentSignature>,
    args: &[HirExpr],
    declared: HirType,
) -> HirType {
    let Some(ret) = signature.and_then(|sig| sig.ret.as_ref().map(|ret| (sig, ret))) else {
        return declared;
    };
    let (signature, written) = ret;
    let mut bindings: Bindings = HashMap::new();
    for (arg, names) in args.iter().zip(&signature.params) {
        let HirType::Tensor { shape, .. } = arg.ty.referent() else {
            continue;
        };
        for (axis, name) in names {
            let found = shape.get(*axis).copied().flatten();
            let slot = bindings.entry(name.clone()).or_insert(None);
            if slot.is_none() {
                *slot = found;
            }
        }
    }
    bind_result(&declared, written, &bindings)
}

/// `declared` with the axes `written` names bound: the tensors, tuples and borrows it is made
/// of. A result inside any other type (an `Option`, say) keeps its declared form.
fn bind_result(declared: &HirType, written: &Type, bindings: &Bindings) -> HirType {
    match (declared, written) {
        (
            HirType::Tensor {
                element,
                shape,
                names,
            },
            Type::Tensor { shape: dims, .. },
        ) if shape.len() == dims.len() => {
            let shape = shape
                .iter()
                .zip(dims)
                .map(|(extent, dim)| match (extent, &dim.extent) {
                    (None, TensorExtent::NamedDynamic(id)) => {
                        bindings.get(&id.name).copied().flatten()
                    }
                    _ => *extent,
                })
                .collect();
            HirType::Tensor {
                element: element.clone(),
                shape,
                names: names.clone(),
            }
        }
        (HirType::Tuple(parts), Type::Tuple { elements, .. }) if parts.len() == elements.len() => {
            HirType::Tuple(
                parts
                    .iter()
                    .zip(elements)
                    .map(|(part, written)| bind_result(part, written, bindings))
                    .collect(),
            )
        }
        (HirType::Reference { inner, mutable }, Type::Reference { inner: written, .. }) => {
            HirType::Reference {
                inner: Box::new(bind_result(inner, written, bindings)),
                mutable: *mutable,
            }
        }
        _ => declared.clone(),
    }
}
