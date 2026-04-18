use ast_types::*;
use inkwell::values::*;
use inkwell::{FloatPredicate, IntPredicate};

use crate::errors::{CodegenError, CodegenResult};
use crate::type_mapping::TypeMapper;
use crate::types::Type;

use super::context::CodegenContext;

/// Rust-level representation of a folded constant value.
///
/// Arithmetic is performed in Rust (wrapping for integers, IEEE-754 for floats)
/// so we never need inkwell's `const_*` arithmetic methods, which have an
/// inconsistent availability across feature configurations.
#[derive(Clone, Debug)]
enum FoldedConst {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

impl FoldedConst {
    fn from_literal(lit: &shared_types::Literal) -> Self {
        match lit {
            shared_types::Literal::Integer(v) => FoldedConst::Int(*v),
            shared_types::Literal::Float(v) => FoldedConst::Float(*v),
            shared_types::Literal::Boolean(v) => FoldedConst::Bool(*v),
            shared_types::Literal::String(s) => FoldedConst::Str(s.clone()),
        }
    }

    fn from_llvm(bv: BasicValueEnum<'_>) -> CodegenResult<Self> {
        match bv {
            BasicValueEnum::IntValue(i) => {
                // LLVM stores booleans as i1 integers; anything else is a general int.
                if i.get_type().get_bit_width() == 1 {
                    Ok(FoldedConst::Bool(i.get_zero_extended_constant() != Some(0)))
                } else {
                    Ok(FoldedConst::Int(
                        i.get_sign_extended_constant().unwrap_or(0),
                    ))
                }
            }
            BasicValueEnum::FloatValue(f) => Ok(FoldedConst::Float(
                f.get_constant().map(|(v, _)| v).unwrap_or(0.0),
            )),
            BasicValueEnum::StructValue(_) => Err(CodegenError::InternalError(
                "cannot reconstruct string const for nested evaluation".into(),
            )),
            _ => Err(CodegenError::InternalError(
                "unexpected LLVM value kind in const context".into(),
            )),
        }
    }

    fn cast_to(self, target: &Type) -> Self {
        match (self, target) {
            (FoldedConst::Int(i), t) if t.is_integer() => FoldedConst::Int(i),
            (FoldedConst::Int(i), t) if t.is_float() => FoldedConst::Float(i as f64),
            (FoldedConst::Float(f), t) if t.is_integer() => FoldedConst::Int(f as i64),
            (FoldedConst::Float(f), t) if t.is_float() => FoldedConst::Float(f),
            (FoldedConst::Bool(b), t) if t.is_integer() => FoldedConst::Int(b as i64),
            (v, _) => v,
        }
    }
}

impl<'ctx> CodegenContext<'ctx> {
    /// Generate code for a literal expression
    pub(crate) fn codegen_literal(
        &self,
        lit: &shared_types::Literal,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
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

    /// Generate code for an identifier (variable reference).
    ///
    /// Checks `const_values` first so a local variable can shadow a same-named constant.
    pub(crate) fn codegen_identifier(&self, name: &str) -> CodegenResult<BasicValueEnum<'ctx>> {
        if let Some(val) = self.const_values.get(name) {
            return Ok(*val);
        }

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

    /// Evaluate a constant expression to a compile-time LLVM value.
    ///
    /// Only valid for expressions that passed `is_const_expr` in semantic analysis.
    /// Folds the expression in Rust first, then creates an LLVM constant value.
    pub(crate) fn codegen_const_expr(
        &self,
        expr: &ast_types::Expr,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let folded = Self::fold_const(expr, &self.const_values)?;
        Ok(self.const_folded_to_llvm(&folded))
    }

    /// Rust-level constant folder. Returns a `FoldedConst` scalar.
    fn fold_const(
        expr: &ast_types::Expr,
        consts: &std::collections::HashMap<String, BasicValueEnum<'_>>,
    ) -> CodegenResult<FoldedConst> {
        match expr {
            ast_types::Expr::Literal(lit, _) => Ok(FoldedConst::from_literal(lit)),
            ast_types::Expr::Paren(inner, _) => Self::fold_const(inner, consts),
            ast_types::Expr::Unary { op, operand, .. } => {
                let v = Self::fold_const(operand, consts)?;
                match op {
                    ast_types::UnaryOp::Negate => match v {
                        FoldedConst::Int(i) => Ok(FoldedConst::Int(i.wrapping_neg())),
                        FoldedConst::Float(f) => Ok(FoldedConst::Float(-f)),
                        _ => Err(CodegenError::InternalError(
                            "negate on non-numeric const".into(),
                        )),
                    },
                    ast_types::UnaryOp::Not => match v {
                        FoldedConst::Bool(b) => Ok(FoldedConst::Bool(!b)),
                        _ => Err(CodegenError::InternalError("not on non-bool const".into())),
                    },
                    ast_types::UnaryOp::BitNot => match v {
                        FoldedConst::Int(i) => Ok(FoldedConst::Int(!i)),
                        _ => Err(CodegenError::InternalError(
                            "bitnot on non-integer const".into(),
                        )),
                    },
                }
            }
            ast_types::Expr::Binary {
                left, op, right, ..
            } => {
                let l = Self::fold_const(left, consts)?;
                let r = Self::fold_const(right, consts)?;
                use ast_types::BinaryOp;
                match (l, r) {
                    (FoldedConst::Int(a), FoldedConst::Int(b)) => match op {
                        BinaryOp::Add => Ok(FoldedConst::Int(a.wrapping_add(b))),
                        BinaryOp::Subtract => Ok(FoldedConst::Int(a.wrapping_sub(b))),
                        BinaryOp::Multiply => Ok(FoldedConst::Int(a.wrapping_mul(b))),
                        BinaryOp::Divide => {
                            if b == 0 {
                                Err(CodegenError::InternalError("const division by zero".into()))
                            } else {
                                Ok(FoldedConst::Int(a.wrapping_div(b)))
                            }
                        }
                        BinaryOp::Modulo => {
                            if b == 0 {
                                Err(CodegenError::InternalError(
                                    "const remainder by zero".into(),
                                ))
                            } else {
                                Ok(FoldedConst::Int(a.wrapping_rem(b)))
                            }
                        }
                        BinaryOp::Equal => Ok(FoldedConst::Bool(a == b)),
                        BinaryOp::NotEqual => Ok(FoldedConst::Bool(a != b)),
                        BinaryOp::Less => Ok(FoldedConst::Bool(a < b)),
                        BinaryOp::Greater => Ok(FoldedConst::Bool(a > b)),
                        BinaryOp::LessEqual => Ok(FoldedConst::Bool(a <= b)),
                        BinaryOp::GreaterEqual => Ok(FoldedConst::Bool(a >= b)),
                        BinaryOp::And => Ok(FoldedConst::Bool(a != 0 && b != 0)),
                        BinaryOp::Or => Ok(FoldedConst::Bool(a != 0 || b != 0)),
                        BinaryOp::BitAnd => Ok(FoldedConst::Int(a & b)),
                        BinaryOp::BitOr => Ok(FoldedConst::Int(a | b)),
                        BinaryOp::BitXor => Ok(FoldedConst::Int(a ^ b)),
                        BinaryOp::Shl => Ok(FoldedConst::Int(a.wrapping_shl(b as u32))),
                    },
                    (FoldedConst::Float(a), FoldedConst::Float(b)) => match op {
                        BinaryOp::Add => Ok(FoldedConst::Float(a + b)),
                        BinaryOp::Subtract => Ok(FoldedConst::Float(a - b)),
                        BinaryOp::Multiply => Ok(FoldedConst::Float(a * b)),
                        BinaryOp::Divide => Ok(FoldedConst::Float(a / b)),
                        BinaryOp::Modulo => Ok(FoldedConst::Float(a % b)),
                        BinaryOp::Equal => Ok(FoldedConst::Bool(a == b)),
                        BinaryOp::NotEqual => Ok(FoldedConst::Bool(a != b)),
                        BinaryOp::Less => Ok(FoldedConst::Bool(a < b)),
                        BinaryOp::Greater => Ok(FoldedConst::Bool(a > b)),
                        BinaryOp::LessEqual => Ok(FoldedConst::Bool(a <= b)),
                        BinaryOp::GreaterEqual => Ok(FoldedConst::Bool(a >= b)),
                        _ => Err(CodegenError::InternalError(
                            "unsupported binary op on float const".into(),
                        )),
                    },
                    _ => Err(CodegenError::InternalError(
                        "type mismatch in const binary expression".into(),
                    )),
                }
            }
            ast_types::Expr::Cast {
                expr: inner,
                target_type,
                ..
            } => {
                let v = Self::fold_const(inner, consts)?;
                let target = crate::types::Type::from_ast(target_type);
                Ok(v.cast_to(&target))
            }
            ast_types::Expr::Identifier(ident) => {
                // Reconstruct FoldedConst from an already-emitted LLVM const value.
                let bv = consts
                    .get(&ident.name)
                    .copied()
                    .ok_or_else(|| CodegenError::UndefinedVariable(ident.name.clone()))?;
                FoldedConst::from_llvm(bv)
            }
            _ => Err(CodegenError::InternalError(
                "non-constant expression in const context".into(),
            )),
        }
    }

    fn const_folded_to_llvm(&self, v: &FoldedConst) -> BasicValueEnum<'ctx> {
        match v {
            FoldedConst::Int(i) => self.context.i32_type().const_int(*i as u64, true).into(),
            FoldedConst::Float(f) => self.context.f64_type().const_float(*f).into(),
            FoldedConst::Bool(b) => self.context.bool_type().const_int(*b as u64, false).into(),
            FoldedConst::Str(s) => {
                let bytes: Vec<_> = s
                    .bytes()
                    .chain(std::iter::once(0u8))
                    .map(|b| self.context.i8_type().const_int(b as u64, false))
                    .collect();
                let arr = self.context.i8_type().const_array(&bytes);
                let global = self.module.add_global(arr.get_type(), None, "str.data");
                global.set_initializer(&arr);
                global.set_constant(true);
                global.set_linkage(inkwell::module::Linkage::Private);
                let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                let len = self.context.i64_type().const_int(s.len() as u64, false);
                let fat_type = self
                    .context
                    .struct_type(&[ptr_type.into(), self.context.i64_type().into()], false);
                fat_type
                    .const_named_struct(&[global.as_pointer_value().into(), len.into()])
                    .into()
            }
        }
    }

    /// Compare two string fat-pointers for byte-level equality.
    ///
    /// Uses the length field for an O(1) short-circuit before falling back to
    /// `memcmp`. When lengths differ the length passed to `memcmp` is set to 0
    /// (memcmp with n=0 returns 0), so `len_eq` being false drives the final AND
    /// to false without reading out-of-bounds memory.
    pub(crate) fn codegen_string_eq(
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
    pub(crate) fn codegen_binary(
        &self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        left_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let lhs = self.codegen_expr(left)?;
        let rhs = self.codegen_expr(right)?;

        // Coerce both operands to the left-operand semantic type.  Literals always
        // emit at their default width (i32 / f64); when the expression context is
        // wider (e.g. `i64_var - 3000000000`) both sides must match before any
        // arithmetic or comparison instruction is emitted.  The coercion is a no-op
        // when the LLVM types already agree.
        let target_llvm = self.type_mapper.map_type(left_ty)?;
        let lhs = self.coerce_if_needed(lhs, target_llvm, left_ty)?;
        let rhs = self.coerce_if_needed(rhs, target_llvm, left_ty)?;

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

            // Bitwise operators
            BinaryOp::BitAnd => Ok(self
                .builder
                .build_and(lhs.into_int_value(), rhs.into_int_value(), "bandtmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
            BinaryOp::BitOr => Ok(self
                .builder
                .build_or(lhs.into_int_value(), rhs.into_int_value(), "bortmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
            BinaryOp::BitXor => Ok(self
                .builder
                .build_xor(lhs.into_int_value(), rhs.into_int_value(), "xortmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
            BinaryOp::Shl => Ok(self
                .builder
                .build_left_shift(lhs.into_int_value(), rhs.into_int_value(), "shltmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
        }
    }

    /// Generate code for a unary expression
    pub(crate) fn codegen_unary(
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
            UnaryOp::BitNot => Ok(self
                .builder
                .build_not(val.into_int_value(), "bnottmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
        }
    }

    /// Generate code for an expression
    pub(crate) fn codegen_expr(&self, expr: &Expr) -> CodegenResult<BasicValueEnum<'ctx>> {
        match expr {
            Expr::Cast {
                expr: inner_expr,
                target_type,
                span,
            } => {
                let llvm_ty = crate::types::Type::from_ast(target_type);
                self.codegen_cast(inner_expr, &llvm_ty, span)
            }
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

    /// Generate an `as` type cast cast from inner to target
    pub(crate) fn codegen_cast(
        &self,
        inner: &Expr,
        target_type: &crate::types::Type,
        span: &shared_types::Span,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let value = self.codegen_expr(inner)?;
        let inner_ty = self.expr_types.get(&(span.start + 1)).ok_or_else(|| {
            CodegenError::InternalError("missing type information for cast".to_string())
        })?;

        if inner_ty == target_type {
            return Ok(value);
        }

        let target_llvm = self.type_mapper.map_type(target_type)?;

        match (inner_ty, target_type) {
            // Bool to int
            (crate::types::Type::Bool, t2) if t2.is_integer() => {
                let int_value = value.into_int_value();
                Ok(self
                    .builder
                    .build_int_z_extend(int_value, target_llvm.into_int_type(), "cast_bool")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                    .into())
            }
            // Float to Int
            (t1, t2) if t1.is_float() && t2.is_integer() => {
                let float_value = value.into_float_value();
                if t2.is_unsigned_int() {
                    Ok(self
                        .builder
                        .build_float_to_unsigned_int(
                            float_value,
                            target_llvm.into_int_type(),
                            "cast_f2u",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_float_to_signed_int(
                            float_value,
                            target_llvm.into_int_type(),
                            "cast_f2s",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            // Int to Float
            (t1, t2) if t1.is_integer() && t2.is_float() => {
                let int_value = value.into_int_value();
                if t1.is_unsigned_int() {
                    Ok(self
                        .builder
                        .build_unsigned_int_to_float(
                            int_value,
                            target_llvm.into_float_type(),
                            "cast_u2f",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_signed_int_to_float(
                            int_value,
                            target_llvm.into_float_type(),
                            "cast_s2f",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            // Float to Float
            (t1, t2) if t1.is_float() && t2.is_float() => {
                let float_value = value.into_float_value();
                // F32 to F64 is Ext, F64 to F32 is Trunc
                // Assuming Type::F32 and Type::F64 only
                if matches!(t2, crate::types::Type::F64) {
                    Ok(self
                        .builder
                        .build_float_ext(float_value, target_llvm.into_float_type(), "cast_f2f")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_float_trunc(float_value, target_llvm.into_float_type(), "cast_f2f")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            // Int to Int
            (t1, t2) if t1.is_integer() && t2.is_integer() => {
                let int_value = value.into_int_value();
                let from_width = int_value.get_type().get_bit_width();
                let to_width = target_llvm.into_int_type().get_bit_width();

                if to_width > from_width {
                    if t1.is_unsigned_int() {
                        Ok(self
                            .builder
                            .build_int_z_extend(int_value, target_llvm.into_int_type(), "cast_ext")
                            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                            .into())
                    } else {
                        Ok(self
                            .builder
                            .build_int_s_extend(int_value, target_llvm.into_int_type(), "cast_ext")
                            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                            .into())
                    }
                } else if to_width < from_width {
                    Ok(self
                        .builder
                        .build_int_truncate(int_value, target_llvm.into_int_type(), "cast_trunc")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(value)
                }
            }
            _ => Err(CodegenError::InternalError(
                "Invalid cast reached backend".to_string(),
            )),
        }
    }
}
