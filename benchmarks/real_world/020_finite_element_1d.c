#include <stdio.h>
#include <math.h>

static double K_diag[2001], K_upper[2001], K_lower[2001];
static double M_diag[2001], M_upper[2001], M_lower[2001];
static double F_arr[2001], u_arr[2001];
static double A_diag[2001], A_upper[2001], A_lower[2001];
static double c_prime[2001], d_prime[2001];

int main(void) {
    int n_elem = 2000, n_nodes = 2001;
    double domain_len = 10.0;
    double h = domain_len / (double)n_elem;
    double pi = 3.14159265358979;

    double ke_diag = 1.0/h, ke_off = -1.0/h;
    double me_diag = h*2.0/6.0, me_off = h/6.0;

    for (int e = 0; e < n_elem; e++) {
        int i = e, j = e+1;
        K_diag[i] += ke_diag; K_diag[j] += ke_diag;
        K_upper[i] += ke_off; K_lower[j] += ke_off;
        M_diag[i] += me_diag; M_diag[j] += me_diag;
        M_upper[i] += me_off; M_lower[j] += me_off;

        double x_mid = ((double)i + 0.5) * h;
        double arg = pi * x_mid / domain_len;
        double a = arg - (double)(int)(arg/(2*pi)) * 2*pi;
        if (a > pi) a -= 2*pi;
        if (a < -pi) a += 2*pi;
        double a3=a*a*a, a5=a3*a*a, a7=a5*a*a, a9=a7*a*a;
        double sin_val = a - a3/6 + a5/120 - a7/5040 + a9/362880;
        double f_val = sin_val * (1.0 + pi*pi/100.0);

        F_arr[i] += f_val*h/2; F_arr[j] += f_val*h/2;
    }

    for (int i = 0; i < n_nodes; i++) {
        A_diag[i] = K_diag[i] + M_diag[i];
        A_upper[i] = K_upper[i] + M_upper[i];
        A_lower[i] = K_lower[i] + M_lower[i];
    }

    A_diag[0] = 1; A_upper[0] = 0; F_arr[0] = 0;
    A_diag[n_nodes-1] = 1; A_lower[n_nodes-1] = 0; F_arr[n_nodes-1] = 0;

    // Thomas algorithm
    c_prime[0] = A_upper[0] / A_diag[0];
    d_prime[0] = F_arr[0] / A_diag[0];
    for (int i = 1; i < n_nodes; i++) {
        double denom = A_diag[i] - A_lower[i]*c_prime[i-1];
        c_prime[i] = A_upper[i] / denom;
        d_prime[i] = (F_arr[i] - A_lower[i]*d_prime[i-1]) / denom;
    }
    u_arr[n_nodes-1] = d_prime[n_nodes-1];
    for (int i = n_nodes-2; i >= 0; i--)
        u_arr[i] = d_prime[i] - c_prime[i]*u_arr[i+1];

    double error_l2 = 0, error_max = 0;
    for (int idx = 0; idx < n_nodes; idx++) {
        double x = (double)idx * h;
        double arg = pi*x/domain_len;
        double a = arg - (double)(int)(arg/(2*pi))*2*pi;
        if (a > pi) a -= 2*pi; if (a < -pi) a += 2*pi;
        double a3=a*a*a,a5=a3*a*a,a7=a5*a*a,a9=a7*a*a;
        double u_exact = a-a3/6+a5/120-a7/5040+a9/362880;
        double err = fabs(u_arr[idx]-u_exact);
        error_l2 += err*err*h;
        if (err > error_max) error_max = err;
    }
    error_l2 = sqrt(error_l2);

    double u_sum = 0, u_max = 0;
    for (int idx = 0; idx < n_nodes; idx++) {
        u_sum += u_arr[idx] * (double)(idx+1);
        if (u_arr[idx] > u_max) u_max = u_arr[idx];
    }

    double checksum = u_sum + error_l2*1000000 + error_max*100000 + u_max*10000;
    printf("%.6f\n", checksum);
    return 0;
}
