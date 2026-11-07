//! Compiler-known operator traits.
//!
//! Operators on user types are sugar for trait method calls. The operator traits are
//! lang-items recognized by name, so a user writes only `impl Add for T`, never a
//! `trait Add` declaration. This table maps each trait to the operator(s) it provides and
//! the method that backs each.

use ast_types::{BinaryOp, UnaryOp};

use super::{OperatorMethod, TypeChecker};
use crate::types::Type;

/// The shape an operator trait's methods take, which is also how the operator reaches
/// them.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum OperatorForm {
    /// `add(self, rhs: Rhs) -> Self::Output`: both operands are handed over.
    Value,
    /// `eq(&self, rhs: &Rhs) -> bool`: both operands are only read.
    Compare,
    /// `add_assign(&mut self, rhs: Rhs)`: reached by `OP=`, never by `OP`.
    InPlace,
}

/// The static description of one operator trait: which operators it provides, the form
/// of its methods, and any required supertrait (`Comparable: PartialEq`).
pub(crate) struct OpTraitSpec {
    /// Binary operators provided by this trait: `(method name, operator)`.
    pub binary: &'static [(&'static str, BinaryOp)],
    /// Unary operators provided by this trait: `(method name, operator)`.
    pub unary: &'static [(&'static str, UnaryOp)],
    pub form: OperatorForm,
    /// A supertrait whose impl is required on the same type, if any.
    pub supertrait: Option<&'static str>,
}

/// Return the lang-item description of an operator trait, or `None` if `name` is not an
/// operator trait.
pub(crate) fn operator_trait_spec(name: &str) -> Option<OpTraitSpec> {
    let spec = match name {
        "Add" => OpTraitSpec {
            binary: &[("add", BinaryOp::Add)],
            unary: &[],
            form: OperatorForm::Value,
            supertrait: None,
        },
        "Sub" => OpTraitSpec {
            binary: &[("sub", BinaryOp::Subtract)],
            unary: &[],
            form: OperatorForm::Value,
            supertrait: None,
        },
        "Mul" => OpTraitSpec {
            binary: &[("mul", BinaryOp::Multiply)],
            unary: &[],
            form: OperatorForm::Value,
            supertrait: None,
        },
        "Div" => OpTraitSpec {
            binary: &[("div", BinaryOp::Divide)],
            unary: &[],
            form: OperatorForm::Value,
            supertrait: None,
        },
        "Rem" => OpTraitSpec {
            binary: &[("rem", BinaryOp::Modulo)],
            unary: &[],
            form: OperatorForm::Value,
            supertrait: None,
        },
        "BitAnd" => OpTraitSpec {
            binary: &[("bitand", BinaryOp::BitAnd)],
            unary: &[],
            form: OperatorForm::Value,
            supertrait: None,
        },
        "BitOr" => OpTraitSpec {
            binary: &[("bitor", BinaryOp::BitOr)],
            unary: &[],
            form: OperatorForm::Value,
            supertrait: None,
        },
        "BitXor" => OpTraitSpec {
            binary: &[("bitxor", BinaryOp::BitXor)],
            unary: &[],
            form: OperatorForm::Value,
            supertrait: None,
        },
        "Shl" => OpTraitSpec {
            binary: &[("shl", BinaryOp::Shl)],
            unary: &[],
            form: OperatorForm::Value,
            supertrait: None,
        },
        "MatMul" => OpTraitSpec {
            binary: &[("matmul", BinaryOp::MatMul)],
            unary: &[],
            form: OperatorForm::Value,
            supertrait: None,
        },
        "Neg" => OpTraitSpec {
            binary: &[],
            unary: &[("neg", UnaryOp::Negate)],
            form: OperatorForm::Value,
            supertrait: None,
        },
        // The boolean `!a` has no trait: only the bitwise complement `~a` overloads.
        "BitNot" => OpTraitSpec {
            binary: &[],
            unary: &[("bitnot", UnaryOp::BitNot)],
            form: OperatorForm::Value,
            supertrait: None,
        },
        "PartialEq" => OpTraitSpec {
            binary: &[("eq", BinaryOp::Equal), ("ne", BinaryOp::NotEqual)],
            unary: &[],
            form: OperatorForm::Compare,
            supertrait: None,
        },
        "Comparable" => OpTraitSpec {
            binary: &[
                ("lt", BinaryOp::Less),
                ("le", BinaryOp::LessEqual),
                ("gt", BinaryOp::Greater),
                ("ge", BinaryOp::GreaterEqual),
            ],
            unary: &[],
            form: OperatorForm::Compare,
            supertrait: Some("PartialEq"),
        },
        "AddAssign" => OpTraitSpec {
            binary: &[("add_assign", BinaryOp::Add)],
            unary: &[],
            form: OperatorForm::InPlace,
            supertrait: None,
        },
        "SubAssign" => OpTraitSpec {
            binary: &[("sub_assign", BinaryOp::Subtract)],
            unary: &[],
            form: OperatorForm::InPlace,
            supertrait: None,
        },
        "MulAssign" => OpTraitSpec {
            binary: &[("mul_assign", BinaryOp::Multiply)],
            unary: &[],
            form: OperatorForm::InPlace,
            supertrait: None,
        },
        "DivAssign" => OpTraitSpec {
            binary: &[("div_assign", BinaryOp::Divide)],
            unary: &[],
            form: OperatorForm::InPlace,
            supertrait: None,
        },
        "RemAssign" => OpTraitSpec {
            binary: &[("rem_assign", BinaryOp::Modulo)],
            unary: &[],
            form: OperatorForm::InPlace,
            supertrait: None,
        },
        "BitAndAssign" => OpTraitSpec {
            binary: &[("bitand_assign", BinaryOp::BitAnd)],
            unary: &[],
            form: OperatorForm::InPlace,
            supertrait: None,
        },
        "BitOrAssign" => OpTraitSpec {
            binary: &[("bitor_assign", BinaryOp::BitOr)],
            unary: &[],
            form: OperatorForm::InPlace,
            supertrait: None,
        },
        "BitXorAssign" => OpTraitSpec {
            binary: &[("bitxor_assign", BinaryOp::BitXor)],
            unary: &[],
            form: OperatorForm::InPlace,
            supertrait: None,
        },
        "ShlAssign" => OpTraitSpec {
            binary: &[("shl_assign", BinaryOp::Shl)],
            unary: &[],
            form: OperatorForm::InPlace,
            supertrait: None,
        },
        _ => return None,
    };
    Some(spec)
}

/// The comparison methods a trait supplies from its required one, so an impl may omit
/// them. The parser writes them into the impl; this names them for the rule that every
/// other method must be written.
pub(crate) fn is_provided_method(method: &str) -> bool {
    matches!(method, "ne" | "le" | "gt" | "ge")
}

/// The trait a comparison method belongs to, for a receiver whose comparison is built in
/// rather than written as an impl.
pub(crate) fn comparison_method(method: &str) -> Option<(&'static str, BinaryOp)> {
    ["PartialEq", "Comparable"]
        .into_iter()
        .find_map(|trait_name| {
            let spec = operator_trait_spec(trait_name)?;
            let (_, op) = spec.binary.iter().find(|(name, _)| *name == method)?;
            Some((trait_name, *op))
        })
}

/// Whether `name` is a compiler-known operator trait.
pub(crate) fn is_operator_trait(name: &str) -> bool {
    operator_trait_spec(name).is_some()
}

impl TypeChecker {
    /// The method `receiver OP rhs` calls, read through a borrow of the receiver as a
    /// method call's receiver is.
    pub(crate) fn binary_operator_method(
        &self,
        receiver: &Type,
        op: BinaryOp,
    ) -> Option<OperatorMethod> {
        let name = nominal_type_name(receiver.referent())?;
        let method = self.operator_binary_impls.get(&(name.to_string(), op))?;
        self.operator_method(name, method)
    }

    /// The method `OP receiver` calls.
    pub(crate) fn unary_operator_method(
        &self,
        receiver: &Type,
        op: UnaryOp,
    ) -> Option<OperatorMethod> {
        let name = nominal_type_name(receiver.referent())?;
        let method = self.operator_unary_impls.get(&(name.to_string(), op))?;
        self.operator_method(name, method)
    }

    /// The `*Assign` method `place OP= rhs` calls. The place is taken as it stands: a
    /// `&mut T` binding is not a `T`, and its referent is the place `*r`.
    pub(crate) fn assign_operator_method(
        &self,
        place: &Type,
        op: BinaryOp,
    ) -> Option<OperatorMethod> {
        let name = nominal_type_name(place)?;
        let method = self.operator_assign_impls.get(&(name.to_string(), op))?;
        self.operator_method(name, method)
    }

    fn operator_method(&self, type_name: &str, method: &str) -> Option<OperatorMethod> {
        let mangled = format!("{type_name}__{method}");
        let Some(Type::Function { params, ret, .. }) = self.functions.get(&mangled) else {
            return None;
        };
        Some(OperatorMethod {
            rhs: params.get(1).cloned(),
            result: (**ret).clone(),
            mangled,
        })
    }
}

fn nominal_type_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Struct(name) | Type::Enum(name) | Type::Newtype(name) => Some(name),
        _ => None,
    }
}
