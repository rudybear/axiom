# Self-Hosting Bootstrap

This directory contains the first steps toward AXIOM self-hosting: a subset of the AXIOM lexer written in AXIOM itself.

## What is self-hosting?

A self-hosting compiler is one that can compile its own source code. This is a
significant milestone for any language because it proves the language is
expressive enough for systems-level programming. The journey from "hello world"
to "the compiler compiles itself" is long, but these examples are the first
concrete step.

## Current examples

### `lexer.axm` — Character classifier

A simplified lexer that classifies individual ASCII character codes into token
types. Given the expression `1 + 2 * 3`, it identifies each non-whitespace
character as a NUMBER, PLUS, STAR, etc.

Token type encoding:
- 0 = WHITESPACE (skip)
- 1 = NUMBER (digits 0-9)
- 2 = PLUS (+)
- 3 = MINUS (-)
- 4 = STAR (*)
- 5 = SLASH (/)
- 6 = UNKNOWN

Expected output:
```
1
2
1
4
1
```

### `token_counter.axm` — Token frequency counter

Extends the classifier to count how many tokens of each broad category (number
vs. operator) appear in the expression `1+2*3-4/5`.

Expected output:
```
5
4
```

## Running

```bash
# Emit LLVM IR (always works)
axiom compile --emit=llvm-ir examples/self_host/lexer.axm

# Compile and run (requires clang on PATH)
axiom compile examples/self_host/lexer.axm -o lexer
./lexer
```

## Roadmap to full self-hosting

1. **Character classification** (this milestone) — prove that AXIOM can express
   the core logic of a lexer: comparisons, branching, function calls.
2. **String iteration** — requires array/slice types and indexing support in
   codegen so we can iterate over actual input strings.
3. **Token stream** — requires struct types and dynamic arrays so we can build a
   `Vec<Token>`-equivalent in AXIOM.
4. **Recursive-descent parser** — requires the above plus recursive function
   calls (already supported).
5. **AST construction** — requires heap allocation and sum types.
6. **Full self-hosted lexer** — AXIOM lexer reimplemented in AXIOM, producing
   the same token stream as the Rust lexer.
7. **Full self-hosted compiler** — the ultimate goal.

## What this proves

Even at this early stage, these examples demonstrate that AXIOM can:
- Define and call pure functions
- Use `if`/`else` control flow with `and`/`or` logical operators
- Perform integer arithmetic and comparisons
- Mutate local variables
- Compile through the full pipeline: parse -> HIR -> LLVM IR -> native binary
