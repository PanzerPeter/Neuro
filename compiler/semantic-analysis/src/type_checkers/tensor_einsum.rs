// The Einstein-notation contraction `einsum("bij,bjk->bik", a, b)`.
//
// Reached from the free-function arm of `check_plain_call`, beside the panic and
// standard-output resolvers, and shadowed by a user function of the same name the way
// they are. `einsum` is the one variadic call in the language, and only because the
// subscript literal fixes its arity: the letters left of `->` say how many operands
// there are and what rank each one has, so the call is as checkable as a declared
// signature would be.
//
// The subscripts are read as SYNTAX, not as a value. A string a program computes could
// not decide a result shape the rest of type checking depends on, which is why the
// language requires a literal.
//
// Every operand is READ. Like a reduction, a contraction allocates its own result and
// summarises buffers their owners keep, so nothing is moved and a borrowed operand is
// accepted.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{ArrayLen, TensorAxis, Type};
use ast_types::Expr;
use shared_types::{Literal, Span};
use std::collections::HashMap;

pub(crate) const EINSUM_FUNC: &str = "einsum";

/// One subscript string, split into the letters of each operand and of the result.
pub(crate) struct Subscripts {
    pub(crate) inputs: Vec<Vec<char>>,
    pub(crate) output: Vec<char>,
}

/// Split `text` at its `->` and read each side's letters.
///
/// Returns the reason a subscript string is not one, phrased for the diagnostic that
/// quotes it. Whitespace is not accepted anywhere: a subscript is dense by construction
/// and a stray space is far more likely to be a typo than an intention.
pub(crate) fn parse_subscripts(text: &str) -> Result<Subscripts, String> {
    let Some((left, right)) = text.split_once("->") else {
        return Err("it has no `->`; write the result's letters after one, \
                    e.g. \"ij,jk->ik\""
            .to_string());
    };
    if right.contains("->") {
        return Err("it has more than one `->`; a contraction has one result".to_string());
    }
    let inputs = left
        .split(',')
        .map(|piece| letters(piece, "left of `->`"))
        .collect::<Result<Vec<Vec<char>>, String>>()?;
    let output = letters(right, "right of `->`")?;
    Ok(Subscripts { inputs, output })
}

/// One operand's or the result's letters, rejecting anything that is not one.
fn letters(piece: &str, side: &str) -> Result<Vec<char>, String> {
    piece
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphabetic() {
                Ok(letter)
            } else {
                Err(format!(
                    "'{}' {} is not a subscript letter; subscripts are ASCII letters \
                     separated by `,`",
                    letter.escape_debug(),
                    side
                ))
            }
        })
        .collect()
}

impl TypeChecker {
    /// Type-check a call to `einsum` and return the type it produces: the element type
    /// when the result has no axes, and a tensor of the output letters' extents otherwise.
    ///
    /// Returns `None` when `func_name` is not `einsum`, so the caller falls through to
    /// ordinary function resolution.
    pub(super) fn resolve_einsum_builtin(
        &mut self,
        func_name: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        if func_name != EINSUM_FUNC {
            return None;
        }

        let Some((subscripts, operands)) = args.split_first() else {
            self.record_error(TypeError::EinsumSubscriptNotLiteral { span });
            return Some(Type::Unknown);
        };
        let Expr::Literal(Literal::String(text), _) = subscripts else {
            // The operands are still worth checking: a mistake in one of them is a
            // separate mistake, and reporting only the subscript would hide it.
            for operand in operands {
                self.check_expr(operand, None);
            }
            self.record_error(TypeError::EinsumSubscriptNotLiteral {
                span: subscripts.span(),
            });
            return Some(Type::Unknown);
        };

        let operand_types: Vec<Type> = operands
            .iter()
            .map(|operand| self.check_expr(operand, None).unwrap_or(Type::Unknown))
            .collect();

        let parsed = match parse_subscripts(text) {
            Ok(parsed) => parsed,
            Err(reason) => {
                self.record_error(TypeError::EinsumMalformedSubscripts {
                    subscripts: text.clone(),
                    reason,
                    span: subscripts.span(),
                });
                return Some(Type::Unknown);
            }
        };
        if parsed.inputs.len() != operands.len() {
            self.record_error(TypeError::EinsumOperandCount {
                expected: parsed.inputs.len(),
                found: operands.len(),
                span,
            });
            return Some(Type::Unknown);
        }

        let mut extents: HashMap<char, usize> = HashMap::new();
        let mut element: Option<Type> = None;
        for (position, ((operand, ty), subscript)) in operands
            .iter()
            .zip(operand_types.iter())
            .zip(parsed.inputs.iter())
            .enumerate()
        {
            if !self.bind_einsum_operand(
                position,
                operand.span(),
                ty,
                subscript,
                &mut extents,
                &mut element,
            ) {
                return Some(Type::Unknown);
            }
        }

        let Some(element) = element else {
            // No operand at all: there is nothing to take an element type from, and the
            // subscript cannot supply one.
            self.record_error(TypeError::EinsumOperandCount {
                expected: 1,
                found: 0,
                span,
            });
            return Some(Type::Unknown);
        };

        let mut shape = Vec::with_capacity(parsed.output.len());
        let mut seen: Vec<char> = Vec::with_capacity(parsed.output.len());
        for letter in parsed.output {
            if seen.contains(&letter) {
                self.record_error(TypeError::EinsumOutputLetterRepeated {
                    letter,
                    span: subscripts.span(),
                });
                return Some(Type::Unknown);
            }
            seen.push(letter);
            let Some(extent) = extents.get(&letter) else {
                self.record_error(TypeError::EinsumOutputLetterUnbound {
                    letter,
                    span: subscripts.span(),
                });
                return Some(Type::Unknown);
            };
            shape.push(TensorAxis {
                name: None,
                extent: ArrayLen::Fixed(*extent),
            });
        }

        // An empty output subscript contracts everything away, so the result has no axes
        // to describe. It is the element type rather than `Tensor<T, []>`, matching the
        // whole-tensor `.sum()`: both spell "one number out of a buffer", and a reader
        // should not have to know which one produced it to use the answer.
        if shape.is_empty() {
            return Some(element);
        }
        Some(Type::Tensor {
            element: Box::new(element),
            shape,
        })
    }

    /// Match one operand against its subscript, binding each letter's extent and fixing
    /// the contraction's element type. Returns whether the operand was usable.
    fn bind_einsum_operand(
        &mut self,
        position: usize,
        span: Span,
        ty: &Type,
        subscript: &[char],
        extents: &mut HashMap<char, usize>,
        element: &mut Option<Type>,
    ) -> bool {
        if matches!(ty, Type::Unknown) {
            return false;
        }
        let Type::Tensor {
            element: operand_element,
            shape,
        } = ty.referent()
        else {
            self.record_error(TypeError::EinsumOperandNotTensor {
                position,
                ty: ty.clone(),
                span,
            });
            return false;
        };
        if shape.len() != subscript.len() {
            self.record_error(TypeError::EinsumOperandRank {
                position,
                subscript: subscript.iter().collect(),
                expected: subscript.len(),
                found: shape.len(),
                span,
            });
            return false;
        }

        let operand_element = (**operand_element).clone();
        match element {
            None => {
                if !operand_element.is_integer()
                    && !operand_element.is_float()
                    && !operand_element.is_half_float()
                {
                    self.record_error(TypeError::EinsumElementType {
                        element: operand_element,
                        span,
                    });
                    return false;
                }
                *element = Some(operand_element);
            }
            Some(expected) if *expected != operand_element => {
                self.record_error(TypeError::EinsumElementMismatch {
                    position,
                    expected: expected.clone(),
                    found: operand_element,
                    span,
                });
                return false;
            }
            Some(_) => {}
        }

        for (letter, axis) in subscript.iter().zip(shape.iter()) {
            let ArrayLen::Fixed(extent) = axis.extent else {
                self.record_error(TypeError::EinsumSymbolicExtent {
                    position,
                    name: axis.extent.to_string(),
                    span,
                });
                return false;
            };
            match extents.get(letter) {
                Some(bound) if *bound != extent => {
                    self.record_error(TypeError::EinsumExtentConflict {
                        letter: *letter,
                        first: *bound,
                        second: extent,
                        span,
                    });
                    return false;
                }
                Some(_) => {}
                None => {
                    extents.insert(*letter, extent);
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::parse_subscripts;

    #[test]
    fn splits_a_matmul_subscript() {
        let parsed = match parse_subscripts("ij,jk->ik") {
            Ok(parsed) => parsed,
            Err(reason) => panic!("{reason}"),
        };
        assert_eq!(parsed.inputs, vec![vec!['i', 'j'], vec!['j', 'k']]);
        assert_eq!(parsed.output, vec!['i', 'k']);
    }

    #[test]
    fn a_trace_has_one_operand_and_no_output_letters() {
        let parsed = match parse_subscripts("ii->") {
            Ok(parsed) => parsed,
            Err(reason) => panic!("{reason}"),
        };
        assert_eq!(parsed.inputs, vec![vec!['i', 'i']]);
        assert!(parsed.output.is_empty());
    }

    #[test]
    fn a_missing_arrow_is_reported() {
        assert!(parse_subscripts("ij,jk").is_err());
    }

    #[test]
    fn a_second_arrow_is_reported() {
        assert!(parse_subscripts("ij->jk->ik").is_err());
    }

    #[test]
    fn a_space_is_not_a_subscript_letter() {
        assert!(parse_subscripts("ij, jk->ik").is_err());
    }

    #[test]
    fn a_digit_is_not_a_subscript_letter() {
        assert!(parse_subscripts("i1->i").is_err());
    }
}
