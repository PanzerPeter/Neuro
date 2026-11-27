// The standard library's runtime intrinsics, as lowering types them. Semantic analysis has
// already confined each call to a library body; this copy of the table mirrors the checker's
// so the slices stay independent.

use neuro_hir::{HirCollectionKind, HirType};

/// Each intrinsic's parameter types and result.
pub(crate) fn os_intrinsic_signature(name: &str) -> Option<(Vec<HirType>, HirType)> {
    let text = || HirType::Reference {
        inner: Box::new(HirType::String),
        mutable: false,
    };
    let text_and_status = || HirType::Tuple(vec![HirType::String, HirType::I32]);
    let bytes = || HirType::Reference {
        inner: Box::new(HirType::Slice(Box::new(HirType::U8))),
        mutable: false,
    };
    let signature = match name {
        "__read_text" | "__remove_file" | "__env_var" => (vec![text()], text_and_status()),
        "__read_bytes" => (
            vec![text()],
            HirType::Tuple(vec![
                HirType::Collection {
                    kind: HirCollectionKind::Vec,
                    args: vec![HirType::U8],
                },
                HirType::String,
                HirType::I32,
            ]),
        ),
        "__write_text" => (vec![text(), text()], text_and_status()),
        "__write_bytes" => (
            vec![
                text(),
                HirType::Reference {
                    inner: Box::new(HirType::Slice(Box::new(HirType::U8))),
                    mutable: false,
                },
            ],
            text_and_status(),
        ),
        "__read_line" => (vec![], text_and_status()),
        "__extend_bytes" | "__extend_text" => (
            vec![
                HirType::Reference {
                    inner: Box::new(HirType::Collection {
                        kind: HirCollectionKind::Vec,
                        args: vec![HirType::U8],
                    }),
                    mutable: true,
                },
                match name {
                    "__extend_text" => text(),
                    _ => bytes(),
                },
            ],
            HirType::Void,
        ),
        "__utf8_text" => (vec![bytes()], HirType::String),
        "__arg_count" | "__monotonic_nanos" => (vec![], HirType::U64),
        "__arg" => (vec![HirType::U64], HirType::String),
        _ => return None,
    };
    Some(signature)
}
