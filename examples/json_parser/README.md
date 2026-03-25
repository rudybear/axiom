# JSON Parser

A complete JSON parser written in AXIOM. Parses a hardcoded JSON string (encoded as an integer array of character codes) through two phases:

1. **Lexer** -- tokenizes the input into JSON tokens (braces, brackets, colons, commas, strings, numbers, booleans, null)
2. **Parser** -- recursive descent parser that validates the JSON structure and counts elements by type

## JSON Input

```json
{"name":"AXIOM","version":1,"features":["pure","arena","gpu"],"meta":{"stable":true,"count":42},"data":null}
```

## Features Used

- `@module`, `@intent`, `@pure` annotations
- Heap allocation (`heap_alloc`, `heap_free`)
- Pointer read/write (`ptr_read_i32`, `ptr_write_i32`)
- Recursive function calls (recursive descent parsing)
- Character-by-character string processing

## Expected Output

- 2 objects, 1 array, 3 string values, 2 numbers, 1 boolean, 1 null
- 10 total values

## Run

```bash
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/json_parser/json_parser.axm
```
