// The standard library's runtime intrinsics: what `std::fs`, `std::env`, `std::io` and
// `std::time` call to reach the operating system.
//
// A program cannot name them. Each resolves only while a library body is being checked, so a
// call written anywhere else is an undefined function, and a declaration cannot take the name
// either, because every one contains the reserved `__`.

use super::TypeChecker;
use crate::types::{CollectionKind, Type};
use ast_types::{Expr, Item, ModuleId, PRELUDE_MODULE};
use shared_types::Span;
use std::collections::HashSet;

/// The prefix module resolution gives every standard-library name outside the prelude.
const LIBRARY_PREFIX: &str = "std::";

/// Each intrinsic's parameter types and result. Mirrored in `hir-lowering` and in the
/// backend's `os_runtime.rs`, which own their copies so no slice depends on another.
pub(crate) fn os_intrinsic_signature(name: &str) -> Option<(Vec<Type>, Type)> {
    let text = || Type::Reference {
        inner: Box::new(Type::String),
        mutable: false,
    };
    let text_and_status = || Type::Tuple(vec![Type::String, Type::I32]);
    let bytes = || Type::Reference {
        inner: Box::new(Type::Slice(Box::new(Type::U8))),
        mutable: false,
    };
    let signature = match name {
        "__read_text" | "__remove_file" | "__env_var" => (vec![text()], text_and_status()),
        "__read_bytes" => (
            vec![text()],
            Type::Tuple(vec![
                Type::Collection {
                    kind: CollectionKind::Vec,
                    args: vec![Type::U8],
                },
                Type::String,
                Type::I32,
            ]),
        ),
        "__write_text" => (vec![text(), text()], text_and_status()),
        "__write_bytes" => (
            vec![
                text(),
                Type::Reference {
                    inner: Box::new(Type::Slice(Box::new(Type::U8))),
                    mutable: false,
                },
            ],
            text_and_status(),
        ),
        "__read_line" => (vec![], text_and_status()),
        // Byte copies the backend emits inline: a run of bytes or a text's bytes appended in
        // one copy, and a run the caller has checked is UTF-8 copied out as text.
        "__extend_bytes" | "__extend_text" => (
            vec![
                Type::Reference {
                    inner: Box::new(Type::Collection {
                        kind: CollectionKind::Vec,
                        args: vec![Type::U8],
                    }),
                    mutable: true,
                },
                match name {
                    "__extend_text" => text(),
                    _ => bytes(),
                },
            ],
            Type::Void,
        ),
        "__utf8_text" => (vec![bytes()], Type::String),
        "__arg_count" | "__monotonic_nanos" => (vec![], Type::U64),
        "__arg" => (vec![Type::U64], Type::String),
        _ => return None,
    };
    Some(signature)
}

/// The modules of the standard library in `items`: the prelude, and every module declaring a
/// `std::` name, which only the library can.
pub(crate) fn library_modules(items: &[Item]) -> HashSet<ModuleId> {
    let mut modules: HashSet<ModuleId> = items
        .iter()
        .filter_map(|item| {
            let (name, module) = match item {
                Item::Function(def) => (&def.name.name, def.module),
                Item::Struct(def) => (&def.name.name, def.module),
                _ => return None,
            };
            name.starts_with(LIBRARY_PREFIX).then_some(module)
        })
        .collect();
    modules.insert(PRELUDE_MODULE);
    modules
}

impl TypeChecker {
    /// Type-check a call to a runtime intrinsic from a library body, or answer `None` so the
    /// caller reports whatever an unknown name gets.
    pub(super) fn resolve_os_intrinsic(
        &mut self,
        func_name: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        if !self.library_modules.contains(&self.current_module) {
            return None;
        }
        let (params, ret) = os_intrinsic_signature(func_name)?;
        self.check_call_args(args, &params, span);
        Some(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_intrinsic_name_is_reserved() {
        for name in [
            "__read_text",
            "__read_bytes",
            "__write_text",
            "__write_bytes",
            "__remove_file",
            "__env_var",
            "__read_line",
            "__arg_count",
            "__arg",
            "__monotonic_nanos",
            "__extend_bytes",
            "__extend_text",
            "__utf8_text",
        ] {
            assert!(os_intrinsic_signature(name).is_some(), "{name}");
            assert!(name.contains("__"), "{name} could be declared by a program");
        }
        assert!(os_intrinsic_signature("read_text").is_none());
    }
}
