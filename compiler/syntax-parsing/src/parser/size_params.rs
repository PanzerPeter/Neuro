// Size parameters: a bare generic name re-kinded to an extent or a shape by how its
// declaration uses it.
//
// A function, method, struct or enum decides its own from its signature or fields as it
// is parsed. A name passed to another type's size parameter (an `impl` header, `Ring<N>`
// in a signature or a field) is settled by `kind_passed_sizes` from that type's
// declaration, once every declaration it can name is in hand.

use std::collections::HashMap;

use ast_types::{
    ArraySize, GenericArg, GenericParam, GenericParamKind, Item, TensorDim, TensorExtent, Type,
    VariantPayload,
};

/// Re-kind every bound-less type parameter that `types` use as an array length or a
/// tensor extent into an extent parameter, and one they splice into a tensor shape
/// (`Tensor<T, S>`, `[..S, N]`) into a shape parameter.
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
        if !matches!(generic.kind, GenericParamKind::Type) || !generic.bounds.is_empty() {
            continue;
        }
        // A name used both ways is a shape: the checker then refuses the extent use,
        // which names the conflict, rather than an unknown shape parameter.
        let uses = named.iter().filter(|(name, _)| *name == generic.name.name);
        if let Some(shape) = uses.map(|(_, shape)| *shape).reduce(|a, b| a || b) {
            generic.kind = if shape {
                GenericParamKind::Shape
            } else {
                GenericParamKind::Extent
            };
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

/// Collect the names used as array lengths, tensor extents or shape splices anywhere in
/// `ty`, including under references, sequences, tuples, and other generic applications,
/// each with whether it was a splice.
fn collect_size_names(ty: &Type, out: &mut Vec<(String, bool)>) {
    match ty {
        Type::Tensor {
            element_type,
            shape,
            ..
        } => {
            collect_shape_names(shape, out);
            collect_size_names(element_type, out);
        }
        Type::Array { element, size, .. } => {
            if let ArraySize::Const(name) = size {
                out.push((name.name.clone(), false));
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
                match arg {
                    GenericArg::Type(inner) => collect_size_names(inner, out),
                    GenericArg::Shape { dims, .. } => collect_shape_names(dims, out),
                    GenericArg::Const { .. } => {}
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

fn collect_shape_names(shape: &[TensorDim], out: &mut Vec<(String, bool)>) {
    for dim in shape {
        // The dimension NAME is not a parameter: it lives in the tensor type's own
        // namespace, so only the extent can re-kind a generic.
        match &dim.extent {
            TensorExtent::Param(name) => out.push((name.name.clone(), false)),
            TensorExtent::Splice(name) => out.push((name.name.clone(), true)),
            TensorExtent::Literal(_) | TensorExtent::Dynamic(_) => {}
        }
    }
}

/// Re-kind the bare names an `impl` header, a function or method signature, or a
/// struct's fields pass to a type's extent or shape parameters (`impl<T, CAP> Ring<T, CAP>`
/// and `func f<CAP>(r: &Ring<i32, CAP>)` for `struct Ring<T, CAP> { data: [T; CAP] }`).
///
/// The parser runs this over each file, where a declaration is named as written, and the
/// driver runs it again over the resolved program, where every name is canonical and so
/// an `impl` reaches a type declared in another module. A name two declarations of the
/// file share is skipped here, since only resolution knows which one the `impl` means.
pub fn kind_passed_sizes(items: &mut [Item]) {
    let mut declared: HashMap<String, Option<SizeKinds>> = HashMap::new();
    collect_extent_positions(items, &mut declared);
    rekind_impls(items, &declared);
}

/// Per parameter of a declaration, its kind when it is a size.
type SizeKinds = Vec<Option<GenericParamKind>>;

/// For each generic struct and enum, the kind of each parameter when it is a size (an
/// extent or a shape), or `None` for a name declared more than once.
fn collect_extent_positions(items: &[Item], out: &mut HashMap<String, Option<SizeKinds>>) {
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
            .map(|g| {
                matches!(g.kind, GenericParamKind::Extent | GenericParamKind::Shape)
                    .then(|| g.kind.clone())
            })
            .collect();
        out.entry(name.clone())
            .and_modify(|seen| *seen = None)
            .or_insert(Some(positions));
    }
}

fn rekind_impls(items: &mut [Item], declared: &HashMap<String, Option<SizeKinds>>) {
    for item in items {
        match item {
            Item::Impl(def) => {
                let header = Type::Generic {
                    name: def.type_name.clone(),
                    args: def
                        .type_args
                        .iter()
                        .cloned()
                        .map(GenericArg::Type)
                        .collect(),
                    span: def.type_name.span,
                };
                rekind_passed([&header], declared, &mut def.generics);
                for method in &mut def.methods {
                    let types = method.params.iter().map(|p| &p.ty);
                    let types = types.chain(method.return_type.as_ref());
                    rekind_passed(types, declared, &mut method.generics);
                }
            }
            Item::Function(def) => {
                let types = def.params.iter().map(|p| &p.ty);
                let types = types.chain(def.return_type.as_ref());
                rekind_passed(types, declared, &mut def.generics);
            }
            Item::Struct(def) => {
                let types = def.fields.iter().map(|f| &f.ty);
                rekind_passed(types, declared, &mut def.generics);
            }
            Item::Module(def) => rekind_impls(&mut def.items, declared),
            _ => {}
        }
    }
}

/// Re-kind each bound-less type parameter that `types` pass to a declared type's size
/// parameter (`Ring<N>`, `Layer<S>`) to that parameter's kind.
fn rekind_passed<'a>(
    types: impl IntoIterator<Item = &'a Type>,
    declared: &HashMap<String, Option<SizeKinds>>,
    generics: &mut [GenericParam],
) {
    let mut passed = Vec::new();
    for ty in types {
        collect_passed(ty, declared, &mut passed);
    }
    for (name, kind) in passed {
        if let Some(generic) = generics.iter_mut().find(|g| {
            g.name.name == name && g.bounds.is_empty() && matches!(g.kind, GenericParamKind::Type)
        }) {
            generic.kind = kind;
        }
    }
}

fn collect_passed(
    ty: &Type,
    declared: &HashMap<String, Option<SizeKinds>>,
    out: &mut Vec<(String, GenericParamKind)>,
) {
    match ty {
        Type::Generic { name, args, .. } => {
            let kinds = declared.get(&name.name).and_then(Option::as_ref);
            for (index, arg) in args.iter().enumerate() {
                let GenericArg::Type(inner) = arg else {
                    continue;
                };
                match (inner, kinds.and_then(|k| k.get(index)).cloned().flatten()) {
                    (Type::Named(passed), Some(kind)) => out.push((passed.name.clone(), kind)),
                    _ => collect_passed(inner, declared, out),
                }
            }
        }
        Type::Reference { inner, .. } => collect_passed(inner, declared, out),
        Type::Array { element, .. } | Type::Slice { element, .. } => {
            collect_passed(element, declared, out);
        }
        Type::Tuple { elements, .. } => {
            for element in elements {
                collect_passed(element, declared, out);
            }
        }
        Type::Function { params, ret, .. } => {
            for param in params {
                collect_passed(param, declared, out);
            }
            collect_passed(ret, declared, out);
        }
        Type::Tensor { .. } | Type::Named(_) | Type::ImplTrait { .. } | Type::DynTrait { .. } => {}
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
