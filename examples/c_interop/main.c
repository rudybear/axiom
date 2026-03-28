/*
 * Example: C program calling AXIOM-compiled functions.
 *
 * Build steps:
 *   axiom compile axiom_math.axm --lib -o libaxiom_math.a
 *   axiom header axiom_math.axm -o axiom_math.h
 *   gcc main.c -L. -laxiom_math -o main -lm
 *   ./main
 */

#include <stdio.h>
#include "axiom_math.h"

int main() {
    double a[] = {1.0, 2.0, 3.0, 4.0, 5.0};
    double b[] = {2.0, 3.0, 4.0, 5.0, 6.0};

    /* dot product: 1*2 + 2*3 + 3*4 + 4*5 + 5*6 = 2+6+12+20+30 = 70 */
    double dp = dot_product(a, b, 5);
    printf("dot product: %f\n", dp);

    /* array sum: 1+2+3+4+5 = 15 */
    double sum = array_sum(a, 5);
    printf("array sum: %f\n", sum);

    /* scale by 2: [2, 4, 6, 8, 10] */
    scale(a, 5, 2.0);
    printf("scaled: ");
    for (int i = 0; i < 5; i++) {
        printf("%f ", a[i]);
    }
    printf("\n");

    return 0;
}
