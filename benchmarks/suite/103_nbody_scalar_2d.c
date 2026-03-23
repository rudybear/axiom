#include <stdio.h>
#include <math.h>
int main(void) {
    double x0=0,y0=0, x1=1,y1=0, x2=0,y2=1;
    double vx0=0,vy0=0.01, vx1=0,vy1=-0.005, vx2=-0.005,vy2=0;
    double dt = 0.001;
    for (int step = 0; step < 1000; step++) {
        double dx01=x1-x0, dy01=y1-y0;
        double r01_sq = dx01*dx01+dy01*dy01+0.0001;
        double r01 = sqrt(r01_sq);
        double f01 = 1.0/(r01_sq*r01);
        vx0+=f01*dx01*dt; vy0+=f01*dy01*dt;
        vx1-=f01*dx01*dt; vy1-=f01*dy01*dt;
        x0+=vx0*dt; y0+=vy0*dt;
        x1+=vx1*dt; y1+=vy1*dt;
        x2+=vx2*dt; y2+=vy2*dt;
    }
    double energy = 0.5*(vx0*vx0+vy0*vy0+vx1*vx1+vy1*vy1+vx2*vx2+vy2*vy2);
    printf("%f\n", energy);
    return 0;
}
