//! The standard collections `Vec<T>`, `HashMap<K, V>`, `BTreeMap<K, V>`, and the
//! growable text buffer `String`.
//!
//! They are specified as library types, but the language exposes no allocator and no
//! raw pointers, so nothing in `.nr` source could implement them: the compiler knows
//! them all by name and lowers their operations directly. Everything user-visible is
//! still ordinary type checking — a collection type is a nominal type with type
//! arguments, its operations are builtin methods, and it obeys move-by-default.
//!
//! `String` joins the family because it is the same machine: one growable heap buffer
//! behind an owning header. Its element type is fixed (UTF-8 bytes) rather than a type
//! argument, so it is the one nullary kind.

use ast_types::Expr;
use shared_types::Span;

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{CollectionKind, Type};

/// The `Option<T>` returned by the fallible readers (`Vec::pop`, `Map::get`). It comes
/// from the prelude rather than the compiler, so a program compiled without one gets a
/// plain "unknown type" diagnostic instead of a silently wrong result type.
pub(crate) const OPTION_ENUM: &str = "Option";

/// The lang-item trait a `HashMap` key must implement: `func hash(&self) -> u64`.
pub(crate) const HASHABLE_TRAIT: &str = "Hashable";

/// The single method a [`HASHABLE_TRAIT`] impl provides.
pub(crate) const HASH_METHOD: &str = "hash";

impl TypeChecker {
    /// Resolve a `Vec<T>` / `HashMap<K, V>` / `BTreeMap<K, V>` / `String` annotation,
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
    /// A `Copy` type is bit-copied in and out with no ownership consequences. `string`
    /// is the one non-`Copy` exception: its fat pointer is duplicated exactly as
    /// `.clone()` does, which is faithful while string data is immutable. A reference
    /// is excluded because the borrow checker cannot see through a heap buffer to
    /// verify the referent outlives the collection.
    fn check_storable(&mut self, kind: CollectionKind, ty: &Type, span: Span) -> Option<()> {
        let storable = matches!(ty, Type::String)
            || (self.is_type_copy(ty)
                && !matches!(
                    ty,
                    Type::Reference { .. }
                        | Type::Collection { .. }
                        | Type::Generic(_)
                        | Type::ConstValue(_)
                        | Type::Void
                        | Type::Function { .. }
                        | Type::DynObject(_)
                        | Type::Unknown
                ));
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
        let Type::Struct(name) = ty else {
            return reject(
                self,
                "only integer, `bool`, `char`, `string`, and struct keys are supported".to_string(),
            );
        };

        // A struct key routes equality, hashing, and ordering through its own impls, so
        // each required lang-item trait must actually be implemented for it.
        let name = name.clone();
        let mut required = vec!["PartialEq"];
        match kind {
            CollectionKind::HashMap => required.push(HASHABLE_TRAIT),
            CollectionKind::BTreeMap => required.push("Comparable"),
            CollectionKind::Vec | CollectionKind::String => {}
        }
        for trait_name in required {
            if !self.trait_impls_contains(trait_name, &name) {
                return reject(
                    self,
                    format!("struct keys require `impl {} for {}`", trait_name, name),
                );
            }
        }
        Some(())
    }

    /// Type-check `Vec::new()` / `HashMap::new()` / `BTreeMap::new()` / `String::new()`.
    ///
    /// The element types come from the expected type: an empty collection carries no
    /// value to infer from, so the binding must be annotated. A nullary kind has nothing
    /// to infer, so it needs no annotation.
    pub(crate) fn check_collection_new(
        &mut self,
        kind: CollectionKind,
        args: &[Expr],
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        if !args.is_empty() {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: 0,
                found: args.len(),
                span,
            });
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
                    // good as an owned `string` — the same latitude `+` gives its operands.
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
            // Only the storing methods take ownership; a lookup key is read like a
            // `==` operand and leaves the caller's binding usable.
            if spec.stores_args {
                self.record_move(arg);
            }
        }

        Some(spec.result.resolve_result(self, &params, span))
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

    /// Whether `(trait_name, type_name)` has an `impl` block. Wraps the private impl
    /// table so the collection key rules can consult it.
    fn trait_impls_contains(&self, trait_name: &str, type_name: &str) -> bool {
        self.type_implements_trait(&Type::Struct(type_name.to_string()), trait_name)
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
        self.instantiate_generic_enum(OPTION_ENUM, &[inner], span)
    }
}

/// Which of a collection's type arguments a method parameter takes.
#[derive(Clone, Copy, PartialEq)]
enum ParamSlot {
    /// A `u64`-shaped position index (`Vec::get`).
    Index,
    /// The collection's key type — argument 0 of a map.
    Key,
    /// The collection's element/value type — the last type argument.
    Value,
    /// Borrowed UTF-8 text: a `string` or an immutable `&string` (`String::push_str`).
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
    /// `Option<T>` over the element/value type.
    OptionValue,
    /// A freshly built `Vec<K>` of the map's keys.
    KeyVec,
    /// A freshly allocated owned immutable `string` (`String::to_string`).
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
    /// Whether the call takes ownership of its arguments (an insertion does; a
    /// lookup does not).
    stores_args: bool,
}

/// Look up `method` in the surface of `kind`, or `None` if it has no such method.
fn collection_method(kind: CollectionKind, method: &str) -> Option<MethodSpec> {
    let spec = match (kind, method) {
        (_, "len") => MethodSpec {
            params: &[],
            result: ResultShape::Len,
            mutating: false,
            stores_args: false,
        },
        (_, "clear") => MethodSpec {
            params: &[],
            result: ResultShape::Unit,
            mutating: true,
            stores_args: false,
        },
        (CollectionKind::Vec, "push") => MethodSpec {
            params: &[ParamSlot::Value],
            result: ResultShape::Unit,
            mutating: true,
            stores_args: true,
        },
        (CollectionKind::Vec, "pop") => MethodSpec {
            params: &[],
            result: ResultShape::OptionValue,
            mutating: true,
            stores_args: false,
        },
        (CollectionKind::Vec, "get") => MethodSpec {
            params: &[ParamSlot::Index],
            result: ResultShape::OptionValue,
            mutating: false,
            stores_args: false,
        },
        (CollectionKind::HashMap | CollectionKind::BTreeMap, "insert") => MethodSpec {
            params: &[ParamSlot::Key, ParamSlot::Value],
            result: ResultShape::Unit,
            mutating: true,
            stores_args: true,
        },
        (CollectionKind::HashMap | CollectionKind::BTreeMap, "get") => MethodSpec {
            params: &[ParamSlot::Key],
            result: ResultShape::OptionValue,
            mutating: false,
            stores_args: false,
        },
        (CollectionKind::HashMap | CollectionKind::BTreeMap, "contains_key") => MethodSpec {
            params: &[ParamSlot::Key],
            result: ResultShape::Bool,
            mutating: false,
            stores_args: false,
        },
        (CollectionKind::HashMap | CollectionKind::BTreeMap, "remove") => MethodSpec {
            params: &[ParamSlot::Key],
            result: ResultShape::Bool,
            mutating: true,
            stores_args: false,
        },
        (CollectionKind::String, "push_str") => MethodSpec {
            params: &[ParamSlot::Text],
            result: ResultShape::Unit,
            mutating: true,
            stores_args: false,
        },
        (CollectionKind::String, "to_string") => MethodSpec {
            params: &[],
            result: ResultShape::OwnedString,
            mutating: false,
            stores_args: false,
        },
        (CollectionKind::HashMap | CollectionKind::BTreeMap, "keys") => MethodSpec {
            params: &[],
            result: ResultShape::KeyVec,
            mutating: false,
            stores_args: false,
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
            errs.iter().any(|e| e.contains("impl PartialEq for Id")),
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
                val hit: Option<i32> = m.get("a")
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
                mut b = String::new()
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
                mut b: String = String::new()
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
                mut b = String::new()
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
                mut b: String<i32> = String::new()
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
                val b = String::new()
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
    fn a_declared_string_type_shadows_the_builder() {
        let errs = errors(
            r#"
            @derive(Copy, Clone)
            struct String { n: i32 }
            func main() -> i32 {
                val s: String = String { n: 1 }
                return s.n
            }
            "#,
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
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
