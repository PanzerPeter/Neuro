// Codegen for enum construction: every surface form (unit `E::V`, tuple
// `E::V(..)`, struct `E::V { .. }`) reaches here as a single `EnumConstruct` node.
//
// An enum value is a tagged union `{ i32 tag, [W x [K x i64]] payload }`. The tag is
// the variant discriminant; each payload field is written into its own slot as raw
// 64-bit words, through a zeroed stack cell that the field's own type is stored into
// and the slot type loaded back out of. Going through memory is what makes the slot
// type-agnostic: a `string` fat pointer, a struct, or a tuple round-trips bit-exactly
// for the eventual `match` extraction, exactly as a scalar does.

use inkwell::values::{BasicValueEnum, PointerValue};
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Build a tagged-union value for `enum_name`'s `tag`-th variant from `payload`
    /// (the variant's fields, already in declared order).
    pub(crate) fn codegen_enum_construct(
        &mut self,
        enum_name: &str,
        tag: u32,
        payload: &[HirExpr],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let mut fields = Vec::with_capacity(payload.len());
        for field in payload {
            let field_ty = Type::from_hir(&field.ty);
            fields.push((self.codegen_expr(field)?, field_ty));
        }
        let borrowed: Vec<(BasicValueEnum<'ctx>, &Type)> =
            fields.iter().map(|(v, t)| (*v, t)).collect();
        self.codegen_enum_value(enum_name, tag, &borrowed)
    }

    /// Build a tagged-union value from already-evaluated payload values. Shared by the
    /// surface `EnumConstruct` node and by the collection readers, which produce their
    /// `Option<T>` payload from a heap load rather than from a sub-expression.
    pub(crate) fn codegen_enum_value(
        &mut self,
        enum_name: &str,
        tag: u32,
        payload: &[(BasicValueEnum<'ctx>, &Type)],
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let enum_ty = self.type_mapper.enum_struct_type(enum_name)?;

        // The payload array type is the enum struct's second field.
        let payload_array_ty = enum_ty
            .get_field_type_at_index(1)
            .ok_or_else(|| {
                CodegenError::InternalError(format!("enum '{}' has no payload field", enum_name))
            })?
            .into_array_type();

        let slot_ty = self.type_mapper.enum_slot_type(enum_name)?;
        let mut payload_val = payload_array_ty.get_undef();
        for (slot, (value, _)) in payload.iter().enumerate() {
            let cell = self.enum_payload_cell(slot_ty)?;
            self.builder
                .build_store(cell, *value)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            let encoded = self
                .builder
                .build_load(slot_ty, cell, "enum.words")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            payload_val = self
                .builder
                .build_insert_value(payload_val, encoded, slot as u32, "enum.slot")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_array_value();
        }

        let tag_val = self.context.i32_type().const_int(tag as u64, false);
        let mut agg = enum_ty.get_undef();
        agg = self
            .builder
            .build_insert_value(agg, tag_val, 0, "enum.tag")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();
        agg = self
            .builder
            .build_insert_value(agg, payload_val, 1, "enum.payload")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();

        Ok(agg.into())
    }

    /// A zeroed stack cell of an enum's payload slot type, used to reinterpret one
    /// payload field as raw words and back.
    ///
    /// The cell is zeroed because a field narrower than the slot leaves the remaining
    /// words unwritten, and the slot is loaded whole: without this, those words would
    /// be undef and the enum value would carry poison through every copy of it.
    pub(super) fn enum_payload_cell(
        &self,
        slot_ty: inkwell::types::ArrayType<'ctx>,
    ) -> CodegenResult<PointerValue<'ctx>> {
        // In the entry block, not at the builder's position: a cell built inside a
        // loop body would otherwise grow the stack by one slot per iteration.
        let cell = self.entry_alloca(slot_ty, "enum.cell")?;
        self.builder
            .build_store(cell, slot_ty.const_zero())
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(cell)
    }
}
