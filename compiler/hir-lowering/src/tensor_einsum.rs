//! The Einstein-notation contraction `einsum("bij,bjk->bik", a, b)`.
//!
//! Reached from the free-function arm of `lower_plain_call`, after the user-declared
//! functions, so a program's own `einsum` shadows this one exactly as it shadows the
//! panic and standard-output builtins.
//!
//! Lowering is where the notation stops existing. Each letter is interned to an index
//! into one extent table, which turns the subscript string into the three index tables
//! [`HirExprKind::TensorEinsum`] carries and leaves no backend parsing anything.
//!
//! The type checker has already validated the call, so a subscript that does not match
//! its operands here is a divergence between the two surfaces and becomes a
//! `LoweringError`, as this slice's CONTEXT.md requires.

use ast_types::Expr;
use neuro_hir::{AxisNames, HirExpr, HirExprKind, HirType};
use shared_types::{Literal, Span};
use std::collections::HashMap;

use crate::{Lowerer, LoweringError};

pub(crate) const EINSUM_FUNC: &str = "einsum";

fn malformed(detail: String) -> LoweringError {
    LoweringError::Malformed { detail }
}

impl Lowerer {
    /// Lower one contraction into a [`HirExprKind::TensorEinsum`].
    ///
    /// The operands are left as they are: a contraction reads the buffers it summarises,
    /// so nothing is moved and each operand keeps its binding.
    pub(crate) fn lower_tensor_einsum(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let Some((Expr::Literal(Literal::String(text), _), operand_exprs)) = args.split_first()
        else {
            return Err(malformed(
                "`einsum` reached lowering without a literal subscript string".to_string(),
            ));
        };
        let (inputs, output) = split_subscripts(text)?;
        if inputs.len() != operand_exprs.len() {
            return Err(malformed(format!(
                "`einsum(\"{text}\", ...)` reached lowering with {} operands for {} subscripts",
                operand_exprs.len(),
                inputs.len()
            )));
        }

        let mut operands = Vec::with_capacity(operand_exprs.len());
        for operand in operand_exprs {
            operands.push(self.lower_expr(operand, None)?);
        }

        // One pass binds every letter to an extent and to its index in `extents`; the
        // subscripts become those indices in the same pass, so nothing re-reads the
        // string afterwards.
        let mut letters: HashMap<char, usize> = HashMap::new();
        let mut extents: Vec<usize> = Vec::new();
        let mut element: Option<HirType> = None;
        let mut resolved_inputs = Vec::with_capacity(inputs.len());
        for (subscript, operand) in inputs.iter().zip(operands.iter()) {
            let HirType::Tensor {
                element: operand_element,
                shape,
                ..
            } = operand.ty.referent().clone()
            else {
                return Err(malformed(
                    "`einsum` reached lowering on a non-tensor operand".to_string(),
                ));
            };
            let shape = crate::static_extents(&shape)?;
            if shape.len() != subscript.len() {
                return Err(malformed(format!(
                    "`einsum` reached lowering with subscript '{}' on a rank-{} operand",
                    subscript.iter().collect::<String>(),
                    shape.len()
                )));
            }
            element.get_or_insert(*operand_element);

            let mut axes = Vec::with_capacity(subscript.len());
            for (letter, extent) in subscript.iter().zip(shape.iter()) {
                let index = *letters.entry(*letter).or_insert_with(|| {
                    extents.push(*extent);
                    extents.len() - 1
                });
                if extents[index] != *extent {
                    return Err(malformed(format!(
                        "`einsum` reached lowering binding '{letter}' to both {} and {extent}",
                        extents[index]
                    )));
                }
                axes.push(index);
            }
            resolved_inputs.push(axes);
        }

        let Some(element) = element else {
            return Err(malformed(
                "`einsum` reached lowering with no operands".to_string(),
            ));
        };

        let mut result_axes = Vec::with_capacity(output.len());
        for letter in &output {
            let index = *letters.get(letter).ok_or_else(|| {
                malformed(format!(
                    "`einsum` reached lowering with the unbound output letter '{letter}'"
                ))
            })?;
            result_axes.push(index);
        }

        // An empty output subscript contracts everything away and the result is the bare
        // element, matching what the checker typed the call as.
        let ty = if result_axes.is_empty() {
            element
        } else {
            let shape: Vec<usize> = result_axes.iter().map(|index| extents[*index]).collect();
            HirType::Tensor {
                element: Box::new(element),
                shape: neuro_hir::static_shape(&shape),
                names: AxisNames(vec![None; shape.len()]),
            }
        };
        Ok(HirExpr::new(
            HirExprKind::TensorEinsum {
                operands,
                inputs: resolved_inputs,
                output: result_axes,
                extents,
            },
            ty,
            span,
        ))
    }
}

/// Split a subscript string into each operand's letters and the result's.
///
/// Deliberately a second reading of the string rather than a value carried over from
/// the checker: this slice re-derives every resolved fact it needs, the way it re-derives
/// a reduction's axis, so the two surfaces stay independently checkable.
fn split_subscripts(text: &str) -> Result<(Vec<Vec<char>>, Vec<char>), LoweringError> {
    let (left, right) = text.split_once("->").ok_or_else(|| {
        malformed(format!(
            "`einsum(\"{text}\", ...)` reached lowering without a `->`"
        ))
    })?;
    let inputs = left
        .split(',')
        .map(|piece| piece.chars().collect::<Vec<char>>())
        .collect();
    Ok((inputs, right.chars().collect()))
}

#[cfg(test)]
mod tests {
    use super::split_subscripts;

    #[test]
    fn splits_a_batch_matmul_subscript() {
        let (inputs, output) = match split_subscripts("bij,bjk->bik") {
            Ok(split) => split,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(inputs, vec![vec!['b', 'i', 'j'], vec!['b', 'j', 'k']]);
        assert_eq!(output, vec!['b', 'i', 'k']);
    }

    #[test]
    fn an_empty_output_side_yields_no_letters() {
        let (inputs, output) = match split_subscripts("ii->") {
            Ok(split) => split,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(inputs, vec![vec!['i', 'i']]);
        assert!(output.is_empty());
    }
}
