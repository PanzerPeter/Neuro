// Codegen for enum construction (§3.5): every surface form (unit `E::V`, tuple
// `E::V(..)`, struct `E::V { .. }`) reaches here as a single `EnumConstruct` node.
//
// An enum value is a tagged union `{ i32 tag, [W x i64] payload }`. The tag is the
// variant discriminant; each scalar payload field is packed losslessly into its own
// 64-bit slot — integers/`char`/`bool` zero-extend, floats bitcast to their integer
// width then zero-extend. Packing into fixed `i64` slots gives every value of an enum
// one identical LLVM type without computing a target-specific union size, and the
// encoding round-trips bit-exactly for the eventual `match` extraction.

use inkwell::values::{BasicValue, BasicValueEnum};
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
        let enum_ty = self.type_mapper.enum_struct_type(enum_name)?;

        // The payload array type is the enum struct's second field.
        let payload_array_ty = enum_ty
            .get_field_type_at_index(1)
            .ok_or_else(|| {
                CodegenError::InternalError(format!("enum '{}' has no payload field", enum_name))
            })?
            .into_array_type();

        let mut payload_val = payload_array_ty.get_undef();
        for (slot, field) in payload.iter().enumerate() {
            let field_ty = Type::from_hir(&field.ty);
            let value = self.codegen_expr(field)?;
            let encoded = self.encode_enum_payload_field(value, &field_ty)?;
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

    /// Encode one scalar payload field into its 64-bit slot (§3.5). A float is first
    /// bitcast to the integer of its own width to preserve the exact bit pattern,
    /// then every value zero-extends to `i64`; the low bits recover the original on
    /// extraction. Payloads are restricted to scalar primitives by semantic analysis,
    /// so no other shape reaches here.
    fn encode_enum_payload_field(
        &self,
        value: BasicValueEnum<'ctx>,
        field_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let i64_ty = self.context.i64_type();

        let int_value = if field_ty.is_float() {
            let int_width = match field_ty {
                Type::F16 | Type::BF16 => self.context.i16_type(),
                Type::F32 => self.context.i32_type(),
                Type::F64 => i64_ty,
                _ => {
                    return Err(CodegenError::InternalError(
                        "non-float type took the float encoding path".to_string(),
                    ))
                }
            };
            self.builder
                .build_bit_cast(value, int_width, "enum.fbits")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_int_value()
        } else {
            value.into_int_value()
        };

        let widened = self
            .builder
            .build_int_z_extend(int_value, i64_ty, "enum.slot64")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(widened.as_basic_value_enum())
    }
}
