#include <stdio.h>
#include <math.h>
#include <stdint.h>

#define N_BODIES 5

static double bx[N_BODIES], by[N_BODIES], bz[N_BODIES];
static double bvx[N_BODIES], bvy[N_BODIES], bvz[N_BODIES];
static double bmass[N_BODIES];

static void advance(double dt) {
    for (int i = 0; i < N_BODIES; i++) {
        for (int j = i + 1; j < N_BODIES; j++) {
            double dx = bx[i] - bx[j];
            double dy = by[i] - by[j];
            double dz = bz[i] - bz[j];
            double dist2 = dx*dx + dy*dy + dz*dz;
            double dist = sqrt(dist2);
            double mag = dt / (dist2 * dist);

            bvx[i] -= dx * bmass[j] * mag;
            bvy[i] -= dy * bmass[j] * mag;
            bvz[i] -= dz * bmass[j] * mag;

            bvx[j] += dx * bmass[i] * mag;
            bvy[j] += dy * bmass[i] * mag;
            bvz[j] += dz * bmass[i] * mag;
        }
    }
    for (int i = 0; i < N_BODIES; i++) {
        bx[i] += dt * bvx[i];
        by[i] += dt * bvy[i];
        bz[i] += dt * bvz[i];
    }
}

static double energy(void) {
    double e = 0.0;
    for (int i = 0; i < N_BODIES; i++) {
        e += 0.5 * bmass[i] * (bvx[i]*bvx[i] + bvy[i]*bvy[i] + bvz[i]*bvz[i]);
        for (int j = i + 1; j < N_BODIES; j++) {
            double dx = bx[i]-bx[j], dy = by[i]-by[j], dz = bz[i]-bz[j];
            e -= bmass[i] * bmass[j] / sqrt(dx*dx+dy*dy+dz*dz);
        }
    }
    return e;
}

int main(void) {
    double pi = 3.14159265358979323;
    double solar_mass = 4.0 * pi * pi;
    double days_per_year = 365.24;

    /* Sun */
    bx[0]=0; by[0]=0; bz[0]=0;
    bvx[0]=0; bvy[0]=0; bvz[0]=0;
    bmass[0]=solar_mass;

    /* Jupiter */
    bx[1]=4.84143144246472090; by[1]=-1.16032004402742839; bz[1]=-0.103622044471123109;
    bvx[1]=0.00166007664274403694*days_per_year;
    bvy[1]=0.00769901118419740425*days_per_year;
    bvz[1]=-0.0000690460016972063023*days_per_year;
    bmass[1]=0.000954791938424326609*solar_mass;

    /* Saturn */
    bx[2]=8.34336671824457987; by[2]=4.12479856412430479; bz[2]=-0.403523417114321381;
    bvx[2]=-0.00276742510726862411*days_per_year;
    bvy[2]=0.00499852801234917238*days_per_year;
    bvz[2]=0.0000230417297573763929*days_per_year;
    bmass[2]=0.000285885980666130812*solar_mass;

    /* Uranus */
    bx[3]=12.8943695621391310; by[3]=-15.1111514016986312; bz[3]=-0.223307578892655734;
    bvx[3]=0.00296460137564761618*days_per_year;
    bvy[3]=0.00237847173959480950*days_per_year;
    bvz[3]=-0.0000296589568540237556*days_per_year;
    bmass[3]=0.0000436624404335156298*solar_mass;

    /* Neptune */
    bx[4]=15.3796971148509165; by[4]=-25.9193146099879641; bz[4]=0.179258772950371181;
    bvx[4]=0.00268067772490389322*days_per_year;
    bvy[4]=0.00162824170038242295*days_per_year;
    bvz[4]=-0.0000951592254519715870*days_per_year;
    bmass[4]=0.0000515138902046611451*solar_mass;

    /* Offset momentum */
    double px=0, py=0, pz=0;
    for (int i=1; i<N_BODIES; i++) {
        px += bvx[i]*bmass[i]; py += bvy[i]*bmass[i]; pz += bvz[i]*bmass[i];
    }
    bvx[0]=-px/solar_mass; bvy[0]=-py/solar_mass; bvz[0]=-pz/solar_mass;

    printf("%.9f\n", energy());

    int steps = 50000000;
    for (int s = 0; s < steps; s++) advance(0.01);

    printf("%.9f\n", energy());
    return 0;
}
