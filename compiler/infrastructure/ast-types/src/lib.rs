//! Abstract Syntax Tree type definitions. Pure data structures with no business
//! logic, living in infrastructure so syntax-parsing (constructs), module-resolution and
//! argument-binding (rewrite), semantic-analysis (checks), and hir-lowering (lowers) can
//! share them without cross-slice deps.

pub mod expressions;
pub mod items;
pub mod statements;
pub mod types;

pub use expressions::{
    BinaryOp, ClosureParam, EnumPatternPayload, Expr, FieldInit, FieldPattern, InterpPart,
    MatchArm, Pattern, TensorIndexArg, UnaryOp,
};
pub use items::{
    Attribute, AttributeNamedArg, ConstDef, EnumDef, EnumVariant, FieldDef, FunctionDef,
    GenericParam, GenericParamKind, ImplDef, ImportDef, ImportName, ImportSelection, Item,
    MethodDef, ModuleDef, ModuleId, NewtypeDef, PRELUDE_MODULE, ParamLabel, Parameter, SelfParam,
    StructDef, TraitBound, TraitDef, TraitMethod, VariantPayload,
};
pub use statements::{LoopAdapter, LoopAdapterKind, Place, Stmt};
pub use types::{ArraySize, GenericArg, TensorDim, TensorExtent, Type};
