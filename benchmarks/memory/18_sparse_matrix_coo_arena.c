#include <stdio.h>
#include <stdlib.h>

/* COO sparse matrix: allocate triplets, convert to CSR, multiply vector */

int main() {
    int n = 1000;
    int nnz = 50000;

    int *coo_row = (int *)malloc(nnz * sizeof(int));
    int *coo_col = (int *)malloc(nnz * sizeof(int));
    int *coo_val = (int *)malloc(nnz * sizeof(int));

    long long seed = 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    for (int i = 0; i < nnz; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        coo_row[i] = (int)(seed % n);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        coo_col[i] = (int)(seed % n);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        coo_val[i] = (int)(seed % 100) + 1;
    }

    int *row_count = (int *)calloc(n, sizeof(int));
    for (int i = 0; i < nnz; i++) row_count[coo_row[i]]++;

    int *row_ptr = (int *)malloc((n + 1) * sizeof(int));
    row_ptr[0] = 0;
    for (int i = 0; i < n; i++) row_ptr[i + 1] = row_ptr[i] + row_count[i];

    int *csr_col = (int *)malloc(nnz * sizeof(int));
    int *csr_val = (int *)malloc(nnz * sizeof(int));

    int *insert_pos = (int *)malloc(n * sizeof(int));
    for (int i = 0; i < n; i++) insert_pos[i] = row_ptr[i];
    for (int i = 0; i < nnz; i++) {
        int r = coo_row[i];
        int pos = insert_pos[r]++;
        csr_col[pos] = coo_col[i];
        csr_val[pos] = coo_val[i];
    }

    int *x = (int *)malloc(n * sizeof(int));
    int *y = (int *)malloc(n * sizeof(int));
    for (int i = 0; i < n; i++) x[i] = i + 1;

    long long total_sum = 0;
    for (int iter = 0; iter < 20; iter++) {
        for (int i = 0; i < n; i++) {
            long long sum = 0;
            for (int j = row_ptr[i]; j < row_ptr[i + 1]; j++) {
                sum += (long long)csr_val[j] * x[csr_col[j]];
            }
            y[i] = (int)(sum % 1000000007);
        }
        long long ysum = 0;
        for (int i = 0; i < n; i++) ysum += y[i];
        total_sum += ysum;
    }

    free(coo_row); free(coo_col); free(coo_val);
    free(row_count); free(row_ptr); free(csr_col); free(csr_val);
    free(insert_pos); free(x); free(y);

    printf("%lld\n", total_sum);
    return 0;
}
