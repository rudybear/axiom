#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

static int *tokens;

static int64_t hash_combine(int64_t h, int64_t val) {
    return ((h * 31) + val) % 1000000007LL;
}

static int64_t parse_expr(int pos, int limit);

static int64_t parse_atom(int pos, int limit) {
    if (pos >= limit) return 0;
    int tok = tokens[pos];
    if (tok == 1) return (int64_t)(pos * 7 + 13);
    if (tok == 3) {
        int64_t inner = parse_expr(pos + 1, limit);
        return inner;
    }
    return (int64_t)tok;
}

static int64_t parse_term(int pos, int limit) {
    int64_t left = parse_atom(pos, limit);
    int p = pos + 1;
    if (p < limit) {
        int tok = tokens[p];
        if (tok == 2) {
            int64_t right = parse_atom(p + 1, limit);
            left = hash_combine(left, right);
        }
    }
    return left;
}

static int64_t parse_expr(int pos, int limit) {
    int64_t result = parse_term(pos, limit);
    int p = pos + 2;
    for (int step = 0; step < 5; step++) {
        int pp = p + step * 2;
        if (pp < limit) {
            int tok = tokens[pp];
            if (tok == 2) {
                int64_t next = parse_term(pp + 1, limit);
                result = hash_combine(result, next);
            }
        }
    }
    return result;
}

int main(void) {
    int n = 1000000;
    tokens = (int *)calloc(n, sizeof(int));

    int64_t seed = 42;
    for (int i = 0; i < n; i++) {
        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        tokens[i] = (int)(seed % 5) + 1;
    }

    int64_t checksum = 0;
    int window = 20;
    int num_windows = n / window;

    for (int w = 0; w < num_windows; w++) {
        int start = w * window;
        int64_t result = parse_expr(start, start + window);
        checksum = hash_combine(checksum, result);
    }

    printf("%lld\n", (long long)checksum);
    free(tokens);
    return 0;
}
