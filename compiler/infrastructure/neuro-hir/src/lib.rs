//! Typed High-Level IR (HIR).
//!
//! Infrastructure crate defining the **typed HIR**: the stable, backend-agnostic
//! contract between the frontend (parser + type checker) and every backend. Both
//! `llvm-backend` (scalar / control-flow path) and `mlir-backend` (tensor / AD /
//! GPU path, Phase 3+) are intended to lower from this HIR rather than consuming
//! the AST directly.
//!
//! # Relationship to the AST
//!
//! The HIR mirrors the surface AST ([`ast_types`]) one-to-one in structure, with
//! two defining differences:
//!
//! - **Every expression carries its resolved type** ([`HirExpr::ty`]). Types are
//!   fully resolved [`HirType`]s, never unresolved name annotations, and there is
//!   no error-recovery `Unknown` variant — a program that reaches the HIR has
//!   type-checked.
//! - **Syntactic noise is normalized away** — the parenthesis grouping node is
//!   dropped, since tree structure already encodes grouping.
//!
//! This crate is pure data with no business logic, following the same VSA
//! infrastructure pattern as [`ast_types`]. The AST → HIR lowering and the
//! backend migration onto HIR are separate, later pipeline steps; this crate is
//! only the shared type contract they exchange.

pub mod expressions;
pub mod items;
pub mod statements;
pub mod types;

pub use expressions::{
    HirBindingSource, HirExpr, HirExprKind, HirFieldInit, HirMatchArm, HirMatchBinding,
    HirMatchTest,
};
pub use items::{
    HirConst, HirEnum, HirEnumField, HirEnumVariant, HirField, HirFunction, HirImpl, HirItem,
    HirMethod, HirParam, HirProgram, HirSelfParam, HirStruct, HirTrait,
};
pub use statements::HirStmt;
pub use types::HirType;

#[cfg(test)]
mod tests {
    use super::*;
    use ast_types::BinaryOp;
    use shared_types::{Literal, Span};

    fn span() -> Span {
        Span::new(0, 1)
    }

    #[test]
    fn type_display_covers_scalars_and_composites() {
        assert_eq!(HirType::I32.to_string(), "i32");
        assert_eq!(HirType::F64.to_string(), "f64");
        assert_eq!(HirType::Bool.to_string(), "bool");
        assert_eq!(HirType::String.to_string(), "string");
        assert_eq!(HirType::Struct("Point".to_string()).to_string(), "Point");
        assert_eq!(
            HirType::Reference {
                inner: Box::new(HirType::String),
                mutable: false,
            }
            .to_string(),
            "&string"
        );
        assert_eq!(
            HirType::Reference {
                inner: Box::new(HirType::I32),
                mutable: true,
            }
            .to_string(),
            "&mut i32"
        );
        assert_eq!(
            HirType::Array {
                element: Box::new(HirType::I64),
                size: 3,
            }
            .to_string(),
            "[i64; 3]"
        );
        assert_eq!(
            HirType::Function {
                params: vec![HirType::I32, HirType::Bool],
                ret: Box::new(HirType::Void),
            }
            .to_string(),
            "fn(i32, bool) -> void"
        );
    }

    #[test]
    fn referent_peels_exactly_one_reference_layer() {
        let r = HirType::Reference {
            inner: Box::new(HirType::I32),
            mutable: true,
        };
        assert!(r.is_reference());
        assert_eq!(r.referent(), &HirType::I32);
        // A non-reference returns itself unchanged.
        assert!(!HirType::I32.is_reference());
        assert_eq!(HirType::I32.referent(), &HirType::I32);
    }

    #[test]
    fn expression_carries_resolved_type() {
        let lit = HirExpr::new(
            HirExprKind::Literal(Literal::Integer(1, None)),
            HirType::I32,
            span(),
        );
        let sum = HirExpr::new(
            HirExprKind::Binary {
                op: BinaryOp::Add,
                left: Box::new(lit.clone()),
                right: Box::new(lit.clone()),
            },
            HirType::I32,
            span(),
        );
        assert_eq!(sum.ty, HirType::I32);
        // Equality is structural, so an identically-built node compares equal.
        assert_eq!(sum, sum.clone());
    }

    #[test]
    fn program_assembles_a_typed_function() {
        // func answer() -> i32 { return 42 }
        let body = vec![HirStmt::Return {
            value: Some(HirExpr::new(
                HirExprKind::Literal(Literal::Integer(42, None)),
                HirType::I32,
                span(),
            )),
            span: span(),
        }];
        let program = HirProgram {
            items: vec![HirItem::Function(HirFunction {
                name: "answer".to_string(),
                params: vec![],
                return_type: HirType::I32,
                body,
                span: span(),
            })],
        };

        let HirItem::Function(f) = &program.items[0] else {
            panic!("expected a function item");
        };
        assert_eq!(f.name, "answer");
        assert_eq!(f.return_type, HirType::I32);
        assert_eq!(f.body.len(), 1);
    }
}
