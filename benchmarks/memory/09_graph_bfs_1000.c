#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* BFS on 1000-node graph with heap-allocated adjacency lists and queue */

int main() {
    int num_nodes = 1000;
    int max_edges = 10000;

    int *adj_offset = (int *)malloc((num_nodes + 1) * sizeof(int));
    int *adj_list = (int *)malloc(max_edges * 2 * sizeof(int));

    long long seed = 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    int *degree = (int *)calloc(num_nodes, sizeof(int));
    int *edges_src = (int *)malloc(max_edges * sizeof(int));
    int *edges_dst = (int *)malloc(max_edges * sizeof(int));

    for (int e = 0; e < max_edges; e++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int u = (int)(seed % num_nodes);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int v = (int)(seed % num_nodes);
        edges_src[e] = u;
        edges_dst[e] = v;
        degree[u]++;
        degree[v]++;
    }

    adj_offset[0] = 0;
    for (int i = 0; i < num_nodes; i++) {
        adj_offset[i + 1] = adj_offset[i] + degree[i];
    }

    int *insert_pos = (int *)calloc(num_nodes, sizeof(int));
    for (int i = 0; i < num_nodes; i++) insert_pos[i] = adj_offset[i];

    for (int e = 0; e < max_edges; e++) {
        int u = edges_src[e], v = edges_dst[e];
        adj_list[insert_pos[u]++] = v;
        adj_list[insert_pos[v]++] = u;
    }

    int *queue = (int *)malloc(num_nodes * sizeof(int));
    int *visited = (int *)malloc(num_nodes * sizeof(int));
    int *dist = (int *)malloc(num_nodes * sizeof(int));

    long long total_dist = 0;
    int total_reached = 0;

    for (int src = 0; src < 10; src++) {
        int start = src * 100;
        for (int i = 0; i < num_nodes; i++) {
            visited[i] = 0;
            dist[i] = -1;
        }

        visited[start] = 1;
        dist[start] = 0;
        queue[0] = start;
        int q_front = 0, q_back = 1;

        while (q_front < q_back) {
            int u = queue[q_front++];
            int d = dist[u];
            for (int e = adj_offset[u]; e < adj_offset[u + 1]; e++) {
                int v = adj_list[e];
                if (!visited[v]) {
                    visited[v] = 1;
                    dist[v] = d + 1;
                    queue[q_back++] = v;
                }
            }
        }

        for (int i = 0; i < num_nodes; i++) {
            if (dist[i] >= 0) {
                total_dist += dist[i];
                total_reached++;
            }
        }
    }

    free(adj_offset); free(adj_list); free(degree);
    free(edges_src); free(edges_dst); free(insert_pos);
    free(queue); free(visited); free(dist);

    long long checksum = total_dist + total_reached;
    printf("%lld\n", checksum);
    return 0;
}
