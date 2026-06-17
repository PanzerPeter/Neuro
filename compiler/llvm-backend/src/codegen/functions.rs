use ast_types::*;
use inkwell::types::*;
use inkwell::values::*;
use std::collections::HashMap;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::CodegenContext;

impl<'ctx> CodegenContext<'ctx> {
    /// Generate code for a function call
    /// Lower a free/associated function call. Returns `None` when the callee
    /// returns unit `()` — such a call is valid only in statement position, where
    /// the result is discarded (see `codegen_call_dispatch`).
    pub(crate) fn codegen_call(
        &mut self,
        func_name: &str,
        args: &[Expr],
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        let function = *self
            .functions
            .get(func_name)
            .ok_or_else(|| CodegenError::UndefinedFunction(func_name.to_string()))?;

        let mut arg_values = Vec::new();
        for arg in args {
            let val = self.codegen_expr(arg)?;
            arg_values.push(BasicMetadataValueEnum::from(val));
        }

        let call_result = self
            .builder
            .build_call(function, &arg_values, "calltmp")
            .map_err(|e| CodegenError::LlvmError(format!("failed to build call: {}", e)))?;

        Ok(call_result.try_as_basic_value().basic())
    }

    /// Lower a method call, prepending the receiver to the argument list. Returns
    /// `None` when the method returns unit `()`.
    ///
    /// A `&self` method takes the struct by value (read-only, so mutations do not
    /// escape). A `&mut self` method takes the struct by pointer — detected by its
    /// first LLVM parameter being a pointer — so the receiver's storage address is
    /// passed and field writes in the body propagate back to the caller (§2.5).
    pub(crate) fn codegen_method_call(
        &mut self,
        mangled_name: &str,
        receiver: &Expr,
        args: &[Expr],
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        let function = *self
            .functions
            .get(mangled_name)
            .ok_or_else(|| CodegenError::UndefinedFunction(mangled_name.to_string()))?;

        let self_by_pointer = function
            .get_type()
            .get_param_types()
            .first()
            .is_some_and(|t| t.is_pointer_type());

        let struct_name = mangled_name.split("__").next().unwrap_or(mangled_name);
        let self_arg: BasicValueEnum<'ctx> = if self_by_pointer {
            // `&mut self`: pass the receiver place's address (auto-loading through a
            // `&mut Struct` receiver), so the callee writes through to it.
            let (self_ptr, _) = self.get_struct_ptr_and_type(receiver, struct_name)?;
            self_ptr.into()
        } else {
            // `&self`: pass the struct value, dereferencing a `&Struct` borrow (§2.4).
            match self.codegen_expr(receiver)? {
                BasicValueEnum::PointerValue(ptr) => {
                    let struct_ty = self.get_struct_llvm_type(struct_name)?;
                    self.builder
                        .build_load(struct_ty, ptr, "deref.self")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                }
                other => other,
            }
        };
        let mut arg_values: Vec<BasicMetadataValueEnum> =
            vec![BasicMetadataValueEnum::from(self_arg)];

        for arg in args {
            let val = self.codegen_expr(arg)?;
            arg_values.push(BasicMetadataValueEnum::from(val));
        }

        let call_result = self
            .builder
            .build_call(function, &arg_values, "calltmp")
            .map_err(|e| CodegenError::LlvmError(format!("failed to build method call: {}", e)))?;

        Ok(call_result.try_as_basic_value().basic())
    }

    /// Generate LLVM functions for all supported methods in an `impl` block.
    pub(crate) fn codegen_impl(
        &mut self,
        impl_def: &ImplDef,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        let struct_name = &impl_def.type_name.name;
        for method in &impl_def.methods {
            // Consuming `self` is rejected in semantic analysis; everything else
            // (`&self`, `&mut self`, associated functions) is lowered.
            if matches!(method.self_param, Some(SelfParam::Owned)) {
                continue;
            }
            self.codegen_method(method, struct_name, func_types)?;
        }
        Ok(())
    }

    /// Generate an LLVM function for a single method.
    ///
    /// The method is lowered to a free function under its mangled name
    /// (`StructName__methodName`). For `&self` methods the struct is the first
    /// parameter, named `self`, passed by value.
    pub(crate) fn codegen_method(
        &mut self,
        method: &MethodDef,
        struct_name: &str,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        let mangled = format!("{}__{}", struct_name, method.name.name);

        let func_type_info = func_types
            .get(&mangled)
            .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;

        let (param_types, return_type) = match func_type_info {
            Type::Function { params, ret } => (params, &**ret),
            _ => {
                return Err(CodegenError::InternalError(
                    "method type information is not a function type".to_string(),
                ))
            }
        };

        // A `&mut self` receiver is passed by pointer so the body's field writes
        // reach the caller's value (§2.5); every other parameter is by value.
        let self_by_pointer = matches!(method.self_param, Some(SelfParam::RefMut));

        let mut llvm_param_types = Vec::new();
        for (i, param_ty) in param_types.iter().enumerate() {
            let llvm_ty: BasicTypeEnum = if i == 0 && self_by_pointer {
                self.context
                    .ptr_type(inkwell::AddressSpace::default())
                    .into()
            } else if let Type::Struct(name) = param_ty {
                self.get_struct_llvm_type(name)?.into()
            } else {
                self.type_mapper.map_type(param_ty)?
            };
            llvm_param_types.push(BasicMetadataTypeEnum::from(llvm_ty));
        }

        let llvm_ret_type = if matches!(return_type, Type::Void) {
            self.context.void_type().fn_type(&llvm_param_types, false)
        } else if let Type::Struct(name) = return_type {
            self.get_struct_llvm_type(name)?
                .fn_type(&llvm_param_types, false)
        } else {
            let ret_basic_type = self.type_mapper.map_type(return_type)?;
            ret_basic_type.fn_type(&llvm_param_types, false)
        };

        let function = self.module.add_function(&mangled, llvm_ret_type, None);
        self.functions.insert(mangled.clone(), function);

        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);
        self.current_function = Some(function);
        self.variables.clear();
        self.variable_types.clear();

        // Bind parameters: param[0] is `self` for instance methods.
        let non_self_start = if method.self_param.is_some() { 1 } else { 0 };

        if method.self_param.is_some() {
            // Record `self`'s Neuro type so a `self.field = …` write in a `&mut self`
            // body resolves its struct here, not from the (per-function, possibly
            // stale) type-pass `type_env` left over from another item.
            self.type_env
                .insert("self".to_string(), Type::Struct(struct_name.to_string()));

            let self_val = function
                .get_nth_param(0)
                .ok_or_else(|| CodegenError::InternalError("missing self parameter".to_string()))?;
            if self_by_pointer {
                // `&mut self`: the parameter is already a pointer to the caller's
                // struct. Bind `self` directly to it (no copy) so reads and field
                // writes go through to the caller. The recorded type is the struct
                // (not the pointer), matching how an owned struct binding reads.
                let struct_ty = self.get_struct_llvm_type(struct_name)?;
                self.variables
                    .insert("self".to_string(), self_val.into_pointer_value());
                self.variable_types
                    .insert("self".to_string(), struct_ty.into());
            } else {
                // `&self`: allocate and store a private copy of the struct value.
                let self_type = self_val.get_type();
                let alloca = self
                    .builder
                    .build_alloca(self_type, "self")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                self.builder
                    .build_store(alloca, self_val)
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                self.variables.insert("self".to_string(), alloca);
                self.variable_types.insert("self".to_string(), self_type);
            }
        }

        for (i, param) in method.params.iter().enumerate() {
            let param_val = function
                .get_nth_param((non_self_start + i) as u32)
                .ok_or_else(|| CodegenError::InternalError(format!("missing parameter {}", i)))?;
            let param_type = param_val.get_type();
            let alloca = self
                .builder
                .build_alloca(param_type, &param.name.name)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.builder
                .build_store(alloca, param_val)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            self.variables.insert(param.name.name.clone(), alloca);
            self.variable_types
                .insert(param.name.name.clone(), param_type);
        }

        self.codegen_body(&method.body, return_type)
    }

    /// Generate code for a function definition
    pub(crate) fn codegen_function(
        &mut self,
        func_def: &FunctionDef,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        // Get function type information
        let func_type_info = func_types
            .get(&func_def.name.name)
            .ok_or_else(|| CodegenError::UndefinedFunction(func_def.name.name.clone()))?;

        let (param_types, return_type) = match func_type_info {
            Type::Function { params, ret } => (params, &**ret),
            _ => {
                return Err(CodegenError::InternalError(
                    "function type information is not a function type".to_string(),
                ))
            }
        };

        // Map parameter types to LLVM types
        let mut llvm_param_types = Vec::new();
        for param_ty in param_types {
            let llvm_ty = self.type_mapper.map_type(param_ty)?;
            llvm_param_types.push(BasicMetadataTypeEnum::from(llvm_ty));
        }

        // Map return type to LLVM type
        let llvm_ret_type = if matches!(return_type, Type::Void) {
            self.context.void_type().fn_type(&llvm_param_types, false)
        } else {
            let ret_basic_type = self.type_mapper.map_type(return_type)?;
            ret_basic_type.fn_type(&llvm_param_types, false)
        };

        // Create the function
        let function = self
            .module
            .add_function(&func_def.name.name, llvm_ret_type, None);

        // Record the function for later calls
        self.functions.insert(func_def.name.name.clone(), function);

        // Create entry basic block
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        // Set current function for return statements
        self.current_function = Some(function);

        // Clear variables for new function scope
        self.variables.clear();
        self.variable_types.clear();

        // Allocate and store parameters
        for (i, param) in func_def.params.iter().enumerate() {
            let param_val = function
                .get_nth_param(i as u32)
                .ok_or_else(|| CodegenError::InternalError(format!("missing parameter {}", i)))?;

            let param_type = param_val.get_type();

            let alloca = self
                .builder
                .build_alloca(param_type, &param.name.name)
                .map_err(|e| {
                    CodegenError::LlvmError(format!("failed to allocate parameter: {}", e))
                })?;

            self.builder.build_store(alloca, param_val).map_err(|e| {
                CodegenError::LlvmError(format!("failed to store parameter: {}", e))
            })?;

            self.variables.insert(param.name.name.clone(), alloca);
            self.variable_types
                .insert(param.name.name.clone(), param_type);
        }

        self.codegen_body(&func_def.body, return_type)
    }

    /// Lower a function or method body, treating a value-producing tail statement
    /// as the implicit return value.
    ///
    /// A statement-position `if` parses to `Stmt::If`, so a trailing `if/else` used
    /// as the implicit return is not a `Stmt::Expr`; it must still yield the
    /// if-expression's value rather than falling through with `unreachable`, which
    /// produces no instruction at `-O0` and lets execution run off the end of the
    /// function.
    fn codegen_body(&mut self, body: &[Stmt], return_type: &Type) -> CodegenResult<()> {
        let tail_is_value = !matches!(return_type, Type::Void)
            && matches!(
                body.last(),
                Some(Stmt::Expr(_))
                    | Some(Stmt::If {
                        else_block: Some(_),
                        ..
                    })
            );

        if tail_is_value {
            for stmt in &body[..body.len() - 1] {
                self.codegen_stmt(stmt)?;
            }
            // A preceding statement may have diverged (e.g. an unconditional `panic`),
            // terminating the block before the tail expression. Skip the tail and the
            // return in that case — there is no live block to emit into.
            if !self.current_block_terminated() {
                let ret_val = match body.last() {
                    Some(Stmt::Expr(expr)) => self.codegen_expr(expr)?,
                    Some(Stmt::If {
                        condition,
                        then_block,
                        else_if_blocks,
                        else_block,
                        span,
                    }) => self.codegen_if_expr(
                        condition,
                        then_block,
                        else_if_blocks,
                        else_block,
                        span,
                    )?,
                    _ => {
                        return Err(CodegenError::InternalError(
                            "tail statement is not value-producing".to_string(),
                        ))
                    }
                };
                // The tail expression itself may diverge (`func f() -> i32 { panic("x") }`),
                // in which case the block is already terminated and `ret_val` is a discarded
                // placeholder — do not append a `ret` after the `unreachable`.
                if !self.current_block_terminated() {
                    self.builder
                        .build_return(Some(&ret_val))
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                }
            }
        } else {
            for stmt in body {
                self.codegen_stmt(stmt)?;
            }

            // Ensure every basic block has a terminator (LLVM verifier requirement).
            // If the current block has no terminator it is either dead code (a merge
            // block whose predecessors all returned/broke) or a genuine missing return.
            // In both cases we emit `unreachable`; dead blocks are eliminated by LLVM
            // later, while genuine missing returns produce undefined behaviour — the
            // correct long-term fix is return-path analysis in semantic analysis.
            if let Some(current_bb) = self.builder.get_insert_block() {
                if current_bb.get_terminator().is_none() {
                    if matches!(return_type, Type::Void) {
                        self.builder.build_return(None).map_err(|e| {
                            CodegenError::LlvmError(format!("failed to build void return: {}", e))
                        })?;
                    } else {
                        self.builder.build_unreachable().map_err(|e| {
                            CodegenError::LlvmError(format!(
                                "failed to build unreachable terminator: {}",
                                e
                            ))
                        })?;
                    }
                }
            }
        }

        Ok(())
    }
}
