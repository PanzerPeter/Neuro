//! Closure lowering (§3.12).
//!
//! A closure is represented uniformly as a `{ fn_ptr, env_ptr }` fat pointer. Each
//! closure literal is lifted to a top-level function whose first LLVM parameter is a
//! pointer to its captured-environment struct; the value site allocates that struct
//! in the defining frame, snapshots each Copy capture into it, and pairs the
//! function pointer with the environment pointer. A call loads both halves and
//! dispatches indirectly.

use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum};
use inkwell::AddressSpace;
use neuro_hir::{HirCapture, HirClosure, HirExpr};

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::CodegenContext;

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
        let env_struct_ty = self.closure_env_type(&closure.captures)?;
        for (i, cap) in closure.captures.iter().enumerate() {
            let sem_ty = Type::from_hir(&cap.ty);
            let llvm_ty = self.get_any_llvm_type(&sem_ty)?;
            let field_ptr = self
                .builder
                .build_struct_gep(env_struct_ty, env_ptr, i as u32, &cap.name)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let loaded = self
                .builder
                .build_load(llvm_ty, field_ptr, &cap.name)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let alloca = self
                .builder
                .build_alloca(llvm_ty, &cap.name)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_store(alloca, loaded)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.variables.insert(cap.name.clone(), alloca);
            self.variable_types.insert(cap.name.clone(), llvm_ty);
            self.type_env.insert(cap.name.clone(), sem_ty);
        }

        // User parameters follow the environment pointer (LLVM params 1..=n).
        for (i, param) in closure.params.iter().enumerate() {
            let param_val = function.get_nth_param((i + 1) as u32).ok_or_else(|| {
                CodegenError::InternalError(format!("closure is missing parameter {}", i))
            })?;
            let param_type = param_val.get_type();
            let alloca = self
                .builder
                .build_alloca(param_type, &param.name)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_store(alloca, param_val)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.variables.insert(param.name.clone(), alloca);
            self.variable_types.insert(param.name.clone(), param_type);
            self.type_env
                .insert(param.name.clone(), Type::from_hir(&param.ty));
        }

        // Captures are Copy and parameters are borrowed/Copy, so the body opens a
        // drop scope with no registered owned bindings — locals register themselves.
        self.push_drop_scope();
        let ret_ty = Type::from_hir(&closure.return_type);
        self.codegen_body(&closure.body, &ret_ty)
    }

    /// Build the anonymous LLVM struct type for a closure's captured environment,
    /// one field per capture in layout order. Used at both the definition site (to
    /// GEP/load) and the value site (to alloca/store), which must agree on layout.
    fn closure_env_type(
        &self,
        captures: &[HirCapture],
    ) -> CodegenResult<inkwell::types::StructType<'ctx>> {
        let mut field_tys: Vec<BasicTypeEnum<'ctx>> = Vec::with_capacity(captures.len());
        for cap in captures {
            field_tys.push(self.get_any_llvm_type(&Type::from_hir(&cap.ty))?);
        }
        Ok(self.context.struct_type(&field_tys, false))
    }

    /// Lower a closure value expression: allocate the environment in the current
    /// frame, snapshot each capture into it, and build the `{ fn_ptr, env_ptr }`
    /// fat pointer.
    pub(crate) fn codegen_closure_value(
        &mut self,
        name: &str,
        captures: &[HirCapture],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let env_struct_ty = self.closure_env_type(captures)?;
        let env_ptr = self
            .builder
            .build_alloca(env_struct_ty, "closure.env")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        for (i, cap) in captures.iter().enumerate() {
            let value = self.codegen_identifier(&cap.name)?;
            let field_ptr = self
                .builder
                .build_struct_gep(env_struct_ty, env_ptr, i as u32, "closure.cap")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_store(field_ptr, value)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
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
            .build_insert_value(closure_ty.get_undef(), fn_ptr, 0, "closure.fn")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();
        let fat = self
            .builder
            .build_insert_value(with_fn, env_ptr, 1, "closure.val")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
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
                )))
            }
        };

        let fat = self.codegen_expr(callee)?;
        let BasicValueEnum::StructValue(fat) = fat else {
            return Err(CodegenError::InternalError(
                "closure callee did not lower to a fat pointer".into(),
            ));
        };
        let fn_ptr = self
            .builder
            .build_extract_value(fat, 0, "closure.fn")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let env_ptr = self
            .builder
            .build_extract_value(fat, 1, "closure.env")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();

        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        let mut call_args: Vec<BasicMetadataValueEnum<'ctx>> = vec![env_ptr.into()];
        let mut param_types: Vec<BasicMetadataTypeEnum<'ctx>> = vec![ptr_ty.into()];
        for arg in args {
            let value = self.codegen_expr(arg)?;
            // A closure argument is passed by copy (captures are Copy), so a by-value
            // `Drop` place is still moved into the callee here.
            self.mark_moved_for_drop(arg);
            param_types.push(value.get_type().into());
            call_args.push(value.into());
        }

        let fn_type = if matches!(ret_ty, Type::Void) {
            self.context.void_type().fn_type(&param_types, false)
        } else {
            self.get_any_llvm_type(&ret_ty)?
                .fn_type(&param_types, false)
        };

        let call = self
            .builder
            .build_indirect_call(fn_type, fn_ptr, &call_args, "closure.call")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(call.try_as_basic_value().basic())
    }
}
