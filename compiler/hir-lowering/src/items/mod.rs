//! Top-level item lowering: each item's HIR, with registration in [`register`] and
//! generic instances in [`mono`].

use std::borrow::Cow;
use std::collections::HashSet;

use ast_types::{
    Attribute, ConstDef, EnumDef, Expr, FunctionDef, GenericArg, ImplDef, Item, MethodDef,
    SelfParam, StructDef, VariantPayload,
};
use neuro_hir::{
    HirConst, HirEnum, HirEnumField, HirEnumVariant, HirField, HirFunction, HirImpl, HirItem,
    HirMethod, HirParam, HirProgram, HirSelfParam, HirStmt, HirStruct, HirTarget, HirType,
};
use shared_types::Literal;

use crate::{Lowerer, LoweringError};

mod mono;
mod register;

/// The attribute pinning a function's body to a GPU. Its form (on a free function, at
/// most a literal `fallback:`) is the checker's rule; here it only has to be recognised.
const GPU_ATTRIBUTE: &str = "gpu";

const FALLBACK_LABEL: &str = "fallback";

/// The attribute making a function a hand-written kernel. Its form (a `threads:` array of
/// one to three positive integer literals) is the checker's rule.
const KERNEL_ATTRIBUTE: &str = "kernel";

const THREADS_LABEL: &str = "threads";

/// The handle a kernel writes a tensor through: a `&mut` tensor the call lends it.
const KERNEL_OUT_TYPE: &str = "KernelOut";

fn is_kernel(func: &FunctionDef) -> bool {
    func.attributes
        .iter()
        .any(|attr| attr.name.name == KERNEL_ATTRIBUTE)
}

/// A kernel parameter as the reference a launch passes: a bare `Tensor` input is `&Tensor`
/// and `KernelOut<Tensor>` is `&mut Tensor`. `None` for a scalar, passed as written.
fn kernel_param_reference(ty: &ast_types::Type) -> Option<ast_types::Type> {
    let (inner, mutable, span) = match ty {
        ast_types::Type::Tensor { span, .. } => (ty, false, *span),
        ast_types::Type::Generic { name, args, span } if name.name == KERNEL_OUT_TYPE => {
            let [GenericArg::Type(inner)] = args.as_slice() else {
                return None;
            };
            (inner, true, *span)
        }
        _ => return None,
    };
    Some(ast_types::Type::Reference {
        inner: Box::new(inner.clone()),
        mutable,
        lifetime: None,
        span,
    })
}

fn target_of(attributes: &[Attribute]) -> HirTarget {
    if let Some(kernel) = attributes
        .iter()
        .find(|attr| attr.name.name == KERNEL_ATTRIBUTE)
    {
        return HirTarget::Kernel {
            threads: block_shape(kernel),
        };
    }
    let Some(gpu) = attributes
        .iter()
        .find(|attr| attr.name.name == GPU_ATTRIBUTE)
    else {
        return HirTarget::Host;
    };
    let falls_back = gpu.named.iter().any(|arg| {
        arg.label.name == FALLBACK_LABEL
            && matches!(arg.value, Expr::Literal(Literal::Boolean(true), _))
    });
    match falls_back {
        true => HirTarget::GpuOrHost,
        false => HirTarget::Gpu,
    }
}

/// `threads:` as a block shape, 1 along every axis it does not name.
fn block_shape(kernel: &Attribute) -> [u32; 3] {
    let mut threads = [1; 3];
    let elements = kernel
        .named
        .iter()
        .filter(|arg| arg.label.name == THREADS_LABEL)
        .find_map(|arg| match &arg.value {
            Expr::ArrayLiteral { elements, .. } => Some(elements),
            _ => None,
        });
    for (slot, element) in threads.iter_mut().zip(elements.into_iter().flatten()) {
        if let Expr::Literal(Literal::Integer(n, _), _) = element {
            *slot = u32::try_from(*n).unwrap_or(1);
        }
    }
    threads
}

impl Lowerer {
    /// `items` with each `@kernel` function's tensor parameters written as the references
    /// its callers lend, so a kernel lowers like any function taking `&` and `&mut`
    /// tensors. Records which parameters are bare inputs, the ones a call borrows.
    pub(crate) fn kernel_signatures<'a>(&mut self, items: &'a [Item]) -> Cow<'a, [Item]> {
        if !items
            .iter()
            .any(|item| matches!(item, Item::Function(func) if is_kernel(func)))
        {
            return Cow::Borrowed(items);
        }
        let mut items = items.to_vec();
        for item in &mut items {
            let Item::Function(func) = item else {
                continue;
            };
            if !is_kernel(func) {
                continue;
            }
            let mut inputs = Vec::with_capacity(func.params.len());
            for param in &mut func.params {
                inputs.push(matches!(param.ty, ast_types::Type::Tensor { .. }));
                if let Some(reference) = kernel_param_reference(&param.ty) {
                    param.ty = reference;
                }
            }
            self.kernel_inputs.insert(func.name.name.clone(), inputs);
        }
        Cow::Owned(items)
    }

    /// The arguments of a call to `callee`, each tensor a kernel borrows written as the
    /// borrow it is; unchanged when `callee` is not a kernel.
    pub(crate) fn borrow_kernel_inputs<'a>(
        &self,
        callee: &str,
        args: &'a [Expr],
    ) -> Cow<'a, [Expr]> {
        let Some(inputs) = self.kernel_inputs.get(callee) else {
            return Cow::Borrowed(args);
        };
        let mut borrowed = args.to_vec();
        for (arg, _) in borrowed.iter_mut().zip(inputs).filter(|(_, input)| **input) {
            let span = arg.span();
            *arg = Expr::Reference {
                operand: Box::new(arg.clone()),
                mutable: false,
                span,
            };
        }
        Cow::Owned(borrowed)
    }

    /// Lower a function body, marking it as a kernel's when `target` says so: only there
    /// do `thread_id` and `block_id` name the thread's grid position.
    fn lower_function_body(
        &mut self,
        body: &[ast_types::Stmt],
        return_type: &HirType,
        target: HirTarget,
    ) -> Result<Vec<HirStmt>, LoweringError> {
        self.in_kernel = matches!(target, HirTarget::Kernel { .. });
        let lowered = self.lower_body(body, return_type);
        self.in_kernel = false;
        lowered
    }

    /// Lower every top-level item to its HIR form.
    pub(crate) fn lower_program(&mut self, items: &[Item]) -> Result<HirProgram, LoweringError> {
        let mut hir_items = Vec::with_capacity(items.len());
        // The `@grad` functions, concrete and instantiated, whose derivatives are derived
        // once everything they might call has been lowered.
        let mut grads = Vec::new();
        // The `@grad` methods, as (type, method): each gains a derivative method of its own.
        let mut grad_methods = Vec::new();
        // The `@no_grad` functions, concrete and instantiated: a constant to every
        // derivative that calls them.
        let mut no_grad = HashSet::new();
        for item in items {
            match item {
                // A generic template is not lowered directly; only its concrete
                // instantiations, discovered at call sites, reach the HIR.
                Item::Function(func) if !func.generics.is_empty() => {}
                Item::Function(func) => {
                    let lowered = self.lower_function(func)?;
                    if crate::autodiff::is_grad(&func.attributes) {
                        grads.push(lowered.name.clone());
                    }
                    if crate::autodiff::is_no_grad(&func.attributes) {
                        let _ = no_grad.insert(lowered.name.clone());
                    }
                    hir_items.push(HirItem::Function(lowered));
                }
                // Generic struct / impl templates are likewise never lowered directly;
                // each concrete instance is emitted from the monomorphization worklist.
                Item::Struct(def) if !def.generics.is_empty() => {}
                Item::Enum(def) if !def.generics.is_empty() => {}
                Item::Impl(def) if !def.generics.is_empty() || !def.type_args.is_empty() => {}
                Item::Struct(def) => hir_items.push(HirItem::Struct(self.lower_struct(def)?)),
                Item::Enum(def) => hir_items.push(HirItem::Enum(self.lower_enum(def)?)),
                Item::Impl(def) => {
                    let lowered = self.lower_impl(def)?;
                    for method in &def.methods {
                        if crate::autodiff::is_grad(&method.attributes) {
                            grad_methods
                                .push((lowered.type_name.clone(), method.name.name.clone()));
                        }
                    }
                    hir_items.push(HirItem::Impl(lowered));
                }
                Item::Const(def) => hir_items.push(HirItem::Const(self.lower_const(def)?)),
                // A newtype is transparent at runtime and produces no HIR item; it
                // survives only as the `HirType::Newtype` its annotations resolve to.
                // An import, an inline `module` block, and the `@no_prelude` marker are
                // all consumed by module resolution well before lowering.
                Item::Newtype(_) | Item::Import(_) | Item::Module(_) | Item::NoPrelude(_) => {}
                // A trait emits no code of its own: each `impl Trait for Type`
                // lowers via the ordinary impl path above, with any omitted default
                // method already injected by the parser. The item carries only the
                // declaration-ordered method list backends need to lay out vtables for
                // dynamic dispatch.
                Item::Trait(def) => hir_items.push(HirItem::Trait(neuro_hir::HirTrait {
                    name: def.name.name.clone(),
                    methods: def.methods.iter().map(|m| m.name.name.clone()).collect(),
                    span: def.span,
                })),
            }
        }

        // Drain the monomorphization worklists: lowering the ordinary items above (and
        // each instance below) enqueues every generic function and struct instantiation
        // it references, so this runs until the transitive closure is emitted.
        // Struct instances are drained first because emitting their method bodies can in
        // turn enqueue generic-function instances.
        loop {
            if let Some(me) = self.mono_enum_pending.pop() {
                self.emit_mono_enum(&me)?;
                continue;
            }
            if let Some(ms) = self.mono_struct_pending.pop() {
                self.emit_mono_struct(&ms)?;
                continue;
            }
            if let Some(instance) = self.mono_pending.pop() {
                let hir_fn = self.lower_mono_instance(&instance)?;
                // A `@grad` template's derivative is derived per instance, where the
                // shapes the reverse pass builds its gradients from are concrete.
                let attributes = self
                    .generic_templates
                    .get(&instance.fn_name)
                    .map(|template| template.attributes.as_slice())
                    .unwrap_or_default();
                if crate::autodiff::is_grad(attributes) {
                    grads.push(hir_fn.name.clone());
                }
                if crate::autodiff::is_no_grad(attributes) {
                    let _ = no_grad.insert(hir_fn.name.clone());
                }
                self.mono_items.push(HirItem::Function(hir_fn));
                continue;
            }
            break;
        }
        hir_items.append(&mut self.mono_items);
        // Lifted closures are appended last; their bodies are self-contained and the
        // backend pre-declares every function signature before emitting any body, so
        // position among the items does not matter.
        hir_items.append(&mut self.closure_items);
        // `@grad` lowers to the function plus its derivative: the bundle struct and
        // `__f__rev`, derived from the lowered bodies.
        let derived = crate::autodiff::derive_reverses(
            &hir_items,
            &grads,
            &self.grad_specializations,
            &self.grad_params,
            &no_grad,
        )?;
        hir_items.extend(derived);
        crate::autodiff::derive_method_reverses(
            &mut hir_items,
            &grad_methods,
            &self.grad_params,
            &self.structs,
            &no_grad,
        )?;
        // After the derivatives, which are host code of their own and read the operations
        // as written.
        let target = match self.transfers {
            true => HirTarget::FollowsOperands,
            false => HirTarget::HostOperation,
        };
        crate::tensor_ops::outline(&mut hir_items, target);

        Ok(HirProgram { items: hir_items })
    }

    fn lower_struct(&mut self, def: &StructDef) -> Result<HirStruct, LoweringError> {
        let mut fields = Vec::with_capacity(def.fields.len());
        for field in &def.fields {
            fields.push(HirField {
                name: field.name.name.clone(),
                ty: self.resolve_type(&field.ty)?,
                span: field.span,
            });
        }
        Ok(HirStruct {
            name: def.name.name.clone(),
            written_name: def.name.name.clone(),
            fields,
            span: def.span,
        })
    }

    fn lower_enum(&mut self, def: &EnumDef) -> Result<HirEnum, LoweringError> {
        let mut variants = Vec::with_capacity(def.variants.len());
        for variant in &def.variants {
            let fields = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(tys) => {
                    let mut fields = Vec::with_capacity(tys.len());
                    for ty in tys {
                        fields.push(HirEnumField {
                            name: None,
                            ty: self.resolve_type(ty)?,
                        });
                    }
                    fields
                }
                VariantPayload::Struct(field_defs) => {
                    let mut fields = Vec::with_capacity(field_defs.len());
                    for field in field_defs {
                        fields.push(HirEnumField {
                            name: Some(field.name.name.clone()),
                            ty: self.resolve_type(&field.ty)?,
                        });
                    }
                    fields
                }
            };
            variants.push(HirEnumVariant {
                name: variant.name.name.clone(),
                fields,
                span: variant.span,
            });
        }
        Ok(HirEnum {
            name: def.name.name.clone(),
            variants,
            span: def.span,
        })
    }

    fn lower_const(&mut self, def: &ConstDef) -> Result<HirConst, LoweringError> {
        let ty = self.resolve_type(&def.ty)?;
        let value = self.lower_expr(&def.value, Some(&ty))?;
        Ok(HirConst {
            name: def.name.name.clone(),
            ty,
            value,
            span: def.span,
        })
    }

    fn lower_function(&mut self, func: &FunctionDef) -> Result<HirFunction, LoweringError> {
        let mut params = Vec::with_capacity(func.params.len());
        for param in &func.params {
            params.push(HirParam {
                name: param.name.name.clone(),
                ty: self.resolve_type(&param.ty)?,
                span: param.span,
            });
        }
        let return_type = self.declared_return_type(&func.return_type, &func.body)?;

        let target = target_of(&func.attributes);
        self.push_scope();
        for param in &params {
            self.define(param.name.clone(), param.ty.clone());
        }
        let body = self.lower_function_body(&func.body, &return_type, target)?;
        self.pop_scope();

        Ok(HirFunction {
            name: func.name.name.clone(),
            params,
            return_type,
            body,
            target,
            span: func.span,
        })
    }

    fn lower_impl(&mut self, def: &ImplDef) -> Result<HirImpl, LoweringError> {
        let struct_name = def.type_name.name.clone();
        let self_type = self.impl_target_type(&struct_name)?;
        let saved_ty = self.enter_impl_assoc(def)?;
        let mut methods = Vec::new();
        for method in &def.methods {
            // Owned `self` on a `Copy` receiver is a valid operator-trait method;
            // the checker rejected it on any non-`Copy` type, so it is lowered like any
            // other method (an owned `Copy` receiver is ABI-identical to `&self`).
            let lowered = self.lower_method(&self_type, method);
            match lowered {
                Ok(m) => methods.push(m),
                Err(e) => {
                    self.type_subst = saved_ty;
                    return Err(e);
                }
            }
        }
        self.type_subst = saved_ty;
        Ok(HirImpl {
            type_name: struct_name,
            self_type,
            trait_name: def.trait_name.as_ref().map(|t| t.name.clone()),
            methods,
            span: def.span,
        })
    }

    fn lower_method(
        &mut self,
        self_ty: &HirType,
        method: &MethodDef,
    ) -> Result<HirMethod, LoweringError> {
        let mut params = Vec::with_capacity(method.params.len());
        for param in &method.params {
            params.push(HirParam {
                name: param.name.name.clone(),
                ty: self.resolve_type(&param.ty)?,
                span: param.span,
            });
        }
        let return_type = match &method.return_type {
            Some(t) => self.resolve_type(t)?,
            None => HirType::Void,
        };
        let self_param = method.self_param.as_ref().map(lower_self_param);

        self.push_scope();
        if self_param.is_some() {
            self.define("self".to_string(), self_ty.clone());
        }
        for param in &params {
            self.define(param.name.clone(), param.ty.clone());
        }
        let body = self.lower_body(&method.body, &return_type)?;
        self.pop_scope();

        Ok(HirMethod {
            name: method.name.name.clone(),
            self_param,
            params,
            return_type,
            body,
            span: method.span,
        })
    }

    /// Lower a function/method body. The trailing expression of a non-`void` body is
    /// an implicit return, so it is typed against the declared return type, exactly
    /// the contextual hint the checker applies; every other statement lowers
    /// with no expected type.
    pub(crate) fn lower_body(
        &mut self,
        body: &[ast_types::Stmt],
        return_type: &HirType,
    ) -> Result<Vec<HirStmt>, LoweringError> {
        // The declared return type is the contextual type for `return` operands and the
        // fallback instance for a generic-enum construction in a position no expected
        // type reaches (a tail `if` branch), so it is tracked for the whole body.
        let saved_return = std::mem::replace(&mut self.current_return, return_type.clone());
        let lowered = self.lower_body_stmts(body, return_type);
        self.current_return = saved_return;
        lowered
    }

    fn lower_body_stmts(
        &mut self,
        body: &[ast_types::Stmt],
        return_type: &HirType,
    ) -> Result<Vec<HirStmt>, LoweringError> {
        let mut out = Vec::with_capacity(body.len());
        let last = body.len().saturating_sub(1);
        for (i, stmt) in body.iter().enumerate() {
            let is_tail = i == last;
            if is_tail && !matches!(return_type, HirType::Void) {
                match stmt {
                    ast_types::Stmt::Expr(expr) => {
                        out.push(HirStmt::Expr(self.lower_expr(expr, Some(return_type))?));
                        continue;
                    }
                    // A statement-position `if` parses to `Stmt::If`, so a trailing
                    // `if/else` acting as the implicit return must be lowered as an
                    // expression; otherwise its branches lower as statement blocks and
                    // whatever they evaluate to is discarded.
                    ast_types::Stmt::If {
                        condition,
                        then_block,
                        else_if_blocks,
                        else_block,
                        span,
                    } if else_block.is_some() => {
                        let tail = self.lower_if_expr(
                            condition,
                            then_block,
                            else_if_blocks,
                            else_block,
                            Some(return_type),
                            *span,
                        )?;
                        out.push(HirStmt::Expr(tail));
                        continue;
                    }
                    _ => {}
                }
            }
            self.lower_stmt_into(stmt, &mut out)?;
        }
        Ok(out)
    }
}

/// The expression a body evaluates to: its trailing expression, or the operand of a
/// trailing `return`. Used for return-position `impl Trait` resolution.
fn body_result_expr(body: &[ast_types::Stmt]) -> Option<&ast_types::Expr> {
    match body.last()? {
        ast_types::Stmt::Expr(expr) => Some(expr),
        ast_types::Stmt::Return { value, .. } => value.as_ref(),
        _ => None,
    }
}

/// Lower the surface `self` receiver kind to its HIR counterpart.
fn lower_self_param(sp: &SelfParam) -> HirSelfParam {
    match sp {
        SelfParam::Ref => HirSelfParam::Ref,
        SelfParam::RefMut => HirSelfParam::RefMut,
        SelfParam::Owned => HirSelfParam::Owned,
    }
}
