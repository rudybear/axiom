#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

static int lex_classify(int c) {
    if (c >= 48 && c <= 57) return 1;
    if (c >= 97 && c <= 122) return 2;
    if (c == 32 || c == 10) return 3;
    if (c == 40 || c == 41) return 4;
    if (c == 123 || c == 125) return 5;
    if (c == 59) return 6;
    return 7;
}

static int64_t hash_combine(int64_t a, int64_t b) {
    return ((a * 31) + b) % 1000000007LL;
}

static int64_t parse_window(int *tokens, int start, int end) {
    int64_t h = 0;
    for (int i = start; i < end; i++) {
        int tok = tokens[i];
        h = hash_combine(h, (int64_t)tok);
        if (tok == 1) h = hash_combine(h, (int64_t)tok * 7);
        if (tok == 2) h = hash_combine(h, (int64_t)tok * 13);
        if (tok == 4) h = hash_combine(h, (int64_t)tok * 29);
    }
    return h;
}

static int64_t hir_dispatch(int node_type, int64_t val) {
    if (node_type == 0) return val * 3 + 7;
    if (node_type == 1) return (val * 37 + 11) % 1000000007LL;
    if (node_type == 2) return (val * 101 + 53) % 1000000007LL;
    if (node_type == 3) return (val * 17 + 31) % 1000000007LL;
    if (node_type == 4) return (val * 19 + 41) % 1000000007LL;
    if (node_type == 5) return (val * 23 + 47) % 1000000007LL;
    if (node_type == 6) return (val * 29 + 59) % 1000000007LL;
    return (val * 43 + 67) % 1000000007LL;
}

static int codegen_emit(int *buf, int pos, int opcode) {
    buf[pos] = opcode + 48;
    buf[pos+1] = 58;
    buf[pos+2] = 32;
    return 3;
}

int main(void) {
    int n = 500000;
    int window_size = 16;

    /* Phase 1: Lexing */
    int *tokens = (int *)calloc(n, sizeof(int));
    int64_t seed = 42;

    for (int i = 0; i < n; i++) {
        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        int c = (int)(seed % 128);
        tokens[i] = lex_classify(c);
    }

    /* Phase 2: Parsing */
    int num_windows = n / window_size;
    int64_t *parse_results = (int64_t *)calloc(num_windows, sizeof(int64_t));

    for (int w = 0; w < num_windows; w++) {
        int start = w * window_size;
        parse_results[w] = parse_window(tokens, start, start + window_size);
    }

    /* Phase 3: HIR Walk */
    int64_t hir_accum = 0;
    for (int i = 0; i < num_windows; i++) {
        int64_t pv = parse_results[i];
        int node_type = (int)(pv % 8);
        hir_accum = (hir_accum + hir_dispatch(node_type, pv)) % 1000000007LL;
    }

    /* Phase 4: Codegen */
    int *emit_buf = (int *)calloc(num_windows * 4, sizeof(int));
    int emit_pos = 0;

    for (int i = 0; i < num_windows; i++) {
        int64_t pv = parse_results[i];
        int opcode = (int)(pv % 8);
        int written = codegen_emit(emit_buf, emit_pos, opcode);
        emit_pos += written;
    }

    /* Final checksum */
    int64_t checksum = hir_accum;
    for (int i = 0; i < emit_pos; i++) {
        checksum = hash_combine(checksum, (int64_t)emit_buf[i]);
    }

    printf("%lld\n", (long long)checksum);
    free(tokens);
    free(parse_results);
    free(emit_buf);
    return 0;
}
