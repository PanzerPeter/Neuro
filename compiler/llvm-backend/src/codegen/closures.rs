//! Closure lowering.
//!
//! A closure is represented uniformly as a `{ fn_ptr, env_ptr }` fat pointer. Each
//! closure literal is lifted to a top-level function whose first LLVM parameter is a
//! pointer to its captured-environment struct; the value site allocates that struct
//! in the defining frame, stores each capture into it (a copy of the value, the value
//! a `move` closure takes over, or the address of a borrowed binding), and pairs the
//! function pointer with the environment pointer. A call loads both halves and
//! dispatches indirectly. The body reads its captures where the environment holds them.
//!
//! A capture that owns something is followed by an `i1` field saying whether it still
//! does. The scope that built the closure releases it while that flag stands; a body
//! that moves the capture out clears it, which is how a `once` call settles who frees it.
use inkwell::AddressSpace;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, PointerValue};
use neuro_hir::{HirCapture, HirClosure, HirExpr, HirExprKind, HirType};
use shared_types::Span;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::{CodegenContext, DropEntry, DropTarget};

impl<'ctx> CodegenContext<'ctx> {
    /// Declare a lifted closure's LLVM signature: `(env_ptr, params...) -> ret`.
    /// The environment pointer is an implicit first parameter carrying the captures.
    pub(crate) fn declare_closure(&mut self, closure: &HirClosure) -> CodegenResult<()> {
        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        let mut llvm_params: Vec<BasicMetadataTypeEnum<'ctx>> = vec![ptr_ty.into()];
        for param in &closure.params {
            let ty = Type::from_hir(&param.ty);
            llvm_params.push(self.get_any_llvm_type(&ty)?.into());
        }

        let ret_ty = Type::from_hir(&closure.return_type);
        let fn_type = if matches!(ret_ty, Type::Void) {
            self.context.void_type().fn_type(&llvm_params, false)
        } else {
            self.get_any_llvm_type(&ret_ty)?
                .fn_type(&llvm_params, false)
        };

        let function = self.module.add_function(&closure.name, fn_type, None);
        self.functions.insert(closure.name.clone(), function);
        Ok(())
    }

    /// Emit the body of a lifted closure. The prologue loads each capture from the
    /// environment pointer into a local, then binds the user parameters exactly like a
    /// free function.
    pub(crate) fn codegen_closure(&mut self, closure: &HirClosure) -> CodegenResult<()> {
        let function = *self
            .functions
            .get(&closure.name)
            .ok_or_else(|| CodegenError::UndefinedFunction(closure.name.clone()))?;

        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);
        self.current_function = Some(function);
        self.borrowed_self = false;
        self.variables.clear();
        self.variable_types.clear();
        self.type_env.clear();

        // Load captures out of the environment struct that the value site populated.
        let env_ptr = function
            .get_nth_param(0)
            .ok_or_else(|| {
                CodegenError::InternalError("closure is missing its environment parameter".into())
            })?
            .into_pointer_value();
        // Each capture is read and written in place, so a write a `move` closure makes
        // to its own copy is still there at the next call.
        let env = self.closure_env_layout(&closure.captures)?;
        self.captured_drop_entries.clear();
        for (i, cap) in closure.captures.iter().enumerate() {
            let sem_ty = Type::from_hir(&cap.ty);
            let llvm_ty = self.get_any_llvm_type(&sem_ty)?;
            let field_ptr =
                self.builder
                    .build_struct_gep(env.struct_ty, env_ptr, i as u32, &cap.name)?;
            self.variables.insert(cap.name.clone(), field_ptr);
            self.variable_types.insert(cap.name.clone(), llvm_ty);
            self.type_env.insert(cap.name.clone(), sem_ty);
            if let Some((flag_index, target)) = env.owned_flag(i) {
                let flag_ptr = self.builder.build_struct_gep(
                    env.struct_ty,
                    env_ptr,
                    flag_index,
                    "capture.owns.ptr",
                )?;
                self.captured_drop_entries.insert(
                    cap.name.clone(),
                    DropEntry {
                        name: cap.name.clone(),
                        storage_ptr: field_ptr,
                        flag_ptr,
                        target,
                        held: Vec::new(),
                    },
                );
            }
        }

        // User parameters follow the environment pointer (LLVM params 1..=n).
        for (i, param) in closure.params.iter().enumerate() {
            let param_val = function.get_nth_param((i + 1) as u32).ok_or_else(|| {
                CodegenError::InternalError(format!("closure is missing parameter {}", i))
            })?;
            let param_type = param_val.get_type();
            let alloca = self.entry_alloca(param_type, &param.name)?;
            self.builder.build_store(alloca, param_val)?;
            self.variables.insert(param.name.clone(), alloca);
            self.variable_types.insert(param.name.clone(), param_type);
            self.type_env
                .insert(param.name.clone(), Type::from_hir(&param.ty));
        }

        // The body owns none of its captures: what the environment owns, the scope that
        // built it releases. Locals register themselves in the body's own scope.
        self.push_drop_scope();
        let ret_ty = Type::from_hir(&closure.return_type);
        let emitted = self.codegen_body(&closure.body, &ret_ty);
        self.captured_drop_entries.clear();
        emitted
    }

    /// The layout of a closure's captured environment: one field per capture in order,
    /// then one `i1` ownership flag per capture that owns something. The definition site
    /// (to GEP) and the value site (to alloca and store) must agree on it.
    fn closure_env_layout(&self, captures: &[HirCapture]) -> CodegenResult<EnvLayout<'ctx>> {
        let mut field_tys: Vec<BasicTypeEnum<'ctx>> = Vec::with_capacity(captures.len());
        let mut owned = Vec::new();
        for (i, cap) in captures.iter().enumerate() {
            let ty = Type::from_hir(&cap.ty);
            field_tys.push(self.get_any_llvm_type(&ty)?);
            if cap.borrowed {
                continue;
            }
            if let Some(target) = self.captured_drop_target(&ty) {
                owned.push((i, target));
            }
        }
        field_tys.extend(
            owned
                .iter()
                .map(|_| BasicTypeEnum::from(self.context.bool_type())),
        );
        Ok(EnvLayout {
            struct_ty: self.context.struct_type(&field_tys, false),
            capture_count: captures.len(),
            owned,
        })
    }

    /// Lower a closure value expression: allocate the environment in the current
    /// frame, store each capture into it, and build the `{ fn_ptr, env_ptr }` fat
    /// pointer.
    ///
    /// A capture that owns something moves in: its source binding is disowned, and the
    /// environment's flag takes over the ownership the source had, which the innermost
    /// scope then releases while the flag still stands.
    pub(crate) fn codegen_closure_value(
        &mut self,
        name: &str,
        captures: &[HirCapture],
        span: Span,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let env = self.closure_env_layout(captures)?;
        let env_ptr = self.entry_alloca(env.struct_ty, "closure.env")?;
        for (i, cap) in captures.iter().enumerate() {
            let field_ptr =
                self.builder
                    .build_struct_gep(env.struct_ty, env_ptr, i as u32, "closure.cap")?;
            let ty = Type::from_hir(&cap.ty);
            let source_ty = match (&cap.ty, cap.borrowed) {
                (HirType::Reference { inner, .. }, true) => (**inner).clone(),
                _ => cap.ty.clone(),
            };
            let source = HirExpr::new(HirExprKind::Variable(cap.name.clone()), source_ty, span);
            let value = if cap.borrowed {
                self.codegen_reference(&source, &ty)?
            } else {
                self.codegen_identifier(&cap.name)?
            };
            self.builder.build_store(field_ptr, value)?;
            let Some((flag_index, target)) = env.owned_flag(i) else {
                continue;
            };
            let owns = self.owns_binding_now(&cap.name)?;
            self.mark_moved_for_drop(&source);
            let flag_ptr = self.builder.build_struct_gep(
                env.struct_ty,
                env_ptr,
                flag_index,
                "closure.owns",
            )?;
            self.builder.build_store(flag_ptr, owns)?;
            self.register_captured_drop(field_ptr, flag_ptr, target);
        }

        let function = *self
            .functions
            .get(name)
            .ok_or_else(|| CodegenError::UndefinedFunction(name.to_string()))?;
        let fn_ptr = function.as_global_value().as_pointer_value();

        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        let closure_ty = self
            .context
            .struct_type(&[ptr_ty.into(), ptr_ty.into()], false);
        let with_fn = self
            .builder
            .build_insert_value(closure_ty.get_undef(), fn_ptr, 0, "closure.fn")?
            .into_struct_value();
        let fat = self
            .builder
            .build_insert_value(with_fn, env_ptr, 1, "closure.val")?
            .into_struct_value();
        Ok(fat.into())
    }

    /// Lower a call through a closure value (`f(args)` where `f` is a closure binding
    /// or a function-typed parameter): extract the function and environment pointers
    /// and dispatch indirectly, passing the environment as the hidden first argument.
    pub(crate) fn codegen_indirect_call(
        &mut self,
        callee: &HirExpr,
        args: &[HirExpr],
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        let ret_ty = match &callee.ty {
            neuro_hir::HirType::Function { ret, .. } => Type::from_hir(ret),
            other => {
                return Err(CodegenError::InternalError(format!(
                    "indirect call target is not a function type: {:?}",
                    other
                )));
            }
        };

        let value = self.codegen_expr(callee)?;
        let target = self.split_function_value(value)?;
        // A callee whose lifted body is known gets the plain-call treatment of its
        // `string` arguments; any other function value may retain what it is passed.
        let lifted = self.lifted_closure_name(callee);
        let read_only = |ctx: &Self, index| {
            lifted
                .as_deref()
                .is_some_and(|name| ctx.string_ownership.param_is_read_only(name, index))
        };
        let forwarded = lifted
            .as_deref()
            .and_then(|name| self.string_ownership.forwarded_param(name));
        let mut forwarded_owner = None;
        let mut values = Vec::with_capacity(args.len());
        let mut forwarded_args = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            let value = self.codegen_expr(arg)?;
            if forwarded == Some(index) {
                forwarded_owner = Some(self.argument_string_owner(arg)?);
            } else if let Some(name) = &lifted
                && let Some(owner) = self.forwarded_argument_owner(name, index, arg)
            {
                forwarded_args.push((index, owner));
            }
            // A closure argument is passed by copy (captures are Copy), so a by-value
            // `Drop` place is still moved into the callee here, unless the body only
            // reads it and the caller's own scope stays its releaser.
            if !read_only(self, index) {
                self.mark_moved_for_drop(arg);
            }
            values.push(value);
        }
        let result = self.call_function_value(&target, &values, &ret_ty)?;
        if let Some(name) = &lifted {
            self.release_owned_arguments(name, args, &values)?;
            self.release_forwarded_arguments(&forwarded_args, &values)?;
            if let Some(owner) = forwarded_owner {
                self.forwarded_string_owner = Some((name.clone(), owner));
            }
        }
        Ok(result)
    }

    /// The generated name of the lifted body `callee` provably calls: a closure literal
    /// called in place, or an immutable binding a closure literal initialized. `None`
    /// for any other function value (a parameter, a field, a `mut` binding), whose body
    /// is not known at the call.
    pub(crate) fn lifted_closure_name(&self, callee: &HirExpr) -> Option<String> {
        match &callee.kind {
            HirExprKind::Closure { name, .. } => Some(name.clone()),
            HirExprKind::Variable(name) => self
                .variables
                .get(name)
                .and_then(|alloca| self.closure_bindings.get(alloca))
                .cloned(),
            _ => None,
        }
    }

    /// Split an already-lowered function value into the halves a call needs.
    ///
    /// Separate from [`Self::call_function_value`] so a caller that calls the same
    /// function repeatedly — a tensor traversal, one call per element — evaluates the
    /// value once, outside its loop, the way the surface says it is evaluated once.
    pub(crate) fn split_function_value(
        &self,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<FunctionValueHalves<'ctx>> {
        let BasicValueEnum::StructValue(fat) = value else {
            return Err(CodegenError::InternalError(
                "closure callee did not lower to a fat pointer".into(),
            ));
        };
        let fn_ptr = self
            .builder
            .build_extract_value(fat, 0, "closure.fn")?
            .into_pointer_value();
        let env_ptr = self
            .builder
            .build_extract_value(fat, 1, "closure.env")?
            .into_pointer_value();
        Ok(FunctionValueHalves { fn_ptr, env_ptr })
    }

    /// Dispatch through a split function value, passing the environment as the hidden
    /// first argument. Returns `None` when the callee returns unit `()`.
    pub(crate) fn call_function_value(
        &self,
        target: &FunctionValueHalves<'ctx>,
        args: &[BasicValueEnum<'ctx>],
        ret_ty: &Type,
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        let mut call_args: Vec<BasicMetadataValueEnum<'ctx>> = vec![target.env_ptr.into()];
        let mut param_types: Vec<BasicMetadataTypeEnum<'ctx>> = vec![ptr_ty.into()];
        for value in args {
            param_types.push(value.get_type().into());
            call_args.push((*value).into());
        }

        let fn_type = if matches!(ret_ty, Type::Void) {
            self.context.void_type().fn_type(&param_types, false)
        } else {
            self.get_any_llvm_type(ret_ty)?.fn_type(&param_types, false)
        };

        let call =
            self.builder
                .build_indirect_call(fn_type, target.fn_ptr, &call_args, "closure.call")?;
        Ok(call.try_as_basic_value().basic())
    }
}

/// A closure environment's struct type, and which captures own their value.
struct EnvLayout<'ctx> {
    struct_ty: inkwell::types::StructType<'ctx>,
    capture_count: usize,
    /// Each owning capture's index, with how its value is released, in field order.
    owned: Vec<(usize, DropTarget)>,
}

impl EnvLayout<'_> {
    /// The field index of capture `index`'s ownership flag, with its release, when the
    /// capture owns its value.
    fn owned_flag(&self, index: usize) -> Option<(u32, DropTarget)> {
        self.owned
            .iter()
            .position(|(owned, _)| *owned == index)
            .map(|slot| {
                (
                    (self.capture_count + slot) as u32,
                    self.owned[slot].1.clone(),
                )
            })
    }
}

/// The two halves of a `{ fn_ptr, env_ptr }` function value, extracted once.
pub(crate) struct FunctionValueHalves<'ctx> {
    fn_ptr: PointerValue<'ctx>,
    env_ptr: PointerValue<'ctx>,
}
