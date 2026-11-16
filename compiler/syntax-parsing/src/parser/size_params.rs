// Size parameters: a bare generic name re-kinded to an extent by how its declaration uses it.
//
// A function, method, struct or enum decides its own from its signature or fields as it
// is parsed. An `impl` header passes its names to a type declared elsewhere, so it is
// settled by `kind_impl_extents` once every declaration it can name is in hand.

use std::collections::HashMap;

use ast_types::{
    ArraySize, GenericArg, GenericParam, GenericParamKind, Item, TensorExtent, Type, VariantPayload,
};

/// Re-kind every bound-less type parameter that `types` use as an array length or a
/// tensor extent into an extent parameter.
///
/// A parameter carrying trait bounds is left alone: a bound names a type, so the two
/// readings cannot both be right, and leaving it makes the checker report the extent as
/// an unknown one rather than silently changing what the bound meant. A `const`
/// parameter is left alone too, so the checker can refuse it in the size position.
pub(super) fn rekind_size_params<'a>(
    types: impl IntoIterator<Item = &'a Type>,
    generics: &mut [GenericParam],
) {
    let mut named = Vec::new();
    for ty in types {
        collect_size_names(ty, &mut named);
    }
    for generic in generics.iter_mut() {
        if matches!(generic.kind, GenericParamKind::Type)
            && generic.bounds.is_empty()
            && named.contains(&generic.name.name)
        {
            generic.kind = GenericParamKind::Extent;
        }
    }
}

/// The types a variant's payload carries.
pub(super) fn payload_types(payload: &VariantPayload) -> Vec<&Type> {
    match payload {
        VariantPayload::Unit => Vec::new(),
        VariantPayload::Tuple(types) => types.iter().collect(),
        VariantPayload::Struct(fields) => fields.iter().map(|f| &f.ty).collect(),
    }
}

/// Collect the names used as array lengths or tensor extents anywhere in `ty`,
/// including under references, sequences, tuples, and other generic applications.
fn collect_size_names(ty: &Type, out: &mut Vec<String>) {
    match ty {
        Type::Tensor {
            element_type,
            shape,
            ..
        } => {
            for dim in shape {
                // The dimension NAME is not a parameter: it lives in the tensor type's
                // own namespace, so only the extent can re-kind a generic.
                if let TensorExtent::Param(name) = &dim.extent {
                    out.push(name.name.clone());
                }
            }
            collect_size_names(element_type, out);
        }
        Type::Array { element, size, .. } => {
            if let ArraySize::Const(name) = size {
                out.push(name.name.clone());
            }
            collect_size_names(element, out);
        }
        Type::Reference { inner, .. } => collect_size_names(inner, out),
        Type::Slice { element, .. } => collect_size_names(element, out),
        Type::Tuple { elements, .. } => {
            for element in elements {
                collect_size_names(element, out);
            }
        }
        Type::Generic { args, .. } => {
            for arg in args {
                if let GenericArg::Type(inner) = arg {
                    collect_size_names(inner, out);
                }
            }
        }
        Type::Function { params, ret, .. } => {
            for param in params {
                collect_size_names(param, out);
            }
            collect_size_names(ret, out);
        }
        Type::Named(_) | Type::ImplTrait { .. } | Type::DynTrait { .. } => {}
    }
}

/// Re-kind an `impl` header's bare names passed to a type's extent parameters
/// (`impl<T, CAP> Ring<T, CAP>` for `struct Ring<T, CAP> { data: [T; CAP] }`).
///
/// The parser runs this over each file, where a declaration is named as written, and the
/// driver runs it again over the resolved program, where every name is canonical and so
/// an `impl` reaches a type declared in another module. A name two declarations of the
/// file share is skipped here, since only resolution knows which one the `impl` means.
pub fn kind_impl_extents(items: &mut [Item]) {
    let mut declared: HashMap<String, Option<Vec<bool>>> = HashMap::new();
    collect_extent_positions(items, &mut declared);
    rekind_impls(items, &declared);
}

/// For each generic struct and enum, which of its parameters are extents, or `None`
/// for a name declared more than once.
fn collect_extent_positions(items: &[Item], out: &mut HashMap<String, Option<Vec<bool>>>) {
    for item in items {
        let (name, generics) = match item {
            Item::Struct(def) => (&def.name.name, &def.generics),
            Item::Enum(def) => (&def.name.name, &def.generics),
            Item::Module(def) => {
                collect_extent_positions(&def.items, out);
                continue;
            }
            _ => continue,
        };
        let positions = generics
            .iter()
            .map(|g| matches!(g.kind, GenericParamKind::Extent))
            .collect();
        out.entry(name.clone())
            .and_modify(|seen| *seen = None)
            .or_insert(Some(positions));
    }
}

fn rekind_impls(items: &mut [Item], declared: &HashMap<String, Option<Vec<bool>>>) {
    for item in items {
        let def = match item {
            Item::Impl(def) => def,
            Item::Module(def) => {
                rekind_impls(&mut def.items, declared);
                continue;
            }
            _ => continue,
        };
        let Some(Some(positions)) = declared.get(&def.type_name.name) else {
            continue;
        };
        for (arg, is_extent) in def.type_args.iter().zip(positions) {
            let Type::Named(name) = arg else { continue };
            if !is_extent {
                continue;
            }
            if let Some(generic) = def.generics.iter_mut().find(|g| {
                g.name.name == name.name
                    && g.bounds.is_empty()
                    && matches!(g.kind, GenericParamKind::Type)
            }) {
                generic.kind = GenericParamKind::Extent;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parse;
    use ast_types::{GenericParamKind, Item};

    fn kinds(item: &Item) -> Vec<&GenericParamKind> {
        let generics = match item {
            Item::Function(def) => &def.generics,
            Item::Struct(def) => &def.generics,
            Item::Enum(def) => &def.generics,
            Item::Impl(def) => &def.generics,
            other => panic!("no generic list on {other:?}"),
        };
        generics.iter().map(|g| &g.kind).collect()
    }

    fn parsed(source: &str) -> Vec<Item> {
        parse(source).expect("parses")
    }

    #[test]
    fn an_array_length_makes_a_function_parameter_an_extent() {
        let items = parsed("func f<T, N>(a: [T; N]) -> T { a[0] }");
        assert_eq!(
            kinds(&items[0]),
            [&GenericParamKind::Type, &GenericParamKind::Extent]
        );
    }

    #[test]
    fn struct_fields_and_enum_payloads_size_their_parameters() {
        let items = parsed(
            "struct Dense<IN, OUT, T> { w: Tensor<f32, [IN, OUT]>, tag: T }\n\
             enum Slot<N> { Empty, Full { t: Tensor<i32, [N]> } }",
        );
        assert_eq!(
            kinds(&items[0]),
            [
                &GenericParamKind::Extent,
                &GenericParamKind::Extent,
                &GenericParamKind::Type
            ]
        );
        assert_eq!(kinds(&items[1]), [&GenericParamKind::Extent]);
    }

    #[test]
    fn an_impl_takes_the_kind_its_type_declares_wherever_the_type_is_written() {
        let items = parsed(
            "impl<T, CAP> Ring<T, CAP> { }\n\
             struct Ring<T, CAP> { data: [T; CAP] }",
        );
        assert_eq!(
            kinds(&items[0]),
            [&GenericParamKind::Type, &GenericParamKind::Extent]
        );
    }

    #[test]
    fn a_const_or_bounded_parameter_keeps_its_kind() {
        let items = parsed(
            "func f<const N: u32, M: Copy>(a: [i32; N], t: Tensor<f32, [M]>) { }\n\
             struct Ring<CAP> { data: [i32; CAP] }\n\
             impl<const CAP: u64> Ring<CAP> { }",
        );
        let [first, second] = kinds(&items[0])[..] else {
            panic!("two parameters");
        };
        assert!(matches!(first, GenericParamKind::Const(_)));
        assert_eq!(second, &GenericParamKind::Type);
        assert!(matches!(kinds(&items[2])[0], GenericParamKind::Const(_)));
    }

    /// Two declarations of one name in a file are told apart only by module resolution,
    /// which the driver's own pass follows.
    #[test]
    fn an_impl_of_a_name_declared_twice_is_left_for_the_resolved_program() {
        let items = parsed(
            "struct Ring<CAP> { data: [i32; CAP] }\n\
             module inner { struct Ring<T> { v: T } }\n\
             impl<CAP> Ring<CAP> { }",
        );
        assert_eq!(kinds(&items[2]), [&GenericParamKind::Type]);
    }
}
