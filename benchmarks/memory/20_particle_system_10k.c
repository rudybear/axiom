#include <stdio.h>
#include <stdlib.h>

/* 10K particles (heap-allocated), 100 timesteps, add/remove dynamically */
/* Particle: 6 int slots [x, y, vx, vy, life, active] */

int main() {
    int max_particles = 15000;
    int initial = 10000;
    int slots = 6;

    int *particles = (int *)malloc(max_particles * slots * sizeof(int));
    int active_count = 0;

    long long seed = 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    for (int i = 0; i < initial; i++) {
        int base = i * slots;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        particles[base] = (int)(seed % 10000);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        particles[base + 1] = (int)(seed % 10000);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        particles[base + 2] = (int)(seed % 200) - 100;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        particles[base + 3] = (int)(seed % 200) - 100;
        particles[base + 4] = (int)(seed % 100) + 50;
        particles[base + 5] = 1;
        active_count++;
    }

    int *free_list = (int *)malloc(max_particles * sizeof(int));
    int free_top = 0;
    int next_slot = initial;

    long long total_checksum = 0;

    for (int step = 0; step < 100; step++) {
        for (int i = 0; i < next_slot; i++) {
            int base = i * slots;
            if (particles[base + 5] == 1) {
                particles[base] += particles[base + 2];
                particles[base + 1] += particles[base + 3];
                particles[base + 3] += 2;
                particles[base + 4]--;
                if (particles[base + 4] <= 0) {
                    particles[base + 5] = 0;
                    active_count--;
                    if (free_top < max_particles)
                        free_list[free_top++] = i;
                }
            }
        }

        int spawn = 100;
        for (int s = 0; s < spawn; s++) {
            int slot = -1;
            if (free_top > 0) slot = free_list[--free_top];
            else if (next_slot < max_particles) slot = next_slot++;
            if (slot != -1) {
                int base = slot * slots;
                seed = (lcg_a * seed + lcg_c) % lcg_m;
                particles[base] = (int)(seed % 10000);
                seed = (lcg_a * seed + lcg_c) % lcg_m;
                particles[base + 1] = (int)(seed % 10000);
                seed = (lcg_a * seed + lcg_c) % lcg_m;
                particles[base + 2] = (int)(seed % 200) - 100;
                seed = (lcg_a * seed + lcg_c) % lcg_m;
                particles[base + 3] = (int)(seed % 200) - 100;
                particles[base + 4] = (int)(seed % 100) + 50;
                particles[base + 5] = 1;
                active_count++;
            }
        }

        long long step_sum = 0;
        for (int i = 0; i < next_slot; i++) {
            int base = i * slots;
            if (particles[base + 5] == 1) {
                step_sum += particles[base] + particles[base + 1];
            }
        }
        total_checksum += step_sum;
    }

    free(particles);
    free(free_list);

    total_checksum += active_count;
    printf("%lld\n", total_checksum);
    return 0;
}
