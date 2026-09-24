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
    /// them to build a struct's LLVM aggregate wherever one appears: a parameter, a
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

    /// Record the source-level name of each struct key, for the derived debug rendering.
    pub(crate) fn set_struct_written_names(&mut self, names: HashMap<String, String>) {
        self.struct_written_names = names;
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
            Some(base_expr) => {
                let collecting = self.literal_string_moves.take();
                let value = self.codegen_expr(base_expr);
                self.literal_string_moves = collecting;
                let value = value?.into_struct_value();
                let taken = |path: &[String]| {
                    path.first()
                        .is_some_and(|field| fields.iter().all(|init| &init.name != field))
                };
                if self.literal_string_moves.is_some() {
                    let flags = self.load_held_string_flags(base_expr)?;
                    if let Some(moves) = &mut self.literal_string_moves {
                        for (path, owns) in flags.into_iter().filter(|(path, _)| taken(path)) {
                            moves
                                .flags
                                .push(([moves.path.clone(), path].concat(), owns));
                        }
                    }
                }
                // The update moves every field it does not write out of the base, so the
                // base stops owning those; the ones it overrides stay the base's to release.
                for (field, _) in &def {
                    if taken(std::slice::from_ref(field)) {
                        let taken = HirExpr::new(
                            HirExprKind::FieldAccess {
                                object: Box::new(base_expr.clone()),
                                field: field.clone(),
                            },
                            base_expr.ty.clone(),
                            base_expr.span,
                        );
                        self.mark_moved_for_drop(&taken);
                    }
                }
                value
            }
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
            // A place stored into a struct field is moved into the aggregate,
            // so it must not also be dropped at the surrounding scope's exit.
            let val = self.codegen_literal_position(field_init.name.clone(), &field_init.value)?;
            agg = self
                .builder
                .build_insert_value(
                    agg,
                    val,
                    idx as u32,
                    &format!("{}.{}", name, field_init.name),
                )?
                .into_struct_value();
        }
        Ok(agg.into())
    }

    /// Read a single field from a struct.
    ///
    /// A named binding is addressed and the field loaded through a GEP. Any other
    /// object (a chained access (`o.inner.v`), a call result, a struct literal)
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

        let field_ptr = self.builder.build_struct_gep(
            llvm_ty,
            ptr,
            idx as u32,
            &format!("{}.ptr", field_name),
        )?;

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

    /// Store a value into a field of a struct place.
    ///
    /// `object` is the struct the field belongs to, already a place: a binding, or a
    /// field of one. Reaching it by GEP rather than as a value is what makes the write
    /// stick, since a loaded struct is a copy.
    pub(crate) fn codegen_field_assignment(
        &mut self,
        object: &HirExpr,
        field_name: &str,
        value: &HirExpr,
    ) -> CodegenResult<()> {
        let Type::Struct(struct_name) = Type::from_hir(&object.ty).referent().clone() else {
            return Err(CodegenError::UnsupportedType(format!(
                "field '{}' is not reached through a struct",
                field_name
            )));
        };

        // The value first (the language evaluates it before the place): when the holder is an element of a `Vec`, the value
        // may grow that `Vec` and free the buffer an address taken earlier points into.
        let val = self.codegen_expr(value)?;
        let Some(ptr) = self.held_place_ptr(object)? else {
            return Err(CodegenError::UnsupportedType(format!(
                "the holder of field '{}' is not a place",
                field_name
            )));
        };
        let llvm_struct_ty = self.get_struct_llvm_type(&struct_name)?;
        let idx = self.struct_field_index(&struct_name, field_name)?;

        let field_ptr = self.builder.build_struct_gep(
            llvm_struct_ty,
            ptr,
            idx as u32,
            &format!("{}.ptr", field_name),
        )?;

        // Ordered as a binding's reassignment is: the field may be read on the way to
        // replacing itself, so its prior value loses its owner only once the new one
        // has been built. Drop tracking is keyed by binding, so only a field of a named
        // holder has a prior value it can find.
        if let HirExprKind::Variable(object_name) = &object.kind {
            let object_name = object_name.clone();
            let path = [field_name.to_string()];
            self.drop_displaced_held_value(&object_name, &path)?;
            // A `string` position the assignment hands a fresh buffer takes ownership
            // of it here: the release above left every such position disarmed, because
            // the type says nothing about what the incoming value owns.
            self.arm_stored_string_positions(&object_name, &path, value)?;
        }
        self.builder
            .build_store(field_ptr, val)
            .map_err(|e| CodegenError::LlvmError(format!("failed to store field: {}", e)))?;
        self.mark_moved_for_drop(value);
        Ok(())
    }

    /// The storage address of a place expression, when it has one.
    ///
    /// A binding uses its alloca; a field of a struct that itself has storage is a GEP
    /// into the parent. Anything else is a temporary, which has no storage a write
    /// could reach, and the caller decides what to do about that.
    pub(crate) fn held_place_ptr(
        &mut self,
        expr: &HirExpr,
    ) -> CodegenResult<Option<PointerValue<'ctx>>> {
        match &expr.kind {
            HirExprKind::Variable(name) => {
                let Some(ptr) = self.variables.get(name).copied() else {
                    return Ok(None);
                };
                // A binding that reaches its value indirectly stores the value's
                // address, not the value, so one load reaches the place it names. Two
                // shapes do: a borrow of an aggregate, and the `self` of a `&mut self`
                // method, whose type is the struct itself. A borrowed slice is neither
                // — its slot holds the fat pointer by value — which is why the LLVM
                // representation, not the semantic type alone, decides.
                let ty = Type::from_hir(&expr.ty);
                let indirect_shape = matches!(ty, Type::Reference { .. })
                    || matches!(ty.referent(), Type::Struct(_));
                let is_indirect = self
                    .variable_types
                    .get(name)
                    .is_some_and(|ty| ty.is_pointer_type());
                if indirect_shape && is_indirect {
                    let loaded = self
                        .builder
                        .build_load(
                            self.context.ptr_type(inkwell::AddressSpace::default()),
                            ptr,
                            "place.deref",
                        )?
                        .into_pointer_value();
                    return Ok(Some(loaded));
                }
                Ok(Some(ptr))
            }

            HirExprKind::FieldAccess { object, field } => {
                let Type::Struct(parent_name) = Type::from_hir(&object.ty).referent().clone()
                else {
                    return Ok(None);
                };
                let Some(parent_ptr) = self.held_place_ptr(object)? else {
                    return Ok(None);
                };
                let parent_llvm = self.get_struct_llvm_type(&parent_name)?;
                let idx = self.struct_field_index(&parent_name, field)?;
                let field_ptr = self.builder.build_struct_gep(
                    parent_llvm,
                    parent_ptr,
                    idx as u32,
                    &format!("{}.ptr", field),
                )?;
                Ok(Some(field_ptr))
            }

            // An element of an array that itself has storage, or an aggregate element of
            // a `Vec` or a borrowed slice, whose slot lives in the buffer behind the
            // header or the fat pointer.
            HirExprKind::Index { object, index } => {
                let obj_ty = Type::from_hir(&object.ty);
                match obj_ty.referent() {
                    Type::Collection {
                        kind: crate::types::CollectionKind::Vec,
                        ..
                    } => return self.vec_element_place(object, &obj_ty, index),
                    Type::Slice(_) => return self.slice_element_place(object, &obj_ty, index),
                    _ => {}
                }
                let Type::Array { element, size } = obj_ty.referent().clone() else {
                    return Ok(None);
                };
                let base = if matches!(obj_ty, Type::Reference { .. }) {
                    self.codegen_expr(object)?.into_pointer_value()
                } else {
                    match self.held_place_ptr(object)? {
                        Some(ptr) => ptr,
                        None => return Ok(None),
                    }
                };
                let elem_llvm = self.get_any_llvm_type(&element)?;
                let slot =
                    self.array_element_ptr(base, elem_llvm, size, index, index.span.start)?;
                Ok(Some(slot))
            }

            // A tuple element, reached the same way a struct field is: a tuple lowers
            // to an anonymous LLVM struct, so the position IS the field index.
            HirExprKind::TupleIndex { object, index } => {
                let Some(base) = self.held_place_ptr(object)? else {
                    return Ok(None);
                };
                let tuple_llvm = self.get_any_llvm_type(&Type::from_hir(object.ty.referent()))?;
                if !tuple_llvm.is_struct_type() {
                    return Ok(None);
                }
                let slot = self.builder.build_struct_gep(
                    tuple_llvm.into_struct_type(),
                    base,
                    *index as u32,
                    "tup.ptr",
                )?;
                Ok(Some(slot))
            }

            HirExprKind::Deref { operand } => {
                Ok(Some(self.codegen_expr(operand)?.into_pointer_value()))
            }

            _ => Ok(None),
        }
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
                        .build_load(*var_ty, alloca, "deref.struct.ptr")?
                        .into_pointer_value();
                    Ok((struct_ptr, llvm_ty))
                } else {
                    Ok((alloca, llvm_ty))
                }
            }
            // A field of a struct that itself has storage: GEP into the parent rather
            // than materializing a copy. This is what lets an adapter's `&mut self`
            // method drive the iterator it wraps (`self.inner.next()`) and have the
            // advance stick; reaching the field as a value would discard it.
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
                let field_ptr = self.builder.build_struct_gep(
                    parent_llvm,
                    parent_ptr,
                    idx as u32,
                    &format!("{}.ptr", field),
                )?;
                Ok((field_ptr, self.get_struct_llvm_type(struct_name)?))
            }
            other => Err(CodegenError::UnsupportedType(format!(
                "a method receiver must be a place, not {:?}",
                other
            ))),
        }
    }
}
