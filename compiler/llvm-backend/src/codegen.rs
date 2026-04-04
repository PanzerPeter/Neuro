// NEURO Programming Language - LLVM Backend
// Code generation context and LLVM IR generation

use ast_types::{
    BinaryOp, Expr, FieldInit, FunctionDef, ImplDef, Item, MethodDef, SelfParam, Stmt, UnaryOp,
};
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context as LLVMContext;
use inkwell::module::Module;
use inkwell::types::StructType;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue,
};
use inkwell::{FloatPredicate, IntPredicate};
use std::collections::HashMap;

use crate::errors::{CodegenError, CodegenResult};
use crate::type_mapping::TypeMapper;
use crate::types::Type;

struct LoopTargets<'ctx> {
    continue_bb: BasicBlock<'ctx>,
    break_bb: BasicBlock<'ctx>,
}

pub(crate) struct CodegenContext<'ctx> {
    context: &'ctx LLVMContext,
    pub(crate) module: Module<'ctx>,
    builder: Builder<'ctx>,
    type_mapper: TypeMapper<'ctx>,

    /// Local variables in the current function (name -> pointer to stack allocation)
    variables: HashMap<String, PointerValue<'ctx>>,

    /// Types of local variables (needed for opaque pointers)
    variable_types: HashMap<String, BasicTypeEnum<'ctx>>,

    /// Function declarations (name -> LLVM function)
    functions: HashMap<String, FunctionValue<'ctx>>,

    /// Current function being compiled (for return type checking)
    current_function: Option<FunctionValue<'ctx>>,

    /// Type information for expressions (needed for operator codegen)
    expr_types: HashMap<usize, Type>, // Maps expression span.start -> Type

    /// Variable type information during type collection (name -> Type)
    type_env: HashMap<String, Type>,

    /// Active loop targets for break/continue statements.
    loop_targets: Vec<LoopTargets<'ctx>>,

    /// Struct field definitions (name → ordered [(field_name, field_type)]).
    /// Populated before code generation begins; used by GEP and insertvalue.
    struct_defs: HashMap<String, Vec<(String, Type)>>,

    /// Maps FieldAccess span.start → struct name of the object.
    /// Needed because FieldAccess and its first sub-expression (the object Identifier)
    /// share the same span.start, causing expr_types collisions.
    fa_struct_names: HashMap<usize, String>,
}

impl<'ctx> CodegenContext<'ctx> {
    pub(crate) fn new(context: &'ctx LLVMContext, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        let type_mapper = TypeMapper::new(context);

        Self {
            context,
            module,
            builder,
            type_mapper,
            variables: HashMap::new(),
            variable_types: HashMap::new(),
            functions: HashMap::new(),
            current_function: None,
            expr_types: HashMap::new(),
            type_env: HashMap::new(),
            loop_targets: Vec::new(),
            struct_defs: HashMap::new(),
            fa_struct_names: HashMap::new(),
        }
    }

    /// Populate the struct definition table before code generation begins.
    pub(crate) fn set_struct_defs(&mut self, defs: HashMap<String, Vec<(String, Type)>>) {
        self.struct_defs = defs;
    }

    /// Build (or reconstruct) the LLVM struct type for a named struct.
    ///
    /// LLVM deduplicates anonymous struct types by structure, so reconstructing
    /// the type each call is safe and avoids storing LLVM types in the context.
    fn get_struct_llvm_type(&self, name: &str) -> CodegenResult<StructType<'ctx>> {
        let def = self.struct_defs.get(name).ok_or_else(|| {
            CodegenError::UnsupportedType(format!("unknown struct type '{}'", name))
        })?;
        let mut field_llvm_types = Vec::new();
        for (_, field_ty) in def {
            field_llvm_types.push(self.type_mapper.map_type(field_ty)?);
        }
        Ok(self.context.struct_type(&field_llvm_types, false))
    }

    /// Generate code for a literal expression
    fn codegen_literal(&self, lit: &shared_types::Literal) -> CodegenResult<BasicValueEnum<'ctx>> {
        match lit {
            shared_types::Literal::Integer(val) => {
                // Default to i32 for integer literals
                Ok(self.context.i32_type().const_int(*val as u64, true).into())
            }
            shared_types::Literal::Float(val) => {
                // Default to f64 for float literals
                Ok(self.context.f64_type().const_float(*val).into())
            }
            shared_types::Literal::Boolean(val) => Ok(self
                .context
                .bool_type()
                .const_int(*val as u64, false)
                .into()),
            shared_types::Literal::String(s) => {
                // Place the UTF-8 bytes in read-only memory; LLVM appends the null terminator.
                let global_string =
                    self.builder
                        .build_global_string_ptr(s, "str")
                        .map_err(|e| {
                            CodegenError::LlvmError(format!(
                                "failed to create string constant: {}",
                                e
                            ))
                        })?;

                // byte count excludes the null terminator — callers should not rely on it
                let byte_len = self.context.i64_type().const_int(s.len() as u64, false);

                // Build { ptr, i64 } via insertvalue instructions rather than a constant
                // aggregate so that LLVM emits a PC-relative reference for the pointer field,
                // which is required for PIE/PIC builds (avoids R_X86_64_32 relocations).
                let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                let fat_ptr_type = self
                    .context
                    .struct_type(&[ptr_type.into(), self.context.i64_type().into()], false);

                let with_ptr = self
                    .builder
                    .build_insert_value(
                        fat_ptr_type.get_undef(),
                        global_string.as_pointer_value(),
                        0,
                        "str.ptr",
                    )
                    .map_err(|e| CodegenError::LlvmError(format!("failed to insert ptr: {}", e)))?
                    .into_struct_value();

                let fat_ptr = self
                    .builder
                    .build_insert_value(with_ptr, byte_len, 1, "str.fat")
                    .map_err(|e| CodegenError::LlvmError(format!("failed to insert len: {}", e)))?
                    .into_struct_value();

                Ok(fat_ptr.into())
            }
        }
    }

    /// Generate code for an identifier (variable reference)
    fn codegen_identifier(&self, name: &str) -> CodegenResult<BasicValueEnum<'ctx>> {
        let ptr = self
            .variables
            .get(name)
            .ok_or_else(|| CodegenError::UndefinedVariable(name.to_string()))?;

        let var_type = self.variable_types.get(name).ok_or_else(|| {
            CodegenError::InternalError(format!("missing type for variable {}", name))
        })?;

        self.builder
            .build_load(*var_type, *ptr, name)
            .map_err(|e| CodegenError::LlvmError(format!("failed to load variable: {}", e)))
    }

    /// Get the external `memcmp` declaration, inserting it on first use.
    /// memcmp(s1: ptr, s2: ptr, n: i64) -> i32 — libc, always available on Linux/macOS.
    fn get_or_declare_memcmp(&self) -> FunctionValue<'ctx> {
        if let Some(f) = self.module.get_function("memcmp") {
            return f;
        }
        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
        let fn_type = self.context.i32_type().fn_type(
            &[
                ptr_type.into(),
                ptr_type.into(),
                self.context.i64_type().into(),
            ],
            false,
        );
        self.module
            .add_function("memcmp", fn_type, Some(inkwell::module::Linkage::External))
    }

    /// Compare two string fat-pointers for byte-level equality.
    ///
    /// Uses the length field for an O(1) short-circuit before falling back to
    /// `memcmp`. When lengths differ the length passed to `memcmp` is set to 0
    /// (memcmp with n=0 returns 0), so `len_eq` being false drives the final AND
    /// to false without reading out-of-bounds memory.
    fn codegen_string_eq(
        &self,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        let lhs_struct = lhs.into_struct_value();
        let rhs_struct = rhs.into_struct_value();

        let ptr1 = self
            .builder
            .build_extract_value(lhs_struct, 0, "s1.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let len1 = self
            .builder
            .build_extract_value(lhs_struct, 1, "s1.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let ptr2 = self
            .builder
            .build_extract_value(rhs_struct, 0, "s2.ptr")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let len2 = self
            .builder
            .build_extract_value(rhs_struct, 1, "s2.len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_int_value();

        let len_eq = self
            .builder
            .build_int_compare(IntPredicate::EQ, len1, len2, "len_eq")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let zero_len = self.context.i64_type().const_int(0, false);
        let cmp_len = self
            .builder
            .build_select(len_eq, len1, zero_len, "cmp_len")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let memcmp_fn = self.get_or_declare_memcmp();
        let call = self
            .builder
            .build_call(
                memcmp_fn,
                &[ptr1.into(), ptr2.into(), cmp_len.into()],
                "memcmp_res",
            )
            .map_err(|e| CodegenError::LlvmError(format!("failed to call memcmp: {}", e)))?;

        let memcmp_val = call
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| CodegenError::InternalError("memcmp returned void".to_string()))?
            .into_int_value();

        let content_eq = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                memcmp_val,
                self.context.i32_type().const_int(0, false),
                "content_eq",
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        self.builder
            .build_and(len_eq, content_eq, "str_eq")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Generate code for a binary expression
    fn codegen_binary(
        &self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        left_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let lhs = self.codegen_expr(left)?;
        let rhs = self.codegen_expr(right)?;

        match op {
            // Arithmetic operators
            BinaryOp::Add => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_add(lhs.into_float_value(), rhs.into_float_value(), "addtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_add(lhs.into_int_value(), rhs.into_int_value(), "addtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Subtract => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_sub(lhs.into_float_value(), rhs.into_float_value(), "subtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_sub(lhs.into_int_value(), rhs.into_int_value(), "subtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Multiply => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_mul(lhs.into_float_value(), rhs.into_float_value(), "multmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_mul(lhs.into_int_value(), rhs.into_int_value(), "multmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Divide => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_div(lhs.into_float_value(), rhs.into_float_value(), "divtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned integer division
                    Ok(self
                        .builder
                        .build_int_unsigned_div(
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "divtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed integer division
                    Ok(self
                        .builder
                        .build_int_signed_div(lhs.into_int_value(), rhs.into_int_value(), "divtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Modulo => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_rem(lhs.into_float_value(), rhs.into_float_value(), "modtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned integer modulo
                    Ok(self
                        .builder
                        .build_int_unsigned_rem(
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "modtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed integer modulo
                    Ok(self
                        .builder
                        .build_int_signed_rem(lhs.into_int_value(), rhs.into_int_value(), "modtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }

            // Comparison operators
            BinaryOp::Equal => {
                if matches!(left_ty, Type::String) {
                    Ok(self.codegen_string_eq(lhs, rhs)?.into())
                } else if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OEQ,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "eqtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::EQ,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "eqtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::NotEqual => {
                if matches!(left_ty, Type::String) {
                    let eq = self.codegen_string_eq(lhs, rhs)?;
                    Ok(self
                        .builder
                        .build_not(eq, "str_ne")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::ONE,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "netmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::NE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "netmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Less => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OLT,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "lttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned less than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::ULT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "lttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed less than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SLT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "lttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Greater => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OGT,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "gttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned greater than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::UGT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "gttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed greater than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SGT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "gttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::LessEqual => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OLE,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "letmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned less than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::ULE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "letmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed less than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SLE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "letmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::GreaterEqual => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OGE,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "getmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned greater than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::UGE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "getmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed greater than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SGE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "getmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }

            // Logical operators (short-circuit evaluation would require basic blocks, using simple AND/OR for Phase 1)
            BinaryOp::And => Ok(self
                .builder
                .build_and(lhs.into_int_value(), rhs.into_int_value(), "andtmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
            BinaryOp::Or => Ok(self
                .builder
                .build_or(lhs.into_int_value(), rhs.into_int_value(), "ortmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
        }
    }

    /// Generate code for a unary expression
    fn codegen_unary(
        &self,
        op: UnaryOp,
        operand: &Expr,
        operand_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let val = self.codegen_expr(operand)?;

        match op {
            UnaryOp::Negate => {
                if TypeMapper::is_float_type(operand_ty) {
                    Ok(self
                        .builder
                        .build_float_neg(val.into_float_value(), "negtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_neg(val.into_int_value(), "negtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            UnaryOp::Not => Ok(self
                .builder
                .build_not(val.into_int_value(), "nottmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
        }
    }

    /// Generate code for a function call
    fn codegen_call(&self, func_name: &str, args: &[Expr]) -> CodegenResult<BasicValueEnum<'ctx>> {
        let function = self
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
            .build_call(*function, &arg_values, "calltmp")
            .map_err(|e| CodegenError::LlvmError(format!("failed to build call: {}", e)))?;

        call_result.try_as_basic_value().basic().ok_or_else(|| {
            CodegenError::InternalError(
                "function call returned void when value expected".to_string(),
            )
        })
    }

    /// Call a method: load the receiver as a value and prepend it to the argument list.
    ///
    /// For `&self` methods the struct is passed by value — this is sound because
    /// `&self` methods are read-only; mutations inside the method body do not
    /// propagate back to the caller (ownership semantics are pending).
    fn codegen_method_call(
        &self,
        mangled_name: &str,
        receiver: &Expr,
        args: &[Expr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let function = self
            .functions
            .get(mangled_name)
            .ok_or_else(|| CodegenError::UndefinedFunction(mangled_name.to_string()))?;

        // Load the receiver struct value.
        let self_val = self.codegen_expr(receiver)?;
        let mut arg_values: Vec<BasicMetadataValueEnum> =
            vec![BasicMetadataValueEnum::from(self_val)];

        for arg in args {
            let val = self.codegen_expr(arg)?;
            arg_values.push(BasicMetadataValueEnum::from(val));
        }

        let call_result = self
            .builder
            .build_call(*function, &arg_values, "calltmp")
            .map_err(|e| CodegenError::LlvmError(format!("failed to build method call: {}", e)))?;

        call_result.try_as_basic_value().basic().ok_or_else(|| {
            CodegenError::InternalError("method call returned void when value expected".to_string())
        })
    }

    /// Generate LLVM functions for all supported methods in an `impl` block.
    pub(crate) fn codegen_impl(
        &mut self,
        impl_def: &ImplDef,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        let struct_name = &impl_def.type_name.name;
        for method in &impl_def.methods {
            if matches!(
                method.self_param,
                Some(SelfParam::RefMut) | Some(SelfParam::Owned)
            ) {
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
    fn codegen_method(
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

        let mut llvm_param_types = Vec::new();
        for param_ty in param_types {
            let llvm_ty = if let Type::Struct(name) = param_ty {
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
            // Allocate and store the `self` struct value.
            let self_val = function
                .get_nth_param(0)
                .ok_or_else(|| CodegenError::InternalError("missing self parameter".to_string()))?;
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

        let has_implicit_return = !matches!(return_type, Type::Void)
            && !method.body.is_empty()
            && matches!(method.body.last(), Some(Stmt::Expr(_)));

        if has_implicit_return {
            for stmt in &method.body[..method.body.len() - 1] {
                self.codegen_stmt(stmt)?;
            }
            if let Some(Stmt::Expr(expr)) = method.body.last() {
                let ret_val = self.codegen_expr(expr)?;
                self.builder
                    .build_return(Some(&ret_val))
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            }
        } else {
            for stmt in &method.body {
                self.codegen_stmt(stmt)?;
            }
            if !matches!(return_type, Type::Void) {
                let current_bb = self.builder.get_insert_block().ok_or_else(|| {
                    CodegenError::InternalError("no insert block after method body".to_string())
                })?;
                if current_bb.get_terminator().is_none() {
                    return Err(CodegenError::MissingReturn);
                }
            } else if let Some(current_bb) = self.builder.get_insert_block() {
                if current_bb.get_terminator().is_none() {
                    self.builder
                        .build_return(None)
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                }
            }
        }

        Ok(())
    }

    /// Build a struct aggregate value from a struct literal expression.
    fn codegen_struct_literal(
        &self,
        name: &str,
        fields: &[FieldInit],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let llvm_ty = self.get_struct_llvm_type(name)?;
        let def = self
            .struct_defs
            .get(name)
            .ok_or_else(|| CodegenError::UnsupportedType(format!("unknown struct '{}'", name)))?;

        let mut agg = llvm_ty.get_undef();
        for field_init in fields {
            let idx = def
                .iter()
                .position(|(n, _)| n == &field_init.name.name)
                .ok_or_else(|| {
                    CodegenError::InternalError(format!(
                        "struct '{}' has no field '{}'",
                        name, field_init.name.name
                    ))
                })?;
            let val = self.codegen_expr(&field_init.value)?;
            agg = self
                .builder
                .build_insert_value(
                    agg,
                    val,
                    idx as u32,
                    &format!("{}.{}", name, field_init.name.name),
                )
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_struct_value();
        }
        Ok(agg.into())
    }

    /// Load a single field from a struct variable.
    fn codegen_field_access(
        &self,
        object: &Expr,
        field_name: &str,
        struct_name: &str,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let (ptr, llvm_ty) = self.get_struct_ptr_and_type(object, struct_name)?;

        let def = self.struct_defs.get(struct_name).ok_or_else(|| {
            CodegenError::UnsupportedType(format!("unknown struct '{}'", struct_name))
        })?;
        let (idx, (_, field_ty)) = def
            .iter()
            .enumerate()
            .find(|(_, (n, _))| n == field_name)
            .ok_or_else(|| {
                CodegenError::InternalError(format!(
                    "struct '{}' has no field '{}'",
                    struct_name, field_name
                ))
            })?;

        let field_ptr = self
            .builder
            .build_struct_gep(llvm_ty, ptr, idx as u32, &format!("{}.ptr", field_name))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let llvm_field_ty = self.type_mapper.map_type(field_ty)?;
        self.builder
            .build_load(llvm_field_ty, field_ptr, field_name)
            .map_err(|e| CodegenError::LlvmError(format!("failed to load field: {}", e)))
    }

    /// Store a value into a field of a named struct variable.
    fn codegen_field_assignment(
        &self,
        object_name: &str,
        field_name: &str,
        value: &Expr,
    ) -> CodegenResult<()> {
        let ptr = self
            .variables
            .get(object_name)
            .copied()
            .ok_or_else(|| CodegenError::UndefinedVariable(object_name.to_string()))?;

        let struct_ty = self
            .type_env
            .get(object_name)
            .ok_or_else(|| {
                CodegenError::InternalError(format!("no type for variable '{}'", object_name))
            })?
            .clone();

        let struct_name = match struct_ty {
            Type::Struct(ref n) => n.clone(),
            _ => {
                return Err(CodegenError::UnsupportedType(format!(
                    "'{}' is not a struct",
                    object_name
                )))
            }
        };

        let llvm_struct_ty = self.get_struct_llvm_type(&struct_name)?;
        let def = self.struct_defs.get(&struct_name).ok_or_else(|| {
            CodegenError::UnsupportedType(format!("unknown struct '{}'", struct_name))
        })?;

        let idx = def
            .iter()
            .position(|(n, _)| n == field_name)
            .ok_or_else(|| {
                CodegenError::InternalError(format!(
                    "struct '{}' has no field '{}'",
                    struct_name, field_name
                ))
            })?;

        let field_ptr = self
            .builder
            .build_struct_gep(
                llvm_struct_ty,
                ptr,
                idx as u32,
                &format!("{}.ptr", field_name),
            )
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;

        let val = self.codegen_expr(value)?;
        self.builder
            .build_store(field_ptr, val)
            .map_err(|e| CodegenError::LlvmError(format!("failed to store field: {}", e)))?;
        Ok(())
    }

    /// Get the alloca pointer and LLVM struct type for a struct object expression.
    /// Only simple identifier objects are supported (no chained access).
    fn get_struct_ptr_and_type(
        &self,
        object: &Expr,
        struct_name: &str,
    ) -> CodegenResult<(PointerValue<'ctx>, StructType<'ctx>)> {
        match object {
            Expr::Identifier(ident) => {
                let ptr = self
                    .variables
                    .get(&ident.name)
                    .copied()
                    .ok_or_else(|| CodegenError::UndefinedVariable(ident.name.clone()))?;
                let llvm_ty = self.get_struct_llvm_type(struct_name)?;
                Ok((ptr, llvm_ty))
            }
            _ => Err(CodegenError::UnsupportedType(
                "chained field access is not yet supported".to_string(),
            )),
        }
    }

    /// Generate code for an expression
    fn codegen_expr(&self, expr: &Expr) -> CodegenResult<BasicValueEnum<'ctx>> {
        match expr {
            Expr::Literal(lit, _) => self.codegen_literal(lit),
            Expr::Identifier(ident) => self.codegen_identifier(&ident.name),
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                // The left-operand type is stored at span.start + 1 by visit_expr_for_types.
                // span.start holds the result type (e.g. Bool for comparisons), which is not
                // what codegen_binary needs when dispatching on the operand kind.
                let left_ty = self.expr_types.get(&(span.start + 1)).ok_or_else(|| {
                    CodegenError::InternalError(
                        "missing type information for expression".to_string(),
                    )
                })?;
                self.codegen_binary(left, *op, right, left_ty)
            }
            Expr::Unary { op, operand, span } => {
                let operand_ty = self.expr_types.get(&span.start).ok_or_else(|| {
                    CodegenError::InternalError(
                        "missing type information for expression".to_string(),
                    )
                })?;
                self.codegen_unary(*op, operand, operand_ty)
            }
            Expr::Call { func, args, span } => {
                match &**func {
                    Expr::Identifier(ident) => self.codegen_call(&ident.name, args),

                    // Method call: `instance.method(args)` — pass self as first arg
                    Expr::FieldAccess { field, .. } => {
                        let struct_name = self
                            .fa_struct_names
                            .get(&span.start)
                            .ok_or_else(|| {
                                CodegenError::InternalError(
                                    "missing struct name for method call".to_string(),
                                )
                            })?
                            .clone();
                        let mangled = format!("{}__{}", struct_name, field.name);
                        // `object` is the FieldAccess's own object sub-expression.
                        // Extract it from the func expression.
                        let object = if let Expr::FieldAccess { object, .. } = &**func {
                            object
                        } else {
                            unreachable!()
                        };
                        self.codegen_method_call(&mangled, object, args)
                    }

                    // Associated function call: `TypeName::func(args)`
                    Expr::Path {
                        type_name, member, ..
                    } => {
                        let mangled = format!("{}__{}", type_name.name, member.name);
                        self.codegen_call(&mangled, args)
                    }

                    _ => Err(CodegenError::UnsupportedType(
                        "unsupported call expression".to_string(),
                    )),
                }
            }

            Expr::Path { .. } => {
                // A path expression used outside of a call position has no value
                // representation at runtime; semantic analysis should have caught this.
                Err(CodegenError::InternalError(
                    "path expression used outside of call position".to_string(),
                ))
            }
            Expr::Paren(inner, _) => self.codegen_expr(inner),

            Expr::StructLiteral { name, fields, .. } => {
                self.codegen_struct_literal(&name.name, fields)
            }

            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                // The struct name for this field access was stored in fa_struct_names during
                // type collection (keyed by the FieldAccess span.start). We cannot use
                // expr_types here because the FieldAccess and its first sub-expression
                // (the object Identifier) share the same span.start, and the later insert
                // of the field type overwrites the earlier insert of the struct type.
                let struct_name = self
                    .fa_struct_names
                    .get(&span.start)
                    .ok_or_else(|| {
                        CodegenError::InternalError(
                            "missing struct name for field access".to_string(),
                        )
                    })?
                    .clone();
                self.codegen_field_access(object, &field.name, &struct_name)
            }
        }
    }

    /// Generate code for a variable declaration statement
    fn codegen_var_decl(&mut self, name: &str, init: Option<&Expr>) -> CodegenResult<()> {
        // Get the type of the variable (we need to infer from initializer or explicit type)
        let init_val = if let Some(expr) = init {
            Some(self.codegen_expr(expr)?)
        } else {
            None
        };

        if let Some(val) = init_val {
            let val_type = val.get_type();

            // Allocate space on the stack
            let alloca = self.builder.build_alloca(val_type, name).map_err(|e| {
                CodegenError::LlvmError(format!("failed to allocate variable: {}", e))
            })?;

            // Store the initial value
            self.builder.build_store(alloca, val).map_err(|e| {
                CodegenError::LlvmError(format!("failed to store initial value: {}", e))
            })?;

            // Record the variable and its type
            self.variables.insert(name.to_string(), alloca);
            self.variable_types.insert(name.to_string(), val_type);
        }

        Ok(())
    }

    /// Generate code for an assignment statement
    fn codegen_assignment(&self, name: &str, value: &Expr) -> CodegenResult<()> {
        // Generate code for the value expression
        let val = self.codegen_expr(value)?;

        // Lookup the variable pointer (must already exist)
        let var_ptr = self
            .variables
            .get(name)
            .ok_or_else(|| CodegenError::UndefinedVariable(name.to_string()))?;

        // Store the new value into the variable
        self.builder.build_store(*var_ptr, val).map_err(|e| {
            CodegenError::LlvmError(format!("failed to store value in assignment: {}", e))
        })?;

        Ok(())
    }

    /// Generate code for a return statement
    fn codegen_return(&self, value: Option<&Expr>) -> CodegenResult<()> {
        if let Some(expr) = value {
            let ret_val = self.codegen_expr(expr)?;
            self.builder
                .build_return(Some(&ret_val))
                .map_err(|e| CodegenError::LlvmError(format!("failed to build return: {}", e)))?;
        } else {
            self.builder.build_return(None).map_err(|e| {
                CodegenError::LlvmError(format!("failed to build void return: {}", e))
            })?;
        }
        Ok(())
    }

    /// Generate code for an if/else statement
    fn codegen_if(
        &mut self,
        condition: &Expr,
        then_block: &[Stmt],
        else_if_blocks: &[(Expr, Vec<Stmt>)],
        else_block: &Option<Vec<Stmt>>,
    ) -> CodegenResult<()> {
        let cond_val = self.codegen_expr(condition)?;

        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("if statement outside function".to_string())
        })?;

        let then_bb = self.context.append_basic_block(parent_fn, "then");
        let else_bb = self.context.append_basic_block(parent_fn, "else");
        let merge_bb = self.context.append_basic_block(parent_fn, "ifcont");

        // Build conditional branch
        self.builder
            .build_conditional_branch(cond_val.into_int_value(), then_bb, else_bb)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to build conditional branch: {}", e))
            })?;

        // Generate then block
        self.builder.position_at_end(then_bb);
        for stmt in then_block {
            self.codegen_stmt(stmt)?;
        }
        // After nested control flow the builder may be positioned at a block that is NOT
        // then_bb (e.g. the merge block of an inner if).  Checking then_bb would miss that
        // case, so we check whichever block the builder currently occupies.
        if let Some(current_bb) = self.builder.get_insert_block() {
            if current_bb.get_terminator().is_none() {
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build branch: {}", e))
                    })?;
            }
        }

        // Generate else-if and else blocks.
        // Each else-if arm is the condition of the next level: the remaining arms and
        // the final else become the recursive else_if/else_block so they remain mutually
        // exclusive with the current arm.
        self.builder.position_at_end(else_bb);
        if let Some(((elif_cond, elif_stmts), rest)) = else_if_blocks.split_first() {
            self.codegen_if(elif_cond, elif_stmts, rest, else_block)?;
        } else if let Some(else_stmts) = else_block {
            for stmt in else_stmts {
                self.codegen_stmt(stmt)?;
            }
        }
        // Same: check current insert block, not the fixed else_bb, for the same reason.
        if let Some(current_bb) = self.builder.get_insert_block() {
            if current_bb.get_terminator().is_none() {
                self.builder
                    .build_unconditional_branch(merge_bb)
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build branch: {}", e))
                    })?;
            }
        }

        // Continue at merge block
        self.builder.position_at_end(merge_bb);

        Ok(())
    }

    /// Generate code for a while statement
    fn codegen_while(&mut self, condition: &Expr, body: &[Stmt]) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;

        let cond_bb = self.context.append_basic_block(parent_fn, "while.cond");
        let body_bb = self.context.append_basic_block(parent_fn, "while.body");
        let exit_bb = self.context.append_basic_block(parent_fn, "while.exit");

        let current_bb = self.builder.get_insert_block().ok_or_else(|| {
            CodegenError::InternalError("no insert block before while".to_string())
        })?;

        if current_bb.get_terminator().is_none() {
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
        }

        self.builder.position_at_end(cond_bb);
        let cond_val = self.codegen_expr(condition)?;
        self.builder
            .build_conditional_branch(cond_val.into_int_value(), body_bb, exit_bb)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to build conditional branch: {}", e))
            })?;

        self.builder.position_at_end(body_bb);
        self.loop_targets.push(LoopTargets {
            continue_bb: cond_bb,
            break_bb: exit_bb,
        });
        for stmt in body {
            if let Some(current_bb) = self.builder.get_insert_block() {
                if current_bb.get_terminator().is_some() {
                    break;
                }
            }
            self.codegen_stmt(stmt)?;
        }
        let _ = self.loop_targets.pop();

        if let Some(tail_bb) = self.builder.get_insert_block() {
            if tail_bb.get_terminator().is_none() {
                self.builder
                    .build_unconditional_branch(cond_bb)
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build branch: {}", e))
                    })?;
            }
        }

        self.builder.position_at_end(exit_bb);
        Ok(())
    }

    /// Generate code for a for-range statement (`for i in start..end { ... }`).
    fn codegen_for_range(
        &mut self,
        iterator: &shared_types::Identifier,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
        body: &[Stmt],
    ) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;

        let start_val = self.codegen_expr(start)?;
        let end_val = self.codegen_expr(end)?;
        let iter_name = iterator.name.clone();

        let iter_alloca = self
            .builder
            .build_alloca(start_val.get_type(), &iter_name)
            .map_err(|e| CodegenError::LlvmError(format!("failed to allocate iterator: {}", e)))?;
        self.builder
            .build_store(iter_alloca, start_val)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to initialize iterator: {}", e))
            })?;

        let previous_var = self.variables.insert(iter_name.clone(), iter_alloca);
        let previous_var_type = self
            .variable_types
            .insert(iter_name.clone(), start_val.get_type());

        let cond_bb = self.context.append_basic_block(parent_fn, "for.cond");
        let body_bb = self.context.append_basic_block(parent_fn, "for.body");
        let step_bb = self.context.append_basic_block(parent_fn, "for.step");
        let exit_bb = self.context.append_basic_block(parent_fn, "for.exit");

        let current_bb = self.builder.get_insert_block().ok_or_else(|| {
            CodegenError::InternalError("no insert block before for-range".to_string())
        })?;

        if current_bb.get_terminator().is_none() {
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
        }

        self.builder.position_at_end(cond_bb);
        let iter_val = self.codegen_identifier(&iter_name)?;
        let iter_int = iter_val.into_int_value();
        let end_int = end_val.into_int_value();

        let iter_sem_ty = self
            .expr_types
            .get(&start.span().start)
            .cloned()
            .unwrap_or(Type::I32);
        let cmp_predicate = match (TypeMapper::is_unsigned_int(&iter_sem_ty), inclusive) {
            (true, true) => IntPredicate::ULE,
            (true, false) => IntPredicate::ULT,
            (false, true) => IntPredicate::SLE,
            (false, false) => IntPredicate::SLT,
        };

        let cond_val = self
            .builder
            .build_int_compare(cmp_predicate, iter_int, end_int, "for.cond")
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to build for condition compare: {}", e))
            })?;
        self.builder
            .build_conditional_branch(cond_val, body_bb, exit_bb)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to build conditional branch: {}", e))
            })?;

        self.builder.position_at_end(body_bb);
        self.loop_targets.push(LoopTargets {
            continue_bb: step_bb,
            break_bb: exit_bb,
        });
        for stmt in body {
            if let Some(current_bb) = self.builder.get_insert_block() {
                if current_bb.get_terminator().is_some() {
                    break;
                }
            }
            self.codegen_stmt(stmt)?;
        }
        let _ = self.loop_targets.pop();

        if let Some(tail_bb) = self.builder.get_insert_block() {
            if tail_bb.get_terminator().is_none() {
                self.builder
                    .build_unconditional_branch(step_bb)
                    .map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build branch: {}", e))
                    })?;
            }
        }

        self.builder.position_at_end(step_bb);
        let current_iter = self.codegen_identifier(&iter_name)?.into_int_value();
        let one = current_iter.get_type().const_int(1, false);
        let next_iter = self
            .builder
            .build_int_add(current_iter, one, "for.next")
            .map_err(|e| CodegenError::LlvmError(format!("failed to increment iterator: {}", e)))?;
        self.builder
            .build_store(iter_alloca, next_iter)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to store incremented iterator: {}", e))
            })?;
        self.builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;

        self.builder.position_at_end(exit_bb);

        if let Some(previous) = previous_var {
            self.variables.insert(iter_name.clone(), previous);
        } else {
            self.variables.remove(&iter_name);
        }

        if let Some(previous_ty) = previous_var_type {
            self.variable_types.insert(iter_name.clone(), previous_ty);
        } else {
            self.variable_types.remove(&iter_name);
        }

        Ok(())
    }

    /// Generate code for a statement
    fn codegen_stmt(&mut self, stmt: &Stmt) -> CodegenResult<()> {
        match stmt {
            Stmt::VarDecl { name, init, .. } => self.codegen_var_decl(&name.name, init.as_ref()),
            Stmt::Assignment { target, value, .. } => self.codegen_assignment(&target.name, value),
            Stmt::Return { value, .. } => self.codegen_return(value.as_ref()),
            Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => self.codegen_if(condition, then_block, else_if_blocks, else_block),
            Stmt::While {
                condition, body, ..
            } => self.codegen_while(condition, body),
            Stmt::ForRange {
                iterator,
                start,
                end,
                inclusive,
                body,
                ..
            } => self.codegen_for_range(iterator, start, end, *inclusive, body),
            Stmt::Break { .. } => {
                let targets = self.loop_targets.last().ok_or_else(|| {
                    CodegenError::InternalError(
                        "break used outside loop during codegen".to_string(),
                    )
                })?;

                if let Some(current_bb) = self.builder.get_insert_block() {
                    if current_bb.get_terminator().is_none() {
                        self.builder
                            .build_unconditional_branch(targets.break_bb)
                            .map_err(|e| {
                                CodegenError::LlvmError(format!(
                                    "failed to build break branch: {}",
                                    e
                                ))
                            })?;
                    }
                }

                Ok(())
            }
            Stmt::Continue { .. } => {
                let targets = self.loop_targets.last().ok_or_else(|| {
                    CodegenError::InternalError(
                        "continue used outside loop during codegen".to_string(),
                    )
                })?;

                if let Some(current_bb) = self.builder.get_insert_block() {
                    if current_bb.get_terminator().is_none() {
                        self.builder
                            .build_unconditional_branch(targets.continue_bb)
                            .map_err(|e| {
                                CodegenError::LlvmError(format!(
                                    "failed to build continue branch: {}",
                                    e
                                ))
                            })?;
                    }
                }

                Ok(())
            }
            Stmt::FieldAssignment {
                object,
                field,
                value,
                ..
            } => self.codegen_field_assignment(&object.name, &field.name, value),

            Stmt::Expr(expr) => {
                self.codegen_expr(expr)?;
                Ok(())
            }
        }
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

        // Generate function body
        // Handle expression-based returns: if the last statement is an expression
        // and the function has a non-void return type, treat it as an implicit return
        let has_implicit_return = !matches!(return_type, Type::Void)
            && !func_def.body.is_empty()
            && matches!(func_def.body.last(), Some(Stmt::Expr(_)));

        if has_implicit_return {
            // Generate all statements except the last one
            for stmt in &func_def.body[..func_def.body.len() - 1] {
                self.codegen_stmt(stmt)?;
            }

            // Generate implicit return from the last expression
            if let Some(Stmt::Expr(expr)) = func_def.body.last() {
                let ret_val = self.codegen_expr(expr)?;
                self.builder.build_return(Some(&ret_val)).map_err(|e| {
                    CodegenError::LlvmError(format!("failed to build implicit return: {}", e))
                })?;
            }
        } else {
            // Generate all statements normally
            for stmt in &func_def.body {
                self.codegen_stmt(stmt)?;
            }

            // Ensure function has a return if it's non-void
            if !matches!(return_type, Type::Void) {
                let current_bb = self.builder.get_insert_block().ok_or_else(|| {
                    CodegenError::InternalError("no insert block after function body".to_string())
                })?;

                if current_bb.get_terminator().is_none() {
                    return Err(CodegenError::MissingReturn);
                }
            } else if let Some(current_bb) = self.builder.get_insert_block() {
                // Add void return if missing
                if current_bb.get_terminator().is_none() {
                    self.builder.build_return(None).map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build void return: {}", e))
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Store type information for expressions (needed for codegen)
    pub(crate) fn store_expr_types(
        &mut self,
        items: &[Item],
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        for item in items {
            match item {
                Item::Function(func_def) => {
                    self.visit_function_for_types(func_def, func_types)?;
                }
                Item::Impl(impl_def) => {
                    self.visit_impl_for_types(impl_def, func_types)?;
                }
                Item::Struct(_) => {}
            }
        }
        Ok(())
    }

    /// Walk an impl block's method bodies to populate the type-info maps.
    fn visit_impl_for_types(
        &mut self,
        impl_def: &ImplDef,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        let struct_name = &impl_def.type_name.name;
        for method in &impl_def.methods {
            if matches!(
                method.self_param,
                Some(SelfParam::RefMut) | Some(SelfParam::Owned)
            ) {
                continue;
            }
            self.visit_method_for_types(method, struct_name, func_types)?;
        }
        Ok(())
    }

    fn visit_method_for_types(
        &mut self,
        method: &MethodDef,
        struct_name: &str,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        let mangled = format!("{}__{}", struct_name, method.name.name);
        self.type_env.clear();

        let func_type_info = func_types
            .get(&mangled)
            .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;

        let param_types = match func_type_info {
            Type::Function { params, .. } => params,
            _ => {
                return Err(CodegenError::InternalError(
                    "method type is not a function type".to_string(),
                ))
            }
        };

        // param_types[0] is `self` for instance methods; bind it in type_env.
        if method.self_param.is_some() {
            self.type_env
                .insert("self".to_string(), Type::Struct(struct_name.to_string()));
        }

        let non_self_start = if method.self_param.is_some() { 1 } else { 0 };
        for (i, param) in method.params.iter().enumerate() {
            if let Some(ty) = param_types.get(non_self_start + i) {
                self.type_env.insert(param.name.name.clone(), ty.clone());
            }
        }

        for stmt in &method.body {
            self.visit_stmt_for_types(stmt, func_types)?;
        }
        Ok(())
    }

    fn visit_function_for_types(
        &mut self,
        func_def: &FunctionDef,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        // Clear type environment for new function
        self.type_env.clear();

        // Populate type environment with parameter types
        let func_type_info = func_types
            .get(&func_def.name.name)
            .ok_or_else(|| CodegenError::UndefinedFunction(func_def.name.name.clone()))?;

        let param_types = match func_type_info {
            Type::Function { params, .. } => params,
            _ => {
                return Err(CodegenError::InternalError(
                    "function type information is not a function type".to_string(),
                ))
            }
        };

        for (i, param) in func_def.params.iter().enumerate() {
            let param_ty = param_types.get(i).ok_or_else(|| {
                CodegenError::InternalError(format!("missing type for parameter {}", i))
            })?;
            self.type_env
                .insert(param.name.name.clone(), param_ty.clone());
        }

        for stmt in &func_def.body {
            self.visit_stmt_for_types(stmt, func_types)?;
        }
        Ok(())
    }

    fn visit_stmt_for_types(
        &mut self,
        stmt: &Stmt,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        match stmt {
            Stmt::VarDecl { name, init, .. } => {
                if let Some(expr) = init {
                    self.visit_expr_for_types(expr, func_types)?;
                    // Get the type of the initializer and store it for this variable
                    let var_ty = self.expr_types.get(&expr.span().start).ok_or_else(|| {
                        CodegenError::InternalError(
                            "missing type for variable initializer".to_string(),
                        )
                    })?;
                    self.type_env.insert(name.name.clone(), var_ty.clone());
                }
            }
            Stmt::Assignment { value, .. } => {
                // Visit the value expression to collect its type
                self.visit_expr_for_types(value, func_types)?;
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    self.visit_expr_for_types(expr, func_types)?;
                }
            }
            Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                self.visit_expr_for_types(condition, func_types)?;
                for stmt in then_block {
                    self.visit_stmt_for_types(stmt, func_types)?;
                }
                for (cond, stmts) in else_if_blocks {
                    self.visit_expr_for_types(cond, func_types)?;
                    for stmt in stmts {
                        self.visit_stmt_for_types(stmt, func_types)?;
                    }
                }
                if let Some(stmts) = else_block {
                    for stmt in stmts {
                        self.visit_stmt_for_types(stmt, func_types)?;
                    }
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.visit_expr_for_types(condition, func_types)?;
                for stmt in body {
                    self.visit_stmt_for_types(stmt, func_types)?;
                }
            }
            Stmt::ForRange {
                iterator,
                start,
                end,
                inclusive: _,
                body,
                ..
            } => {
                self.visit_expr_for_types(start, func_types)?;
                self.visit_expr_for_types(end, func_types)?;

                if let Some(iterator_ty) = self.expr_types.get(&start.span().start).cloned() {
                    self.type_env.insert(iterator.name.clone(), iterator_ty);
                }

                for stmt in body {
                    self.visit_stmt_for_types(stmt, func_types)?;
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::FieldAssignment { value, object, .. } => {
                self.visit_expr_for_types(value, func_types)?;
                // Ensure the object variable is in type_env so field access codegen can resolve it
                if !self.type_env.contains_key(&object.name) {
                    if let Some(ty) = self.expr_types.get(&object.span.start).cloned() {
                        self.type_env.insert(object.name.clone(), ty);
                    }
                }
            }
            Stmt::Expr(expr) => {
                self.visit_expr_for_types(expr, func_types)?;
            }
        }
        Ok(())
    }

    fn visit_expr_for_types(
        &mut self,
        expr: &Expr,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        match expr {
            Expr::Literal(lit, span) => {
                let ty = match lit {
                    shared_types::Literal::Integer(_) => Type::I32,
                    shared_types::Literal::Float(_) => Type::F64,
                    shared_types::Literal::Boolean(_) => Type::Bool,
                    shared_types::Literal::String(_) => Type::String,
                };
                self.expr_types.insert(span.start, ty);
            }
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                self.visit_expr_for_types(left, func_types)?;
                self.visit_expr_for_types(right, func_types)?;

                // Infer the result type from the operator and left operand type
                let left_ty = self
                    .expr_types
                    .get(&left.span().start)
                    .ok_or_else(|| {
                        CodegenError::InternalError("missing type for left operand".to_string())
                    })?
                    .clone();

                let result_ty = match op {
                    BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Modulo => left_ty.clone(),
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::Greater
                    | BinaryOp::LessEqual
                    | BinaryOp::GreaterEqual
                    | BinaryOp::And
                    | BinaryOp::Or => Type::Bool,
                };

                self.expr_types.insert(span.start, result_ty);
                // Store left type for binary codegen
                self.expr_types.insert(span.start + 1, left_ty);
            }
            Expr::Unary { operand, span, .. } => {
                self.visit_expr_for_types(operand, func_types)?;
                let operand_ty = self
                    .expr_types
                    .get(&operand.span().start)
                    .ok_or_else(|| {
                        CodegenError::InternalError("missing type for operand".to_string())
                    })?
                    .clone();
                self.expr_types.insert(span.start, operand_ty.clone());
                self.expr_types.insert(span.start + 1, operand_ty);
            }
            Expr::Call { func, args, span } => {
                for arg in args {
                    self.visit_expr_for_types(arg, func_types)?;
                }

                match &**func {
                    Expr::Identifier(ident) => {
                        let func_type = func_types
                            .get(&ident.name)
                            .ok_or_else(|| CodegenError::UndefinedFunction(ident.name.clone()))?;
                        let ret_ty = match func_type {
                            Type::Function { ret, .. } => &**ret,
                            _ => {
                                return Err(CodegenError::InternalError(
                                    "called object is not a function".to_string(),
                                ))
                            }
                        };
                        self.expr_types.insert(span.start, ret_ty.clone());
                    }

                    // Method call: `instance.method(args)`
                    Expr::FieldAccess { object, field, .. } => {
                        self.visit_expr_for_types(object, func_types)?;
                        let struct_name = match self.expr_types.get(&object.span().start).cloned() {
                            Some(Type::Struct(n)) => n,
                            _ => {
                                return Err(CodegenError::InternalError(
                                    "method call on non-struct type during type collection"
                                        .to_string(),
                                ))
                            }
                        };
                        let mangled = format!("{}__{}", struct_name, field.name);
                        let func_type = func_types
                            .get(&mangled)
                            .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;
                        let ret_ty = match func_type {
                            Type::Function { ret, .. } => (**ret).clone(),
                            _ => {
                                return Err(CodegenError::InternalError(
                                    "method type is not a function".to_string(),
                                ))
                            }
                        };
                        self.expr_types.insert(span.start, ret_ty);
                        // Store struct name so codegen_expr can reconstruct the mangled name.
                        self.fa_struct_names.insert(span.start, struct_name);
                    }

                    // Associated function call: `TypeName::func(args)`
                    Expr::Path {
                        type_name, member, ..
                    } => {
                        let mangled = format!("{}__{}", type_name.name, member.name);
                        let func_type = func_types
                            .get(&mangled)
                            .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;
                        let ret_ty = match func_type {
                            Type::Function { ret, .. } => (**ret).clone(),
                            _ => {
                                return Err(CodegenError::InternalError(
                                    "associated function type is not a function".to_string(),
                                ))
                            }
                        };
                        self.expr_types.insert(span.start, ret_ty);
                    }

                    _ => {}
                }
            }

            Expr::Path {
                type_name,
                member,
                span,
            } => {
                let mangled = format!("{}__{}", type_name.name, member.name);
                if let Some(Type::Function { ret, .. }) = func_types.get(&mangled) {
                    self.expr_types.insert(span.start, (**ret).clone());
                }
            }
            Expr::Paren(inner, span) => {
                self.visit_expr_for_types(inner, func_types)?;
                // Parenthesized expressions have the same type as their inner expression
                let inner_ty = self.expr_types.get(&inner.span().start).ok_or_else(|| {
                    CodegenError::InternalError(
                        "missing type for parenthesized expression".to_string(),
                    )
                })?;
                self.expr_types.insert(span.start, inner_ty.clone());
            }
            Expr::Identifier(ident) => {
                // Look up the type from the type environment
                let ty = self
                    .type_env
                    .get(&ident.name)
                    .ok_or_else(|| CodegenError::UndefinedVariable(ident.name.clone()))?;
                self.expr_types.insert(ident.span.start, ty.clone());
            }

            Expr::StructLiteral { name, fields, span } => {
                for field_init in fields {
                    self.visit_expr_for_types(&field_init.value, func_types)?;
                }
                self.expr_types
                    .insert(span.start, Type::Struct(name.name.clone()));
            }

            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                self.visit_expr_for_types(object, func_types)?;

                let struct_name = match self.expr_types.get(&object.span().start).cloned() {
                    Some(Type::Struct(n)) => n,
                    _ => {
                        return Err(CodegenError::InternalError(
                            "field access on non-struct type during type collection".to_string(),
                        ))
                    }
                };

                // Store the struct name keyed by the FieldAccess span so codegen_expr can
                // retrieve it without colliding with the object Identifier's expr_types entry
                // (both share the same span.start when the object is a bare identifier).
                self.fa_struct_names.insert(span.start, struct_name.clone());

                let field_ty = self
                    .struct_defs
                    .get(&struct_name)
                    .and_then(|def| {
                        def.iter()
                            .find(|(n, _)| n == &field.name)
                            .map(|(_, ty)| ty.clone())
                    })
                    .ok_or_else(|| {
                        CodegenError::InternalError(format!(
                            "unknown field '{}' on struct '{}'",
                            field.name, struct_name
                        ))
                    })?;

                self.expr_types.insert(span.start, field_ty);
            }
        }
        Ok(())
    }
}
