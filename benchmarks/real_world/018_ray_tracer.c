#include <stdio.h>
#include <math.h>
#include <stdint.h>

static int img_r[65536], img_g[65536], img_b[65536];

int main(void) {
    int width = 256, height = 256, total = 65536;

    double sphere_cx[3]={-1.5, 0.5, 2.0}, sphere_cy[3]={0.0,-0.5,0.5}, sphere_cz[3]={5.0,4.0,6.0};
    double sphere_rad[3]={1.0, 0.8, 1.2};
    double sphere_r[3]={0.9,0.1,0.2}, sphere_g[3]={0.1,0.9,0.2}, sphere_b[3]={0.1,0.2,0.9};
    double sphere_spec[3]={50.0, 30.0, 80.0};

    double light_x=-3.0, light_y=-5.0, light_z=2.0, light_intensity=1.5;
    double fov_scale = 1.0, ambient = 0.1;

    for (int py = 0; py < height; py++) {
        for (int px = 0; px < width; px++) {
            double ray_dx = ((double)px - (double)width/2.0)/(double)width * fov_scale;
            double ray_dy = ((double)py - (double)height/2.0)/(double)height * fov_scale;
            double ray_dz = 1.0;
            double ray_len = sqrt(ray_dx*ray_dx+ray_dy*ray_dy+ray_dz*ray_dz);
            ray_dx/=ray_len; ray_dy/=ray_len; ray_dz/=ray_len;

            double closest_t = 99999.0; int hit_sphere = -1;
            for (int s = 0; s < 3; s++) {
                double oc_x=-sphere_cx[s], oc_y=-sphere_cy[s], oc_z=-sphere_cz[s];
                double a=ray_dx*ray_dx+ray_dy*ray_dy+ray_dz*ray_dz;
                double b=2.0*(oc_x*ray_dx+oc_y*ray_dy+oc_z*ray_dz);
                double c=oc_x*oc_x+oc_y*oc_y+oc_z*oc_z-sphere_rad[s]*sphere_rad[s];
                double disc=b*b-4.0*a*c;
                if (disc >= 0.0) {
                    double sq=sqrt(disc), t1=(-b-sq)/(2*a), t2=(-b+sq)/(2*a);
                    double t=t1; if(t<0.001)t=t2;
                    if (t>0.001 && t<closest_t) { closest_t=t; hit_sphere=s; }
                }
            }

            double fr=0,fg=0,fb=0;
            if (hit_sphere >= 0) {
                double hx=closest_t*ray_dx, hy=closest_t*ray_dy, hz=closest_t*ray_dz;
                double nx=(hx-sphere_cx[hit_sphere])/sphere_rad[hit_sphere];
                double ny=(hy-sphere_cy[hit_sphere])/sphere_rad[hit_sphere];
                double nz=(hz-sphere_cz[hit_sphere])/sphere_rad[hit_sphere];

                double lx=light_x-hx, ly=light_y-hy, lz=light_z-hz;
                double l_len=sqrt(lx*lx+ly*ly+lz*lz);
                lx/=l_len; ly/=l_len; lz/=l_len;

                int in_shadow = 0;
                for (int s2=0; s2<3; s2++) {
                    if (s2!=hit_sphere) {
                        double sx=hx-sphere_cx[s2], sy=hy-sphere_cy[s2], sz=hz-sphere_cz[s2];
                        double sa=lx*lx+ly*ly+lz*lz;
                        double sb=2*(sx*lx+sy*ly+sz*lz);
                        double sc=sx*sx+sy*sy+sz*sz-sphere_rad[s2]*sphere_rad[s2];
                        double sd=sb*sb-4*sa*sc;
                        if (sd>=0) { double st=(-sb-sqrt(sd))/(2*sa); if(st>0.001&&st<l_len) in_shadow=1; }
                    }
                }

                double ndotl=nx*lx+ny*ly+nz*lz; if(ndotl<0)ndotl=0;
                double rx=2*ndotl*nx-lx, ry=2*ndotl*ny-ly, rz=2*ndotl*nz-lz;
                double vx=-ray_dx, vy=-ray_dy, vz=-ray_dz;
                double rdotv=rx*vx+ry*vy+rz*vz; if(rdotv<0)rdotv=0;
                double spec=pow(rdotv, sphere_spec[hit_sphere]);

                double sf = in_shadow ? 0.0 : 1.0;
                fr = sphere_r[hit_sphere]*(ambient+sf*light_intensity*ndotl)+sf*spec*0.5;
                fg = sphere_g[hit_sphere]*(ambient+sf*light_intensity*ndotl)+sf*spec*0.5;
                fb = sphere_b[hit_sphere]*(ambient+sf*light_intensity*ndotl)+sf*spec*0.5;
                if(fr>1)fr=1; if(fg>1)fg=1; if(fb>1)fb=1;
            }

            int idx = py*width+px;
            img_r[idx]=(int)(fr*255); img_g[idx]=(int)(fg*255); img_b[idx]=(int)(fb*255);
        }
    }

    int64_t r_sum=0, g_sum=0, b_sum=0;
    for (int i=0; i<total; i++) { r_sum+=img_r[i]; g_sum+=img_g[i]; b_sum+=img_b[i]; }

    int64_t checksum = r_sum*10000+g_sum*100+b_sum;
    printf("%lld\n", (long long)checksum);
    return 0;
}
