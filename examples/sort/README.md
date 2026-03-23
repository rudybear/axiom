# Sorting Algorithm Examples

This directory contains three classic sorting algorithms implemented in AXIOM,
each operating on `array[i32, 100]`.

## Programs

### bubble_sort.axm
Bubble sort — repeatedly swaps adjacent out-of-order elements until the array
is sorted. O(n^2) worst-case. Simple but inefficient for large inputs.

### insertion_sort.axm
Insertion sort — builds the sorted array one element at a time by inserting
each new element into its correct position. O(n^2) worst-case, but efficient
on nearly-sorted data.

### selection_sort.axm
Selection sort — repeatedly selects the minimum from the unsorted region and
moves it to the sorted region. O(n^2) in all cases. Minimal swaps compared
to bubble sort.

## Running

```bash
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/sort/bubble_sort.axm
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/sort/insertion_sort.axm
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/sort/selection_sort.axm
```

## Language Features Demonstrated

- `array[i32, 100]` fixed-size arrays with `array_zeros` initialization
- `for ... in range()` loops
- `while` loops (insertion sort)
- Nested conditionals and swaps
- `@pure`, `@complexity`, `@intent`, `@module` annotations
- `print_i32` for output
