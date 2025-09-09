//! NEURO Parser Implementation
//! 
//! Recursive descent parser that converts tokens into an Abstract Syntax Tree.
//! Handles expressions, statements, and top-level items with error recovery.

use crate::error::ParseError;
use shared_types::{
    Token, TokenType, Keyword, Span,
    Program, Item, Function, Parameter, Block, Statement, Expression, 
    ast::{Literal, Identifier, BinaryExpression, BinaryOperator, 
         UnaryExpression, UnaryOperator, CallExpression,
         LetStatement, AssignmentStatement, ReturnStatement, IfStatement, WhileStatement,
         BreakStatement, ContinueStatement},
    Type,
};

/// Parser for NEURO source code
pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    /// Create a new parser with the given tokens
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            current: 0,
        }
    }

    /// Parse the tokens into a complete program
    pub fn parse(&mut self) -> Result<Program, ParseError> {
        let start_span = if self.is_at_end() {
            Span::new(0, 0)
        } else {
            self.current_span()
        };
        let mut items = Vec::new();

        while !self.is_at_end() {
            if self.match_token(&TokenType::Newline) {
                continue;
            }
            items.push(self.parse_item()?);
        }

        let end_span = if items.is_empty() {
            start_span
        } else {
            self.previous_span()
        };
        let span = Span::new(start_span.start, end_span.end);

        Ok(Program { items, span })
    }

    /// Parse a single expression (for eval command)
    pub fn parse_expression_only(&mut self) -> Result<Expression, ParseError> {
        // Skip any leading newlines
        while self.match_token(&TokenType::Newline) {
            // continue
        }
        
        let expr = self.parse_expression()?;
        
        // Consume optional semicolon
        self.match_token(&TokenType::Semicolon);
        
        // Skip any trailing newlines
        while self.match_token(&TokenType::Newline) {
            // continue
        }
        
        // Should be at end of tokens now
        if !self.is_at_end() {
            let span = self.current_span();
            return Err(ParseError::invalid_statement(
                "Expected end of expression",
                span,
            ));
        }
        
        Ok(expr)
    }

    /// Parse a top-level item
    fn parse_item(&mut self) -> Result<Item, ParseError> {
        match self.peek().token_type {
            TokenType::Keyword(Keyword::Fn) => Ok(Item::Function(self.parse_function()?)),
            TokenType::Keyword(Keyword::Struct) => Ok(Item::Struct(self.parse_struct()?)),
            TokenType::Keyword(Keyword::Import) => Ok(Item::Import(self.parse_import()?)),
            // Add more item types here as needed
            _ => {
                let span = self.current_span();
                Err(ParseError::invalid_statement(
                    "Expected function, struct, or import declaration",
                    span,
                ))
            }
        }
    }

    /// Parse a function definition
    fn parse_function(&mut self) -> Result<Function, ParseError> {
        let start_span = self.advance().span; // consume 'fn'
        
        let name = match &self.advance().token_type {
            TokenType::Identifier(name) => name.clone(),
            other => return Err(ParseError::unexpected_token(
                other.clone(),
                vec![TokenType::Identifier("function_name".to_string())],
                self.previous_span(),
            )),
        };

        self.consume(TokenType::LeftParen, "Expected '(' after function name")?;

        let mut parameters = Vec::new();
        if !self.check(&TokenType::RightParen) {
            loop {
                parameters.push(self.parse_parameter()?);
                if !self.match_token(&TokenType::Comma) {
                    break;
                }
            }
        }

        self.consume(TokenType::RightParen, "Expected ')' after parameters")?;

        let return_type = if self.match_token(&TokenType::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end);

        Ok(Function {
            name,
            parameters,
            return_type,
            body,
            span,
        })
    }

    /// Parse a function parameter
    fn parse_parameter(&mut self) -> Result<Parameter, ParseError> {
        let start_span = self.current_span();
        
        let name = match &self.advance().token_type {
            TokenType::Identifier(name) => name.clone(),
            other => return Err(ParseError::unexpected_token(
                other.clone(),
                vec![TokenType::Identifier("parameter_name".to_string())],
                self.previous_span(),
            )),
        };

        // Parse optional type annotation: param_name: type
        let param_type = if self.match_token(&TokenType::Colon) {
            self.parse_type()?
        } else {
            Type::Unknown
        };

        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end);

        Ok(Parameter {
            name,
            param_type,
            span,
        })
    }

    /// Parse a struct definition
    fn parse_struct(&mut self) -> Result<shared_types::Struct, ParseError> {
        let start_span = self.advance().span; // consume 'struct'
        
        let name = match &self.advance().token_type {
            TokenType::Identifier(name) => name.clone(),
            other => return Err(ParseError::unexpected_token(
                other.clone(),
                vec![TokenType::Identifier("struct_name".to_string())],
                self.previous_span(),
            )),
        };

        self.consume(TokenType::LeftBrace, "Expected '{' after struct name")?;
        
        let mut fields = Vec::new();
        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Newline) {
                continue;
            }
            fields.push(self.parse_struct_field()?);
        }

        let end = self.consume(TokenType::RightBrace, "Expected '}'")?;
        let span = Span::new(start_span.start, end.end);

        Ok(shared_types::Struct { name, fields, span })
    }

    /// Parse a struct field
    fn parse_struct_field(&mut self) -> Result<shared_types::StructField, ParseError> {
        let start_span = self.current_span();
        
        let name = match &self.advance().token_type {
            TokenType::Identifier(name) => name.clone(),
            other => return Err(ParseError::unexpected_token(
                other.clone(),
                vec![TokenType::Identifier("field_name".to_string())],
                self.previous_span(),
            )),
        };

        self.consume(TokenType::Colon, "Expected ':' after field name")?;
        let field_type = self.parse_type()?;
        self.consume(TokenType::Comma, "Expected ',' after field type")?;
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end);

        Ok(shared_types::StructField {
            name,
            field_type,
            span,
        })
    }

    /// Parse an import statement
    fn parse_import(&mut self) -> Result<shared_types::Import, ParseError> {
        let start_span = self.advance().span; // consume 'import'
        
        // Parse module path which can be: identifier::identifier::...
        let mut path_parts = Vec::new();
        
        // Get first identifier or string
        let first_token = self.advance();
        let path = match &first_token.token_type {
            TokenType::Identifier(name) => {
                path_parts.push(name.clone());
                
                // Handle namespace path like std::math
                while self.match_token(&TokenType::DoubleColon) {
                    let next_token = self.advance();
                    match &next_token.token_type {
                        TokenType::Identifier(name) => path_parts.push(name.clone()),
                        other => return Err(ParseError::unexpected_token(
                            other.clone(),
                            vec![TokenType::Identifier("module_name".to_string())],
                            next_token.span,
                        )),
                    };
                }
                
                path_parts.join("::")
            },
            TokenType::String(path) => path.clone(),
            other => return Err(ParseError::unexpected_token(
                other.clone(),
                vec![TokenType::Identifier("module_name".to_string())],
                first_token.span,
            )),
        };

        self.consume(TokenType::Semicolon, "Expected ';' after import")?;
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end);

        Ok(shared_types::Import { path, span })
    }

    /// Parse a block of statements
    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let start = self.consume(TokenType::LeftBrace, "Expected '{'")?;
        let mut statements = Vec::new();

        while !self.check(&TokenType::RightBrace) && !self.is_at_end() {
            if self.match_token(&TokenType::Newline) {
                continue;
            }
            statements.push(self.parse_statement()?);
        }

        let end = self.consume(TokenType::RightBrace, "Expected '}'")?;
        let span = Span::new(start.start, end.end);

        Ok(Block { statements, span })
    }

    /// Parse a statement
    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match &self.peek().token_type {
            TokenType::Keyword(Keyword::Let) => Ok(Statement::Let(self.parse_let_statement()?)),
            TokenType::Keyword(Keyword::Return) => Ok(Statement::Return(self.parse_return_statement()?)),
            TokenType::Keyword(Keyword::If) => Ok(Statement::If(self.parse_if_statement()?)),
            TokenType::Keyword(Keyword::While) => Ok(Statement::While(self.parse_while_statement()?)),
            TokenType::Keyword(Keyword::Break) => Ok(Statement::Break(self.parse_break_statement()?)),
            TokenType::Keyword(Keyword::Continue) => Ok(Statement::Continue(self.parse_continue_statement()?)),
            TokenType::LeftBrace => Ok(Statement::Block(self.parse_block()?)),
            TokenType::Identifier(_) => {
                // Look ahead to see if this is an assignment or expression
                if self.peek_next().token_type == TokenType::Assign {
                    Ok(Statement::Assignment(self.parse_assignment_statement()?))
                } else {
                    let expr = self.parse_expression()?;
                    self.consume(TokenType::Semicolon, "Expected ';' after expression")?;
                    Ok(Statement::Expression(expr))
                }
            }
            _ => {
                let expr = self.parse_expression()?;
                self.consume(TokenType::Semicolon, "Expected ';' after expression")?;
                Ok(Statement::Expression(expr))
            }
        }
    }

    /// Parse a let statement
    fn parse_let_statement(&mut self) -> Result<LetStatement, ParseError> {
        let start_span = self.advance().span; // consume 'let'
        
        let mutable = self.match_token(&TokenType::Keyword(Keyword::Mut));
        
        let name = match &self.advance().token_type {
            TokenType::Identifier(name) => name.clone(),
            other => return Err(ParseError::unexpected_token(
                other.clone(),
                vec![TokenType::Identifier("variable_name".to_string())],
                self.previous_span(),
            )),
        };

        let var_type = None; // TODO: Implement type annotation parsing
        
        let initializer = if self.match_token(&TokenType::Assign) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        self.consume(TokenType::Semicolon, "Expected ';' after let statement")?;
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end);

        Ok(LetStatement {
            name,
            var_type,
            initializer,
            mutable,
            span,
        })
    }

    /// Parse an assignment statement
    fn parse_assignment_statement(&mut self) -> Result<shared_types::AssignmentStatement, ParseError> {
        let start_span = self.current_span();
        
        let target = match &self.advance().token_type {
            TokenType::Identifier(name) => name.clone(),
            other => return Err(ParseError::unexpected_token(
                other.clone(),
                vec![TokenType::Identifier("variable_name".to_string())],
                self.previous_span(),
            )),
        };

        self.consume(TokenType::Assign, "Expected '=' in assignment")?;
        let value = self.parse_expression()?;
        self.consume(TokenType::Semicolon, "Expected ';' after assignment")?;
        
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end);

        Ok(shared_types::AssignmentStatement {
            target,
            value,
            span,
        })
    }

    /// Parse a return statement
    fn parse_return_statement(&mut self) -> Result<ReturnStatement, ParseError> {
        let start_span = self.advance().span; // consume 'return'
        
        let value = if self.check(&TokenType::Semicolon) {
            None
        } else {
            Some(self.parse_expression()?)
        };

        self.consume(TokenType::Semicolon, "Expected ';' after return statement")?;
        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end);

        Ok(ReturnStatement { value, span })
    }

    /// Parse an if statement
    fn parse_if_statement(&mut self) -> Result<IfStatement, ParseError> {
        let start_span = self.advance().span; // consume 'if'
        
        let condition = self.parse_expression()?;
        let then_branch = self.parse_block()?;
        
        let else_branch = if self.match_token(&TokenType::Keyword(Keyword::Else)) {
            Some(self.parse_block()?)
        } else {
            None
        };

        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end);

        Ok(IfStatement {
            condition,
            then_branch,
            else_branch,
            span,
        })
    }

    /// Parse a while statement
    fn parse_while_statement(&mut self) -> Result<WhileStatement, ParseError> {
        let start_span = self.advance().span; // consume 'while'
        
        let condition = self.parse_expression()?;
        let body = self.parse_block()?;

        let end_span = self.previous_span();
        let span = Span::new(start_span.start, end_span.end);

        Ok(WhileStatement {
            condition,
            body,
            span,
        })
    }

    /// Parse a break statement
    fn parse_break_statement(&mut self) -> Result<BreakStatement, ParseError> {
        let span = self.advance().span; // consume 'break'
        self.consume(TokenType::Semicolon, "Expected ';' after break statement")?;
        Ok(BreakStatement { span })
    }

    /// Parse a continue statement
    fn parse_continue_statement(&mut self) -> Result<ContinueStatement, ParseError> {
        let span = self.advance().span; // consume 'continue'
        self.consume(TokenType::Semicolon, "Expected ';' after continue statement")?;
        Ok(ContinueStatement { span })
    }

    /// Parse an expression
    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_logical_or()
    }

    /// Parse logical OR expressions (||)
    fn parse_logical_or(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_logical_and()?;

        while let Some(op) = self.match_binary_op(&[
            (TokenType::LogicalOr, BinaryOperator::LogicalOr),
        ]) {
            let operator = op;
            let right = self.parse_logical_and()?;
            let span = Span::new(expr.span().start, right.span().end);
            expr = Expression::Binary(BinaryExpression {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
                span,
            });
        }

        Ok(expr)
    }

    /// Parse logical AND expressions (&&)
    fn parse_logical_and(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_equality()?;

        while let Some(op) = self.match_binary_op(&[
            (TokenType::LogicalAnd, BinaryOperator::LogicalAnd),
        ]) {
            let operator = op;
            let right = self.parse_equality()?;
            let span = Span::new(expr.span().start, right.span().end);
            expr = Expression::Binary(BinaryExpression {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
                span,
            });
        }

        Ok(expr)
    }

    /// Parse equality expressions (== !=)
    fn parse_equality(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_comparison()?;

        while let Some(op) = self.match_binary_op(&[
            (TokenType::Equal, BinaryOperator::Equal),
            (TokenType::NotEqual, BinaryOperator::NotEqual),
        ]) {
            let operator = op;
            let right = self.parse_comparison()?;
            let span = Span::new(expr.span().start, right.span().end);
            expr = Expression::Binary(BinaryExpression {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
                span,
            });
        }

        Ok(expr)
    }

    /// Parse comparison expressions (< <= > >=)
    fn parse_comparison(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_term()?;

        while let Some(op) = self.match_binary_op(&[
            (TokenType::Greater, BinaryOperator::Greater),
            (TokenType::GreaterEqual, BinaryOperator::GreaterEqual),
            (TokenType::Less, BinaryOperator::Less),
            (TokenType::LessEqual, BinaryOperator::LessEqual),
        ]) {
            let operator = op;
            let right = self.parse_term()?;
            let span = Span::new(expr.span().start, right.span().end);
            expr = Expression::Binary(BinaryExpression {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
                span,
            });
        }

        Ok(expr)
    }

    /// Parse term expressions (+ -)
    fn parse_term(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_factor()?;

        while let Some(op) = self.match_binary_op(&[
            (TokenType::Plus, BinaryOperator::Add),
            (TokenType::Minus, BinaryOperator::Subtract),
        ]) {
            let operator = op;
            let right = self.parse_factor()?;
            let span = Span::new(expr.span().start, right.span().end);
            expr = Expression::Binary(BinaryExpression {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
                span,
            });
        }

        Ok(expr)
    }

    /// Parse factor expressions (* / %)
    fn parse_factor(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_unary()?;

        while let Some(op) = self.match_binary_op(&[
            (TokenType::Star, BinaryOperator::Multiply),
            (TokenType::Slash, BinaryOperator::Divide),
            (TokenType::Percent, BinaryOperator::Modulo),
        ]) {
            let operator = op;
            let right = self.parse_unary()?;
            let span = Span::new(expr.span().start, right.span().end);
            expr = Expression::Binary(BinaryExpression {
                left: Box::new(expr),
                operator,
                right: Box::new(right),
                span,
            });
        }

        Ok(expr)
    }

    /// Parse unary expressions (- !)
    fn parse_unary(&mut self) -> Result<Expression, ParseError> {
        if let Some(op) = self.match_unary_op(&[
            (TokenType::Minus, UnaryOperator::Negate),
            (TokenType::LogicalNot, UnaryOperator::Not),
        ]) {
            let start_span = self.previous_span();
            let operand = self.parse_unary()?;
            let span = Span::new(start_span.start, operand.span().end);
            Ok(Expression::Unary(UnaryExpression {
                operator: op,
                operand: Box::new(operand),
                span,
            }))
        } else {
            self.parse_call()
        }
    }

    /// Parse call expressions
    fn parse_call(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek().token_type {
                TokenType::LeftParen => {
                    self.advance(); // consume '('
                    let mut arguments = Vec::new();
                    
                    if !self.check(&TokenType::RightParen) {
                        loop {
                            arguments.push(self.parse_expression()?);
                            if !self.match_token(&TokenType::Comma) {
                                break;
                            }
                        }
                    }
                    
                    let end = self.consume(TokenType::RightParen, "Expected ')' after arguments")?;
                    let span = Span::new(expr.span().start, end.end);
                    expr = Expression::Call(CallExpression {
                        function: Box::new(expr),
                        arguments,
                        span,
                    });
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    /// Parse primary expressions (literals, identifiers, parenthesized expressions)
    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        let token = self.advance();
        
        match &token.token_type {
            TokenType::Integer(value) => {
                let num: i64 = value.parse().map_err(|_| {
                    ParseError::InvalidNumber {
                        literal: value.clone(),
                        span: token.span,
                    }
                })?;
                Ok(Expression::Literal(Literal::Integer(num, token.span)))
            }
            TokenType::Float(value) => {
                let num: f64 = value.parse().map_err(|_| {
                    ParseError::InvalidNumber {
                        literal: value.clone(),
                        span: token.span,
                    }
                })?;
                Ok(Expression::Literal(Literal::Float(num, token.span)))
            }
            TokenType::String(value) => {
                Ok(Expression::Literal(Literal::String(value.clone(), token.span)))
            }
            TokenType::Boolean(value) => {
                Ok(Expression::Literal(Literal::Boolean(*value, token.span)))
            }
            TokenType::Identifier(name) => {
                Ok(Expression::Identifier(Identifier {
                    name: name.clone(),
                    span: token.span,
                }))
            }
            TokenType::LeftParen => {
                let expr = self.parse_expression()?;
                self.consume(TokenType::RightParen, "Expected ')' after expression")?;
                Ok(expr)
            }
            _ => Err(ParseError::invalid_expression(
                "Expected literal, identifier, or '('",
                token.span,
            )),
        }
    }

    /// Parse a type annotation
    fn parse_type(&mut self) -> Result<Type, ParseError> {
        // For now, just parse basic types
        match &self.advance().token_type {
            TokenType::Identifier(name) => match name.as_str() {
                "int" => Ok(Type::Int),
                "float" => Ok(Type::Float),
                "bool" => Ok(Type::Bool),
                "string" => Ok(Type::String),
                _ => Ok(Type::Generic(name.clone())),
            },
            other => Err(ParseError::invalid_statement(
                format!("Expected type, found {:?}", other),
                self.previous_span(),
            )),
        }
    }

    // Helper methods

    fn match_binary_op(&mut self, ops: &[(TokenType, BinaryOperator)]) -> Option<BinaryOperator> {
        for (token_type, op) in ops {
            if self.check(token_type) {
                self.advance();
                return Some(*op);
            }
        }
        None
    }

    fn match_unary_op(&mut self, ops: &[(TokenType, UnaryOperator)]) -> Option<UnaryOperator> {
        for (token_type, op) in ops {
            if self.check(token_type) {
                self.advance();
                return Some(*op);
            }
        }
        None
    }

    fn match_token(&mut self, token_type: &TokenType) -> bool {
        if self.check(token_type) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check(&self, token_type: &TokenType) -> bool {
        if self.is_at_end() {
            false
        } else {
            std::mem::discriminant(&self.peek().token_type) == std::mem::discriminant(token_type)
        }
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek().token_type, TokenType::EndOfFile)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }
    
    fn peek_next(&self) -> &Token {
        if self.current + 1 >= self.tokens.len() {
            &self.tokens[self.tokens.len() - 1] // Return EOF token
        } else {
            &self.tokens[self.current + 1]
        }
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    fn current_span(&self) -> Span {
        self.peek().span
    }

    fn previous_span(&self) -> Span {
        self.previous().span
    }

    fn consume(&mut self, token_type: TokenType, _message: &str) -> Result<Span, ParseError> {
        if self.check(&token_type) {
            Ok(self.advance().span)
        } else {
            Err(ParseError::unexpected_token(
                self.peek().token_type.clone(),
                vec![token_type],
                self.current_span(),
            ))
        }
    }
}