// `if` chains, and the `val PATTERN = scrutinee` head that `if` and `while` share.
//
// An `if val` link parses straight to the `match` it means and a `while val` loop to a
// `while` around one, so no later stage learns either form exists, and both accept
// every pattern a `match` arm does.

use lexical_analysis::TokenKind;
use shared_types::{Identifier, Literal, Span};

use crate::errors::ParseResult;
use crate::precedence::Precedence;
use ast_types::{Expr, MatchArm, Pattern, Stmt};

use super::Parser;

/// A parsed `if` chain, before it is placed as a statement or an expression.
pub(super) struct IfChain {
    shape: IfShape,
    /// The chain ends in a bare `else`, so it can carry a value.
    has_else: bool,
}

enum IfShape {
    Plain {
        condition: Expr,
        then_block: Vec<Stmt>,
        else_if_blocks: Vec<(Expr, Vec<Stmt>)>,
        else_block: Option<Vec<Stmt>>,
        span: Span,
    },
    Match(Expr),
}

impl IfChain {
    pub(super) fn into_stmt(self) -> Stmt {
        match self.shape {
            IfShape::Plain {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            } => Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            },
            IfShape::Match(expr) => Stmt::Expr(expr),
        }
    }

    pub(super) fn into_expr(self) -> Expr {
        match self.shape {
            IfShape::Plain {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            } => Expr::If {
                condition: Box::new(condition),
                then_block,
                else_if_blocks,
                else_block,
                span,
            },
            IfShape::Match(expr) => expr,
        }
    }
}

impl Parser {
    /// Parse an `if` chain. The `if` keyword is already consumed; `start_span` is its
    /// span. `in_statement` keeps each position's rule for the newlines before an `else`.
    pub(super) fn parse_if_chain(
        &mut self,
        start_span: Span,
        in_statement: bool,
    ) -> ParseResult<IfChain> {
        self.skip_newlines();
        if self.check(&TokenKind::Val) {
            self.advance(); // consume 'val'
            return self.parse_if_val(start_span, in_statement);
        }

        let condition = self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
        self.skip_newlines();
        let mut then_block = self.parse_block()?;
        let mut end_span = self.previous_span(start_span);
        self.skip_newlines_to_else(in_statement);

        let mut else_if_blocks = Vec::new();
        let mut else_block = None;
        let mut has_else = false;

        while self.check(&TokenKind::Else) {
            self.advance(); // consume 'else'
            self.skip_newlines();

            if !self.check(&TokenKind::If) {
                else_block = Some(self.parse_block()?);
                end_span = self.previous_span(start_span);
                has_else = true;
                break;
            }
            let if_span = self.advance().map_or(end_span, |token| token.span);
            self.skip_newlines();

            if self.check(&TokenKind::Val) {
                // The `match` an `if val` link means has no seat in a list of boolean
                // conditions, so the rest of the chain becomes this `if`'s else branch.
                let rest = self.nested(|p| p.parse_if_chain(if_span, in_statement))?;
                end_span = self.previous_span(start_span);
                has_else = rest.has_else;
                if !has_else {
                    unit_tail(&mut then_block, end_span);
                    for (_, block) in &mut else_if_blocks {
                        unit_tail(block, end_span);
                    }
                }
                else_block = Some(vec![rest.into_stmt()]);
                break;
            }

            let else_if_condition = self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
            self.skip_newlines();
            let else_if_block = self.parse_block()?;
            end_span = self.previous_span(start_span);
            else_if_blocks.push((else_if_condition, else_if_block));
            self.skip_newlines_to_else(in_statement);
        }

        Ok(IfChain {
            shape: IfShape::Plain {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span: start_span.merge(end_span),
            },
            has_else,
        })
    }

    /// Parse the rest of `if val PATTERN = scrutinee { then } else ...`, the `val`
    /// already consumed, as `match scrutinee { PATTERN => then, _ => else }`.
    fn parse_if_val(&mut self, start_span: Span, in_statement: bool) -> ParseResult<IfChain> {
        let (pattern, scrutinee) = self.parse_val_head("'=' after an `if val` pattern")?;
        self.skip_newlines();
        let mut then_block = self.parse_block()?;
        let then_end = self.previous_span(start_span);
        self.skip_newlines_to_else(in_statement);

        let (otherwise, has_else) = if self.check(&TokenKind::Else) {
            self.advance(); // consume 'else'
            self.skip_newlines();
            if self.check(&TokenKind::If) {
                let if_span = self.advance().map_or(then_end, |token| token.span);
                let rest = self.nested(|p| p.parse_if_chain(if_span, in_statement))?;
                let has_else = rest.has_else;
                (vec![rest.into_stmt()], has_else)
            } else {
                (self.parse_block()?, true)
            }
        } else {
            (Vec::new(), false)
        };
        let end_span = self.previous_span(start_span);
        if !has_else {
            unit_tail(&mut then_block, then_end);
        }

        let otherwise = Expr::Block {
            stmts: otherwise,
            span: then_end.merge(end_span),
        };
        let matched = val_match(
            scrutinee,
            pattern,
            then_block,
            then_end,
            otherwise,
            start_span.merge(end_span),
        );
        Ok(IfChain {
            shape: IfShape::Match(matched),
            has_else,
        })
    }

    /// Parse the rest of `while val PATTERN = scrutinee { body }`, the `val` already
    /// consumed, as `while (true) { match scrutinee { PATTERN => body, _ => break } }`.
    ///
    /// A `while` rather than a `loop`, so a `break` in the body still cannot carry a value
    /// and the loop stays unit. The condition is `(true)` because the bare literal is what
    /// `prefer_loop_over_while_true` flags, and the source holds no `while true` to flag.
    pub(super) fn parse_while_val(
        &mut self,
        start_span: Span,
        label: Option<Identifier>,
    ) -> ParseResult<Stmt> {
        let val_span = self.previous_span(start_span);
        let (pattern, scrutinee) = self.parse_val_head("'=' after a `while val` pattern")?;
        self.skip_newlines();
        let mut body = self.parse_block()?;
        let end_span = self.previous_span(start_span);
        unit_tail(&mut body, end_span);
        let span = start_span.merge(end_span);

        let exit = Expr::Block {
            stmts: vec![Stmt::Break {
                label: None,
                value: None,
                span: val_span,
            }],
            span: val_span,
        };
        let condition = Expr::Paren(
            Box::new(Expr::Literal(Literal::Boolean(true), val_span)),
            val_span,
        );
        let matched = val_match(scrutinee, pattern, body, end_span, exit, span);
        Ok(Stmt::While {
            label,
            condition,
            body: vec![Stmt::Expr(matched)],
            span,
        })
    }

    /// Parse `PATTERN = scrutinee`, the head after the `val` of an `if val` or a
    /// `while val`. The scrutinee is a header: the `{` after it opens the body.
    fn parse_val_head(&mut self, missing_equal: &str) -> ParseResult<(Pattern, Expr)> {
        let pattern = self.parse_pattern()?;
        self.skip_newlines();
        self.consume(TokenKind::Equal, missing_equal)?;
        self.skip_newlines();
        let scrutinee = self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
        Ok((pattern, scrutinee))
    }

    /// A statement `if` skips every newline after a block. An expression `if` skips them
    /// only when an `else` follows: with none to find, the newline is what ends the
    /// expression, and skipping it let a following `-x` or `(...)` line continue it as
    /// an operand.
    fn skip_newlines_to_else(&mut self, in_statement: bool) {
        if in_statement || matches!(self.peek_next_nonnewline_kind(), Some(TokenKind::Else)) {
            self.skip_newlines();
        }
    }
}

/// `match scrutinee { pattern => { then }, _ => otherwise }`.
fn val_match(
    scrutinee: Expr,
    pattern: Pattern,
    then: Vec<Stmt>,
    then_end: Span,
    otherwise: Expr,
    span: Span,
) -> Expr {
    let then_span = pattern.span().merge(then_end);
    let otherwise_span = otherwise.span();
    Expr::Match {
        scrutinee: Box::new(scrutinee),
        arms: vec![
            MatchArm {
                patterns: vec![pattern],
                guard: None,
                body: Box::new(Expr::Block {
                    stmts: then,
                    span: then_span,
                }),
                span: then_span,
            },
            MatchArm {
                patterns: vec![Pattern::Wildcard(otherwise_span)],
                guard: None,
                body: Box::new(otherwise),
                span: otherwise_span,
            },
        ],
        span,
    }
}

/// End a branch that has no `else` partner with `{}` when its last statement is a value,
/// so it is unit whatever that value's type, as the body of an `if` with no `else` is.
/// The `match` the branch now sits in needs its arms to agree; the `if` it stands for
/// does not ask that.
fn unit_tail(block: &mut Vec<Stmt>, span: Span) {
    if matches!(
        block.last(),
        Some(
            Stmt::Expr(_)
                | Stmt::If {
                    else_block: Some(_),
                    ..
                }
        )
    ) {
        block.push(Stmt::Expr(Expr::Block {
            stmts: Vec::new(),
            span,
        }));
    }
}
