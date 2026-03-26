// raytracer.c - A clean raytracer in C for AXIOM conversion study
// Based on tinyray patterns: free functions, explicit structs, Phong lighting
// Renders 600x600 image of Cornell-box-like scene, outputs pixel checksum
//
// Build: gcc -O2 -lm raytracer.c -o raytracer
// Expected: deterministic checksum output

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

// ============================================================
// Vec3 - 3D vector as struct + free functions
// ============================================================
typedef struct { double x, y, z; } Vec3;

Vec3 vec3_new(double x, double y, double z) {
    Vec3 v; v.x = x; v.y = y; v.z = z; return v;
}

Vec3 vec3_add(Vec3 a, Vec3 b) {
    return vec3_new(a.x + b.x, a.y + b.y, a.z + b.z);
}

Vec3 vec3_sub(Vec3 a, Vec3 b) {
    return vec3_new(a.x - b.x, a.y - b.y, a.z - b.z);
}

Vec3 vec3_mul_scalar(Vec3 a, double s) {
    return vec3_new(a.x * s, a.y * s, a.z * s);
}

Vec3 vec3_mul(Vec3 a, Vec3 b) {
    return vec3_new(a.x * b.x, a.y * b.y, a.z * b.z);
}

double vec3_dot(Vec3 a, Vec3 b) {
    return a.x * b.x + a.y * b.y + a.z * b.z;
}

double vec3_length(Vec3 v) {
    return sqrt(v.x * v.x + v.y * v.y + v.z * v.z);
}

Vec3 vec3_normalize(Vec3 v) {
    double len = vec3_length(v);
    if (len > 0.0) {
        double inv = 1.0 / len;
        return vec3_new(v.x * inv, v.y * inv, v.z * inv);
    }
    return v;
}

Vec3 vec3_reflect(Vec3 incident, Vec3 normal) {
    // I - 2 * dot(I, N) * N
    double d = vec3_dot(incident, normal);
    return vec3_sub(incident, vec3_mul_scalar(normal, 2.0 * d));
}

double vec3_max_component(Vec3 v) {
    double m = v.x;
    if (v.y > m) m = v.y;
    if (v.z > m) m = v.z;
    return m;
}

double clamp_f64(double x, double lo, double hi) {
    if (x < lo) return lo;
    if (x > hi) return hi;
    return x;
}

// ============================================================
// Material
// ============================================================
typedef struct {
    Vec3 color;       // diffuse color (0-1 range)
    double specular;  // specular exponent (-1 = no specular)
    double reflective; // reflectivity (0-1)
} Material;

Material material_new(Vec3 color, double specular, double reflective) {
    Material m;
    m.color = color;
    m.specular = specular;
    m.reflective = reflective;
    return m;
}

// ============================================================
// Sphere
// ============================================================
typedef struct {
    Vec3 center;
    double radius;
    Material material;
} Sphere;

Sphere sphere_new(Vec3 center, double radius, Material material) {
    Sphere s;
    s.center = center;
    s.radius = radius;
    s.material = material;
    return s;
}

// ============================================================
// Ray
// ============================================================
typedef struct {
    Vec3 origin;
    Vec3 direction;
} Ray;

Ray ray_new(Vec3 origin, Vec3 direction) {
    Ray r;
    r.origin = origin;
    r.direction = direction;
    return r;
}

// ============================================================
// Light types (enum encoded as int)
// ============================================================
#define LIGHT_AMBIENT     0
#define LIGHT_POINT       1
#define LIGHT_DIRECTIONAL 2

typedef struct {
    int type;
    Vec3 position;   // for point lights
    Vec3 direction;  // for directional lights
    double intensity;
} Light;

Light light_ambient(double intensity) {
    Light l;
    l.type = LIGHT_AMBIENT;
    l.position = vec3_new(0, 0, 0);
    l.direction = vec3_new(0, 0, 0);
    l.intensity = intensity;
    return l;
}

Light light_point(double intensity, Vec3 position) {
    Light l;
    l.type = LIGHT_POINT;
    l.position = position;
    l.direction = vec3_new(0, 0, 0);
    l.intensity = intensity;
    return l;
}

Light light_directional(double intensity, Vec3 direction) {
    Light l;
    l.type = LIGHT_DIRECTIONAL;
    l.position = vec3_new(0, 0, 0);
    l.direction = direction;
    l.intensity = intensity;
    return l;
}

// ============================================================
// Scene
// ============================================================
#define NUM_SPHERES 4
#define NUM_LIGHTS  3
#define MAX_REFLECT_DEPTH 3
#define WIDTH  600
#define HEIGHT 600
#define INF 1e20
#define EPSILON 1e-4

static Sphere spheres[NUM_SPHERES];
static Light  lights[NUM_LIGHTS];

void scene_init(void) {
    // Red sphere (left)
    spheres[0] = sphere_new(
        vec3_new(-2.0, 0.0, 6.0), 1.0,
        material_new(vec3_new(1.0, 0.2, 0.2), 500.0, 0.3)
    );
    // Blue sphere (center)
    spheres[1] = sphere_new(
        vec3_new(0.0, -0.5, 5.0), 1.5,
        material_new(vec3_new(0.2, 0.4, 1.0), 200.0, 0.2)
    );
    // White sphere (right)
    spheres[2] = sphere_new(
        vec3_new(2.5, 0.5, 8.0), 2.0,
        material_new(vec3_new(0.9, 0.9, 0.9), 1000.0, 0.5)
    );
    // Ground sphere (huge, acts as floor)
    spheres[3] = sphere_new(
        vec3_new(0.0, -5001.0, 0.0), 5000.0,
        material_new(vec3_new(0.4, 0.4, 0.3), 50.0, 0.1)
    );

    // Ambient light
    lights[0] = light_ambient(0.2);
    // Point light (upper left)
    lights[1] = light_point(0.6, vec3_new(-5.0, 5.0, -2.0));
    // Directional light (from above-right)
    lights[2] = light_directional(0.3, vec3_new(1.0, 4.0, -2.0));
}

// ============================================================
// Ray-Sphere intersection
// Returns distance to hit, or INF if no hit
// ============================================================
double sphere_intersect(Sphere *s, Ray *r) {
    Vec3 oc = vec3_sub(r->origin, s->center);
    double a = vec3_dot(r->direction, r->direction);
    double b = 2.0 * vec3_dot(oc, r->direction);
    double c = vec3_dot(oc, oc) - s->radius * s->radius;
    double disc = b * b - 4.0 * a * c;

    if (disc < 0.0) return INF;

    double sqrt_disc = sqrt(disc);
    double t1 = (-b - sqrt_disc) / (2.0 * a);
    double t2 = (-b + sqrt_disc) / (2.0 * a);

    if (t1 > EPSILON) return t1;
    if (t2 > EPSILON) return t2;
    return INF;
}

// ============================================================
// Find closest sphere intersection
// Returns sphere index, or -1 if no hit
// ============================================================
int scene_intersect(Ray *r, double min_t, double max_t, double *out_t) {
    int closest_id = -1;
    double closest_t = max_t;

    for (int i = 0; i < NUM_SPHERES; i++) {
        double t = sphere_intersect(&spheres[i], r);
        if (t >= min_t && t < closest_t) {
            closest_t = t;
            closest_id = i;
        }
    }

    *out_t = closest_t;
    return closest_id;
}

// ============================================================
// Shadow check - is point in shadow from a light?
// ============================================================
int in_shadow(Vec3 point, Vec3 light_dir, double max_dist) {
    Ray shadow_ray = ray_new(point, light_dir);
    for (int i = 0; i < NUM_SPHERES; i++) {
        double t = sphere_intersect(&spheres[i], &shadow_ray);
        if (t > EPSILON && t < max_dist) {
            return 1;
        }
    }
    return 0;
}

// ============================================================
// Compute lighting at a point (Phong model)
// ============================================================
double compute_lighting(Vec3 point, Vec3 normal, Vec3 view, double specular) {
    double intensity = 0.0;

    for (int i = 0; i < NUM_LIGHTS; i++) {
        if (lights[i].type == LIGHT_AMBIENT) {
            intensity += lights[i].intensity;
            continue;
        }

        Vec3 light_dir;
        double max_dist;

        if (lights[i].type == LIGHT_POINT) {
            light_dir = vec3_sub(lights[i].position, point);
            max_dist = vec3_length(light_dir);
            light_dir = vec3_normalize(light_dir);
        } else {
            light_dir = vec3_normalize(lights[i].direction);
            max_dist = INF;
        }

        // Shadow check
        if (in_shadow(point, light_dir, max_dist)) {
            continue;
        }

        // Diffuse
        double n_dot_l = vec3_dot(normal, light_dir);
        if (n_dot_l > 0.0) {
            intensity += lights[i].intensity * n_dot_l;
        }

        // Specular
        if (specular > 0.0) {
            Vec3 reflect_dir = vec3_reflect(vec3_mul_scalar(light_dir, -1.0), normal);
            double r_dot_v = vec3_dot(reflect_dir, view);
            if (r_dot_v > 0.0) {
                intensity += lights[i].intensity * pow(r_dot_v / (vec3_length(reflect_dir) * vec3_length(view)), specular);
            }
        }
    }

    return intensity;
}

// ============================================================
// Trace a ray (with reflections)
// ============================================================
Vec3 trace_ray(Ray *r, double min_t, double max_t, int depth) {
    double t;
    int id = scene_intersect(r, min_t, max_t, &t);

    if (id < 0) {
        // Sky gradient
        double sky_t = 0.5 * (r->direction.y + 1.0);
        return vec3_add(
            vec3_mul_scalar(vec3_new(1.0, 1.0, 1.0), 1.0 - sky_t),
            vec3_mul_scalar(vec3_new(0.5, 0.7, 1.0), sky_t)
        );
    }

    // Hit point and normal
    Vec3 hit_point = vec3_add(r->origin, vec3_mul_scalar(r->direction, t));
    Vec3 normal = vec3_normalize(vec3_sub(hit_point, spheres[id].center));

    // Offset point slightly along normal to avoid self-intersection
    Vec3 offset_point = vec3_add(hit_point, vec3_mul_scalar(normal, EPSILON));

    // Compute local color with lighting
    double light_intensity = compute_lighting(
        offset_point, normal,
        vec3_mul_scalar(r->direction, -1.0),
        spheres[id].material.specular
    );

    Vec3 local_color = vec3_mul_scalar(spheres[id].material.color, light_intensity);

    // Reflections
    double reflective = spheres[id].material.reflective;
    if (depth <= 0 || reflective <= 0.0) {
        return local_color;
    }

    Vec3 reflect_dir = vec3_reflect(r->direction, normal);
    Ray reflect_ray = ray_new(offset_point, reflect_dir);
    Vec3 reflected_color = trace_ray(&reflect_ray, EPSILON, INF, depth - 1);

    // Blend local + reflected
    return vec3_add(
        vec3_mul_scalar(local_color, 1.0 - reflective),
        vec3_mul_scalar(reflected_color, reflective)
    );
}

// ============================================================
// Main - render and compute checksum
// ============================================================
int main(void) {
    scene_init();

    Vec3 camera_pos = vec3_new(0.0, 1.0, -3.0);
    double viewport_size = 1.0;
    double projection_plane_z = 1.0;

    // Pixel buffer (r,g,b as integers 0-255)
    int64_t checksum = 0;
    int pixel_count = 0;

    for (int py = 0; py < HEIGHT; py++) {
        for (int px = 0; px < WIDTH; px++) {
            // Convert pixel to viewport coordinates
            double vx = (px - WIDTH / 2.0) * viewport_size / WIDTH;
            double vy = -(py - HEIGHT / 2.0) * viewport_size / HEIGHT;
            double vz = projection_plane_z;

            Vec3 direction = vec3_normalize(vec3_new(vx, vy, vz));
            Ray r = ray_new(camera_pos, direction);

            Vec3 color = trace_ray(&r, EPSILON, INF, MAX_REFLECT_DEPTH);

            // Clamp and convert to 0-255
            int ir = (int)(clamp_f64(color.x, 0.0, 1.0) * 255.0 + 0.5);
            int ig = (int)(clamp_f64(color.y, 0.0, 1.0) * 255.0 + 0.5);
            int ib = (int)(clamp_f64(color.z, 0.0, 1.0) * 255.0 + 0.5);

            checksum += (int64_t)(ir * 17 + ig * 31 + ib * 53);
            checksum ^= (int64_t)((px * 7919 + py * 104729) & 0xFFFFFF);
            pixel_count++;
        }
    }

    printf("Rendered %d pixels\n", pixel_count);
    printf("Checksum: %lld\n", checksum);
    return 0;
}
