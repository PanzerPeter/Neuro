use inkwell::types::*;
use inkwell::values::*;
use neuro_hir::{HirExpr, HirExprKind, HirFieldInit};
use std::collections::HashMap;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::CodegenContext;

impl<'ctx> CodegenContext<'ctx> {
    /// Populate the struct definition table before code generation begins. The field
    /// *types* are also handed to the [`crate::type_mapping::TypeMapper`], which needs
    /// them to build a struct's LLVM aggregate wherever one appears — a parameter, a
    /// return type, or a field of another struct.
    pub(crate) fn set_struct_defs(&mut self, defs: HashMap<String, Vec<(String, Type)>>) {
        let field_types = defs
            .iter()
            .map(|(name, fields)| {
                (
                    name.clone(),
                    fields.iter().map(|(_, ty)| ty.clone()).collect(),
                )
            })
            .collect();
        self.type_mapper.set_struct_fields(field_types);
        self.struct_defs = defs;
    }

    /// The LLVM struct type for a named struct. Field *names* live in `struct_defs`
    /// (for index lookup); the layout itself comes from the type mapper, so both
    /// paths agree on one aggregate.
    pub(crate) fn get_struct_llvm_type(&self, name: &str) -> CodegenResult<StructType<'ctx>> {
        self.type_mapper.struct_type(name)
    }

    /// Build a struct aggregate value from a struct literal expression.
    ///
    /// `base` is the optional functional-update source (`Point { x, ..p }`): the
    /// aggregate is seeded from its value so that fields absent from `fields`
    /// retain the base's values, then each explicit field overwrites its slot.
    pub(crate) fn codegen_struct_literal(
        &mut self,
        name: &str,
        fields: &[HirFieldInit],
        base: Option<&HirExpr>,
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
                .position(|(n, _)| n == &field_init.name)
                .ok_or_else(|| {
                    CodegenError::InternalError(format!(
                        "struct '{}' has no field '{}'",
                        name, field_init.name
                    ))
                })?;
            let val = self.codegen_expr(&field_init.value)?;
            // A place stored into a struct field is moved into the aggregate,
            // so it must not also be dropped at the surrounding scope's exit.
            self.mark_moved_for_drop(&field_init.value);
            agg = self
                .builder
                .build_insert_value(
                    agg,
                    val,
                    idx as u32,
                    &format!("{}.{}", name, field_init.name),
                )
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_struct_value();
        }
        Ok(agg.into())
    }

    /// Read a single field from a struct.
    ///
    /// A named binding is addressed and the field loaded through a GEP. Any other
    /// object — a chained access (`o.inner.v`), a call result, a struct literal —
    /// has no storage of its own, so it is evaluated to a first-class aggregate and
    /// the field extracted from that value.
    pub(crate) fn codegen_field_access(
        &mut self,
        object: &HirExpr,
        field_name: &str,
        struct_name: &str,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let idx = self.struct_field_index(struct_name, field_name)?;

        if !matches!(object.kind, HirExprKind::Variable(_)) {
            let aggregate = self.codegen_expr(object)?;
            let BasicValueEnum::StructValue(struct_val) = aggregate else {
                return Err(CodegenError::InternalError(format!(
                    "field access on a non-aggregate value of struct '{}'",
                    struct_name
                )));
            };
            return self
                .builder
                .build_extract_value(struct_val, idx as u32, field_name)
                .map_err(|e| CodegenError::LlvmError(format!("failed to read field: {}", e)));
        }

        let (ptr, llvm_ty) = self.get_struct_ptr_and_type(object, struct_name)?;
        let field_ty = self
            .struct_defs
            .get(struct_name)
            .and_then(|def| def.get(idx))
            .map(|(_, ty)| ty.clone())
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

        let llvm_field_ty = self.type_mapper.map_type(&field_ty)?;
        self.builder
            .build_load(llvm_field_ty, field_ptr, field_name)
            .map_err(|e| CodegenError::LlvmError(format!("failed to load field: {}", e)))
    }

    /// The declaration-order position of `field_name` in `struct_name`, which is also
    /// its index in the LLVM aggregate.
    fn struct_field_index(&self, struct_name: &str, field_name: &str) -> CodegenResult<usize> {
        let def = self.struct_defs.get(struct_name).ok_or_else(|| {
            CodegenError::UnsupportedType(format!("unknown struct '{}'", struct_name))
        })?;
        def.iter()
            .position(|(n, _)| n == field_name)
            .ok_or_else(|| {
                CodegenError::InternalError(format!(
                    "struct '{}' has no field '{}'",
                    struct_name, field_name
                ))
            })
    }

    /// Store a value into a field of a named struct variable.
    pub(crate) fn codegen_field_assignment(
        &mut self,
        object_name: &str,
        field_name: &str,
        value: &HirExpr,
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
        self.mark_moved_for_drop(value);
        Ok(())
    }

    /// Get the alloca pointer and LLVM struct type for a struct object expression.
    /// Only simple identifier objects are supported (no chained access).
    pub(crate) fn get_struct_ptr_and_type(
        &self,
        object: &HirExpr,
        struct_name: &str,
    ) -> CodegenResult<(PointerValue<'ctx>, StructType<'ctx>)> {
        match &object.kind {
            HirExprKind::Variable(name) => {
                let alloca = self
                    .variables
                    .get(name)
                    .copied()
                    .ok_or_else(|| CodegenError::UndefinedVariable(name.clone()))?;
                let llvm_ty = self.get_struct_llvm_type(struct_name)?;
                // A `&Struct` binding stores a pointer to the struct in its alloca (the
                // mapped LLVM type is `ptr`, not the aggregate). Load that pointer to reach
                // the borrowed struct; an owned struct binding's alloca is the struct itself.
                let var_ty = self.variable_types.get(name).ok_or_else(|| {
                    CodegenError::InternalError(format!("missing type for variable {}", name))
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
            // A field of a struct that itself has storage: GEP into the parent rather
            // than materializing a copy. This is what lets an adapter's `&mut self`
            // method drive the iterator it wraps (`self.inner.next()`) and have the
            // advance stick — reaching the field as a value would discard it.
            HirExprKind::FieldAccess {
                object: parent,
                field,
            } => {
                let Type::Struct(parent_name) = Type::from_hir(&parent.ty).referent().clone()
                else {
                    return Err(CodegenError::UnsupportedType(format!(
                        "field '{}' is not reached through a struct",
                        field
                    )));
                };
                let (parent_ptr, parent_llvm) =
                    self.get_struct_ptr_and_type(parent, &parent_name)?;
                let idx = self.struct_field_index(&parent_name, field)?;
                let field_ptr = self
                    .builder
                    .build_struct_gep(
                        parent_llvm,
                        parent_ptr,
                        idx as u32,
                        &format!("{}.ptr", field),
                    )
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
                Ok((field_ptr, self.get_struct_llvm_type(struct_name)?))
            }
            other => Err(CodegenError::UnsupportedType(format!(
                "a method receiver must be a place, not {:?}",
                other
            ))),
        }
    }
}
