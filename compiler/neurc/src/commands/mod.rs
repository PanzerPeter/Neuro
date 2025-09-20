//! Command implementations for neurc CLI
//!
//! Each subcommand is implemented as a separate module following VSA principles.

pub mod compile;
pub mod check;
pub mod tokenize;
pub mod parse;
pub mod eval;
pub mod analyze;
pub mod llvm;
pub mod build;
pub mod run;
pub mod version;

pub use compile::*;
pub use check::*;
pub use tokenize::*;
pub use parse::*;
pub use eval::*;
pub use analyze::*;
pub use llvm::*;
pub use build::*;
pub use run::*;
pub use version::*;