//! Infix position: binary operators by precedence, calls, the pipe `|>` and the
//! composition `>>` operators.

use ast_types::{BinaryOp, Expr, Stmt};
use lexical_analysis::{Token, TokenKind};
use shared_types::{Identifier, Span};

use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;
use super::expr_index::IndexArguments;
use super::expressions::finish_call;

/// The function names one operand of `>>` contributes to the chain.
///
/// Composition takes *named* functions: a bare name is not a value in this language,
/// so the operand is validated here rather than left to produce a type error about a
/// callee that is not callable. An operand that is already a `Compose` is the left of
/// `f >> g >> h`, and flattening it there is what keeps the chain one node.
pub(super) fn compose_operand(operand: Expr) -> ParseResult<Vec<Identifier>> {
    match operand {
        Expr::Paren(inner, _) => compose_operand(*inner),
        Expr::Identifier(name) => Ok(vec![name]),
        Expr::Compose { functions, .. } => Ok(functions),
        other => Err(ParseError::NotAComposeOperand { span: other.span() }),
    }
}

/// The composed function chain `functions`, applied to `arg`: `h(g(f(arg)))`.
pub(super) fn apply_compose(functions: &[Identifier], arg: Expr, span: Span) -> Expr {
    functions.iter().fold(arg, |value, function| Expr::Call {
        func: Box::new(Expr::Identifier(function.clone())),
        type_args: Vec::new(),
        args: vec![value],
        arg_labels: Vec::new(),
        span,
    })
}

/// The composition chain `callee` names, seen through any parentheses around it.
pub(super) fn as_compose(callee: &Expr) -> Option<&[Identifier]> {
    match callee {
        Expr::Paren(inner, _) => as_compose(inner),
        Expr::Compose { functions, .. } => Some(functions),
        _ => None,
    }
}

impl Parser {
    /// Parse an infix expression (binary operators, function calls, field access, casts)
    pub(super) fn parse_infix(&mut self, left: Expr) -> ParseResult<Expr> {
        let token = self.peek().ok_or_else(|| ParseError::UnexpectedEof {
            expected: "operator or '('".to_string(),
        })?;

        match &token.kind {
            TokenKind::LeftParen => {
                self.advance(); // consume '('
                let (args, arg_labels) = self.parse_call_arguments()?;
                let close = self.consume(TokenKind::RightParen, "')'")?;
                let span = left.span().merge(close.span);

                Ok(finish_call(left, args, arg_labels, span))
            }

            // Turbofish `callee::<T, N>(args)`: explicit generic arguments before a
            // call. Only valid immediately before a call, so a `(` argument list must
            // follow the `>`.
            TokenKind::ColonColon => {
                self.advance(); // consume '::'
                let type_args = self.parse_turbofish_args()?;
                self.consume(TokenKind::LeftParen, "'(' after turbofish `::<...>`")?;
                let (args, arg_labels) = self.parse_call_arguments()?;
                let close = self.consume(TokenKind::RightParen, "')'")?;
                let span = left.span().merge(close.span);
                Ok(Expr::Call {
                    func: Box::new(left),
                    type_args,
                    args,
                    arg_labels,
                    span,
                })
            }

            // Field access `expr.field` or tuple index `expr.0`. A numeric
            // token after the dot is a constant tuple index; an identifier names a
            // struct field. (Chained `t.0.1` is lexed as `t` `.` `0.1`(float), so a
            // nested tuple element is accessed as `(t.0).1`.)
            TokenKind::Dot => {
                self.advance(); // consume '.'
                if let Some(TokenKind::Integer(_)) = self.peek_kind() {
                    let idx_token = self.advance().ok_or_else(|| ParseError::UnexpectedEof {
                        expected: "tuple index".to_string(),
                    })?;
                    let TokenKind::Integer(n) = idx_token.kind else {
                        unreachable!("guarded by peek above")
                    };
                    // No sign check: an integer token carries a magnitude, so a
                    // negative index is a `-` token followed by one and never reaches
                    // here as a single token.
                    let span = left.span().merge(idx_token.span);
                    return Ok(Expr::TupleIndex {
                        object: Box::new(left),
                        index: n as usize,
                        span,
                    });
                }
                let field_token =
                    self.consume(TokenKind::Identifier(String::new()), "field name")?;
                let field = if let TokenKind::Identifier(name) = field_token.kind {
                    Identifier {
                        name,
                        span: field_token.span,
                    }
                } else {
                    return Err(ParseError::UnexpectedToken {
                        found: field_token.kind,
                        expected: "field name".to_string(),
                        span: field_token.span,
                    });
                };
                let span = left.span().merge(field.span);
                Ok(Expr::FieldAccess {
                    object: Box::new(left),
                    field,
                    span,
                })
            }

            // Range expression `start..end` / `start..=end`. Only meaningful as
            // a `string.slice` argument; semantic analysis rejects it elsewhere. The
            // right operand is parsed at `Range` precedence so a stray second `..` ends
            // the expression rather than chaining.
            TokenKind::DotDot | TokenKind::DotDotEqual => {
                let op_token = self.advance().ok_or_else(|| ParseError::UnexpectedEof {
                    expected: "'..' or '..='".to_string(),
                })?;
                let inclusive = matches!(op_token.kind, TokenKind::DotDotEqual);
                let right = self.parse_expr(Precedence::Range)?;
                let span = left.span().merge(right.span());
                Ok(Expr::Range {
                    start: Box::new(left),
                    end: Box::new(right),
                    inclusive,
                    span,
                })
            }

            // Indexing `object[...]`. Binds at call precedence so `arr[i]` is a
            // tight postfix on the preceding primary. One plain argument is the
            // array / `Vec` / `HashMap` index; anything else is a tensor index.
            TokenKind::LeftBracket => {
                self.advance(); // consume '['
                let arguments = self.inside_delimiters(|p| p.parse_index_arguments())?;
                let close = self.consume(TokenKind::RightBracket, "']' to close index")?;
                let span = left.span().merge(close.span);
                Ok(match arguments {
                    IndexArguments::Single(index) => Expr::Index {
                        object: Box::new(left),
                        index: Box::new(index),
                        span,
                    },
                    IndexArguments::Axes(indices) => Expr::TensorIndex {
                        object: Box::new(left),
                        indices,
                        span,
                    },
                })
            }

            // Error propagation `operand?`. A postfix operator: it binds as tightly
            // as a call, so `f(x)? + 1` propagates the call's failure and adds to its
            // payload, and `parse(s)?.field` reads a field of the unwrapped value.
            TokenKind::Question => {
                let op_token = self.advance().ok_or_else(|| ParseError::UnexpectedEof {
                    expected: "'?'".to_string(),
                })?;
                let span = left.span().merge(op_token.span);
                Ok(Expr::Try {
                    operand: Box::new(left),
                    span,
                })
            }

            // Pipeline `left |> target`: the left value becomes the target's first
            // argument. Desugared here, so no later stage learns the
            // operator exists. The right operand is parsed at `Pipeline` precedence,
            // which makes the operator left-associative: a following `|>` ends the
            // target and re-enters the loop with the call as its new left.
            TokenKind::PipeGreater => {
                self.advance(); // consume '|>'
                self.skip_newlines();
                let target = self.parse_expr(Precedence::Pipeline)?;
                self.pipe_into(left, target)
            }

            // Composition `left >> right`, lexed as two adjacent `>`: see
            // `at_compose`. The right operand is parsed at `Compose` precedence, which
            // makes the operator left-associative the way `|>` is, and the chain is
            // flattened as it is built so `f >> g >> h` is one node.
            TokenKind::Greater if self.at_compose() => {
                self.advance(); // consume the first '>'
                self.advance(); // consume the second '>'
                self.skip_newlines();
                let right = self.parse_expr(Precedence::Compose)?;
                let span = left.span().merge(right.span());
                let mut functions = compose_operand(left)?;
                functions.extend(compose_operand(right)?);
                Ok(Expr::Compose { functions, span })
            }

            // Type casts
            TokenKind::As => {
                self.advance(); // consume 'as'
                let target_type = self.parse_type()?;
                let span = left.span().merge(target_type.span());

                Ok(Expr::Cast {
                    expr: Box::new(left),
                    target_type,
                    span,
                })
            }

            kind if self.is_binary_op(kind) => {
                let op_token = self.advance().ok_or_else(|| ParseError::UnexpectedEof {
                    expected: "operator".to_string(),
                })?;
                let op = self.token_to_binary_op(&op_token)?;
                let precedence = self.get_precedence(&op_token.kind);
                // R-to-L coalescing (`??`): recurse at one-step-lower precedence so the
                // outer loop re-enters on the next `??` instead of stopping. Appendix B row 14.
                let right_prec = if matches!(op_token.kind, TokenKind::QuestionQuestion) {
                    Precedence::Lowest
                } else {
                    precedence
                };
                let right = self.parse_expr(right_prec)?;
                let span = left.span().merge(right.span());

                Ok(Expr::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                    span,
                })
            }

            _ => Err(ParseError::UnexpectedToken {
                found: token.kind.clone(),
                expected: "operator or '('".to_string(),
                span: token.span,
            }),
        }
    }

    /// Check if a token kind is a binary operator
    pub(super) fn is_binary_op(&self, kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
                | TokenKind::EqualEqual
                | TokenKind::NotEqual
                | TokenKind::Less
                | TokenKind::Greater
                | TokenKind::LessEqual
                | TokenKind::GreaterEqual
                | TokenKind::AmpAmp
                | TokenKind::PipePipe
                | TokenKind::Amp
                | TokenKind::Pipe
                | TokenKind::Caret
                | TokenKind::LeftShift
                | TokenKind::At
                | TokenKind::QuestionQuestion
        )
    }

    /// Convert a token to a binary operator
    pub(super) fn token_to_binary_op(&self, token: &Token) -> ParseResult<BinaryOp> {
        match &token.kind {
            TokenKind::Plus => Ok(BinaryOp::Add),
            TokenKind::Minus => Ok(BinaryOp::Subtract),
            TokenKind::Star => Ok(BinaryOp::Multiply),
            TokenKind::Slash => Ok(BinaryOp::Divide),
            TokenKind::Percent => Ok(BinaryOp::Modulo),
            TokenKind::EqualEqual => Ok(BinaryOp::Equal),
            TokenKind::NotEqual => Ok(BinaryOp::NotEqual),
            TokenKind::Less => Ok(BinaryOp::Less),
            TokenKind::Greater => Ok(BinaryOp::Greater),
            TokenKind::LessEqual => Ok(BinaryOp::LessEqual),
            TokenKind::GreaterEqual => Ok(BinaryOp::GreaterEqual),
            TokenKind::AmpAmp => Ok(BinaryOp::And),
            TokenKind::PipePipe => Ok(BinaryOp::Or),
            TokenKind::Amp => Ok(BinaryOp::BitAnd),
            TokenKind::Pipe => Ok(BinaryOp::BitOr),
            TokenKind::Caret => Ok(BinaryOp::BitXor),
            TokenKind::LeftShift => Ok(BinaryOp::Shl),
            TokenKind::At => Ok(BinaryOp::MatMul),
            TokenKind::QuestionQuestion => Ok(BinaryOp::NullCoalesce),
            _ => Err(ParseError::UnexpectedToken {
                found: token.kind.clone(),
                expected: "binary operator".to_string(),
                span: token.span,
            }),
        }
    }

    /// Build the call a `value |> target` pipeline stands for.
    ///
    /// The language admits exactly three spellings of `target`, and each is already a
    /// callee shape the rest of the pipeline understands: a function name or
    /// associated path becomes a plain call, a bound method `receiver.method`
    /// becomes the ordinary method call `receiver.method(value)`, and a closure
    /// literal is bound to a temporary first, because a call whose callee is a
    /// closure *literal* is not a form any later stage accepts. Rejecting anything
    /// else here is what keeps `x |> f(a)` a diagnostic about `|>` rather than a
    /// type error about calling a non-callable.
    pub(super) fn pipe_into(&mut self, value: Expr, target: Expr) -> ParseResult<Expr> {
        let span = value.span().merge(target.span());
        match target {
            Expr::Paren(inner, _) => self.pipe_into(value, *inner),

            // `x |> f >> g` applies the composition rather than binding it, and
            // applying it is the nested call it stands for. `>>` binds tighter than
            // `|>` (Appendix B rows 16 and 17), which is what puts the whole chain
            // here as one target.
            Expr::Compose { functions, .. } => Ok(apply_compose(&functions, value, span)),

            Expr::Identifier(_) | Expr::Path { .. } | Expr::FieldAccess { .. } => Ok(Expr::Call {
                func: Box::new(target),
                type_args: Vec::new(),
                args: vec![value],
                arg_labels: Vec::new(),
                span,
            }),

            Expr::Closure { .. } => {
                let tmp = Identifier {
                    name: format!("__pipe_{}", self.next_pipe_id()),
                    span: target.span(),
                };
                let call = Expr::Call {
                    func: Box::new(Expr::Identifier(tmp.clone())),
                    type_args: Vec::new(),
                    args: vec![value],
                    arg_labels: Vec::new(),
                    span,
                };
                Ok(Expr::Block {
                    stmts: vec![
                        Stmt::VarDecl {
                            name: tmp,
                            ty: None,
                            init: Some(target),
                            mutable: false,
                            span,
                        },
                        Stmt::Expr(call),
                    ],
                    span,
                })
            }

            other => Err(ParseError::NotAPipelineTarget { span: other.span() }),
        }
    }

    /// Allocate a unique id for a pipeline temporary.
    pub(super) fn next_pipe_id(&mut self) -> usize {
        let id = self.pipe_counter;
        self.pipe_counter += 1;
        id
    }

    /// Whether the cursor sits on `>>`, the composition operator.
    ///
    /// `>>` is not a token. Lexing it as one would make `Vec<Vec<i32>>` end in a
    /// single token the type parser has to split, the cost every C-family grammar
    /// pays for nested generics; two adjacent `>` cost nothing and are unambiguous,
    /// because right shift is the `.shr(n)` method here (Appendix B) and a comparison
    /// never has `>` as the first token of its right operand.
    pub(super) fn at_compose(&self) -> bool {
        let (Some(first), Some(second)) = (
            self.tokens.get(self.current),
            self.tokens.get(self.current + 1),
        ) else {
            return false;
        };
        matches!(first.kind, TokenKind::Greater)
            && matches!(second.kind, TokenKind::Greater)
            && first.span.end == second.span.start
    }

    /// The precedence of the operator at the cursor.
    ///
    /// Separate from [`Parser::get_precedence`] because `>>` is a token *pair*: only
    /// the cursor can see it, and a lone `>` is a comparison.
    pub(super) fn infix_precedence(&self, kind: &TokenKind) -> Precedence {
        if self.at_compose() {
            return Precedence::Compose;
        }
        self.get_precedence(kind)
    }

    /// Get the precedence of an operator token
    pub(super) fn get_precedence(&self, kind: &TokenKind) -> Precedence {
        match kind {
            TokenKind::PipeGreater => Precedence::Pipeline,
            TokenKind::PipePipe => Precedence::LogicalOr,
            TokenKind::AmpAmp => Precedence::LogicalAnd,
            TokenKind::Pipe => Precedence::BitwiseOr,
            TokenKind::Caret => Precedence::BitwiseXor,
            TokenKind::Amp => Precedence::BitwiseAnd,
            TokenKind::EqualEqual | TokenKind::NotEqual => Precedence::Equality,
            TokenKind::Less
            | TokenKind::Greater
            | TokenKind::LessEqual
            | TokenKind::GreaterEqual => Precedence::Comparison,
            TokenKind::LeftShift => Precedence::Shift,
            TokenKind::QuestionQuestion => Precedence::NullCoalesce,
            TokenKind::Plus | TokenKind::Minus => Precedence::Sum,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Precedence::Product,
            TokenKind::At => Precedence::MatMul,
            TokenKind::DotDot | TokenKind::DotDotEqual => Precedence::Range,
            TokenKind::As => Precedence::Cast,
            TokenKind::LeftParen => Precedence::Call,
            TokenKind::LeftBracket => Precedence::Call,
            TokenKind::Question => Precedence::Call,
            // A turbofish `::<...>` binds like a call: it only ever precedes one.
            TokenKind::ColonColon => Precedence::Call,
            TokenKind::Dot => Precedence::FieldAccess,
            _ => Precedence::Lowest,
        }
    }
}
