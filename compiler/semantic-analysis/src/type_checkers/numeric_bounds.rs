//! The numeric families `Numeric`, `Integer` and `Float` (§3.8).
//!
//! The prelude declares them and the compiler implements them, for the scalars alone, so a
//! body generic over its element type can compute. A type parameter bounded by one answers
//! the predicates below as a scalar of that family does, and each instance is lowered at
//! its concrete type, which the scalar rules already cover.

use super::TypeChecker;
use crate::types::Type;

pub(crate) const NUMERIC_TRAIT: &str = "Numeric";
pub(crate) const INTEGER_TRAIT: &str = "Integer";
pub(crate) const FLOAT_TRAIT: &str = "Float";

/// Whether `trait_name` is one of the three families, which no program may implement.
pub(crate) fn is_numeric_family(trait_name: &str) -> bool {
    matches!(trait_name, NUMERIC_TRAIT | INTEGER_TRAIT | FLOAT_TRAIT)
}

/// Whether the concrete type `ty` implements the family `trait_name`. FP8 is in none of
/// them, because it has no scalar arithmetic (§1.2).
pub(crate) fn scalar_in_family(ty: &Type, trait_name: &str) -> bool {
    match trait_name {
        NUMERIC_TRAIT => ty.is_numeric() || ty.is_half_float(),
        INTEGER_TRAIT => ty.is_integer(),
        FLOAT_TRAIT => ty.is_float() || ty.is_half_float(),
        _ => false,
    }
}

impl TypeChecker {
    /// Whether `ty` has `+ - * /` and the comparisons: a numeric scalar, or a type
    /// parameter bounded by `Numeric`.
    pub(crate) fn is_arithmetic(&self, ty: &Type) -> bool {
        scalar_in_family(ty, NUMERIC_TRAIT) || self.param_in_family(ty, NUMERIC_TRAIT)
    }

    /// Whether `ty` has `%` and the bitwise operators: an integer, or a `T: Integer`.
    pub(crate) fn is_integral(&self, ty: &Type) -> bool {
        ty.is_integer() || self.param_in_family(ty, INTEGER_TRAIT)
    }

    /// Whether `ty` has the float surface: a float scalar, or a `T: Float`.
    pub(crate) fn is_floating(&self, ty: &Type) -> bool {
        scalar_in_family(ty, FLOAT_TRAIT) || self.param_in_family(ty, FLOAT_TRAIT)
    }

    fn param_in_family(&self, ty: &Type, trait_name: &str) -> bool {
        matches!(ty, Type::Generic(param) if self.param_traits(param).iter().any(|t| t == trait_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_family_holds_its_scalars_and_nothing_else() {
        assert!(scalar_in_family(&Type::U8, NUMERIC_TRAIT));
        assert!(scalar_in_family(&Type::BF16, NUMERIC_TRAIT));
        assert!(scalar_in_family(&Type::I64, INTEGER_TRAIT));
        assert!(!scalar_in_family(&Type::F32, INTEGER_TRAIT));
        assert!(scalar_in_family(&Type::F16, FLOAT_TRAIT));
        assert!(!scalar_in_family(&Type::I32, FLOAT_TRAIT));
        for outsider in [Type::Bool, Type::Char, Type::String] {
            for family in [NUMERIC_TRAIT, INTEGER_TRAIT, FLOAT_TRAIT] {
                assert!(!scalar_in_family(&outsider, family));
            }
        }
        assert!(!is_numeric_family("Copy"));
    }
}
