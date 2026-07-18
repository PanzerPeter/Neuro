//! Compiler-known operator traits (§3.10).
//!
//! Operators on user types are sugar for trait method calls. Like `Drop` / `Copy` /
//! `Clone`, the operator traits are lang-items recognized by name (there is no prelude
//! yet, so a user writes only `impl Add for T`, never a `trait Add` declaration). This
//! table maps each trait to the operator(s) it provides and the method that backs each.

use ast_types::{BinaryOp, UnaryOp};

/// The static description of one operator trait: which operators it provides, whether
/// its methods return `Self::Output` (arithmetic/bitwise) or a fixed `bool`
/// (comparison), and any required supertrait (§3.10: `Comparable: PartialEq`).
pub(crate) struct OpTraitSpec {
    /// Binary operators provided by this trait: `(method name, operator)`.
    pub binary: &'static [(&'static str, BinaryOp)],
    /// Unary operators provided by this trait: `(method name, operator)`.
    pub unary: &'static [(&'static str, UnaryOp)],
    /// `true` when the operator methods return the impl's `Output` associated type;
    /// `false` when they return `bool` (the comparison traits).
    pub has_output: bool,
    /// A supertrait whose impl is required on the same type, if any.
    pub supertrait: Option<&'static str>,
}

/// Return the lang-item description of an operator trait, or `None` if `name` is not an
/// operator trait (§3.10).
pub(crate) fn operator_trait_spec(name: &str) -> Option<OpTraitSpec> {
    let spec = match name {
        "Add" => OpTraitSpec {
            binary: &[("add", BinaryOp::Add)],
            unary: &[],
            has_output: true,
            supertrait: None,
        },
        "Sub" => OpTraitSpec {
            binary: &[("sub", BinaryOp::Subtract)],
            unary: &[],
            has_output: true,
            supertrait: None,
        },
        "Mul" => OpTraitSpec {
            binary: &[("mul", BinaryOp::Multiply)],
            unary: &[],
            has_output: true,
            supertrait: None,
        },
        "Div" => OpTraitSpec {
            binary: &[("div", BinaryOp::Divide)],
            unary: &[],
            has_output: true,
            supertrait: None,
        },
        "Rem" => OpTraitSpec {
            binary: &[("rem", BinaryOp::Modulo)],
            unary: &[],
            has_output: true,
            supertrait: None,
        },
        "BitAnd" => OpTraitSpec {
            binary: &[("bitand", BinaryOp::BitAnd)],
            unary: &[],
            has_output: true,
            supertrait: None,
        },
        "BitOr" => OpTraitSpec {
            binary: &[("bitor", BinaryOp::BitOr)],
            unary: &[],
            has_output: true,
            supertrait: None,
        },
        "BitXor" => OpTraitSpec {
            binary: &[("bitxor", BinaryOp::BitXor)],
            unary: &[],
            has_output: true,
            supertrait: None,
        },
        "Shl" => OpTraitSpec {
            binary: &[("shl", BinaryOp::Shl)],
            unary: &[],
            has_output: true,
            supertrait: None,
        },
        "Neg" => OpTraitSpec {
            binary: &[],
            unary: &[("neg", UnaryOp::Negate)],
            has_output: true,
            supertrait: None,
        },
        // `Not` maps to the bitwise complement `~a`, not the boolean `!a` (§3.10).
        "Not" => OpTraitSpec {
            binary: &[],
            unary: &[("not", UnaryOp::BitNot)],
            has_output: true,
            supertrait: None,
        },
        "PartialEq" => OpTraitSpec {
            binary: &[("eq", BinaryOp::Equal), ("ne", BinaryOp::NotEqual)],
            unary: &[],
            has_output: false,
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
            has_output: false,
            supertrait: Some("PartialEq"),
        },
        _ => return None,
    };
    Some(spec)
}

/// Whether `name` is a compiler-known operator trait (§3.10).
pub(crate) fn is_operator_trait(name: &str) -> bool {
    operator_trait_spec(name).is_some()
}
