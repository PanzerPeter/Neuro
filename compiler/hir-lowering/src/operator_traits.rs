//! Compiler-known operator traits, duplicated from the checker per VSA (feature
//! slices share only infrastructure crates, so the small lang-item table is duplicated
//! rather than coupled). Maps each operator trait to the operator(s) it provides.

use ast_types::{BinaryOp, UnaryOp};

/// The shape an operator trait's methods take.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum OperatorForm {
    /// Returns the impl's `Output`.
    Value,
    /// Returns a fixed `bool`.
    Compare,
    /// `&mut self`, reached by `OP=`.
    InPlace,
}

/// Which operators an operator trait provides and the form of its methods.
pub(crate) struct OpTraitSpec {
    pub binary: &'static [(&'static str, BinaryOp)],
    pub unary: &'static [(&'static str, UnaryOp)],
    pub form: OperatorForm,
}

/// Return the operator trait's description, or `None` if `name` is not one.
pub(crate) fn operator_trait_spec(name: &str) -> Option<OpTraitSpec> {
    let spec = match name {
        "Add" => OpTraitSpec {
            binary: &[("add", BinaryOp::Add)],
            unary: &[],
            form: OperatorForm::Value,
        },
        "Sub" => OpTraitSpec {
            binary: &[("sub", BinaryOp::Subtract)],
            unary: &[],
            form: OperatorForm::Value,
        },
        "Mul" => OpTraitSpec {
            binary: &[("mul", BinaryOp::Multiply)],
            unary: &[],
            form: OperatorForm::Value,
        },
        "Div" => OpTraitSpec {
            binary: &[("div", BinaryOp::Divide)],
            unary: &[],
            form: OperatorForm::Value,
        },
        "Rem" => OpTraitSpec {
            binary: &[("rem", BinaryOp::Modulo)],
            unary: &[],
            form: OperatorForm::Value,
        },
        "BitAnd" => OpTraitSpec {
            binary: &[("bitand", BinaryOp::BitAnd)],
            unary: &[],
            form: OperatorForm::Value,
        },
        "BitOr" => OpTraitSpec {
            binary: &[("bitor", BinaryOp::BitOr)],
            unary: &[],
            form: OperatorForm::Value,
        },
        "BitXor" => OpTraitSpec {
            binary: &[("bitxor", BinaryOp::BitXor)],
            unary: &[],
            form: OperatorForm::Value,
        },
        "Shl" => OpTraitSpec {
            binary: &[("shl", BinaryOp::Shl)],
            unary: &[],
            form: OperatorForm::Value,
        },
        "MatMul" => OpTraitSpec {
            binary: &[("matmul", BinaryOp::MatMul)],
            unary: &[],
            form: OperatorForm::Value,
        },
        "Neg" => OpTraitSpec {
            binary: &[],
            unary: &[("neg", UnaryOp::Negate)],
            form: OperatorForm::Value,
        },
        "BitNot" => OpTraitSpec {
            binary: &[],
            unary: &[("bitnot", UnaryOp::BitNot)],
            form: OperatorForm::Value,
        },
        "PartialEq" => OpTraitSpec {
            binary: &[("eq", BinaryOp::Equal), ("ne", BinaryOp::NotEqual)],
            unary: &[],
            form: OperatorForm::Compare,
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
        },
        "AddAssign" => OpTraitSpec {
            binary: &[("add_assign", BinaryOp::Add)],
            unary: &[],
            form: OperatorForm::InPlace,
        },
        "SubAssign" => OpTraitSpec {
            binary: &[("sub_assign", BinaryOp::Subtract)],
            unary: &[],
            form: OperatorForm::InPlace,
        },
        "MulAssign" => OpTraitSpec {
            binary: &[("mul_assign", BinaryOp::Multiply)],
            unary: &[],
            form: OperatorForm::InPlace,
        },
        "DivAssign" => OpTraitSpec {
            binary: &[("div_assign", BinaryOp::Divide)],
            unary: &[],
            form: OperatorForm::InPlace,
        },
        "RemAssign" => OpTraitSpec {
            binary: &[("rem_assign", BinaryOp::Modulo)],
            unary: &[],
            form: OperatorForm::InPlace,
        },
        "BitAndAssign" => OpTraitSpec {
            binary: &[("bitand_assign", BinaryOp::BitAnd)],
            unary: &[],
            form: OperatorForm::InPlace,
        },
        "BitOrAssign" => OpTraitSpec {
            binary: &[("bitor_assign", BinaryOp::BitOr)],
            unary: &[],
            form: OperatorForm::InPlace,
        },
        "BitXorAssign" => OpTraitSpec {
            binary: &[("bitxor_assign", BinaryOp::BitXor)],
            unary: &[],
            form: OperatorForm::InPlace,
        },
        "ShlAssign" => OpTraitSpec {
            binary: &[("shl_assign", BinaryOp::Shl)],
            unary: &[],
            form: OperatorForm::InPlace,
        },
        _ => return None,
    };
    Some(spec)
}

/// The built-in operator a comparison method stands for on a receiver with no impl.
pub(crate) fn comparison_op(method: &str) -> Option<BinaryOp> {
    ["PartialEq", "Comparable"]
        .into_iter()
        .find_map(|trait_name| {
            let spec = operator_trait_spec(trait_name)?;
            let (_, op) = spec.binary.iter().find(|(name, _)| *name == method)?;
            Some(*op)
        })
}
