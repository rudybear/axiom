# Crypto Examples

Simple cryptographic algorithms implemented in AXIOM.

## Programs

### caesar.axm
Caesar cipher operating on an `array[i32, 100]` where each element represents
an ASCII character code. Shifts lowercase letters by a configurable amount,
wrapping around at 'z'. Demonstrates both encryption and decryption, verifying
that decrypt(encrypt(message)) = message.

The message "helloworld" is encoded as ASCII integers, encrypted with shift=3
(producing "khoorxruog"), and then decrypted back.

## Running

```bash
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/crypto/caesar.axm
```

## Language Features Demonstrated

- `array[i32, 100]` with element-wise operations
- Modular arithmetic (`%` operator)
- `@pure` annotation on cipher functions
- Conditional logic with `and` for range checks
- Array parameter passing
