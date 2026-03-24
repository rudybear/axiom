#include <stdio.h>
#include <stdlib.h>

/* Dijkstra shortest path on 500-node graph. Array-based priority queue. */

int main() {
    int num_nodes = 500;
    int max_edges = 5000;

    int *adj_offset = (int *)malloc((num_nodes + 1) * sizeof(int));
    int *adj_target = (int *)malloc(max_edges * 2 * sizeof(int));
    int *adj_weight = (int *)malloc(max_edges * 2 * sizeof(int));

    long long seed = 99;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    int *degree = (int *)calloc(num_nodes, sizeof(int));
    int *edges_src = (int *)malloc(max_edges * sizeof(int));
    int *edges_dst = (int *)malloc(max_edges * sizeof(int));
    int *edges_wt = (int *)malloc(max_edges * sizeof(int));

    for (int e = 0; e < max_edges; e++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int u = (int)(seed % num_nodes);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int v = (int)(seed % num_nodes);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int w = (int)(seed % 100) + 1;
        edges_src[e] = u;
        edges_dst[e] = v;
        edges_wt[e] = w;
        degree[u]++;
        degree[v]++;
    }

    adj_offset[0] = 0;
    for (int i = 0; i < num_nodes; i++)
        adj_offset[i + 1] = adj_offset[i] + degree[i];

    int *insert_pos = (int *)malloc(num_nodes * sizeof(int));
    for (int i = 0; i < num_nodes; i++) insert_pos[i] = adj_offset[i];

    for (int e = 0; e < max_edges; e++) {
        int u = edges_src[e], v = edges_dst[e], w = edges_wt[e];
        adj_target[insert_pos[u]] = v;
        adj_weight[insert_pos[u]] = w;
        insert_pos[u]++;
        adj_target[insert_pos[v]] = u;
        adj_weight[insert_pos[v]] = w;
        insert_pos[v]++;
    }

    int *dist = (int *)malloc(num_nodes * sizeof(int));
    int *visited = (int *)malloc(num_nodes * sizeof(int));

    long long total_dist = 0;
    int total_reached = 0;

    for (int src = 0; src < 5; src++) {
        int start = src * 100;
        for (int i = 0; i < num_nodes; i++) {
            dist[i] = 999999999;
            visited[i] = 0;
        }
        dist[start] = 0;

        for (int step = 0; step < num_nodes; step++) {
            int min_d = 999999999, min_u = -1;
            for (int i = 0; i < num_nodes; i++) {
                if (!visited[i] && dist[i] < min_d) {
                    min_d = dist[i];
                    min_u = i;
                }
            }
            if (min_u == -1) break;
            visited[min_u] = 1;
            for (int e = adj_offset[min_u]; e < adj_offset[min_u + 1]; e++) {
                int v = adj_target[e], w = adj_weight[e];
                int new_d = min_d + w;
                if (new_d < dist[v]) dist[v] = new_d;
            }
        }

        for (int i = 0; i < num_nodes; i++) {
            if (dist[i] < 999999999) {
                total_dist += dist[i];
                total_reached++;
            }
        }
    }

    free(adj_offset); free(adj_target); free(adj_weight);
    free(degree); free(edges_src); free(edges_dst); free(edges_wt);
    free(insert_pos); free(dist); free(visited);

    long long checksum = total_dist + total_reached;
    printf("%lld\n", checksum);
    return 0;
}
