#include <stdio.h>
#include <stdint.h>

int main(void) {
    static int dist[40000];
    int n = 200, inf = 999999;

    for (int i = 0; i < 200; i++)
        for (int j = 0; j < 200; j++)
            dist[i*200+j] = (i == j) ? 0 : inf;

    int64_t seed = 777, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;
    for (int e = 0; e < 10000; e++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int u = (int)(seed % 200);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int v = (int)(seed % 200);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int w = (int)(seed % 100) + 1;
        if (u != v && w < dist[u*200+v]) dist[u*200+v] = w;
    }

    for (int k = 0; k < 200; k++)
        for (int i = 0; i < 200; i++)
            for (int j = 0; j < 200; j++) {
                int via_k = dist[i*200+k] + dist[k*200+j];
                if (via_k < dist[i*200+j]) dist[i*200+j] = via_k;
            }

    int64_t checksum = 0;
    for (int i = 0; i < 200; i++)
        for (int j = 0; j < 200; j++)
            if (dist[i*200+j] < inf) checksum += dist[i*200+j];
    printf("%lld\n", (long long)checksum);
    return 0;
}
