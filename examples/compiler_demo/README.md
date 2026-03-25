# Tiny Language Interpreter

A complete interpreter for a tiny language, written entirely in AXIOM. Demonstrates that AXIOM is expressive enough to implement a full lexer, parser, and tree-walking evaluator.

## Language Features

The interpreted language supports:
- `let x = EXPR;` -- variable bindings
- `print EXPR;` -- print an expression value
- Expressions: integer literals, variable references, `+`, `-`, `*`, `/` with correct precedence

## Example Program

```
let x = 5;
let y = x + 3;
let z = y * 2 - 1;
print x;       // -> 5
print y;       // -> 8
print z;       // -> 15
let w = z / 3 + x;
print w;       // -> 10
```

## Architecture

1. **Lexer** -- Tokenizes the character stream into tokens (identifiers, numbers, operators, keywords)
2. **Parser** -- Recursive descent parser that builds an AST stored in flat arrays. Handles operator precedence (multiplicative before additive).
3. **Evaluator** -- Tree-walking interpreter that maintains a variable environment and evaluates expressions recursively.

## Features Used

- `@module`, `@intent`, `@pure` annotations
- Heap allocation for all data structures
- Recursive descent parsing with operator precedence
- Recursive expression evaluation
- Character-by-character keyword matching

## Run

```bash
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/compiler_demo/tiny_lang.axm
```
