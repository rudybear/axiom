#include <stdio.h>
#include <stdint.h>

int main(void) {
    int capacity = 65536;
    // Using static arrays to match AXIOM stack allocation
    static int64_t keys[65536] = {0};
    static int64_t vals[65536] = {0};
    static int status[65536] = {0};  // 0=empty, 1=occupied, 2=tombstone

    int64_t hash_mult = 2654435761LL;
    int64_t mod32 = 4294967296LL;
    int size = 0;

    int64_t seed = 42;
    int64_t lcg_a = 1103515245LL;
    int64_t lcg_c = 12345LL;
    int64_t lcg_m = 2147483648LL;

    static int64_t inserted_keys[50000] = {0};

    // INSERT 50000
    for (int i = 0; i < 50000; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int64_t key = seed;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int64_t val = seed;
        inserted_keys[i] = key;

        int64_t h = (key * hash_mult) % mod32;
        int idx = (int)(h % (int64_t)capacity);

        int found = 0;
        for (int probe = 0; probe < capacity && !found; probe++) {
            int pos = (idx + probe) % capacity;
            if (status[pos] == 0 || status[pos] == 2) {
                keys[pos] = key; vals[pos] = val; status[pos] = 1;
                size++; found = 1;
            } else if (status[pos] == 1 && keys[pos] == key) {
                vals[pos] = val; found = 1;
            }
        }
    }

    // LOOKUP 50000
    int64_t lookup_sum = 0;
    int found_count = 0;
    for (int i = 0; i < 50000; i++) {
        int64_t key = inserted_keys[i];
        int64_t h = (key * hash_mult) % mod32;
        int idx = (int)(h % (int64_t)capacity);

        int found = 0;
        for (int probe = 0; probe < capacity && !found; probe++) {
            int pos = (idx + probe) % capacity;
            if (status[pos] == 0) { found = 2; }
            else if (status[pos] == 1 && keys[pos] == key) {
                lookup_sum += vals[pos]; found_count++; found = 1;
            }
        }
    }

    // DELETE 25000
    int delete_count = 0;
    for (int i = 0; i < 25000; i++) {
        int64_t key = inserted_keys[i * 2];
        int64_t h = (key * hash_mult) % mod32;
        int idx = (int)(h % (int64_t)capacity);

        int found = 0;
        for (int probe = 0; probe < capacity && !found; probe++) {
            int pos = (idx + probe) % capacity;
            if (status[pos] == 0) { found = 2; }
            else if (status[pos] == 1 && keys[pos] == key) {
                status[pos] = 2; delete_count++; size--; found = 1;
            }
        }
    }

    // Re-lookup
    int relookup_found = 0;
    for (int i = 0; i < 50000; i++) {
        int64_t key = inserted_keys[i];
        int64_t h = (key * hash_mult) % mod32;
        int idx = (int)(h % (int64_t)capacity);

        int found = 0;
        for (int probe = 0; probe < capacity && !found; probe++) {
            int pos = (idx + probe) % capacity;
            if (status[pos] == 0) { found = 2; }
            else if (status[pos] == 1 && keys[pos] == key) {
                relookup_found++; found = 1;
            }
        }
    }

    int64_t checksum = lookup_sum + (int64_t)found_count * 1000 + (int64_t)delete_count * 100 + (int64_t)relookup_found + (int64_t)size;
    printf("%lld\n", (long long)checksum);
    return 0;
}
