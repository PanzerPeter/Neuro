//! The standard collections `Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`, and the
//! growable text buffer `StringBuilder`.
//!
//! They are specified as library types, but the language exposes no allocator and no
//! raw pointers, so nothing in `.nr` source could implement them: the compiler knows
//! them all by name and lowers their operations directly. Everything user-visible is
//! still ordinary type checking: a collection type is a nominal type with type
//! arguments, its operations are builtin methods, and it obeys move-by-default.
//!
//! `StringBuilder` joins the family because it is the same machine: one growable heap buffer
//! behind an owning header. Its element type is fixed (UTF-8 bytes) rather than a type
//! argument, so it is the one nullary kind.

use ast_types::Expr;
use shared_types::Span;

use super::TypeChecker;
use super::derivable::{COPY_TRAIT, HASHABLE_TRAIT, PARTIAL_EQ_TRAIT};
use crate::errors::TypeError;
use crate::types::{CollectionKind, Type};

/// The `Option<T>` returned by the fallible readers (`Vec::pop`, `Map::get`). It comes
/// from the prelude rather than the compiler, so a program compiled without one gets a
/// plain "unknown type" diagnostic instead of a silently wrong result type.
pub(crate) const OPTION_ENUM: &str = "Option";

impl TypeChecker {
    /// Resolve a `Vec<T>` / `HashMap<K, V>` / `BTreeMap<K, V>` / `StringBuilder` annotation,
    /// validating the argument count and each element/key type. Returns `None` after
    /// recording a diagnostic when the application is ill-formed.
    pub(crate) fn resolve_collection(
        &mut self,
        kind: CollectionKind,
        args: Vec<Type>,
        span: Span,
    ) -> Option<Type> {
        if args.len() != kind.arity() {
            self.record_error(TypeError::GenericArgCountMismatch {
                name: kind.name().to_string(),
                expected: kind.arity(),
                found: args.len(),
                span,
            });
            return None;
        }

        let keyed = matches!(kind, CollectionKind::HashMap | CollectionKind::BTreeMap);
        if keyed {
            self.check_key_type(kind, &args[0], span)?;
        }
        for value_ty in args.iter().skip(usize::from(keyed)) {
            self.check_storable(kind, value_ty, span)?;
        }

        Some(Type::Collection { kind, args })
    }

    /// Whether a value of `ty` can live inside a collection's buffer.
    ///
    /// A slot owns what it holds, so any sized value qualifies: a scalar, a `string`, a
    /// struct, a tensor, another collection. A reference is excluded because the borrow
    /// checker cannot see through a heap buffer to verify the referent outlives the
    /// collection, and a function value because its environment count is not released
    /// slot by slot. A type parameter answers for its instances, which are checked
    /// where they are written.
    fn check_storable(&mut self, kind: CollectionKind, ty: &Type, span: Span) -> Option<()> {
        let storable = !matches!(
            ty,
            Type::ConstValue(_) | Type::Void | Type::DynObject(_) | Type::Unknown
        ) && !self.type_holds_reference(ty)
            && !self.holds_function_value(ty);
        if storable {
            return Some(());
        }
        self.record_error(TypeError::InvalidCollectionElement {
            collection: kind.name().to_string(),
            ty: ty.clone(),
            span,
        });
        None
    }

    /// Validate a map key type. Keys need equality plus, per map, a hash or a total
    /// order; the compiler supplies both for the builtin scalar and `string` keys, and
    /// a user type supplies them through the corresponding trait impls.
    fn check_key_type(&mut self, kind: CollectionKind, ty: &Type, span: Span) -> Option<()> {
        let reject = |checker: &mut Self, reason: String| -> Option<()> {
            checker.record_error(TypeError::InvalidCollectionKey {
                collection: kind.name().to_string(),
                ty: ty.clone(),
                reason,
                span,
            });
            None
        };

        if ty.is_float() || ty.is_half_float() {
            return reject(
                self,
                "floating-point values have no total order (NaN compares false against \
                 everything); wrap the key in the standard `OrderedF32` / `OrderedF64` \
                 struct, which rejects NaN"
                    .to_string(),
            );
        }
        if ty.is_integer() || matches!(ty, Type::Bool | Type::Char | Type::String) {
            return Some(());
        }
        // A struct key routes equality, hashing and ordering through the traits it
        // implements, derived or written by hand, so each one the map needs must be there.
        // A type parameter supplies them through its bounds, and each instance's key is
        // one of the kinds above.
        let (Type::Struct(name) | Type::Generic(name)) = ty else {
            return reject(
                self,
                "only integer, `bool`, `char`, `string`, struct and type-parameter keys are \
                 supported"
                    .to_string(),
            );
        };
        let name = name.clone();
        let mut required = vec![PARTIAL_EQ_TRAIT];
        // A slot bit-copies its key, so an abstract key must be `Copy` in every instance.
        if matches!(ty, Type::Generic(_)) {
            required.push(COPY_TRAIT);
        }
        match kind {
            CollectionKind::HashMap => required.push(HASHABLE_TRAIT),
            CollectionKind::BTreeMap => required.push("Comparable"),
            CollectionKind::Vec | CollectionKind::StringBuilder => {}
        }
        for trait_name in required {
            if !self.satisfies_bound(ty, trait_name) {
                return reject(
                    self,
                    format!("map keys require `{}` on {}", trait_name, name),
                );
            }
        }
        Some(())
    }

    /// Type-check `Vec::new()` / `HashMap::new()` / `BTreeMap::new()` / `StringBuilder::new()`,
    /// and `Vec::with_capacity(n)`, which takes the capacity as its one integer argument.
    ///
    /// The element types come from the expected type: an empty collection carries no
    /// value to infer from, so the binding must be annotated. A nullary kind has nothing
    /// to infer, so it needs no annotation.
    pub(crate) fn check_collection_new(
        &mut self,
        kind: CollectionKind,
        with_capacity: bool,
        args: &[Expr],
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        let arity = usize::from(with_capacity);
        if args.len() != arity {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: arity,
                found: args.len(),
                span,
            });
        }
        for arg in args.iter().take(arity) {
            if let Some(arg_ty) = self.check_expr(arg, Some(&Type::U64))
                && !matches!(arg_ty, Type::Unknown)
                && !arg_ty.is_integer()
            {
                self.record_error(TypeError::Mismatch {
                    expected: Type::U64,
                    found: arg_ty,
                    span: arg.span(),
                });
            }
        }
        if kind.arity() == 0 {
            return Type::Collection {
                kind,
                args: Vec::new(),
            };
        }
        match expected {
            Some(Type::Collection {
                kind: expected_kind,
                args,
            }) if *expected_kind == kind => Type::Collection {
                kind,
                args: args.clone(),
            },
            _ => {
                self.record_error(TypeError::CollectionTypeNotInferable {
                    name: kind.name().to_string(),
                    span,
                });
                Type::Unknown
            }
        }
    }

    /// Resolve a method call on a collection receiver, returning its result type, or
    /// `None` when the method is not part of the collection's surface (the caller then
    /// reports `MethodNotFound`).
    pub(crate) fn resolve_collection_method(
        &mut self,
        recv: &Type,
        object: &Expr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        let Type::Collection { kind, args: params } = recv.referent().clone() else {
            return None;
        };
        let spec = collection_method(kind, method)?;

        if spec.mutating {
            self.check_mut_self_receiver(object, recv, span);
        }

        let expected_args: Vec<Type> = spec
            .params
            .iter()
            .map(|slot| slot.resolve(&params))
            .collect();
        if args.len() != expected_args.len() {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: expected_args.len(),
                found: args.len(),
                span,
            });
        }
        for (arg, expected_ty) in args.iter().zip(expected_args.iter()) {
            // An index argument accepts any integer width, matching `arr[i]`; every
            // other argument is the collection's own key or element type.
            if let Some(arg_ty) = self.check_expr(arg, Some(expected_ty)) {
                let ok = match spec.params.first() {
                    Some(ParamSlot::Index) => {
                        matches!(arg_ty, Type::Unknown) || arg_ty.is_integer()
                    }
                    // Appended text is read, never stored, so an immutable borrow is as
                    // good as an owned `string`, the same latitude `+` gives its operands.
                    Some(ParamSlot::Text) => {
                        matches!(&arg_ty, Type::String | Type::Unknown)
                            || matches!(
                                &arg_ty,
                                Type::Reference { inner, mutable: false } if matches!(**inner, Type::String)
                            )
                    }
                    _ => self.assignable(&arg_ty, expected_ty),
                };
                if !ok {
                    self.record_error(TypeError::Mismatch {
                        expected: expected_ty.clone(),
                        found: arg_ty,
                        span: arg.span(),
                    });
                }
            }
        }

        // `push` and `insert` keep what they are handed, so an outliving receiver is a
        // channel out of a `pool` the way a `&mut self` method is, and the argument is
        // moved into the slot like one passed by value.
        if spec.params.contains(&ParamSlot::Value) {
            self.check_pool_collection_store(object, method, args, &expected_args, span);
            for (arg, slot) in args.iter().zip(spec.params) {
                if matches!(slot, ParamSlot::Key | ParamSlot::Value) {
                    self.record_move(arg);
                }
            }
        }

        // A borrowing reader holds the collection for the statement, as `&mut self`
        // receivers already do, so a mutation in the same statement (an arm of the
        // `match` it is the scrutinee of) cannot reallocate the slot it lent.
        if matches!(spec.result, ResultShape::OptionBorrow { mutable: false })
            && Self::is_bare_binding(object)
            && let Some(root) = Self::place_root_name(object)
        {
            self.symbols.add_transient_borrow(&root, false);
        }

        Some(spec.result.resolve_result(self, &params, span))
    }

    /// Whether `object.method(..)` is a collection reader that lends a slot mutably
    /// (`get_mut`), which holds the receiver exclusively for as long as its result lives.
    pub(crate) fn collection_lends_mutably(&self, object: &Expr, method: &str) -> bool {
        let Some(Type::Collection { kind, .. }) =
            self.projection_type(object).map(|ty| ty.referent().clone())
        else {
            return false;
        };
        collection_method(kind, method)
            .is_some_and(|spec| matches!(spec.result, ResultShape::OptionBorrow { mutable: true }))
    }

    /// The element type produced by `v[i]` and by `for x in v`, or `None` when the
    /// collection is not indexable/iterable (the maps are neither).
    pub(crate) fn collection_element(&self, ty: &Type) -> Option<Type> {
        match ty.referent() {
            Type::Collection {
                kind: CollectionKind::Vec,
                args,
            } => args.first().cloned(),
            _ => None,
        }
    }

    /// Instantiate `Option<T>` for a fallible reader's result. The prelude declares it;
    /// a program without one gets `UnknownTypeName` rather than a wrong type.
    pub(crate) fn option_of(&mut self, inner: Type, span: Span) -> Type {
        if !self.is_generic_enum(OPTION_ENUM) {
            self.record_error(TypeError::UnknownTypeName {
                name: OPTION_ENUM.to_string(),
                span,
            });
            return Type::Unknown;
        }
        self.instantiate_generic(OPTION_ENUM, &[inner], span)
    }
}

/// Which of a collection's type arguments a method parameter takes.
#[derive(Clone, Copy, PartialEq)]
enum ParamSlot {
    /// A `u64`-shaped position index (`Vec::get`).
    Index,
    /// The collection's key type: argument 0 of a map.
    Key,
    /// The collection's element/value type: the last type argument.
    Value,
    /// Borrowed UTF-8 text: a `string` or an immutable `&string` (`StringBuilder::push_str`).
    Text,
}

impl ParamSlot {
    fn resolve(self, params: &[Type]) -> Type {
        match self {
            ParamSlot::Index => Type::U64,
            ParamSlot::Key => params.first().cloned().unwrap_or(Type::Unknown),
            ParamSlot::Value => params.last().cloned().unwrap_or(Type::Unknown),
            ParamSlot::Text => Type::String,
        }
    }
}

/// The shape of a collection method's result.
#[derive(Clone, Copy)]
enum ResultShape {
    Unit,
    Bool,
    Len,
    /// `Option<T>` over the element/value type, which the call moves out of its slot.
    OptionValue,
    /// `Option<&T>` / `Option<&mut T>` over the element/value type, borrowing the slot.
    OptionBorrow {
        mutable: bool,
    },
    /// The element type itself, moved out of its slot (`Vec::remove`).
    Value,
    /// A freshly built `Vec<K>` of the map's keys.
    KeyVec,
    /// A freshly allocated owned immutable `string` (`StringBuilder::to_string`).
    OwnedString,
}

impl ResultShape {
    fn resolve_result(self, checker: &mut TypeChecker, params: &[Type], span: Span) -> Type {
        match self {
            ResultShape::Unit => Type::Void,
            ResultShape::Bool => Type::Bool,
            ResultShape::Len => Type::U64,
            ResultShape::OptionValue => {
                let inner = ParamSlot::Value.resolve(params);
                checker.option_of(inner, span)
            }
            ResultShape::OptionBorrow { mutable } => {
                let inner = Type::Reference {
                    inner: Box::new(ParamSlot::Value.resolve(params)),
                    mutable,
                };
                checker.option_of(inner, span)
            }
            ResultShape::Value => ParamSlot::Value.resolve(params),
            ResultShape::KeyVec => Type::Collection {
                kind: CollectionKind::Vec,
                args: vec![ParamSlot::Key.resolve(params)],
            },
            ResultShape::OwnedString => Type::String,
        }
    }
}

/// One entry of the compiler-known collection method surface.
struct MethodSpec {
    params: &'static [ParamSlot],
    result: ResultShape,
    /// Whether the call needs an exclusive borrow of the receiver.
    mutating: bool,
}

/// Look up `method` in the surface of `kind`, or `None` if it has no such method.
fn collection_method(kind: CollectionKind, method: &str) -> Option<MethodSpec> {
    let spec = match (kind, method) {
        (_, "len") => MethodSpec {
            params: &[],
            result: ResultShape::Len,
            mutating: false,
        },
        (_, "clear") => MethodSpec {
            params: &[],
            result: ResultShape::Unit,
            mutating: true,
        },
        (CollectionKind::Vec, "push") => MethodSpec {
            params: &[ParamSlot::Value],
            result: ResultShape::Unit,
            mutating: true,
        },
        (CollectionKind::Vec, "pop") => MethodSpec {
            params: &[],
            result: ResultShape::OptionValue,
            mutating: true,
        },
        (CollectionKind::Vec, "get") => MethodSpec {
            params: &[ParamSlot::Index],
            result: ResultShape::OptionBorrow { mutable: false },
            mutating: false,
        },
        (CollectionKind::Vec, "remove" | "swap_remove") => MethodSpec {
            params: &[ParamSlot::Index],
            result: ResultShape::Value,
            mutating: true,
        },
        (CollectionKind::HashMap | CollectionKind::BTreeMap, "insert") => MethodSpec {
            params: &[ParamSlot::Key, ParamSlot::Value],
            result: ResultShape::OptionValue,
            mutating: true,
        },
        (CollectionKind::HashMap | CollectionKind::BTreeMap, "get") => MethodSpec {
            params: &[ParamSlot::Key],
            result: ResultShape::OptionBorrow { mutable: false },
            mutating: false,
        },
        (CollectionKind::HashMap | CollectionKind::BTreeMap, "get_mut") => MethodSpec {
            params: &[ParamSlot::Key],
            result: ResultShape::OptionBorrow { mutable: true },
            mutating: true,
        },
        (CollectionKind::HashMap | CollectionKind::BTreeMap, "contains_key") => MethodSpec {
            params: &[ParamSlot::Key],
            result: ResultShape::Bool,
            mutating: false,
        },
        (CollectionKind::HashMap | CollectionKind::BTreeMap, "remove") => MethodSpec {
            params: &[ParamSlot::Key],
            result: ResultShape::OptionValue,
            mutating: true,
        },
        (CollectionKind::StringBuilder, "push_str") => MethodSpec {
            params: &[ParamSlot::Text],
            result: ResultShape::Unit,
            mutating: true,
        },
        (CollectionKind::StringBuilder, "to_string") => MethodSpec {
            params: &[],
            result: ResultShape::OwnedString,
            mutating: false,
        },
        (CollectionKind::HashMap | CollectionKind::BTreeMap, "keys") => MethodSpec {
            params: &[],
            result: ResultShape::KeyVec,
            mutating: false,
        },
        _ => return None,
    };
    Some(spec)
}

#[cfg(test)]
mod tests {
    use crate::type_check;
    use syntax_parsing::parse;

    /// The prelude is prepended by the driver, not by `type_check`, so slice-level
    /// tests declare the `Option` the fallible readers return.
    const OPTION_DECL: &str = "enum Option<T> { Some(T), None }\n";

    fn errors(source: &str) -> Vec<String> {
        let ast = parse(&format!("{}{}", OPTION_DECL, source)).expect("source should parse");
        match type_check(&ast) {
            Ok(_) => Vec::new(),
            Err(errs) => errs.iter().map(|e| e.to_string()).collect(),
        }
    }

    #[test]
    fn vec_push_and_len_check() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut v: Vec<i32> = Vec::new()
                v.push(7)
                val n: u64 = v.len()
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn vec_new_without_annotation_is_rejected() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut v = Vec::new()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("cannot infer the element type")),
            "expected an inference diagnostic, got {errs:?}"
        );
    }

    #[test]
    fn regression_bug_102_rejected_annotation_does_not_ask_for_an_annotation() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut v: Vec<&i32> = Vec::new()
                return 0
            }
            "#,
        );
        assert_eq!(
            errs.len(),
            1,
            "expected only the element error, got {errs:?}"
        );
        assert!(errs[0].contains("cannot be stored"), "got {errs:?}");
    }

    #[test]
    fn push_on_immutable_vec_is_rejected() {
        let errs = errors(
            r#"
            func main() -> i32 {
                val v: Vec<i32> = Vec::new()
                v.push(1)
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("mutably")),
            "expected a mutability diagnostic, got {errs:?}"
        );
    }

    #[test]
    fn float_map_key_is_rejected_with_wrapper_hint() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut m: BTreeMap<f64, i32> = BTreeMap::new()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("OrderedF64")),
            "expected the wrapper hint, got {errs:?}"
        );
    }

    #[test]
    fn struct_key_without_trait_impls_is_rejected() {
        let errs = errors(
            r#"
            @derive(Copy, Clone)
            struct Id { n: i32 }
            func main() -> i32 {
                mut m: HashMap<Id, i32> = HashMap::new()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("require `PartialEq` on Id")),
            "expected a missing-impl diagnostic, got {errs:?}"
        );
    }

    #[test]
    fn collection_is_moved_on_assignment() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut a: Vec<i32> = Vec::new()
                val b: Vec<i32> = a
                val n: u64 = a.len()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("use of moved value 'a'")),
            "expected a move diagnostic, got {errs:?}"
        );
    }

    #[test]
    fn map_get_yields_option_of_the_value_type() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut m: HashMap<string, i32> = HashMap::new()
                m.insert("a", 1)
                val hit: Option<&i32> = m.get("a")
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn lookup_key_is_not_moved() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut m: HashMap<string, i32> = HashMap::new()
                val k: string = "key"
                val hit: bool = m.contains_key(k)
                val n: u64 = k.len()
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn string_builder_needs_no_annotation() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut b = StringBuilder::new()
                b.push_str("hi")
                val n: u64 = b.len()
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn push_str_accepts_a_borrow_and_leaves_it_usable() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut b: StringBuilder = StringBuilder::new()
                val piece: string = "text"
                b.push_str(&piece)
                b.push_str(piece)
                val n: u64 = piece.len()
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn to_string_yields_an_owned_string() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut b = StringBuilder::new()
                b.push_str("a")
                val out: string = b.to_string() + "!"
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn string_builder_takes_no_type_arguments() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut b: StringBuilder<i32> = StringBuilder::new()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("expects 0 type argument")),
            "expected an arity diagnostic, got {errs:?}"
        );
    }

    #[test]
    fn push_str_on_immutable_builder_is_rejected() {
        let errs = errors(
            r#"
            func main() -> i32 {
                val b = StringBuilder::new()
                b.push_str("x")
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("mutably")),
            "expected a mutability diagnostic, got {errs:?}"
        );
    }

    #[test]
    fn a_declared_string_builder_type_shadows_the_builder() {
        let errs = errors(
            r#"
            @derive(Copy, Clone)
            struct StringBuilder { n: i32 }
            func main() -> i32 {
                val s: StringBuilder = StringBuilder { n: 1 }
                return s.n
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn the_retired_builder_name_is_not_a_type() {
        let errs = errors(
            r#"
            func fill(b: &mut String) {}
            func main() -> i32 {
                mut b = String::new()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("unknown type name 'String'"))
                && errs.iter().any(|e| e.contains("unknown type 'String'")),
            "expected the annotation and the constructor both rejected, got {errs:?}"
        );
    }

    #[test]
    fn any_sized_value_is_a_collection_element() {
        let errs = errors(
            r#"
            struct P { name: string }
            func main() -> i32 {
                mut people: Vec<P> = Vec::new()
                mut builders: Vec<StringBuilder> = Vec::new()
                mut grid: Vec<Vec<i32>> = Vec::new()
                mut index: HashMap<string, Vec<string>> = HashMap::new()
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn a_reference_element_is_refused() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut v: Vec<&string> = Vec::new()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("cannot be stored")),
            "expected the element error, got {errs:?}"
        );
    }

    #[test]
    fn insertion_moves_the_value_and_the_key() {
        let errs = errors(
            r#"
            struct P { name: string }
            func main() -> i32 {
                mut v: Vec<P> = Vec::new()
                val p = P { name: "a" }
                v.push(p)
                val n = p.name.len()
                mut m: HashMap<string, i32> = HashMap::new()
                val k: string = "key"
                m.insert(k, 1)
                val l = k.len()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("use of moved value 'p'"))
                && errs.iter().any(|e| e.contains("use of moved value 'k'")),
            "expected both moves reported, got {errs:?}"
        );
    }

    #[test]
    fn a_non_copy_element_by_value_names_the_ways_out() {
        let errs = errors(
            r#"
            struct P { name: string }
            func keep(p: P) -> i32 { 0 }
            func main() -> i32 {
                mut v: Vec<P> = Vec::new()
                v.push(P { name: "a" })
                val p = v[0]
                keep(v[0])
                val s = v[0].name
                return 0
            }
            "#,
        );
        let slot_errors: Vec<&String> = errs
            .iter()
            .filter(|e| e.contains("out of a `Vec` slot"))
            .collect();
        assert_eq!(slot_errors.len(), 3, "got {errs:?}");
        assert!(
            slot_errors[0].contains("&v[i]")
                && slot_errors[0].contains("v[i].clone()")
                && slot_errors[0].contains("v.remove(i)"),
            "got {errs:?}"
        );
    }

    #[test]
    fn a_slot_is_a_place_for_writes_borrows_and_receivers() {
        let errs = errors(
            r#"
            struct P { name: string, n: i32 }
            impl P {
                func bump(&mut self) { self.n += 1 }
            }
            func main() -> i32 {
                mut v: Vec<P> = Vec::new()
                v.push(P { name: "a", n: 1 })
                v[0] = P { name: "b", n: 2 }
                v[0].n = 3
                v[0].bump()
                val n: u64 = v[0].name.len()
                val r: &P = &v[0]
                return r.n
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn readers_lend_and_takers_hand_over() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut v: Vec<string> = Vec::new()
                v.push("a")
                {
                    val lent: Option<&string> = v.get(0)
                }
                val taken: string = v.remove(0)
                mut m: HashMap<string, i32> = HashMap::new()
                val displaced: Option<i32> = m.insert("a", 1)
                {
                    val shared: Option<&i32> = m.get("a")
                }
                val gone: Option<i32> = m.remove("a")
                return 0
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn get_mut_holds_the_map_exclusively() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut m: HashMap<string, i32> = HashMap::new()
                val r: Option<&mut i32> = m.get_mut("a")
                val n = m.len()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("while it is mutably borrowed")),
            "expected the exclusive borrow to freeze the map, got {errs:?}"
        );
    }

    #[test]
    fn a_borrowed_head_lends_its_elements() {
        let errs = errors(
            r#"
            struct P { name: string }
            func keep(p: P) -> i32 { 0 }
            func main() -> i32 {
                mut v: Vec<P> = Vec::new()
                for p in &v {
                    keep(p)
                }
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("cannot move out of 'p'")),
            "got {errs:?}"
        );
    }

    #[test]
    fn a_by_value_head_over_non_copy_elements_consumes_the_vec() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut v: Vec<string> = Vec::new()
                for s in v {
                    val n = s.len()
                }
                val after = v.len()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("use of moved value 'v'")),
            "got {errs:?}"
        );
    }

    #[test]
    fn coalescing_a_borrowed_copy_payload_reads_through() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut m: HashMap<string, i32> = HashMap::new()
                val n: i32 = m.get("a") ?? 0
                val chained: i32 = m.get("b") ?? m.get("c") ?? -1
                return n + chained
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn with_capacity_takes_an_integer_and_an_annotation() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut v: Vec<string> = Vec::with_capacity(16)
                mut w: Vec<i32> = Vec::with_capacity("x")
                return 0
            }
            "#,
        );
        assert_eq!(
            errs.len(),
            1,
            "only the string capacity is wrong, got {errs:?}"
        );
    }

    #[test]
    fn unknown_collection_method_is_rejected() {
        let errs = errors(
            r#"
            func main() -> i32 {
                mut v: Vec<i32> = Vec::new()
                v.sort()
                return 0
            }
            "#,
        );
        assert!(
            errs.iter().any(|e| e.contains("sort")),
            "expected a method-not-found diagnostic, got {errs:?}"
        );
    }
}
