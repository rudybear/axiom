/*
 * ECS Benchmark — C reference implementation
 *
 * Identical algorithm to ecs_benchmark.axm so that checksums match.
 *
 * Compile:  gcc -O2 -o ecs_benchmark_c ecs_benchmark.c -lm
 *           clang -O2 -o ecs_benchmark_c ecs_benchmark.c -lm
 * Run:      ./ecs_benchmark_c
 *
 * Layout:  SOA arrays via malloc (mirrors the AXIOM arena layout)
 * Entities: 10,000 with Position(x,y) + Velocity(vx,vy) + Alive(int)
 * Systems:  physics (Euler), bounce (wall reflect), spawn/despawn
 * Ticks:    1000 @ dt = 0.016
 */

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <time.h>

/* ------------------------------------------------------------------ */
/* LCG PRNG — same constants as the AXIOM version                     */
/* ------------------------------------------------------------------ */

static int64_t g_seed = 123456789;

static double lcg_next_f64(double max) {
    g_seed = g_seed * 6364136223846793005LL + 1442695040888963407LL;
    int64_t bits = (uint64_t)g_seed >> 16;
    int64_t masked = bits & 0x7FFFFFFF;
    double fval = (double)masked;
    return fval * max / 2147483647.0;
}

/* ------------------------------------------------------------------ */
/* Systems                                                            */
/* ------------------------------------------------------------------ */

static void system_physics(double *x, double *y, double *vx, double *vy,
                           const int *alive, int start, int end, double dt) {
    for (int i = start; i < end; i++) {
        if (alive[i] == 1) {
            x[i] += vx[i] * dt;
            y[i] += vy[i] * dt;
        }
    }
}

static void system_bounce(double *x, double *y, double *vx, double *vy,
                          const int *alive, int start, int end) {
    for (int i = start; i < end; i++) {
        if (alive[i] == 1) {
            if (x[i] < 0.0) {
                x[i] = 0.0 - x[i];
                vx[i] = 0.0 - vx[i];
            }
            if (x[i] > 1000.0) {
                x[i] = 2000.0 - x[i];
                vx[i] = 0.0 - vx[i];
            }
            if (y[i] < 0.0) {
                y[i] = 0.0 - y[i];
                vy[i] = 0.0 - vy[i];
            }
            if (y[i] > 1000.0) {
                y[i] = 2000.0 - y[i];
                vy[i] = 0.0 - vy[i];
            }
        }
    }
}

static void entity_spawn(double *x, double *y, double *vx, double *vy,
                          int *alive, int id,
                          double px, double py, double dvx, double dvy) {
    x[id] = px;
    y[id] = py;
    vx[id] = dvx;
    vy[id] = dvy;
    alive[id] = 1;
}

static void entity_despawn(int *alive, int id) {
    alive[id] = 0;
}

/* ------------------------------------------------------------------ */
/* Timing helper                                                      */
/* ------------------------------------------------------------------ */

static int64_t clock_ns_now(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000LL + (int64_t)ts.tv_nsec;
}

/* ------------------------------------------------------------------ */
/* Main                                                               */
/* ------------------------------------------------------------------ */

int main(void) {
    const int n = 10000;
    const double dt = 0.016;
    const int ticks = 1000;
    const double world_size = 1000.0;

    /* SOA arrays */
    double *x     = (double *)calloc(n, sizeof(double));
    double *y     = (double *)calloc(n, sizeof(double));
    double *vx    = (double *)calloc(n, sizeof(double));
    double *vy    = (double *)calloc(n, sizeof(double));
    int    *alive = (int    *)calloc(n, sizeof(int));

    /* Initialize */
    g_seed = 123456789;
    for (int i = 0; i < n; i++) {
        double px  = lcg_next_f64(world_size);
        double py  = lcg_next_f64(world_size);
        double dvx = lcg_next_f64(200.0) - 100.0;
        double dvy = lcg_next_f64(200.0) - 100.0;
        entity_spawn(x, y, vx, vy, alive, i, px, py, dvx, dvy);
    }

    /* Timed game loop */
    int64_t t0 = clock_ns_now();

    for (int tick = 0; tick < ticks; tick++) {
        system_physics(x, y, vx, vy, alive, 0, n, dt);
        system_bounce(x, y, vx, vy, alive, 0, n);

        if (tick % 100 == 0) {
            for (int i = 0; i < 50; i++) {
                entity_despawn(alive, i + (tick / 100) * 50);
            }
            for (int i = 0; i < 50; i++) {
                int id = i + (tick / 100) * 50;
                if (id < n) {
                    double px  = lcg_next_f64(world_size);
                    double py  = lcg_next_f64(world_size);
                    double dvx = lcg_next_f64(100.0) - 50.0;
                    double dvy = lcg_next_f64(100.0) - 50.0;
                    entity_spawn(x, y, vx, vy, alive, id, px, py, dvx, dvy);
                }
            }
        }
    }

    int64_t t1 = clock_ns_now();
    int64_t elapsed_ms = (t1 - t0) / 1000000;

    /* Checksum */
    double checksum = 0.0;
    for (int i = 0; i < n; i++) {
        if (alive[i] == 1) {
            checksum += x[i] + y[i];
        }
    }

    printf("C ECS benchmark\n");
    printf("Entities: 10000  Ticks: 1000\n");
    printf("Elapsed (ms): %lld\n", (long long)elapsed_ms);
    printf("Checksum: %.6f\n", checksum);

    free(x);
    free(y);
    free(vx);
    free(vy);
    free(alive);

    return 0;
}
