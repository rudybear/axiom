/*
 * axiom_rt_core.c -- Core runtime: command-line arguments and clock.
 *
 * Provides: axiom_set_args, axiom_get_argc, axiom_get_argv, axiom_clock_ns
 *
 * Included by axiom_rt.c -- do not compile separately.
 */

/* ── Command-line arguments ───────────────────────────────────────── */

static int    axiom_argc_val = 0;
static char **axiom_argv_val = NULL;

void axiom_set_args(int argc, char **argv) {
    axiom_argc_val = argc;
    axiom_argv_val = argv;
}

int axiom_get_argc(void) {
    return axiom_argc_val;
}

const char *axiom_get_argv(int i) {
    if (i >= 0 && i < axiom_argc_val && axiom_argv_val)
        return axiom_argv_val[i];
    return "";
}

/* ── Clock ────────────────────────────────────────────────────────── */

/* Return wall-clock time in nanoseconds (monotonic where possible). */
long long axiom_clock_ns(void) {
#if defined(_WIN32)
    /* On Windows, use QueryPerformanceCounter for high-resolution timing. */
    /* But to avoid pulling in <windows.h>, fall back to clock(). */
    return (long long)clock() * (1000000000LL / CLOCKS_PER_SEC);
#elif defined(__APPLE__)
    /* macOS: clock_gettime is available since 10.12 */
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000000000LL + (long long)ts.tv_nsec;
#else
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000000000LL + (long long)ts.tv_nsec;
#endif
}
