# NEURO Language Grammar Specification

This document provides the complete formal grammar specification for the NEURO programming language, aligned with the current parser implementation in Phase 1.

## Lexical Elements

### Tokens

The NEURO lexer produces the following token types:

#### Literals
- **Integer**: `[0-9]+` (decimal integers)
- **Float**: `[0-9]+\.[0-9]+` (decimal floating-point)
- **String**: `"([^"\\]|\\.)*"` (double-quoted strings with escape sequences)
- **Boolean**: `true | false`

#### Identifiers
- **Identifier**: `[a-zA-Z_][a-zA-Z0-9_]*`
- Must not conflict with reserved keywords

#### Keywords
Reserved words that cannot be used as identifiers:
```
fn let mut return if else while break continue struct import
```

#### Type Keywords
- **Tensor**: Special keyword for tensor type declarations

#### Operators
- **Arithmetic**: `+ - * / %`
- **Comparison**: `== != < <= > >=`
- **Logical**: `&& || !`
- **Assignment**: `=`
- **Unary**: `- !`

#### Punctuation
- **Parentheses**: `( )`
- **Braces**: `{ }`
- **Brackets**: `[ ]`
- **Angle Brackets**: `< >` (for generics)
- **Semicolon**: `;`
- **Comma**: `,`
- **Colon**: `:`
- **Double Colon**: `::`
- **Arrow**: `->`

#### Special Tokens
- **Newline**: `\n` (preserved for formatting)
- **EndOfFile**: Marks end of input

### Comments

Comments are stripped during lexical analysis:
- **Line Comments**: `// [^\n]*`
- **Block Comments**: `/* ([^*]|\*[^/])* */`
- **Nested Block Comments**: Properly supported

### Whitespace

Whitespace characters (space, tab) are ignored except for:
- Separating tokens
- Newlines (preserved as tokens)

## Program Structure

### Top-Level Grammar

```ebnf
Program ::= Item* EOF

Item ::= Function
       | Struct
       | Import
```

### Function Declaration

```ebnf
Function ::= "fn" Identifier "(" ParameterList? ")" ReturnType? Block

ParameterList ::= Parameter ("," Parameter)*

Parameter ::= Identifier ":" Type

ReturnType ::= "->" Type

Block ::= "{" Statement* "}"
```

### Struct Declaration

```ebnf
Struct ::= "struct" Identifier "{" FieldList? "}"

FieldList ::= Field ("," Field)* ","?

Field ::= Identifier ":" Type
```

### Import Declaration

```ebnf
Import ::= "import" ImportPath ";"

ImportPath ::= ModulePath | StringLiteral

ModulePath ::= Identifier ("::" Identifier)*
```

## Type System

### Type Grammar

```ebnf
Type ::= PrimitiveType
       | TensorType
       | FunctionType
       | Identifier

PrimitiveType ::= "int" | "float" | "bool" | "string"

TensorType ::= "Tensor" "<" Type "," "[" DimensionList? "]" ">"

DimensionList ::= Dimension ("," Dimension)*

Dimension ::= Integer | "?"

FunctionType ::= "fn" "(" TypeList? ")" "->" Type

TypeList ::= Type ("," Type)*
```

### Type Examples

```neuro
// Primitive types
int
float
bool
string

// Tensor types
Tensor<float, [3]>
Tensor<int, [2, 3]>
Tensor<bool, [?, 5]>  // Dynamic dimension

// Function types
fn(int, float) -> bool
fn() -> int
fn(Tensor<float, [3]>) -> float
```

## Statements

### Statement Grammar

```ebnf
Statement ::= LetStatement
            | AssignmentStatement
            | ReturnStatement
            | IfStatement
            | WhileStatement
            | BreakStatement
            | ContinueStatement
            | BlockStatement
            | ExpressionStatement

LetStatement ::= "let" "mut"? Identifier (":" Type)? ("=" Expression)? ";"

AssignmentStatement ::= Identifier "=" Expression ";"

ReturnStatement ::= "return" Expression? ";"

IfStatement ::= "if" Expression Block ("else" Block)?

WhileStatement ::= "while" Expression Block

BreakStatement ::= "break" ";"

ContinueStatement ::= "continue" ";"

BlockStatement ::= Block

ExpressionStatement ::= Expression ";"
```

### Statement Examples

```neuro
// Variable declarations
let x: int = 42;
let mut counter = 0;
let name: string;

// Assignment
counter = counter + 1;

// Control flow
if x > 0 { return x; }
while i < n { i = i + 1; }
return result;
break;
continue;

// Expression statements
function_call();
calculate_sum(a, b);
```

## Expressions

### Expression Grammar

```ebnf
Expression ::= LogicalOrExpression

LogicalOrExpression ::= LogicalAndExpression ("||" LogicalAndExpression)*

LogicalAndExpression ::= EqualityExpression ("&&" EqualityExpression)*

EqualityExpression ::= ComparisonExpression (("==" | "!=") ComparisonExpression)*

ComparisonExpression ::= AdditiveExpression (("<" | "<=" | ">" | ">=") AdditiveExpression)*

AdditiveExpression ::= MultiplicativeExpression (("+" | "-") MultiplicativeExpression)*

MultiplicativeExpression ::= UnaryExpression (("*" | "/" | "%") UnaryExpression)*

UnaryExpression ::= ("-" | "!")? CallExpression

CallExpression ::= PrimaryExpression ("(" ArgumentList? ")")*

PrimaryExpression ::= Literal
                    | Identifier
                    | "(" Expression ")"

ArgumentList ::= Expression ("," Expression)*

Literal ::= Integer | Float | String | Boolean
```

### Operator Precedence

From highest to lowest precedence:

1. **Primary**: Literals, identifiers, parenthesized expressions
2. **Call**: Function calls `f(args)`
3. **Unary**: `-expr`, `!expr`
4. **Multiplicative**: `*`, `/`, `%`
5. **Additive**: `+`, `-`
6. **Comparison**: `<`, `<=`, `>`, `>=`
7. **Equality**: `==`, `!=`
8. **Logical AND**: `&&`
9. **Logical OR**: `||`

### Expression Examples

```neuro
// Primary expressions
42
3.14
"hello"
true
identifier
(x + y)

// Function calls
calculate()
process(a, b, c)
nested(outer(inner()))

// Unary expressions
-x
!flag
-calculate()

// Binary expressions
a + b * c       // Equivalent to: a + (b * c)
x < y && y < z  // Equivalent to: (x < y) && (y < z)
(a + b) * c     // Parentheses override precedence
```

## Complete Grammar Example

```neuro
// Function with all supported features
fn fibonacci(n: int) -> int {
    // Variable declarations with type inference
    let mut a = 0;
    let mut b = 1;
    let mut i = 0;

    // While loop with complex condition
    while i < n && b >= 0 {
        // Conditional logic
        if i == 0 {
            i = i + 1;
            continue;
        }

        // Arithmetic and assignment
        let temp = a + b;
        a = b;
        b = temp;
        i = i + 1;

        // Early return
        if b > 1000000 {
            break;
        }
    }

    return a;
}

// Struct with multiple field types
struct DataPoint {
    id: int,
    value: float,
    label: string,
    active: bool,
    coordinates: Tensor<float, [3]>,
}

// Import statements
import std::math;
import "./utilities.nr";

// Main function
fn main() -> int {
    let result = fibonacci(10);
    return result;
}
```

## Current Implementation Status

### Fully Implemented ✅
- Complete lexical analysis for all token types
- Full parsing of function, struct, and import declarations
- All statement types with proper precedence
- Complete expression parsing with operator precedence
- Type parsing for primitives, tensors, and function types
- Block scoping and nested constructs

### Parsed but Limited Semantic Analysis ⚠️
- Generic type parameters (parsed as identifiers)
- Complex tensor operations
- Import resolution and symbol lookup
- Struct instantiation and field access

### Not Yet Implemented ❌
- **For loops**: `for item in collection` (tokenized but not parsed)
- **Attributes**: `#[grad]`, `#[kernel]`, `#[gpu]` (frameworks exist but not parsed)
- **Member access**: `object.field` (AST nodes exist but parser doesn't generate them)
- **Index expressions**: `array[index]` (AST nodes exist but parser doesn't generate them)
- **Pattern matching**: `match` expressions
- **Enums**: Algebraic data types
- **Lambda expressions**: Anonymous functions

### Future Grammar Extensions

Planned additions for Phase 2:

```ebnf
// For loops (planned)
ForStatement ::= "for" Identifier "in" Expression Block

// Attributes (planned)
AttributeList ::= Attribute+
Attribute ::= "#" "[" Identifier ("(" ArgumentList? ")")? "]"

// Member access (planned)
MemberExpression ::= Expression "." Identifier

// Index access (planned)
IndexExpression ::= Expression "[" Expression "]"

// Match expressions (planned)
MatchExpression ::= "match" Expression "{" MatchArm* "}"
MatchArm ::= Pattern "=>" Expression ","
```

## Grammar Notes

1. **Semicolon Rules**: All statements except blocks require semicolons
2. **Precedence**: Left-to-right associativity for same-precedence operators
3. **Comments**: Completely removed during lexical analysis
4. **Whitespace**: Ignored except for token separation and newlines
5. **Error Recovery**: Parser includes error recovery mechanisms
6. **Extensibility**: Grammar designed for easy extension in future phases

