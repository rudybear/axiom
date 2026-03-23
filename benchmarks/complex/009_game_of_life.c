#include <stdio.h>
#include <stdint.h>

int main(void) {
    static int grid[40000], next[40000];
    int rows = 200, cols = 200, gens = 500;

    int64_t seed = 123456789, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;
    for (int i = 0; i < 40000; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        grid[i] = (seed % 3 == 0) ? 1 : 0;
    }

    for (int gen = 0; gen < gens; gen++) {
        for (int i = 0; i < 200; i++) {
            for (int j = 0; j < 200; j++) {
                int im1 = (i + 199) % 200, ip1 = (i + 1) % 200;
                int jm1 = (j + 199) % 200, jp1 = (j + 1) % 200;
                int count = grid[im1*200+jm1] + grid[im1*200+j] + grid[im1*200+jp1]
                          + grid[i*200+jm1] + grid[i*200+jp1]
                          + grid[ip1*200+jm1] + grid[ip1*200+j] + grid[ip1*200+jp1];
                int alive = grid[i*200+j];
                if (alive) next[i*200+j] = (count == 2 || count == 3) ? 1 : 0;
                else next[i*200+j] = (count == 3) ? 1 : 0;
            }
        }
        for (int i = 0; i < 40000; i++) grid[i] = next[i];
    }

    int alive_count = 0;
    for (int i = 0; i < 40000; i++) alive_count += grid[i];
    printf("%d\n", alive_count);
    return 0;
}
