//! Infix position: binary operators by precedence, calls, the pipe `|>` and the
//! composition `>>` operators.

use ast_types::{BinaryOp, COMPOSE_PARAM, ClosureParam, Expr};
use lexical_analysis::{Token, TokenKind};
use shared_types::{Identifier, Span};

use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;
use super::expr_index::IndexArguments;
use super::expressions::finish_call;

/// `stage` applied to `value`: the call a pipeline stage or a composition stage stands
/// for. A function name, a path or a bound method `receiver.method` becomes the call
/// written with it, an instance `f::<T>` the call carrying its turbofish, and a closure
/// literal a call of the literal in place. A composition is applied by putting `value`
/// where its parameter was, so `x |> f >> g` and `(f >> g)(x)` build no function value.
/// Anything else is refused with its span.
pub(super) fn apply_stage(stage: Expr, value: Expr) -> Result<Expr, Span> {
    let span = stage.span();
    let (func, type_args) = match stage {
        Expr::Paren(inner, _) => return apply_stage(*inner, value),
        Expr::Closure { params, body, .. } if is_compose(&params) => {
            return Ok(substitute_compose_param(*body, value));
        }
        Expr::Instance {
            callee, type_args, ..
        } => (callee, type_args),
        stage @ (Expr::Identifier(_)
        | Expr::Path { .. }
        | Expr::FieldAccess { .. }
        | Expr::Closure { .. }) => (Box::new(stage), Vec::new()),
        other => return Err(other.span()),
    };
    Ok(Expr::Call {
        func,
        type_args,
        args: vec![value],
        arg_labels: Vec::new(),
        span,
    })
}

/// Build the call a `value |> target` pipeline stands for, as [`apply_stage`] does.
/// Rejecting any other target here is what keeps `x |> f(a)` a diagnostic about `|>`
/// rather than a type error about calling a non-callable.
fn pipe_into(value: Expr, target: Expr) -> ParseResult<Expr> {
    let span = value.span().merge(target.span());
    let applied =
        apply_stage(target, value).map_err(|span| ParseError::NotAPipelineTarget { span })?;
    Ok(match applied {
        Expr::Call {
            func,
            type_args,
            args,
            arg_labels,
            ..
        } => Expr::Call {
            func,
            type_args,
            args,
            arg_labels,
            span,
        },
        applied => applied,
    })
}

/// Whether a closure's parameter list is the one a composition is parsed to.
pub(super) fn is_compose(params: &[ClosureParam]) -> bool {
    matches!(params, [param] if param.name.name == COMPOSE_PARAM)
}

/// The composition `callee` applied to `value`: its body with `value` in place of its
/// parameter. Any other callee is called with `value`.
pub(super) fn apply_composition(callee: Expr, value: Expr) -> Expr {
    match callee {
        Expr::Paren(inner, _) => apply_composition(*inner, value),
        Expr::Closure { params, body, .. } if is_compose(&params) => {
            substitute_compose_param(*body, value)
        }
        other => {
            let span = other.span();
            Expr::Call {
                func: Box::new(other),
                type_args: Vec::new(),
                args: vec![value],
                arg_labels: Vec::new(),
                span,
            }
        }
    }
}

/// A composition body with `value` in place of its parameter. The body is the chain of
/// one-argument calls `apply_stage` built, so the parameter is the innermost argument.
fn substitute_compose_param(body: Expr, value: Expr) -> Expr {
    match body {
        Expr::Call {
            func,
            type_args,
            mut args,
            arg_labels,
            span,
        } if args.len() == 1 => {
            let inner = args.remove(0);
            Expr::Call {
                func,
                type_args,
                args: vec![substitute_compose_param(inner, value)],
                arg_labels,
                span,
            }
        }
        _ => value,
    }
}

/// Whether `receiver` names a place: a variable, or a field or element of one. A bound
/// method in a composition borrows it, and only a place outlives the expression.
fn is_named_place(receiver: &Expr) -> bool {
    match receiver {
        Expr::Identifier(_) => true,
        Expr::Paren(inner, _)
        | Expr::FieldAccess { object: inner, .. }
        | Expr::TupleIndex { object: inner, .. } => is_named_place(inner),
        _ => false,
    }
}

/// Reject an operand whose stage the composition's body cannot stand for. The body
/// evaluates each stage on every call, which is the composition's own behaviour for every
/// spelling but two: a bound method on a temporary would evaluate the temporary again,
/// and a `move` closure would be rebuilt with nothing left to own.
fn check_compose_operand(operand: &Expr) -> ParseResult<()> {
    match operand {
        Expr::Paren(inner, _) => check_compose_operand(inner),
        Expr::FieldAccess { object, .. } if !is_named_place(object) => {
            Err(ParseError::ComposeReceiverNotAPlace {
                span: object.span(),
            })
        }
        Expr::Closure {
            is_move: true,
            span,
            ..
        } => Err(ParseError::ComposeMoveClosure { span: *span }),
        _ => Ok(()),
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
            // call. Without an argument list it names the instance as a value.
            TokenKind::ColonColon => {
                let colons = self.advance().map(|t| t.span);
                let type_args = self.parse_turbofish_args()?;
                if !self.check(&TokenKind::LeftParen) {
                    let end = self
                        .tokens
                        .get(self.current.saturating_sub(1))
                        .map(|t| t.span)
                        .or(colons)
                        .unwrap_or_else(|| left.span());
                    let span = left.span().merge(end);
                    return Ok(Expr::Instance {
                        callee: Box::new(left),
                        type_args,
                        span,
                    });
                }
                self.advance(); // consume '('
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
            // struct field. (The lexer splits the float in chained `t.0.1` back into
            // two indices.)
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
                pipe_into(left, target)
            }

            // Composition `left >> right`, lexed as two adjacent `>`: see
            // `at_compose`. The right operand is parsed at `Compose` precedence, which
            // makes the operator left-associative the way `|>` is. The composition is the
            // closure `|p| right(left(p))`, and a composition operand is applied by
            // substitution, so `f >> g >> h` is one closure over the nested calls.
            TokenKind::Greater if self.at_compose() => {
                self.advance(); // consume the first '>'
                self.advance(); // consume the second '>'
                self.skip_newlines();
                let right = self.parse_expr(Precedence::Compose)?;
                let span = left.span().merge(right.span());
                check_compose_operand(&left)?;
                check_compose_operand(&right)?;
                let param = Identifier {
                    name: COMPOSE_PARAM.to_string(),
                    span,
                };
                let not_an_operand = |span| ParseError::NotAComposeOperand { span };
                let inner =
                    apply_stage(left, Expr::Identifier(param.clone())).map_err(not_an_operand)?;
                let body = apply_stage(right, inner).map_err(not_an_operand)?;
                Ok(Expr::Closure {
                    params: vec![ClosureParam {
                        name: param,
                        ty: None,
                        span,
                    }],
                    ret: None,
                    body: Box::new(body),
                    is_move: false,
                    span,
                })
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
                // A `??` fallback may leave the scope instead: `opt ?? return 0`.
                let right = if matches!(op, BinaryOp::NullCoalesce) {
                    self.parse_expr_or_exit(right_prec)?
                } else {
                    self.parse_expr(right_prec)?
                };
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
            TokenKind::EqualEqual
            | TokenKind::NotEqual
            | TokenKind::Less
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
