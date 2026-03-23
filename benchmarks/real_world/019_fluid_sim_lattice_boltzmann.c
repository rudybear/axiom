#include <stdio.h>
#include <math.h>

static double f0[20000],f1[20000],f2[20000],f3[20000],f4[20000];
static double f5[20000],f6[20000],f7[20000],f8[20000];
static double g0[20000],g1[20000],g2[20000],g3[20000],g4[20000];
static double g5[20000],g6[20000],g7[20000],g8[20000];
static double rho[20000],ux_arr[20000],uy_arr[20000];
static int obstacle[20000];

int main(void) {
    int nx=200, ny=100, total=20000, nsteps=500;

    int ex[9]={0,1,0,-1,0,1,-1,-1,1};
    int ey_arr[9]={0,0,1,0,-1,1,1,-1,-1};
    double w[9]={4.0/9,1.0/9,1.0/9,1.0/9,1.0/9,1.0/36,1.0/36,1.0/36,1.0/36};

    int obs_cx=nx/4, obs_cy=ny/2, obs_r2=100;
    for(int y=0;y<ny;y++) for(int x=0;x<nx;x++) {
        int dx=x-obs_cx, dy=y-obs_cy;
        if(dx*dx+dy*dy<obs_r2) obstacle[y*nx+x]=1;
    }

    double tau=0.6, omega=1.0/tau, u_inlet=0.04;

    for(int y=0;y<ny;y++) for(int x=0;x<nx;x++) {
        int idx=y*nx+x;
        double u0=u_inlet, v0=0, usq=u0*u0+v0*v0;
        f0[idx]=w[0]*(1-1.5*usq);
        f1[idx]=w[1]*(1+3*u0+4.5*u0*u0-1.5*usq);
        f2[idx]=w[2]*(1-1.5*usq);
        f3[idx]=w[3]*(1-3*u0+4.5*u0*u0-1.5*usq);
        f4[idx]=w[4]*(1-1.5*usq);
        f5[idx]=w[5]*(1+3*u0+4.5*u0*u0-1.5*usq);
        f6[idx]=w[6]*(1-3*u0+4.5*u0*u0-1.5*usq);
        f7[idx]=w[7]*(1-3*u0+4.5*u0*u0-1.5*usq);
        f8[idx]=w[8]*(1+3*u0+4.5*u0*u0-1.5*usq);
    }

    for(int step=0;step<nsteps;step++) {
        // Streaming
        for(int y=0;y<ny;y++) for(int x=0;x<nx;x++) {
            int idx=y*nx+x;
            g0[idx]=f0[idx];
            g1[idx]=f1[y*nx+((x-1+nx)%nx)];
            g2[idx]=(y>0)?f2[(y-1)*nx+x]:f4[idx];
            g3[idx]=f3[y*nx+((x+1)%nx)];
            g4[idx]=(y<ny-1)?f4[(y+1)*nx+x]:f2[idx];
            g5[idx]=(y>0)?f5[(y-1)*nx+((x-1+nx)%nx)]:f7[idx];
            g6[idx]=(y>0)?f6[(y-1)*nx+((x+1)%nx)]:f8[idx];
            g7[idx]=(y<ny-1)?f7[(y+1)*nx+((x+1)%nx)]:f5[idx];
            g8[idx]=(y<ny-1)?f8[(y+1)*nx+((x-1+nx)%nx)]:f6[idx];
        }

        // Bounce-back
        for(int y=0;y<ny;y++) for(int x=0;x<nx;x++) {
            int idx=y*nx+x;
            if(obstacle[idx]) {
                double t1=g1[idx],t2=g2[idx],t3=g3[idx],t4=g4[idx];
                double t5=g5[idx],t6=g6[idx],t7=g7[idx],t8=g8[idx];
                g1[idx]=t3;g3[idx]=t1;g2[idx]=t4;g4[idx]=t2;
                g5[idx]=t7;g7[idx]=t5;g6[idx]=t8;g8[idx]=t6;
            }
        }

        // Collision
        for(int y=0;y<ny;y++) for(int x=0;x<nx;x++) {
            int idx=y*nx+x;
            double r=g0[idx]+g1[idx]+g2[idx]+g3[idx]+g4[idx]+g5[idx]+g6[idx]+g7[idx]+g8[idx];
            double u=(g1[idx]-g3[idx]+g5[idx]-g6[idx]-g7[idx]+g8[idx])/r;
            double v=(g2[idx]-g4[idx]+g5[idx]+g6[idx]-g7[idx]-g8[idx])/r;
            rho[idx]=r; ux_arr[idx]=u; uy_arr[idx]=v;
            double usq=u*u+v*v;
            double feq0=w[0]*r*(1-1.5*usq);
            double feq1=w[1]*r*(1+3*u+4.5*u*u-1.5*usq);
            double feq2=w[2]*r*(1+3*v+4.5*v*v-1.5*usq);
            double feq3=w[3]*r*(1-3*u+4.5*u*u-1.5*usq);
            double feq4=w[4]*r*(1-3*v+4.5*v*v-1.5*usq);
            double feq5=w[5]*r*(1+3*(u+v)+4.5*(u+v)*(u+v)-1.5*usq);
            double feq6=w[6]*r*(1+3*(-u+v)+4.5*(-u+v)*(-u+v)-1.5*usq);
            double feq7=w[7]*r*(1+3*(-u-v)+4.5*(-u-v)*(-u-v)-1.5*usq);
            double feq8=w[8]*r*(1+3*(u-v)+4.5*(u-v)*(u-v)-1.5*usq);
            f0[idx]=g0[idx]-omega*(g0[idx]-feq0);
            f1[idx]=g1[idx]-omega*(g1[idx]-feq1);
            f2[idx]=g2[idx]-omega*(g2[idx]-feq2);
            f3[idx]=g3[idx]-omega*(g3[idx]-feq3);
            f4[idx]=g4[idx]-omega*(g4[idx]-feq4);
            f5[idx]=g5[idx]-omega*(g5[idx]-feq5);
            f6[idx]=g6[idx]-omega*(g6[idx]-feq6);
            f7[idx]=g7[idx]-omega*(g7[idx]-feq7);
            f8[idx]=g8[idx]-omega*(g8[idx]-feq8);
        }

        // Inlet BC
        for(int y=1;y<ny-1;y++) {
            int idx=y*nx;
            double r=rho[idx];
            f1[idx]=f3[idx]+2.0/3*r*u_inlet;
            f5[idx]=f7[idx]+1.0/6*r*u_inlet;
            f8[idx]=f6[idx]+1.0/6*r*u_inlet;
        }
    }

    double total_ke=0, avg_ux=0, avg_uy=0; int count=0;
    for(int y=0;y<ny;y++) for(int x=0;x<nx;x++) {
        int idx=y*nx+x;
        if(!obstacle[idx]) {
            total_ke+=0.5*rho[idx]*(ux_arr[idx]*ux_arr[idx]+uy_arr[idx]*uy_arr[idx]);
            avg_ux+=ux_arr[idx]; avg_uy+=uy_arr[idx]; count++;
        }
    }
    avg_ux/=(double)count; avg_uy/=(double)count;

    double checksum=total_ke*1000000+avg_ux*100000+avg_uy*100000+(double)count;
    printf("%.6f\n", checksum);
    return 0;
}
