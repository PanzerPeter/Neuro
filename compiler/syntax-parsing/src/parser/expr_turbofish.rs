//! Explicit type arguments: `::<...>` after a name or a path, and the
//! `Tensor::<T, [...]>::` construction helpers they open.

use ast_types::{Expr, GenericArg};
use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::errors::{ParseError, ParseResult};

use super::Parser;

impl Parser {
    /// Whether the current `::` is immediately followed by `<`, opening a turbofish
    /// `::<...>` rather than a path member `::name`.
    pub(super) fn colon_colon_opens_turbofish(&self) -> bool {
        matches!(
            self.tokens.get(self.current + 1).map(|t| &t.kind),
            Some(TokenKind::Less)
        )
    }

    /// Parse `Tensor::<T, [d0, ...]>::ctor(args)`: the tensor constructor spelling, whose
    /// turbofish qualifies the *type* rather than the callee.
    ///
    /// The result is an ordinary `Call` on a `Path` whose single type argument is the
    /// assembled `Type::Tensor`. Nothing downstream needs a node of its own: the tensor
    /// type is exactly what a turbofish already carries, and the associated-call arm of
    /// the type checker is already where `Tensor::scalar(v)` (the same constructors
    /// spelled without a turbofish) has to be resolved anyway.
    pub(super) fn parse_tensor_qualified_call(
        &mut self,
        type_name: Identifier,
    ) -> ParseResult<Expr> {
        self.advance(); // consume '::'
        let (args, shape, close_span) = self.parse_generic_type_args(true)?;
        let type_span = type_name.span.merge(close_span);
        let Some((dims, shape_span)) = shape else {
            return Err(ParseError::TensorTypeArity { span: type_span });
        };
        let tensor_type =
            Self::build_tensor_type(type_name.clone(), args, dims, shape_span, type_span)?;

        self.consume(
            TokenKind::ColonColon,
            "'::' and a constructor name after `Tensor::<...>`",
        )?;
        let member_token = self.consume(
            TokenKind::Identifier(String::new()),
            "a tensor constructor name",
        )?;
        let TokenKind::Identifier(member_name) = member_token.kind else {
            return Err(ParseError::UnexpectedToken {
                found: member_token.kind,
                expected: "a tensor constructor name".to_string(),
                span: member_token.span,
            });
        };
        let member = Identifier {
            name: member_name,
            span: member_token.span,
        };

        self.consume(TokenKind::LeftParen, "'(' after a tensor constructor name")?;
        let (call_args, arg_labels) = self.parse_call_arguments()?;
        let close = self.consume(TokenKind::RightParen, "')'")?;
        let span = type_name.span.merge(close.span);
        let path_span = type_span.merge(member.span);

        Ok(Expr::Call {
            func: Box::new(Expr::Path {
                type_name,
                member,
                span: path_span,
            }),
            type_args: vec![GenericArg::Type(tensor_type)],
            args: call_args,
            arg_labels,
            span,
        })
    }

    /// Parse turbofish generic arguments `<T, N, ...>`, positioned just after the
    /// `::`. Each argument is a type or a non-negative integer const value.
    pub(super) fn parse_turbofish_args(&mut self) -> ParseResult<Vec<GenericArg>> {
        self.consume(TokenKind::Less, "'<' after '::' in a turbofish")?;
        self.skip_newlines();
        let mut args = Vec::new();
        loop {
            if let Some(TokenKind::Integer(n)) = self.peek_kind() {
                let value = *n;
                let span = self
                    .advance()
                    .map(|t| t.span)
                    .ok_or(ParseError::UnexpectedEof {
                        expected: "const argument".to_string(),
                    })?;
                // An integer token carries a magnitude, so a negative const argument
                // is a `-` token followed by one and is rejected as an unexpected token
                // before reaching here.
                args.push(GenericArg::Const {
                    value: value as i128,
                    span,
                });
            } else {
                args.push(GenericArg::Type(self.parse_type()?));
            }
            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume ','
            self.skip_newlines();
        }
        self.consume(TokenKind::Greater, "'>' to close turbofish arguments")?;
        Ok(args)
    }
}
