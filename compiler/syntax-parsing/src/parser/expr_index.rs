//! The argument list inside `object[...]`.
//!
//! Two forms share the bracket. One plain expression is the index an array, `Vec`, or
//! `HashMap` takes, and stays [`Expr::Index`] so nothing about those types changes. A
//! comma-separated list, a range, or a bare `..` is the tensor index a tensor takes, and
//! no other indexable type accepts any of them — which is what lets the two be told
//! apart here, before any type is known.

use lexical_analysis::TokenKind;

use crate::ast::{Expr, TensorIndexArg};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;

/// What a bracket held, once parsed.
pub(super) enum IndexArguments {
    /// Exactly one plain expression: the form every indexable type takes.
    Single(Expr),
    /// One argument per axis: the tensor form.
    Axes(Vec<TensorIndexArg>),
}

impl Parser {
    /// Parse the comma-separated contents of an index bracket.
    pub(super) fn parse_index_arguments(&mut self) -> ParseResult<IndexArguments> {
        let mut indices = Vec::new();
        loop {
            self.skip_newlines();
            indices.push(self.parse_index_argument()?);
            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume ','
        }

        if let [TensorIndexArg::Position(_)] = indices.as_slice() {
            // `pop` yields the element the pattern above just matched.
            if let Some(TensorIndexArg::Position(index)) = indices.pop() {
                return Ok(IndexArguments::Single(index));
            }
        }
        Ok(IndexArguments::Axes(indices))
    }

    /// One axis argument: `..`, a range, or a position.
    fn parse_index_argument(&mut self) -> ParseResult<TensorIndexArg> {
        // A bare `..` is only a full-axis slice here: the four meanings of `..` are
        // separated by position, and no range, spread, or rest binding can open an index.
        if self.check(&TokenKind::DotDot) {
            let token = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'..'".to_string(),
            })?;
            return Ok(TensorIndexArg::FullAxis(token.span));
        }
        let expr = self.parse_expr(Precedence::Lowest)?;
        Ok(index_argument_from(expr))
    }
}

/// Classify a parsed index expression. A range is peeled out of one layer of
/// parentheses so `t[(0..3)]` names the same axis range as `t[0..3]`, and out of a
/// `.rev()` call so `t[(0..3).rev()]` names that axis read back to front.
fn index_argument_from(expr: Expr) -> TensorIndexArg {
    let (expr, reversed) = strip_rev(expr);
    let expr = match expr {
        Expr::Paren(inner, _) if matches!(*inner, Expr::Range { .. }) => *inner,
        other => other,
    };
    match expr {
        Expr::Range {
            start,
            end,
            inclusive,
            span,
        } => TensorIndexArg::Range {
            start,
            end,
            inclusive,
            reversed,
            span,
        },
        other => TensorIndexArg::Position(other),
    }
}

/// Split a `.rev()` call off an axis argument, returning its receiver and whether the
/// call was there. A `.rev()` over anything but a range falls through unchanged and is
/// classified as a position, where the type checker reports it as the non-integer it is.
fn strip_rev(expr: Expr) -> (Expr, bool) {
    let Expr::Call { func, args, .. } = &expr else {
        return (expr, false);
    };
    let Expr::FieldAccess { object, field, .. } = func.as_ref() else {
        return (expr, false);
    };
    if field.name != REV_METHOD || !args.is_empty() {
        return (expr, false);
    }
    let receiver = object.as_ref().clone();
    match &receiver {
        Expr::Range { .. } => (receiver, true),
        Expr::Paren(inner, _) if matches!(**inner, Expr::Range { .. }) => (receiver, true),
        _ => (expr, false),
    }
}

/// The range adapter an axis argument may wear.
const REV_METHOD: &str = "rev";
