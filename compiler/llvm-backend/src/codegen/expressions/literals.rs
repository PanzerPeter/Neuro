// NEURO Programming Language - LLVM Backend
// Codegen for expressions: Literals, identifiers, constant folding, and casts.

use ast_types::*;
use inkwell::values::*;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// Trailing byte appended to a string literal's `.rodata` storage so the pointer
/// doubles as a valid C string for FFI. It is deliberately **excluded** from the
/// fat pointer's `len` field: `len` is the UTF-8 byte count of the literal's
/// content, and consumers must treat `len` as authoritative rather than scanning
/// for this terminator — interior NUL bytes (`"a\0b"`) are legal content.
const STRING_NULL_TERMINATOR: u8 = 0;

enum FoldedConst {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

impl FoldedConst {
    fn from_literal(lit: &shared_types::Literal) -> Self {
        match lit {
            shared_types::Literal::Integer(v, _) => FoldedConst::Int(*v),
            shared_types::Literal::Float(v, _) => FoldedConst::Float(*v),
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
            shared_types::Literal::Integer(val, suffix_opt) => {
                use shared_types::IntSuffix;
                let llvm_ty = match suffix_opt {
                    None | Some(IntSuffix::I32) | Some(IntSuffix::U32) => self.context.i32_type(),
                    Some(IntSuffix::I8) | Some(IntSuffix::U8) => self.context.i8_type(),
                    Some(IntSuffix::I16) | Some(IntSuffix::U16) => self.context.i16_type(),
                    Some(IntSuffix::I64) | Some(IntSuffix::U64) => self.context.i64_type(),
                };
                Ok(llvm_ty.const_int(*val as u64, true).into())
            }
            shared_types::Literal::Float(val, suffix_opt) => {
                use shared_types::FloatSuffix;
                let llvm_ty = match suffix_opt {
                    Some(FloatSuffix::F32) => self.context.f32_type(),
                    None | Some(FloatSuffix::F64) => self.context.f64_type(),
                };
                Ok(llvm_ty.const_float(*val).into())
            }
            shared_types::Literal::Boolean(val) => Ok(self
                .context
                .bool_type()
                .const_int(*val as u64, false)
                .into()),
            shared_types::Literal::String(s) => {
                // Literals are not heap-allocated: the UTF-8 bytes live in `.rodata` for the
                // program's lifetime. LLVM appends the `STRING_NULL_TERMINATOR` automatically.
                let global_string =
                    self.builder
                        .build_global_string_ptr(s, "str")
                        .map_err(|e| {
                            CodegenError::LlvmError(format!(
                                "failed to create string constant: {}",
                                e
                            ))
                        })?;

                // `len` is the content's UTF-8 byte count and excludes the appended terminator;
                // it is the authoritative length (interior NULs included). See STRING_NULL_TERMINATOR.
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

    /// Like `codegen_const_expr` but emits the LLVM constant at the width/type
    /// specified by `declared_ty`, avoiding silent i32 truncation for i64/u8/etc.
    pub(crate) fn codegen_const_expr_typed(
        &self,
        expr: &ast_types::Expr,
        declared_ty: &crate::types::Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let folded = Self::fold_const(expr, &self.const_values)?;
        Ok(self.const_folded_to_llvm_typed(&folded, declared_ty))
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
                        BinaryOp::NullCoalesce => Err(CodegenError::InternalError(
                            "operator '??' is not valid in const expressions (Phase 2)".into(),
                        )),
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

    fn const_folded_to_llvm_typed(
        &self,
        v: &FoldedConst,
        ty: &crate::types::Type,
    ) -> BasicValueEnum<'ctx> {
        match v {
            FoldedConst::Int(i) => {
                let llvm_int = self.type_mapper.map_int_type(ty);
                llvm_int.const_int(*i as u64, !ty.is_unsigned_int()).into()
            }
            FoldedConst::Float(f) => match ty {
                crate::types::Type::F32 => self.context.f32_type().const_float(*f).into(),
                _ => self.context.f64_type().const_float(*f).into(),
            },
            _ => self.const_folded_to_llvm(v),
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
                    .chain(std::iter::once(STRING_NULL_TERMINATOR))
                    .map(|b| self.context.i8_type().const_int(b as u64, false))
                    .collect();
                let arr = self.context.i8_type().const_array(&bytes);
                let global = self.module.add_global(arr.get_type(), None, "str.data");
                global.set_initializer(&arr);
                global.set_constant(true);
                global.set_linkage(inkwell::module::Linkage::Private);
                let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                // Length excludes the appended terminator — identical contract to the
                // runtime-literal path above.
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

    /// Generate an `as` type cast cast from inner to target
    pub(crate) fn codegen_cast(
        &mut self,
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
