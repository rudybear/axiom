#include <stdio.h>

int main(void) {
    static double grid[1000000], tmp[1000000];
    int n = 1000, iters = 50;

    for (int i = 0; i < 1000000; i++) { grid[i] = 0.0; tmp[i] = 0.0; }

    for (int i = 0; i < 1000; i++) {
        for (int j = 0; j < 1000; j++) {
            double di = (double)i - 500.0, dj = (double)j - 500.0;
            if (di*di + dj*dj < 10000.0)
                grid[i*1000+j] = 100.0;
        }
    }

    for (int iter = 0; iter < iters; iter++) {
        for (int i = 1; i < 999; i++)
            for (int j = 1; j < 999; j++)
                tmp[i*1000+j] = 0.2*(grid[i*1000+j]+grid[(i-1)*1000+j]+grid[(i+1)*1000+j]+grid[i*1000+j-1]+grid[i*1000+j+1]);
        for (int i = 1; i < 999; i++)
            for (int j = 1; j < 999; j++)
                grid[i*1000+j] = tmp[i*1000+j];
    }

    double checksum = 0.0;
    for (int i = 0; i < 1000000; i++) checksum += grid[i];
    printf("%f\n", checksum);
    return 0;
}
