// Codegen for tuples `(T1, T2, ...)`: literals build an anonymous LLVM
// struct value; element access `t.N` is an `extractvalue` by constant index.

use inkwell::values::BasicValueEnum;
use neuro_hir::HirExpr;

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

impl<'ctx> CodegenContext<'ctx> {
    /// Lower a tuple literal `(e0, e1, ...)` to an anonymous struct value.
    /// Each element is built at its own type; a default-typed integer/float literal
    /// is retargeted to the annotated element width via `coerce_if_needed`, mirroring
    /// the array-literal path.
    pub(crate) fn codegen_tuple_literal(
        &mut self,
        elements: &[HirExpr],
        tuple_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let element_tys = match tuple_ty {
            Type::Tuple(tys) => tys.clone(),
            _ => {
                return Err(CodegenError::InternalError(
                    "tuple literal type is not a tuple".to_string(),
                ))
            }
        };

        let mut field_llvm = Vec::with_capacity(element_tys.len());
        for ty in &element_tys {
            field_llvm.push(self.get_any_llvm_type(ty)?);
        }
        let struct_ty = self.context.struct_type(&field_llvm, false);

        let mut agg = struct_ty.get_undef();
        for (i, (el, el_ty)) in elements.iter().zip(element_tys.iter()).enumerate() {
            let val = self.codegen_expr(el)?;
            let val = self.coerce_if_needed(val, field_llvm[i], el_ty)?;
            agg = self
                .builder
                .build_insert_value(agg, val, i as u32, "tup.elem")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into_struct_value();
        }
        Ok(agg.into())
    }

    /// Lower a tuple element access `object.N`: build the tuple value and
    /// `extractvalue` the N-th field.
    pub(crate) fn codegen_tuple_index(
        &mut self,
        object: &HirExpr,
        index: usize,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        // A `&tuple` receiver auto-derefs: the borrow lowers to the storage pointer,
        // so load the struct value through it before extracting.
        let value = self.codegen_expr(object)?;
        let struct_val = match value {
            BasicValueEnum::StructValue(sv) => sv,
            BasicValueEnum::PointerValue(ptr) => {
                let struct_llvm = self.get_any_llvm_type(&Type::from_hir(object.ty.referent()))?;
                self.builder
                    .build_load(struct_llvm, ptr, "tup.deref")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                    .into_struct_value()
            }
            other => {
                return Err(CodegenError::InternalError(format!(
                    "tuple index on non-tuple value: {:?}",
                    other
                )))
            }
        };
        self.builder
            .build_extract_value(struct_val, index as u32, "tup.idx")
            .map_err(|e| CodegenError::LlvmError(format!("failed to extract tuple element: {}", e)))
    }
}
