/*
 * axiom_rt_strings.c -- String (fat pointer) runtime.
 *
 * Provides: axiom_string_from_literal, axiom_string_len, axiom_string_ptr,
 *           axiom_string_eq, axiom_string_print
 *
 * Included by axiom_rt.c -- do not compile separately.
 */

/* ── String (Fat Pointer) ────────────────────────────────────────── */
/*
 * Strings are packed into an i64 as a fat pointer:
 *   - Upper 32 bits: length (i32)
 *   - Lower 32 bits: pointer (truncated to 32 bits on 32-bit, or index on 64-bit)
 *
 * Actually, on 64-bit systems we cannot pack a 64-bit pointer into 32 bits.
 * Instead we use a different strategy: store strings in a table and return
 * an index packed with the length.  But for simplicity and the common case
 * (string literals whose pointers are known), we use a small string table.
 *
 * Encoding: (len << 32) | table_index
 *
 * API:
 *   axiom_string_from_literal(ptr) -> i64 (packed len + index)
 *   axiom_string_len(s)            -> i32
 *   axiom_string_ptr(s)            -> ptr
 *   axiom_string_eq(a, b)          -> i32 (1 if equal, 0 otherwise)
 *   axiom_string_print(s)          -> void (prints to stdout)
 */

#define AXIOM_STRING_TABLE_MAX 4096

static const char *axiom_string_table[AXIOM_STRING_TABLE_MAX];
static int axiom_string_table_len_arr[AXIOM_STRING_TABLE_MAX];
static int axiom_string_table_count = 0;

long long axiom_string_from_literal(const char *lit) {
    int idx = axiom_string_table_count;
    if (idx >= AXIOM_STRING_TABLE_MAX) {
        fprintf(stderr, "axiom_string_from_literal: string table full\n");
        abort();
    }
    int len = (int)strlen(lit);
    axiom_string_table[idx] = lit;
    axiom_string_table_len_arr[idx] = len;
    axiom_string_table_count++;
    return ((long long)len << 32) | (long long)(unsigned int)idx;
}

int axiom_string_len(long long s) {
    return (int)(s >> 32);
}

const char *axiom_string_ptr(long long s) {
    int idx = (int)(s & 0xFFFFFFFF);
    if (idx < 0 || idx >= axiom_string_table_count) return "";
    return axiom_string_table[idx];
}

int axiom_string_eq(long long a, long long b) {
    int len_a = (int)(a >> 32);
    int len_b = (int)(b >> 32);
    if (len_a != len_b) return 0;
    int idx_a = (int)(a & 0xFFFFFFFF);
    int idx_b = (int)(b & 0xFFFFFFFF);
    if (idx_a < 0 || idx_a >= axiom_string_table_count) return 0;
    if (idx_b < 0 || idx_b >= axiom_string_table_count) return 0;
    return memcmp(axiom_string_table[idx_a], axiom_string_table[idx_b],
                  (size_t)len_a) == 0 ? 1 : 0;
}

void axiom_string_print(long long s) {
    int len = (int)(s >> 32);
    int idx = (int)(s & 0xFFFFFFFF);
    if (idx < 0 || idx >= axiom_string_table_count) return;
    fwrite(axiom_string_table[idx], 1, (size_t)len, stdout);
    fputc('\n', stdout);
}
