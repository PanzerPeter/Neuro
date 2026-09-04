// Field-wise equality for a struct that derives `PartialEq`.
//
// The derive has no `eq` method to dispatch to — the comparison is generated here,
// straight over the aggregate's fields, which is what separates it from a hand-written
// `impl PartialEq` (that route lowers to an ordinary method call before reaching codegen).

use inkwell::values::{BasicValueEnum, IntValue};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// How deep a derived comparison may recurse through nested struct fields. Field types
/// must be declared before use, so a cycle is impossible today; the limit turns any
/// future self-referential layout into a diagnostic instead of a stack overflow.
const MAX_DERIVE_DEPTH: u32 = 64;

fn llvm_err(e: inkwell::builder::BuilderError) -> CodegenError {
    CodegenError::LlvmError(e.to_string())
}

impl<'ctx> CodegenContext<'ctx> {
    /// Compare two values of a `@derive(PartialEq)` struct, field by field.
    ///
    /// Every field comparison is evaluated — none of them can have a side effect, so
    /// an `and` chain is cheaper than the branching a short-circuit would need.
    pub(crate) fn codegen_derived_struct_eq(
        &self,
        name: &str,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
    ) -> CodegenResult<IntValue<'ctx>> {
        self.derived_struct_eq_at_depth(name, lhs, rhs, 0)
    }

    fn derived_struct_eq_at_depth(
        &self,
        name: &str,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
        depth: u32,
    ) -> CodegenResult<IntValue<'ctx>> {
        if depth >= MAX_DERIVE_DEPTH {
            return Err(CodegenError::UnsupportedType(format!(
                "struct '{}' nests more than {} levels deep, or refers to itself",
                name, MAX_DERIVE_DEPTH
            )));
        }
        let fields = self
            .struct_defs
            .get(name)
            .ok_or_else(|| CodegenError::UnsupportedType(format!("unknown struct '{}'", name)))?
            .clone();

        let lhs = self.load_struct_value(&lhs, name)?;
        let rhs = self.load_struct_value(&rhs, name)?;

        let mut result = self.context.bool_type().const_int(1, false);
        for (index, (field_name, field_ty)) in fields.iter().enumerate() {
            let a = self
                .builder
                .build_extract_value(lhs, index as u32, &format!("eq.l.{}", field_name))
                .map_err(llvm_err)?;
            let b = self
                .builder
                .build_extract_value(rhs, index as u32, &format!("eq.r.{}", field_name))
                .map_err(llvm_err)?;
            let field_eq = self.codegen_field_eq(field_ty, a, b, depth)?;
            result = self
                .builder
                .build_and(result, field_eq, "eq.and")
                .map_err(llvm_err)?;
        }
        Ok(result)
    }

    /// Normalize a struct operand to its aggregate value: a `&mut S` operand is the
    /// referent's address and must be read through before its fields can be extracted.
    fn load_struct_value(
        &self,
        value: &BasicValueEnum<'ctx>,
        name: &str,
    ) -> CodegenResult<inkwell::values::StructValue<'ctx>> {
        match value {
            BasicValueEnum::PointerValue(ptr) => {
                let struct_ty = self.type_mapper.struct_type(name)?;
                Ok(self
                    .builder
                    .build_load(struct_ty, *ptr, "eq.deref")
                    .map_err(llvm_err)?
                    .into_struct_value())
            }
            other => Ok(other.into_struct_value()),
        }
    }

    /// Compare one field. Semantic analysis has already restricted the field types a
    /// derive can reach, so anything else here is a frontend bug rather than a program
    /// error.
    fn codegen_field_eq(
        &self,
        ty: &Type,
        lhs: BasicValueEnum<'ctx>,
        rhs: BasicValueEnum<'ctx>,
        depth: u32,
    ) -> CodegenResult<IntValue<'ctx>> {
        match ty.referent() {
            Type::String => {
                let lhs = self.load_string_operand(lhs)?;
                let rhs = self.load_string_operand(rhs)?;
                self.codegen_string_eq(lhs, rhs)
            }
            Type::Struct(nested) => self.derived_struct_eq_at_depth(nested, lhs, rhs, depth + 1),
            other if other.is_float() => self
                .builder
                .build_float_compare(
                    inkwell::FloatPredicate::OEQ,
                    lhs.into_float_value(),
                    rhs.into_float_value(),
                    "eq.f",
                )
                .map_err(llvm_err),
            other if other.is_integer() || matches!(other, Type::Bool | Type::Char) => self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::EQ,
                    lhs.into_int_value(),
                    rhs.into_int_value(),
                    "eq.i",
                )
                .map_err(llvm_err),
            other => Err(CodegenError::UnsupportedType(format!(
                "type {:?} reached derived equality; semantic analysis rejects it",
                other
            ))),
        }
    }

    /// A `string` field is already the `{ ptr, len }` aggregate; only a borrow that is
    /// stored as an address has to be read through.
    fn load_string_operand(
        &self,
        value: BasicValueEnum<'ctx>,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        match value {
            BasicValueEnum::PointerValue(ptr) => {
                let string_ty = self.type_mapper.map_type(&Type::String)?;
                self.builder
                    .build_load(string_ty, ptr, "eq.str")
                    .map_err(llvm_err)
            }
            other => Ok(other),
        }
    }
}
