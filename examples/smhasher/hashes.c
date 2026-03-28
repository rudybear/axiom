// SMHasher hash functions -- C reference implementation
// Compile: gcc -O3 -march=native -ffast-math -o hashes_c hashes.c

#include <stdio.h>
#include <stdint.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
static uint64_t clock_ns(void) {
    LARGE_INTEGER freq, cnt;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&cnt);
    return (uint64_t)((double)cnt.QuadPart / freq.QuadPart * 1e9);
}
#else
#include <time.h>
static uint64_t clock_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + ts.tv_nsec;
}
#endif

// Helper
static inline uint32_t read32_le(const uint8_t *p) {
    return (uint32_t)p[0] | ((uint32_t)p[1]<<8) | ((uint32_t)p[2]<<16) | ((uint32_t)p[3]<<24);
}
static inline uint64_t read64_le(const uint8_t *p) {
    return (uint64_t)p[0] | ((uint64_t)p[1]<<8) | ((uint64_t)p[2]<<16) | ((uint64_t)p[3]<<24) |
           ((uint64_t)p[4]<<32) | ((uint64_t)p[5]<<40) | ((uint64_t)p[6]<<48) | ((uint64_t)p[7]<<56);
}
static inline uint32_t rotl32(uint32_t x, int r) { return (x << r) | (x >> (32 - r)); }
static inline uint64_t rotl64(uint64_t x, int r) { return (x << r) | (x >> (64 - r)); }

// ============= MurmurHash3_x86_32 =============
static inline uint32_t murmur3_fmix32(uint32_t h) {
    h ^= h >> 16; h *= 0x85ebca6b;
    h ^= h >> 13; h *= 0xc2b2ae35;
    h ^= h >> 16; return h;
}

uint32_t murmur3_x86_32(const uint8_t *data, int len, uint32_t seed) {
    uint32_t h = seed;
    int nblocks = len / 4;
    for (int i = 0; i < nblocks; i++) {
        uint32_t k = read32_le(data + i*4);
        k *= 0xcc9e2d51; k = rotl32(k, 15); k *= 0x1b873593;
        h ^= k; h = rotl32(h, 13); h = h * 5 + 0xe6546b64;
    }
    const uint8_t *tail = data + nblocks * 4;
    uint32_t k1 = 0;
    int rem = len - nblocks * 4;
    if (rem >= 3) k1 ^= (uint32_t)tail[2] << 16;
    if (rem >= 2) k1 ^= (uint32_t)tail[1] << 8;
    if (rem >= 1) { k1 ^= tail[0]; k1 *= 0xcc9e2d51; k1 = rotl32(k1, 15); k1 *= 0x1b873593; h ^= k1; }
    h ^= len;
    return murmur3_fmix32(h);
}

// ============= FNV-1a 32-bit =============
uint32_t fnv1a_32(const uint8_t *data, int len) {
    uint32_t h = 0x811c9dc5;
    for (int i = 0; i < len; i++) {
        h ^= data[i];
        h *= 0x01000193;
    }
    return h;
}

// ============= wyhash =============
#define WY_P0 0x2d358dccaa6c78a5ULL
#define WY_P1 0x8bb84b93962eacc9ULL
#define WY_P2 0x4b33a62ed433d4a3ULL
#define WY_P3 0x4d5a2da51de1aa47ULL

static inline uint64_t wymum(uint64_t a, uint64_t b) {
    uint64_t lo = a * b;
    uint64_t hi = (a >> 32) * b ^ a * (b >> 32);
    return lo ^ hi;
}
static inline uint64_t wymix(uint64_t a, uint64_t b) {
    return wymum(a ^ WY_P0, b ^ WY_P1);
}
static inline uint64_t wyr3(const uint8_t *p, int len) {
    return ((uint64_t)p[0] << 16) | ((uint64_t)p[len>>1] << 8) | p[len-1];
}

uint64_t wyhash(const uint8_t *data, int len, uint64_t seed) {
    uint64_t s = seed;
    int p = 0, remaining = len;
    if (remaining > 16) {
        uint64_t s1 = s, s2 = s;
        while (remaining > 16) {
            s = wymix(read64_le(data+p)^WY_P1, read64_le(data+p+8)^s);
            s1 = wymix(read64_le(data+p)^WY_P2, read64_le(data+p+8)^s1);
            p += 16; remaining -= 16;
        }
        s ^= s1 ^ s2;
    }
    if (remaining >= 4) {
        if (remaining <= 8) {
            uint64_t a = read32_le(data+p);
            uint64_t b = read32_le(data+p+remaining-4);
            s = wymix(a^WY_P1, b^s);
        } else {
            uint64_t a = read64_le(data+p);
            uint64_t b = read64_le(data+p+remaining-8);
            s = wymix(a^WY_P1, b^s);
        }
    } else if (remaining > 0) {
        uint64_t a = wyr3(data+p, remaining);
        s = wymix(a^WY_P1, s);
    }
    s ^= (uint64_t)len;
    s = wymum(s, WY_P1);
    s ^= wymum(s, WY_P0);
    return s;
}

// ============= CityHash32 =============
static inline uint32_t city_fmix(uint32_t h) {
    h ^= h >> 16; h *= 0x85ebca6b;
    h ^= h >> 13; h *= 0xc2b2ae35;
    h ^= h >> 16; return h;
}
static inline uint32_t city_mur(uint32_t a, uint32_t h) {
    a *= 0xcc9e2d51; a = rotl32(a, 17); a *= 0x1b873593;
    h ^= a; h = rotl32(h, 19); h = h * 5 + 0xe6546b64;
    return h;
}

uint32_t cityhash32(const uint8_t *data, int len) {
    if (len <= 4) {
        uint32_t b = 0;
        if (len >= 1) b ^= data[0];
        if (len >= 2) b ^= (uint32_t)data[1] << 8;
        if (len >= 3) b ^= (uint32_t)data[2] << 16;
        if (len >= 4) b ^= (uint32_t)data[3] << 24;
        return city_fmix(b * 0xcc9e2d51);
    }
    if (len <= 12) {
        uint32_t a = len, b = len*5, c = 9, d = b;
        a += read32_le(data); b += read32_le(data+len-4); c += read32_le(data+((len>>1)&4));
        return city_fmix(city_mur(c, city_mur(b, city_mur(a, d))) ^ d);
    }
    if (len <= 24) {
        uint32_t a=read32_le(data+(len>>1)-4), b=read32_le(data+4);
        uint32_t c=read32_le(data+len-8), d=read32_le(data+(len>>1));
        uint32_t e=read32_le(data), f=read32_le(data+len-4);
        uint32_t h = len;
        return city_fmix(city_mur(f, city_mur(e, city_mur(d, city_mur(c, city_mur(b, city_mur(a, h)))))));
    }
    // > 24
    uint32_t h = len, g = len * 0xcc9e2d51, f = g;
    uint32_t a0 = rotl32(read32_le(data+len-4)*0xcc9e2d51, 17)*0x1b873593;
    uint32_t a1 = rotl32(read32_le(data+len-8)*0xcc9e2d51, 17)*0x1b873593;
    uint32_t a2 = rotl32(read32_le(data+len-16)*0xcc9e2d51, 17)*0x1b873593;
    uint32_t a3 = rotl32(read32_le(data+len-12)*0xcc9e2d51, 17)*0x1b873593;
    uint32_t a4 = rotl32(read32_le(data+len-20)*0xcc9e2d51, 17)*0x1b873593;
    h ^= a0; h = rotl32(h,19); h = h*5+0xe6546b64;
    h ^= a2; h = rotl32(h,19); h = h*5+0xe6546b64;
    g ^= a1; g = rotl32(g,19); g = g*5+0xe6546b64;
    g ^= a3; g = rotl32(g,19); g = g*5+0xe6546b64;
    f += a4; f = rotl32(f,19); f = f*5+0xe6546b64;

    int iters = (len-1)/20;
    int pp = 0;
    for (int i = 0; i < iters; i++) {
        uint32_t aa = rotl32(read32_le(data+pp)*0xcc9e2d51, 17)*0x1b873593;
        uint32_t bb = read32_le(data+pp+4);
        uint32_t cc = rotl32(read32_le(data+pp+8)*0xcc9e2d51, 17)*0x1b873593;
        uint32_t dd = rotl32(read32_le(data+pp+12)*0xcc9e2d51, 17)*0x1b873593;
        uint32_t ee = rotl32(read32_le(data+pp+16)*0xcc9e2d51, 17)*0x1b873593;
        h ^= aa; h = rotl32(h,18); h = h*5+0xe6546b64;
        f += bb; f = rotl32(f,19); f *= 0xcc9e2d51;
        g += cc; g = rotl32(g,18); g = g*5+0xe6546b64;
        h ^= dd+ee; h = rotl32(h,19); h = h*5+0xe6546b64;
        g ^= ee;
        uint32_t tmp = h; h = f; f = g; g = tmp;
        pp += 20;
    }
    g = rotl32(g,11)*0xcc9e2d51; g = rotl32(g,17)*0xcc9e2d51;
    f = rotl32(f,11)*0xcc9e2d51; f = rotl32(f,17)*0xcc9e2d51;
    h = rotl32(h+g,19); h = h*5+0xe6546b64; h = rotl32(h,17)*0xcc9e2d51;
    h = rotl32(h+f,19); h = h*5+0xe6546b64; h = rotl32(h,17)*0xcc9e2d51;
    return h;
}

int main(void) {
    const char *test = "Hello, World!";
    int len = 13;

    printf("MurmurHash3_x86_32(\"Hello, World!\", seed=0): %u\n",
           murmur3_x86_32((const uint8_t*)test, len, 0));
    printf("FNV-1a_32(\"Hello, World!\"): %u\n",
           fnv1a_32((const uint8_t*)test, len));
    printf("wyhash(\"Hello, World!\", seed=0): %llu\n",
           (unsigned long long)wyhash((const uint8_t*)test, len, 0));
    printf("CityHash32(\"Hello, World!\"): %u\n",
           cityhash32((const uint8_t*)test, len));

    // Benchmark
    int iterations = 10000000;
    uint8_t bench_data[64];
    for (int i = 0; i < 64; i++) bench_data[i] = (uint8_t)((i*17+13) & 0xFF);

    uint32_t cm = 0;
    uint64_t t0 = clock_ns();
    for (int i = 0; i < iterations; i++)
        cm += murmur3_x86_32(bench_data, 64, i);
    uint64_t t1 = clock_ns();
    printf("MurmurHash3: %llu ns, checksum=%u\n", (unsigned long long)(t1-t0), cm);

    uint32_t cf = 0;
    uint64_t t2 = clock_ns();
    for (int i = 0; i < iterations; i++)
        cf += fnv1a_32(bench_data, 64);
    uint64_t t3 = clock_ns();
    printf("FNV-1a: %llu ns, checksum=%u\n", (unsigned long long)(t3-t2), cf);

    uint64_t cw = 0;
    uint64_t t4 = clock_ns();
    for (int i = 0; i < iterations; i++)
        cw += wyhash(bench_data, 64, i);
    uint64_t t5 = clock_ns();
    printf("wyhash: %llu ns, checksum=%llu\n", (unsigned long long)(t5-t4), (unsigned long long)cw);

    uint32_t cc = 0;
    uint64_t t6 = clock_ns();
    for (int i = 0; i < iterations; i++)
        cc += cityhash32(bench_data, 64);
    uint64_t t7 = clock_ns();
    printf("CityHash32: %llu ns, checksum=%u\n", (unsigned long long)(t7-t6), cc);

    return 0;
}
