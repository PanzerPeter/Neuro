use ast_types::*;
use inkwell::types::*;
use inkwell::values::*;
use std::collections::HashMap;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::CodegenContext;

impl<'ctx> CodegenContext<'ctx> {
    /// Populate the struct definition table before code generation begins.
    pub(crate) fn set_struct_defs(&mut self, defs: HashMap<String, Vec<(String, Type)>>) {
        self.struct_defs = defs;
    }

    /// Build (or reconstruct) the LLVM struct type for a named struct.
    ///
    /// LLVM deduplicates anonymous struct types by structure, so reconstructing
    /// the type each call is safe and avoids storing LLVM types in the context.
    pub(crate) fn get_struct_llvm_type(&self, name: &str) -> CodegenResult<StructType<'ctx>> {
        let def = self.struct_defs.get(name).ok_or_else(|| {
            CodegenError::UnsupportedType(format!("unknown struct type '{}'", name))
        })?;
        let mut field_llvm_types = Vec::new();
        for (_, field_ty) in def {
            field_llvm_types.push(self.type_mapper.map_type(field_ty)?);
        }
        Ok(self.context.struct_type(&field_llvm_types, false))
    }

    /// Build a struct aggregate value from a struct literal expression.
    ///
    /// `base` is the optional functional-update source (`Point { x, ..p }`): the
    /// aggregate is seeded from its value so that fields absent from `fields`
    /// retain the base's values, then each explicit field overwrites its slot.
    pub(crate) fn codegen_struct_literal(
        &mut self,
        name: &str,
        fields: &[FieldInit],
        base: Option<&Expr>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let llvm_ty = self.get_struct_llvm_type(name)?;
        let def = self
            .struct_defs
            .get(name)
            .ok_or_else(|| CodegenError::UnsupportedType(format!("unknown struct '{}'", name)))?
            .clone();

        let mut agg = match base {
            Some(base_expr) => self.codegen_expr(base_expr)?.into_struct_value(),
            None => llvm_ty.get_undef(),
        };
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
    pub(crate) fn codegen_field_access(
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
    pub(crate) fn codegen_field_assignment(
        &mut self,
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
    pub(crate) fn get_struct_ptr_and_type(
        &self,
        object: &Expr,
        struct_name: &str,
    ) -> CodegenResult<(PointerValue<'ctx>, StructType<'ctx>)> {
        match object {
            Expr::Identifier(ident) => {
                let alloca = self
                    .variables
                    .get(&ident.name)
                    .copied()
                    .ok_or_else(|| CodegenError::UndefinedVariable(ident.name.clone()))?;
                let llvm_ty = self.get_struct_llvm_type(struct_name)?;
                // A `&Struct` binding stores a pointer to the struct in its alloca (the
                // mapped LLVM type is `ptr`, not the aggregate). Load that pointer to reach
                // the borrowed struct; an owned struct binding's alloca is the struct itself.
                let var_ty = self.variable_types.get(&ident.name).ok_or_else(|| {
                    CodegenError::InternalError(format!("missing type for variable {}", ident.name))
                })?;
                if var_ty.is_pointer_type() {
                    let struct_ptr = self
                        .builder
                        .build_load(*var_ty, alloca, "deref.struct.ptr")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into_pointer_value();
                    Ok((struct_ptr, llvm_ty))
                } else {
                    Ok((alloca, llvm_ty))
                }
            }
            _ => Err(CodegenError::UnsupportedType(
                "chained field access is not yet supported".to_string(),
            )),
        }
    }
}
