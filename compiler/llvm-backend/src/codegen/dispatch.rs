// Dynamic dispatch: vtable emission, trait-object construction, and virtual calls.
//
// Static dispatch (`impl Trait`) needs nothing here — it is monomorphized away before the
// HIR reaches this backend. Dynamic dispatch is the one place the compiler emits a runtime
// method table: a `&dyn Trait` is a `{ data ptr, vtable ptr }` fat pointer, and a call
// through it loads a fixed slot from the vtable and jumps.

use inkwell::types::BasicType;
use inkwell::values::*;
use inkwell::AddressSpace;
use neuro_hir::{HirExpr, HirImpl, HirItem, HirSelfParam};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// Separator between the components of a vtable thunk's symbol name. Distinct from the
/// `__` method mangle so a thunk can never collide with a user method.
const THUNK_MARKER: &str = "__vt__";

impl<'ctx> CodegenContext<'ctx> {
    /// Emit a vtable for every `impl Trait for Type` whose trait is a declared trait.
    ///
    /// Runs after all methods are declared (their signatures must exist) and before any
    /// body is generated, so a `&dyn Trait` construction anywhere in the module finds its
    /// table already emitted regardless of item order.
    pub(crate) fn emit_vtables(&mut self, items: &[HirItem]) -> CodegenResult<()> {
        for item in items {
            let HirItem::Impl(impl_def) = item else {
                continue;
            };
            let Some(trait_name) = impl_def.trait_name.clone() else {
                continue;
            };
            // Compiler-known lang-item traits (`Drop`, the operator traits) are not
            // user-declared and are never used as trait objects, so they get no vtable.
            if !self.trait_methods.contains_key(&trait_name) {
                continue;
            }
            self.emit_vtable_for(&trait_name, impl_def)?;
        }
        Ok(())
    }

    /// Emit one concrete type's method table for one trait: a thunk per trait method, in
    /// the trait's declaration order, collected into a private global array of pointers.
    fn emit_vtable_for(&mut self, trait_name: &str, impl_def: &HirImpl) -> CodegenResult<()> {
        let type_name = impl_def.type_name.clone();
        let key = (trait_name.to_string(), type_name.clone());
        if self.vtables.contains_key(&key) {
            return Ok(());
        }
        let method_order = self
            .trait_methods
            .get(trait_name)
            .cloned()
            .unwrap_or_default();

        let mut slots: Vec<BasicValueEnum<'ctx>> = Vec::with_capacity(method_order.len());
        for method_name in &method_order {
            let method = impl_def
                .methods
                .iter()
                .find(|m| &m.name == method_name)
                .ok_or_else(|| {
                    CodegenError::InternalError(format!(
                        "`impl {} for {}` is missing trait method '{}'",
                        trait_name, type_name, method_name
                    ))
                })?;
            let thunk = self.emit_vtable_thunk(trait_name, &type_name, &method.name, method)?;
            slots.push(thunk.as_global_value().as_pointer_value().into());
        }

        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        let array_ty = ptr_ty.array_type(slots.len() as u32);
        let entries: Vec<PointerValue<'ctx>> =
            slots.into_iter().map(|v| v.into_pointer_value()).collect();
        let global = self.module.add_global(
            array_ty,
            None,
            &format!("neuro_vtable__{}__{}", trait_name, type_name),
        );
        global.set_initializer(&ptr_ty.const_array(&entries));
        global.set_constant(true);
        global.set_linkage(inkwell::module::Linkage::Private);
        self.vtables.insert(key, global);
        Ok(())
    }

    /// Emit the thunk that adapts one concrete method to the uniform vtable calling
    /// convention `(ptr self, args...) -> ret`.
    ///
    /// The adaptation is needed because a `&self` method receives its struct *by value*,
    /// while a trait object only ever holds a pointer to the receiver. The thunk
    /// loads the receiver in that case and forwards; a `&mut self` method already takes a
    /// pointer, so its thunk forwards the pointer unchanged.
    fn emit_vtable_thunk(
        &mut self,
        trait_name: &str,
        type_name: &str,
        method_name: &str,
        method: &neuro_hir::HirMethod,
    ) -> CodegenResult<FunctionValue<'ctx>> {
        let thunk_name = format!(
            "{}{}{}{}{}",
            type_name, THUNK_MARKER, trait_name, THUNK_MARKER, method_name
        );
        if let Some(existing) = self.module.get_function(&thunk_name) {
            return Ok(existing);
        }

        let target_name = format!("{}__{}", type_name, method_name);
        let target = *self
            .functions
            .get(&target_name)
            .ok_or_else(|| CodegenError::UndefinedFunction(target_name.clone()))?;

        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        let mut param_types: Vec<inkwell::types::BasicMetadataTypeEnum<'ctx>> = vec![ptr_ty.into()];
        for param in &method.params {
            let ty = Type::from_hir(&param.ty);
            param_types.push(self.type_mapper.map_type(&ty)?.into());
        }

        let ret_ty = Type::from_hir(&method.return_type);
        let fn_type = if matches!(ret_ty, Type::Void) {
            self.context.void_type().fn_type(&param_types, false)
        } else {
            self.type_mapper
                .map_type(&ret_ty)?
                .fn_type(&param_types, false)
        };
        let thunk = self.module.add_function(&thunk_name, fn_type, None);
        thunk.set_linkage(inkwell::module::Linkage::Private);

        // The thunk is emitted with its own builder position; save and restore the
        // caller's insertion point so vtable emission can run mid-module.
        let saved_block = self.builder.get_insert_block();
        let entry = self.context.append_basic_block(thunk, "entry");
        self.builder.position_at_end(entry);

        let self_ptr = thunk
            .get_nth_param(0)
            .ok_or_else(|| CodegenError::InternalError("thunk lost its self parameter".into()))?
            .into_pointer_value();

        // `&mut self` is already pointer-passed; every other receiver is by value.
        let self_arg: BasicMetadataValueEnum<'ctx> =
            if matches!(method.self_param, Some(HirSelfParam::RefMut)) {
                self_ptr.into()
            } else {
                let struct_ty = self.get_struct_llvm_type(type_name)?;
                self.builder
                    .build_load(struct_ty, self_ptr, "dyn.self")
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                    .into()
            };

        let mut call_args: Vec<BasicMetadataValueEnum<'ctx>> = vec![self_arg];
        for i in 0..method.params.len() {
            let arg = thunk.get_nth_param((i + 1) as u32).ok_or_else(|| {
                CodegenError::InternalError("thunk lost a forwarded parameter".into())
            })?;
            call_args.push(arg.into());
        }

        let call = self
            .builder
            .build_call(target, &call_args, "dyn.fwd")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        match call.try_as_basic_value().basic() {
            Some(value) => {
                self.builder
                    .build_return(Some(&value))
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            }
            None => {
                self.builder
                    .build_return(None)
                    .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
            }
        }

        if let Some(block) = saved_block {
            self.builder.position_at_end(block);
        }
        Ok(thunk)
    }

    /// Lower the unsizing coercion `&T` → `&dyn Trait`: pair the concrete
    /// reference with the `(trait, T)` vtable to form the `{ data, vtable }` fat pointer.
    pub(crate) fn codegen_dyn_coerce(
        &mut self,
        value: &HirExpr,
        target_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let Type::Reference(inner) = target_ty else {
            return Err(CodegenError::InternalError(
                "trait-object coercion target is not a reference".into(),
            ));
        };
        let Type::DynObject(trait_name) = inner.as_ref() else {
            return Err(CodegenError::InternalError(
                "trait-object coercion target is not a trait object".into(),
            ));
        };

        let concrete = Type::from_hir(&value.ty);
        let type_name = match concrete.referent() {
            Type::Struct(name) => name.clone(),
            other => {
                return Err(CodegenError::UnsupportedType(format!(
                    "only a struct can be used as a `dyn {}` trait object, found {:?}",
                    trait_name, other
                )))
            }
        };

        let vtable = *self
            .vtables
            .get(&(trait_name.clone(), type_name.clone()))
            .ok_or_else(|| {
                CodegenError::InternalError(format!(
                    "no vtable emitted for `impl {} for {}`",
                    trait_name, type_name
                ))
            })?;

        let data_ptr = self.codegen_expr(value)?;
        let BasicValueEnum::PointerValue(data_ptr) = data_ptr else {
            return Err(CodegenError::InternalError(
                "trait-object data operand did not lower to a pointer".into(),
            ));
        };

        let fat_ty = self.type_mapper.dyn_ref_type();
        let with_data = self
            .builder
            .build_insert_value(fat_ty.get_undef(), data_ptr, 0, "dyn.data")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();
        let fat = self
            .builder
            .build_insert_value(with_data, vtable.as_pointer_value(), 1, "dyn.obj")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_struct_value();
        Ok(fat.into())
    }

    /// Lower a method call through a trait object: load the method pointer from
    /// the receiver's vtable slot and call it indirectly, passing the data pointer as
    /// `self`. The slot index is the method's position in the trait's declaration order,
    /// identical across every implementor.
    pub(crate) fn codegen_dyn_method_call(
        &mut self,
        trait_name: &str,
        method: &str,
        receiver: &HirExpr,
        args: &[HirExpr],
        result_ty: &Type,
    ) -> CodegenResult<Option<BasicValueEnum<'ctx>>> {
        let slot = self
            .trait_methods
            .get(trait_name)
            .and_then(|ms| ms.iter().position(|m| m == method))
            .ok_or_else(|| {
                CodegenError::InternalError(format!(
                    "trait '{}' has no method '{}' in its vtable layout",
                    trait_name, method
                ))
            })?;

        let fat = self.codegen_expr(receiver)?;
        let BasicValueEnum::StructValue(fat) = fat else {
            return Err(CodegenError::InternalError(
                "trait-object receiver did not lower to a fat pointer".into(),
            ));
        };
        let data_ptr = self
            .builder
            .build_extract_value(fat, 0, "dyn.recv.data")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();
        let vtable_ptr = self
            .builder
            .build_extract_value(fat, 1, "dyn.recv.vt")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();

        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        // SAFETY: the vtable is a private constant array emitted by `emit_vtable_for`
        // with one slot per trait method, and `slot` came from that same method list, so
        // the index is always within the allocation.
        let slot_ptr = unsafe {
            self.builder
                .build_in_bounds_gep(
                    ptr_ty,
                    vtable_ptr,
                    &[self.context.i32_type().const_int(slot as u64, false)],
                    "dyn.slot",
                )
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
        };
        let fn_ptr = self
            .builder
            .build_load(ptr_ty, slot_ptr, "dyn.fn")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?
            .into_pointer_value();

        let mut call_args: Vec<BasicMetadataValueEnum<'ctx>> = vec![data_ptr.into()];
        let mut param_types: Vec<inkwell::types::BasicMetadataTypeEnum<'ctx>> = vec![ptr_ty.into()];
        for arg in args {
            let value = self.codegen_expr(arg)?;
            param_types.push(value.get_type().into());
            call_args.push(value.into());
        }

        let fn_type = if matches!(result_ty, Type::Void) {
            self.context.void_type().fn_type(&param_types, false)
        } else {
            self.type_mapper
                .map_type(result_ty)?
                .fn_type(&param_types, false)
        };

        let call = self
            .builder
            .build_indirect_call(fn_type, fn_ptr, &call_args, "dyn.call")
            .map_err(|e| CodegenError::LlvmError(e.to_string()))?;
        Ok(call.try_as_basic_value().basic())
    }
}
