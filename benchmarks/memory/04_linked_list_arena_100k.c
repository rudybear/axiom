#include <stdio.h>
#include <stdlib.h>

/* Insert 100K nodes into singly-linked list (arena-simulated with single malloc), traverse and sum */
/* Node layout: 2 int slots [value, next_index], -1 = null */

int main() {
    int n = 100000;
    /* Simulate arena: single large allocation */
    int *pool = (int *)malloc(n * 2 * sizeof(int));
    int head = -1;

    long long seed = 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    /* Insert 100K nodes at head */
    for (int i = 0; i < n; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int val = (int)(seed % 1000000);
        int base = i * 2;
        pool[base] = val;
        pool[base + 1] = head;
        head = i;
    }

    /* Traverse and sum */
    long long sum = 0;
    int count = 0;
    int cur = head;
    while (cur != -1) {
        int base = cur * 2;
        sum += pool[base];
        cur = pool[base + 1];
        count++;
    }

    free(pool);

    long long checksum = sum + count;
    printf("%lld\n", checksum);
    return 0;
}
