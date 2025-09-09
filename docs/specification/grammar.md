# NEURO Grammar Specification v1.0

This document defines the complete EBNF grammar for the NEURO programming language.

## Grammar Notation

- `::=` means "is defined as"
- `|` means "or"
- `[]` means optional
- `{}` means zero or more
- `()` means grouping
- `""` means literal strings

## Complete NEURO Grammar

```ebnf
Program ::= {Item}

Item ::= Function
       | Struct  
       | Enum
       | Impl
       | Use
       | Mod
       | Const

Function ::= [Attributes] "fn" Identifier "(" [Parameters] ")" ["->" Type] Block

Parameters ::= Parameter {"," Parameter} [","]
Parameter ::= [Attributes] ["mut"] Identifier ":" Type

Attributes ::= Attribute {Attribute}
Attribute ::= "#[" Identifier [AttributeArgs] "]"
AttributeArgs ::= "(" {AttributeArg} ")"
AttributeArg ::= Identifier ["=" Literal]

Block ::= "{" {Statement} [Expression] "}"

Statement ::= LetStatement
            | ExpressionStatement
            | ItemStatement

LetStatement ::= "let" ["mut"] Identifier [":" Type] ["=" Expression] ";"

ExpressionStatement ::= Expression ";"

Expression ::= AssignmentExpression

AssignmentExpression ::= ConditionalExpression [AssignmentOp ConditionalExpression]
AssignmentOp ::= "=" | "+=" | "-=" | "*=" | "/="

ConditionalExpression ::= LogicalOrExpression ["if" LogicalOrExpression "else" LogicalOrExpression]

LogicalOrExpression ::= LogicalAndExpression {"||" LogicalAndExpression}

LogicalAndExpression ::= EqualityExpression {"&&" EqualityExpression}

EqualityExpression ::= ComparisonExpression {("==" | "!=") ComparisonExpression}

ComparisonExpression ::= ArithmeticExpression {("<" | ">" | "<=" | ">=") ArithmeticExpression}

ArithmeticExpression ::= MultiplicativeExpression {("+" | "-") MultiplicativeExpression}

MultiplicativeExpression ::= UnaryExpression {("*" | "/" | "%") UnaryExpression}

UnaryExpression ::= ("!" | "-" | "+") UnaryExpression
                  | PostfixExpression

PostfixExpression ::= PrimaryExpression {PostfixOp}
PostfixOp ::= "[" Expression "]"
            | "(" [ArgumentList] ")"
            | "." Identifier

ArgumentList ::= Expression {"," Expression} [","]

PrimaryExpression ::= Identifier
                    | Literal
                    | "(" Expression ")"
                    | ArrayExpression
                    | TensorExpression
                    | IfExpression
                    | WhileExpression
                    | ForExpression
                    | MatchExpression
                    | BlockExpression

ArrayExpression ::= "[" [ExpressionList] "]"
ExpressionList ::= Expression {"," Expression} [","]

TensorExpression ::= "tensor!" "(" [TensorArgs] ")"
TensorArgs ::= Expression {"," Expression} [","]

IfExpression ::= "if" Expression Block ["else" (Block | IfExpression)]

WhileExpression ::= "while" Expression Block

ForExpression ::= "for" Identifier "in" Expression Block

MatchExpression ::= "match" Expression "{" {MatchArm} "}"
MatchArm ::= Pattern [MatchGuard] "=>" Expression [","]
MatchGuard ::= "if" Expression

Pattern ::= LiteralPattern
          | IdentifierPattern
          | WildcardPattern
          | StructPattern
          | TuplePattern
          | ArrayPattern

LiteralPattern ::= Literal
IdentifierPattern ::= Identifier
WildcardPattern ::= "_"
StructPattern ::= Identifier "{" {FieldPattern} "}"
FieldPattern ::= Identifier [":" Pattern]
TuplePattern ::= "(" [PatternList] ")"
ArrayPattern ::= "[" [PatternList] "]"
PatternList ::= Pattern {"," Pattern} [","]

Type ::= PrimitiveType
       | ArrayType
       | TensorType
       | TupleType
       | FunctionType
       | GenericType
       | ReferenceType

PrimitiveType ::= "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
                | "f32" | "f64" | "bool" | "char" | "str"

ArrayType ::= "[" Type ";" Expression "]"
            | "[" Type "]"

TensorType ::= "Tensor" "<" Type "," TensorShape ">"
TensorShape ::= "[" ShapeList "]"
ShapeList ::= ShapeElement {"," ShapeElement} [","]
ShapeElement ::= Expression | "_"

TupleType ::= "(" [TypeList] ")"
TypeList ::= Type {"," Type} [","]

FunctionType ::= "fn" "(" [TypeList] ")" ["->" Type]

GenericType ::= Identifier "<" TypeArguments ">"
TypeArguments ::= TypeArgument {"," TypeArgument} [","]
TypeArgument ::= Type | Expression

ReferenceType ::= "&" ["mut"] Type

Struct ::= [Attributes] "struct" Identifier [GenericParams] "{" {FieldDef} "}"
FieldDef ::= [Attributes] Identifier ":" Type [","]

Enum ::= [Attributes] "enum" Identifier [GenericParams] "{" {VariantDef} "}"
VariantDef ::= [Attributes] Identifier [VariantFields] [","]
VariantFields ::= "(" [TypeList] ")"
                | "{" {FieldDef} "}"

GenericParams ::= "<" GenericParam {"," GenericParam} [","] ">"
GenericParam ::= Identifier [GenericBounds]
GenericBounds ::= ":" GenericBound {"+" GenericBound}
GenericBound ::= Identifier

Use ::= "use" UsePath ";"
UsePath ::= Identifier {"::" Identifier}

Mod ::= "mod" Identifier (";" | "{" {Item} "}")

Const ::= "const" Identifier ":" Type "=" Expression ";"

Impl ::= "impl" [GenericParams] [Type "for"] Type "{" {ImplItem} "}"
ImplItem ::= Function | Const

Literal ::= IntegerLiteral
          | FloatLiteral
          | BooleanLiteral
          | StringLiteral
          | CharLiteral

IntegerLiteral ::= DecimalLiteral [IntegerSuffix]
                 | HexLiteral [IntegerSuffix]
                 | BinaryLiteral [IntegerSuffix]
                 | OctalLiteral [IntegerSuffix]

DecimalLiteral ::= DecimalDigit {DecimalDigit | "_"}
HexLiteral ::= "0x" HexDigit {HexDigit | "_"}
BinaryLiteral ::= "0b" BinaryDigit {BinaryDigit | "_"}
OctalLiteral ::= "0o" OctalDigit {OctalDigit | "_"}

IntegerSuffix ::= "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"

FloatLiteral ::= DecimalLiteral "." DecimalLiteral [ExponentPart] [FloatSuffix]
               | DecimalLiteral [ExponentPart] FloatSuffix
ExponentPart ::= ("e" | "E") ["+" | "-"] DecimalLiteral
FloatSuffix ::= "f32" | "f64"

BooleanLiteral ::= "true" | "false"

StringLiteral ::= "\"" {StringChar} "\""
StringChar ::= ~["\"" | "\\"] | "\\" EscapeSequence
EscapeSequence ::= "n" | "t" | "r" | "\\" | "\"" | "'" | "0"

CharLiteral ::= "'" CharChar "'"
CharChar ::= ~["'" | "\\"] | "\\" EscapeSequence

Identifier ::= IdentifierStart {IdentifierContinue}
IdentifierStart ::= Letter | "_"
IdentifierContinue ::= Letter | DecimalDigit | "_"

Letter ::= UnicodeXIDStart
DecimalDigit ::= "0".."9"
HexDigit ::= "0".."9" | "a".."f" | "A".."F"
BinaryDigit ::= "0" | "1"
OctalDigit ::= "0".."7"

BlockExpression ::= Block
```

## Keywords

Reserved keywords in NEURO:

```
fn, let, mut, const, struct, enum, impl, use, mod, pub
if, else, while, for, in, match, break, continue, return
true, false, self, Self
i8, i16, i32, i64, u8, u16, u32, u64, f32, f64, bool, char, str
tensor, Tensor
```

## Operators

### Arithmetic Operators
- `+` (addition)
- `-` (subtraction) 
- `*` (multiplication)
- `/` (division)
- `%` (modulo)

### Comparison Operators
- `==` (equal)
- `!=` (not equal)
- `<` (less than)
- `>` (greater than)
- `<=` (less than or equal)
- `>=` (greater than or equal)

### Logical Operators
- `&&` (logical AND)
- `||` (logical OR)
- `!` (logical NOT)

### Assignment Operators
- `=` (assignment)
- `+=` (add-assign)
- `-=` (subtract-assign)
- `*=` (multiply-assign)
- `/=` (divide-assign)

### Tensor Operators
- `@` (tensor multiplication/matmul)
- `—` (tensor product - Unicode U+2297)
- `` (function composition - Unicode U+2218)

## Comments

```ebnf
Comment ::= LineComment | BlockComment
LineComment ::= "//" {~NewLine} NewLine
BlockComment ::= "/*" {~"*/" | BlockComment} "*/"
```

## Attributes

Attributes modify the behavior of items:

- `#[grad]` - Enable automatic differentiation
- `#[kernel]` - Mark function for GPU compilation  
- `#[gpu]` - GPU-specific optimizations
- `#[inline]` - Inline function calls
- `#[test]` - Mark as test function
- `#[bench]` - Mark as benchmark function

## Whitespace and Tokens

Whitespace includes:
- Space (U+0020)
- Tab (U+0009) 
- Newline (U+000A)
- Carriage Return (U+000D)

Tokens are separated by whitespace or delimiters.

## Examples

### Function Definition
```neuro
#[grad]
fn neural_layer(input: Tensor<f32, [N, D]>, weights: Tensor<f32, [D, H]>) -> Tensor<f32, [N, H]> {
    input @ weights
}
```

### Pattern Matching
```neuro
match result {
    Ok(value) => process(value),
    Err(e) => {
        eprintln!("Error: {}", e);
        default_value()
    }
}
```

### Tensor Operations
```neuro
let a: Tensor<f32, [3, 4]> = tensor!([[1.0, 2.0, 3.0, 4.0],
                                      [5.0, 6.0, 7.0, 8.0], 
                                      [9.0, 10.0, 11.0, 12.0]]);
```

This grammar specification defines the complete syntax of NEURO v1.0, supporting all implemented features including tensor operations, automatic differentiation attributes, and pattern matching.