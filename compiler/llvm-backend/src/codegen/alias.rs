// What a load or store reaches, told to LLVM's type-based alias analysis (`!tbaa`).
//
// A tagged access touches memory of its class alone, and no two classes ever share a
// byte: a buffer of numbers never holds a handle, a header or a variable, and a header is
// never anything but a header. So a store to one class never invalidates a load from
// another. That is what lets a loop writing `v[i]` or `t[i, j]` through a reference keep
// the collection's length and buffer, or the tensor's `data` pointer and device type, in
// registers, test them once outside the loop and vectorize it.
//
// The language has no raw pointers and no way to reinterpret memory, so the classes hold
// for every program. An untagged access still aliases everything, so tagging is opt-in
// per site: a site left untagged costs speed, never correctness. Tag only an access whose
// class is certain.

use inkwell::values::{BasicValue, BasicValueEnum, InstructionValue};

use crate::codegen::context::CodegenContext;
use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

/// The root every class hangs off. One root, so the classes are siblings, which is what
/// makes them disjoint: classes under different roots would alias.
const ROOT: &str = "neuro memory";

#[derive(Debug, Clone, Copy)]
pub(crate) enum MemoryClass {
    /// A number in a tensor's buffer or in the buffer of a `Vec` of numbers. One class for
    /// both, so a tensor that adopts a `Vec`'s buffer is never told apart from it.
    Element,
    /// A field of a DLPack handle.
    TensorHeader,
    /// The variable a tensor's handle is held in.
    TensorHandle,
    /// A field of a collection header (`{ buffer, len, cap, used }`).
    CollectionHeader,
}

impl MemoryClass {
    fn name(self) -> &'static str {
        match self {
            Self::Element => "element",
            Self::TensorHeader => "tensor header",
            Self::TensorHandle => "tensor handle",
            Self::CollectionHeader => "collection header",
        }
    }

    /// Whether a collection element of type `element` is a number, so its slot is an
    /// [`Self::Element`]. Anything else may hold a header or a handle of its own (a
    /// `Vec<Vec<f64>>` slot is a collection header) and stays untagged.
    pub(crate) fn holds_numbers(element: &Type) -> bool {
        element.is_int_like() || element.is_float() || matches!(element, Type::Bool)
    }
}

impl<'ctx> CodegenContext<'ctx> {
    /// Tag `access`, a load or a store, as reaching `class` alone.
    pub(crate) fn tag_access(
        &self,
        access: InstructionValue<'ctx>,
        class: MemoryClass,
    ) -> CodegenResult<()> {
        let offset = self.context.i64_type().const_zero();
        let root = self
            .context
            .metadata_node(&[self.context.metadata_string(ROOT).into()]);
        let node = self.context.metadata_node(&[
            self.context.metadata_string(class.name()).into(),
            root.into(),
            offset.into(),
        ]);
        let tag = self
            .context
            .metadata_node(&[node.into(), node.into(), offset.into()]);
        access
            .set_metadata(tag, self.context.get_kind_id("tbaa"))
            .map_err(|e| CodegenError::LlvmError(e.to_string()))
    }

    /// Tag the load that produced `value`, as [`Self::tag_access`] does, and hand it back.
    pub(crate) fn tag_load(
        &self,
        value: BasicValueEnum<'ctx>,
        class: MemoryClass,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let load = value.as_instruction_value().ok_or_else(|| {
            CodegenError::InternalError("a tagged load is not an instruction".into())
        })?;
        self.tag_access(load, class)?;
        Ok(value)
    }
}
