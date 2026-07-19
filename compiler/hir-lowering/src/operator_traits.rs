//! Compiler-known operator traits, duplicated from the checker per VSA (feature
//! slices share only infrastructure crates, so the small lang-item table is duplicated
//! rather than coupled). Maps each operator trait to the operator(s) it provides.

use ast_types::{BinaryOp, UnaryOp};

/// Which operators an operator trait provides and whether its methods return the impl's
/// `Output` (arithmetic/bitwise) or a fixed `bool` (comparison).
pub(crate) struct OpTraitSpec {
    pub binary: &'static [(&'static str, BinaryOp)],
    pub unary: &'static [(&'static str, UnaryOp)],
    pub has_output: bool,
}

/// Return the operator trait's description, or `None` if `name` is not one.
pub(crate) fn operator_trait_spec(name: &str) -> Option<OpTraitSpec> {
    let spec = match name {
        "Add" => OpTraitSpec {
            binary: &[("add", BinaryOp::Add)],
            unary: &[],
            has_output: true,
        },
        "Sub" => OpTraitSpec {
            binary: &[("sub", BinaryOp::Subtract)],
            unary: &[],
            has_output: true,
        },
        "Mul" => OpTraitSpec {
            binary: &[("mul", BinaryOp::Multiply)],
            unary: &[],
            has_output: true,
        },
        "Div" => OpTraitSpec {
            binary: &[("div", BinaryOp::Divide)],
            unary: &[],
            has_output: true,
        },
        "Rem" => OpTraitSpec {
            binary: &[("rem", BinaryOp::Modulo)],
            unary: &[],
            has_output: true,
        },
        "BitAnd" => OpTraitSpec {
            binary: &[("bitand", BinaryOp::BitAnd)],
            unary: &[],
            has_output: true,
        },
        "BitOr" => OpTraitSpec {
            binary: &[("bitor", BinaryOp::BitOr)],
            unary: &[],
            has_output: true,
        },
        "BitXor" => OpTraitSpec {
            binary: &[("bitxor", BinaryOp::BitXor)],
            unary: &[],
            has_output: true,
        },
        "Shl" => OpTraitSpec {
            binary: &[("shl", BinaryOp::Shl)],
            unary: &[],
            has_output: true,
        },
        "Neg" => OpTraitSpec {
            binary: &[],
            unary: &[("neg", UnaryOp::Negate)],
            has_output: true,
        },
        "Not" => OpTraitSpec {
            binary: &[],
            unary: &[("not", UnaryOp::BitNot)],
            has_output: true,
        },
        "PartialEq" => OpTraitSpec {
            binary: &[("eq", BinaryOp::Equal), ("ne", BinaryOp::NotEqual)],
            unary: &[],
            has_output: false,
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
        },
        _ => return None,
    };
    Some(spec)
}
