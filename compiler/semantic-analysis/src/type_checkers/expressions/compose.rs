//! Type-checking for the composition operator `f >> g`.
//!
//! The chain's type is `(the first stage's parameter) -> (the last stage's return)`,
//! and each stage's return type must be what the next one takes.

use shared_types::{Identifier, Span};

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;

/// One stage of a composition: its parameter type and its return type.
type Stage = (Type, Type);

impl TypeChecker {
    /// Check `f >> g >> h` and return the composed function's type.
    ///
    /// Every stage is resolved before any is reported on, so one mistyped chain
    /// reports every bad name it holds rather than only the leftmost.
    pub(crate) fn check_compose(&mut self, functions: &[Identifier], span: Span) -> Type {
        let mut stages = Vec::with_capacity(functions.len());
        for name in functions {
            if let Some(stage) = self.compose_stage(name) {
                stages.push(stage);
            }
        }
        if stages.len() != functions.len() {
            return Type::Unknown;
        }

        for (index, pair) in stages.windows(2).enumerate() {
            let [(_, produced), (accepted, _)] = pair else {
                continue;
            };
            if self.assignable(produced, accepted) {
                continue;
            }
            self.record_error(TypeError::ComposeStageMismatch {
                left: functions[index].name.clone(),
                right: functions[index + 1].name.clone(),
                found: produced.clone(),
                expected: accepted.clone(),
                span: functions[index + 1].span,
            });
            return Type::Unknown;
        }

        let (Some((param, _)), Some((_, ret))) = (stages.first(), stages.last()) else {
            // The parser builds a chain from two operands, so an empty one is a
            // frontend inconsistency rather than a program the user wrote.
            self.record_error(TypeError::ComposeUndefined {
                name: "<empty composition>".to_string(),
                span,
            });
            return Type::Unknown;
        };
        Type::Function {
            params: vec![param.clone()],
            ret: Box::new(ret.clone()),
        }
    }

    /// Resolve one operand of `>>` to the single-parameter signature it composes with,
    /// recording why it cannot compose when it has none.
    fn compose_stage(&mut self, name: &Identifier) -> Option<Stage> {
        if self.generic_funcs.contains_key(&name.name) {
            self.record_error(TypeError::ComposeGenericFunction {
                name: name.name.clone(),
                span: name.span,
            });
            return None;
        }
        // A local of function type is a *value*, and calling it from the composed
        // function would capture it: captures are Copy-only this phase, and a
        // function type is not Copy.
        if self.symbols.lookup(&name.name).is_some() {
            self.record_error(TypeError::ComposeNotANamedFunction {
                name: name.name.clone(),
                span: name.span,
            });
            return None;
        }
        let Some(Type::Function { params, ret }) = self.functions.get(&name.name).cloned() else {
            self.record_error(TypeError::ComposeUndefined {
                name: name.name.clone(),
                span: name.span,
            });
            return None;
        };
        let [param] = params.as_slice() else {
            self.record_error(TypeError::ComposeArity {
                name: name.name.clone(),
                found: params.len(),
                span: name.span,
            });
            return None;
        };
        Some((param.clone(), *ret))
    }
}
