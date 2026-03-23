#include <stdio.h>

int main(void) {
    static double grid[250000], grid_new[250000];
    int n = 500, iters = 100;

    for (int i = 0; i < 250000; i++) { grid[i] = 0.0; grid_new[i] = 0.0; }

    for (int j = 0; j < 500; j++) {
        grid[j] = 100.0;
        grid_new[j] = 100.0;
    }
    for (int i = 0; i < 500; i++) {
        double val = 100.0 * (1.0 - (double)i / 499.0);
        grid[i*500] = val;
        grid_new[i*500] = val;
    }

    for (int iter = 0; iter < iters; iter++) {
        for (int i = 1; i < 499; i++)
            for (int j = 1; j < 499; j++)
                grid_new[i*500+j] = 0.25 * (grid[(i-1)*500+j] + grid[(i+1)*500+j] + grid[i*500+j-1] + grid[i*500+j+1]);
        for (int i = 1; i < 499; i++)
            for (int j = 1; j < 499; j++)
                grid[i*500+j] = grid_new[i*500+j];
    }

    double checksum = 0.0;
    for (int i = 0; i < 250000; i++) checksum += grid[i];
    printf("%f\n", checksum);
    return 0;
}
