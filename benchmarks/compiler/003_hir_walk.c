#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

static int64_t process_literal(int64_t value) { return value * 3 + 7; }
static int64_t process_binop(int64_t left, int64_t right) { return (left + right) % 1000000007LL; }
static int64_t process_unaryop(int64_t operand) { return (operand * 37 + 11) % 1000000007LL; }
static int64_t process_call(int64_t func_id, int64_t arg) { return (func_id * 101 + arg * 53) % 1000000007LL; }
static int64_t process_if(int64_t cond, int64_t then_val) { return (cond * 17 + then_val * 31) % 1000000007LL; }
static int64_t process_let(int64_t name_hash, int64_t init_val) { return (name_hash + init_val * 19) % 1000000007LL; }
static int64_t process_return(int64_t val) { return (val * 23 + 1) % 1000000007LL; }
static int64_t process_block(int64_t first, int64_t last) { return (first * 41 + last * 43) % 1000000007LL; }

int main(void) {
    int n = 2000000;
    int *node_type = (int *)calloc(n, sizeof(int));
    int *node_left = (int *)calloc(n, sizeof(int));
    int *node_right = (int *)calloc(n, sizeof(int));

    int64_t seed = 42;
    for (int i = 0; i < n; i++) {
        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        node_type[i] = (int)(seed % 8);
        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        node_left[i] = (int)(seed % n);
        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        node_right[i] = (int)(seed % n);
    }

    int64_t accum = 0;
    for (int i = 0; i < n; i++) {
        int ntype = node_type[i];
        int64_t left_val = (int64_t)node_left[i];
        int64_t right_val = (int64_t)node_right[i];

        if (ntype == 0) accum = (accum + process_literal(left_val)) % 1000000007LL;
        if (ntype == 1) accum = (accum + process_binop(left_val, right_val)) % 1000000007LL;
        if (ntype == 2) accum = (accum + process_unaryop(left_val)) % 1000000007LL;
        if (ntype == 3) accum = (accum + process_call(left_val, right_val)) % 1000000007LL;
        if (ntype == 4) accum = (accum + process_if(left_val, right_val)) % 1000000007LL;
        if (ntype == 5) accum = (accum + process_let(left_val, right_val)) % 1000000007LL;
        if (ntype == 6) accum = (accum + process_return(left_val)) % 1000000007LL;
        if (ntype == 7) accum = (accum + process_block(left_val, right_val)) % 1000000007LL;
    }

    printf("%lld\n", (long long)accum);
    free(node_type);
    free(node_left);
    free(node_right);
    return 0;
}
