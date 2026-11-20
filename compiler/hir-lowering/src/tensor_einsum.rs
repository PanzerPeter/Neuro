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

use crate::{Lowerer, LoweringError, malformed};

pub(crate) const EINSUM_FUNC: &str = "einsum";

/// A `...` once split out of a subscript: one entry standing for every axis it covers.
const ELLIPSIS: char = '.';

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
        let mut extents: Vec<Option<usize>> = Vec::new();
        // The indices the ellipsis's axes take, interned by the first operand that writes it.
        let mut leading: Option<Vec<usize>> = None;
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
            let shape = shape.clone();
            let written = subscript.first() == Some(&ELLIPSIS);
            let named = &subscript[usize::from(written)..];
            let covered = shape.len().checked_sub(named.len());
            let Some(covered) = covered.filter(|covered| written || *covered == 0) else {
                return Err(malformed(format!(
                    "`einsum` reached lowering with subscript '{}' on a rank-{} operand",
                    subscript.iter().collect::<String>(),
                    shape.len()
                )));
            };
            element.get_or_insert(*operand_element);

            let (front, shape) = shape.split_at(covered);
            let mut axes = Vec::with_capacity(shape.len() + covered);
            if written {
                let bound = leading.get_or_insert_with(|| {
                    extents.extend_from_slice(front);
                    (extents.len() - front.len()..extents.len()).collect()
                });
                let agrees = bound.len() == front.len()
                    && bound
                        .iter()
                        .zip(front)
                        .all(|(at, extent)| extents[*at] == *extent);
                if !agrees {
                    return Err(malformed(
                        "`einsum` reached lowering with operands whose `...` axes differ"
                            .to_string(),
                    ));
                }
                axes.extend_from_slice(bound);
            }
            for (letter, extent) in named.iter().zip(shape.iter()) {
                let index = *letters.entry(*letter).or_insert_with(|| {
                    extents.push(*extent);
                    extents.len() - 1
                });
                if extents[index] != *extent {
                    return Err(malformed(format!(
                        "`einsum` reached lowering binding '{letter}' to both {} and {}",
                        neuro_hir::extent_display(&extents[index]),
                        neuro_hir::extent_display(extent)
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
            if *letter == ELLIPSIS {
                result_axes.extend(leading.iter().flatten());
                continue;
            }
            let index = *letters.get(letter).ok_or_else(|| {
                malformed(format!(
                    "`einsum` reached lowering with the unbound output letter '{letter}'"
                ))
            })?;
            result_axes.push(index);
        }

        // An empty output subscript contracts everything away and the result is the bare
        // element, matching what the checker typed the call as. One that writes `...` is a
        // tensor whatever the ellipsis covers.
        let ty = if result_axes.is_empty() && !output.contains(&ELLIPSIS) {
            element
        } else {
            let shape: Vec<Option<usize>> =
                result_axes.iter().map(|index| extents[*index]).collect();
            HirType::Tensor {
                element: Box::new(element),
                shape: shape.clone(),
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
    let inputs = left.split(',').map(entries).collect();
    Ok((inputs, entries(right)))
}

/// One side's letters, with a `...` folded into one [`ELLIPSIS`] entry.
fn entries(piece: &str) -> Vec<char> {
    piece.replace("...", ".").chars().collect()
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
    fn an_ellipsis_is_one_entry() {
        let (inputs, output) = match split_subscripts("...ij,...jk->...ik") {
            Ok(split) => split,
            Err(error) => panic!("{error}"),
        };
        assert_eq!(inputs, vec![vec!['.', 'i', 'j'], vec!['.', 'j', 'k']]);
        assert_eq!(output, vec!['.', 'i', 'k']);
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
