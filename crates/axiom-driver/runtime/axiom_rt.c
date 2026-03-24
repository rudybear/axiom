/*
 * axiom_rt.c -- Tiny C runtime for AXIOM I/O primitives, coroutines,
 * threading primitives, a parallel job dispatch system, and a stub
 * rendering API (Vulkan FFI / Lux shader loading infrastructure).
 *
 * Provides file I/O, command-line arguments, a nanosecond clock,
 * stackful coroutines via OS fibers (Windows) or ucontext (POSIX),
 * thread creation/join, atomics, mutexes, a thread-pool job system,
 * and a renderer stub API designed for future Vulkan implementation.
 * Linked only when the AXIOM program uses runtime builtins.
 */

#define _CRT_SECURE_NO_WARNINGS
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <string.h>

#if !defined(_WIN32)
#include <unistd.h>
#endif

/* ── File I/O ─────────────────────────────────────────────────────── */

/* Read entire file into a malloc'd buffer.  Writes byte count to *out_size.
   Returns NULL on failure (and sets *out_size to 0). */
void *axiom_file_read(const char *path, long long *out_size) {
    FILE *f = fopen(path, "rb");
    if (!f) {
        *out_size = 0;
        return NULL;
    }
    fseek(f, 0, SEEK_END);
    long long sz = (long long)ftell(f);
    fseek(f, 0, SEEK_SET);
    void *buf = malloc((size_t)sz);
    if (!buf) {
        fclose(f);
        *out_size = 0;
        return NULL;
    }
    fread(buf, 1, (size_t)sz, f);
    fclose(f);
    *out_size = sz;
    return buf;
}

/* Write `len` bytes from `data` to file at `path` (binary mode). */
void axiom_file_write(const char *path, const void *data, long long len) {
    FILE *f = fopen(path, "wb");
    if (f) {
        fwrite(data, 1, (size_t)len, f);
        fclose(f);
    }
}

/* Return the size of the file in bytes, or -1 on error. */
long long axiom_file_size(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) return -1;
    fseek(f, 0, SEEK_END);
    long long sz = (long long)ftell(f);
    fclose(f);
    return sz;
}

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

/* ── Coroutines ──────────────────────────────────────────────────── */
/*
 * Stackful coroutines for game logic.  Each coroutine gets its own stack
 * and can yield/resume cooperatively.
 *
 * On Windows: implemented via Win32 fibers (CreateFiber / SwitchToFiber).
 * On POSIX:   implemented via ucontext (makecontext / swapcontext).
 *
 * API:
 *   axiom_coro_create(func, arg)  -> handle (i32)
 *   axiom_coro_resume(handle)     -> yielded value (i32), or -1 if done
 *   axiom_coro_yield(value)       -> suspends, resumes caller
 *   axiom_coro_is_done(handle)    -> 1 if finished, 0 if still alive
 *   axiom_coro_destroy(handle)    -> frees resources
 */

#define AXIOM_CORO_MAX   64
#define AXIOM_CORO_STACK (64 * 1024)  /* 64 KB per coroutine stack */

typedef void (*AxiomCoroFunc)(int);

typedef struct {
    int          active;         /* slot is in use */
    int          done;           /* coroutine has returned */
    int          yielded_value;  /* value passed via yield */
    AxiomCoroFunc func;          /* user function (takes i32 arg) */
    int          arg;            /* argument to the function */
#if defined(_WIN32)
    void        *fiber;          /* coroutine fiber */
    void        *caller_fiber;   /* caller (main) fiber */
#else
    /* POSIX ucontext */
    char        *stack;          /* heap-allocated stack */
    /* We store ucontext_t inline.  The header is included below. */
#endif
} AxiomCoro;

static AxiomCoro axiom_coros[AXIOM_CORO_MAX];
static int axiom_current_coro = -1;  /* handle of currently running coro */

/* ---- Platform-specific implementation --------------------------------- */

#if defined(_WIN32)

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

/* Fiber entry: runs the user function, then marks the coro done and yields. */
static void CALLBACK axiom_coro_fiber_entry(LPVOID param) {
    int handle = (int)(intptr_t)param;
    AxiomCoro *c = &axiom_coros[handle];
    c->func(c->arg);
    c->done = 1;
    c->yielded_value = -1;
    /* Switch back to the caller fiber; the coroutine is finished. */
    SwitchToFiber(c->caller_fiber);
}

int axiom_coro_create(AxiomCoroFunc func, int arg) {
    int i;
    for (i = 0; i < AXIOM_CORO_MAX; i++) {
        if (!axiom_coros[i].active) {
            AxiomCoro *c = &axiom_coros[i];
            memset(c, 0, sizeof(AxiomCoro));
            c->active = 1;
            c->done   = 0;
            c->func   = func;
            c->arg    = arg;
            c->yielded_value = 0;
            c->fiber  = CreateFiber(
                AXIOM_CORO_STACK,
                axiom_coro_fiber_entry,
                (LPVOID)(intptr_t)i
            );
            if (!c->fiber) {
                c->active = 0;
                return -1;
            }
            return i;
        }
    }
    return -1;  /* no free slots */
}

int axiom_coro_resume(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return -1;
    AxiomCoro *c = &axiom_coros[handle];
    if (!c->active || c->done) return -1;

    /* Convert main thread to fiber if not already done. */
    c->caller_fiber = ConvertThreadToFiber(NULL);
    if (!c->caller_fiber) {
        /* Already a fiber -- GetCurrentFiber instead. */
        c->caller_fiber = GetCurrentFiber();
    }

    axiom_current_coro = handle;
    SwitchToFiber(c->fiber);
    axiom_current_coro = -1;

    return c->yielded_value;
}

void axiom_coro_yield(int value) {
    if (axiom_current_coro < 0 || axiom_current_coro >= AXIOM_CORO_MAX) return;
    AxiomCoro *c = &axiom_coros[axiom_current_coro];
    c->yielded_value = value;
    SwitchToFiber(c->caller_fiber);
}

int axiom_coro_is_done(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return 1;
    return axiom_coros[handle].done;
}

void axiom_coro_destroy(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return;
    AxiomCoro *c = &axiom_coros[handle];
    if (c->fiber) {
        DeleteFiber(c->fiber);
        c->fiber = NULL;
    }
    c->active = 0;
}

#else /* POSIX ----------------------------------------------------------- */

#include <ucontext.h>

/* We store the ucontext alongside the coro struct via an auxiliary array
   to avoid bloating the struct definition (ucontext_t can be large). */
static ucontext_t axiom_coro_uctx[AXIOM_CORO_MAX];
static ucontext_t axiom_coro_caller_uctx[AXIOM_CORO_MAX];

/* ucontext entry: runs the user function, then marks done. */
static void axiom_coro_uctx_entry(int handle) {
    AxiomCoro *c = &axiom_coros[handle];
    c->func(c->arg);
    c->done = 1;
    c->yielded_value = -1;
    /* Swap back to caller; coroutine is finished. */
    swapcontext(&axiom_coro_uctx[handle], &axiom_coro_caller_uctx[handle]);
}

int axiom_coro_create(AxiomCoroFunc func, int arg) {
    int i;
    for (i = 0; i < AXIOM_CORO_MAX; i++) {
        if (!axiom_coros[i].active) {
            AxiomCoro *c = &axiom_coros[i];
            memset(c, 0, sizeof(AxiomCoro));
            c->active = 1;
            c->done   = 0;
            c->func   = func;
            c->arg    = arg;
            c->yielded_value = 0;

            c->stack = (char *)malloc(AXIOM_CORO_STACK);
            if (!c->stack) { c->active = 0; return -1; }

            getcontext(&axiom_coro_uctx[i]);
            axiom_coro_uctx[i].uc_stack.ss_sp   = c->stack;
            axiom_coro_uctx[i].uc_stack.ss_size  = AXIOM_CORO_STACK;
            axiom_coro_uctx[i].uc_link           = &axiom_coro_caller_uctx[i];
            makecontext(&axiom_coro_uctx[i],
                        (void (*)(void))axiom_coro_uctx_entry, 1, i);

            return i;
        }
    }
    return -1;
}

int axiom_coro_resume(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return -1;
    AxiomCoro *c = &axiom_coros[handle];
    if (!c->active || c->done) return -1;

    axiom_current_coro = handle;
    swapcontext(&axiom_coro_caller_uctx[handle], &axiom_coro_uctx[handle]);
    axiom_current_coro = -1;

    return c->yielded_value;
}

void axiom_coro_yield(int value) {
    if (axiom_current_coro < 0 || axiom_current_coro >= AXIOM_CORO_MAX) return;
    int h = axiom_current_coro;
    axiom_coros[h].yielded_value = value;
    swapcontext(&axiom_coro_uctx[h], &axiom_coro_caller_uctx[h]);
}

int axiom_coro_is_done(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return 1;
    return axiom_coros[handle].done;
}

void axiom_coro_destroy(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return;
    AxiomCoro *c = &axiom_coros[handle];
    if (c->stack) {
        free(c->stack);
        c->stack = NULL;
    }
    c->active = 0;
}

#endif /* _WIN32 / POSIX */

/* ── Threading primitives + Job system ──────────────────────────────── */
/*
 * Provides thread creation/join, atomics, mutexes, and a simple thread
 * pool with work-stealing job dispatch for data-parallel workloads.
 *
 * On Windows: Win32 threads, CRITICAL_SECTION, CONDITION_VARIABLE
 * On POSIX:   pthreads, pthread_mutex, pthread_cond
 */

typedef void (*AxiomThreadFunc)(void*);

/* ---- Platform-specific thread handles ---------------------------------- */

#if defined(_WIN32)

/* windows.h already included above for coroutines; guard just in case */
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef _WINDOWS_
#include <windows.h>
#endif

/* ── Thread create / join ─────────────────────────────────────────── */

#define AXIOM_MAX_THREADS 64

typedef struct {
    HANDLE      handle;
    int         active;
    AxiomThreadFunc func;
    void       *arg;
} AxiomThread;

static AxiomThread axiom_threads[AXIOM_MAX_THREADS];

static DWORD WINAPI axiom_thread_entry(LPVOID param) {
    int id = (int)(intptr_t)param;
    AxiomThread *t = &axiom_threads[id];
    t->func(t->arg);
    return 0;
}

int axiom_thread_create(AxiomThreadFunc func, void *arg) {
    int i;
    for (i = 0; i < AXIOM_MAX_THREADS; i++) {
        if (!axiom_threads[i].active) {
            axiom_threads[i].active = 1;
            axiom_threads[i].func   = func;
            axiom_threads[i].arg    = arg;
            axiom_threads[i].handle = CreateThread(
                NULL, 0, axiom_thread_entry, (LPVOID)(intptr_t)i, 0, NULL
            );
            if (!axiom_threads[i].handle) {
                axiom_threads[i].active = 0;
                return -1;
            }
            return i;
        }
    }
    return -1;
}

void axiom_thread_join(int handle) {
    if (handle < 0 || handle >= AXIOM_MAX_THREADS) return;
    AxiomThread *t = &axiom_threads[handle];
    if (!t->active) return;
    WaitForSingleObject(t->handle, INFINITE);
    CloseHandle(t->handle);
    t->handle = NULL;
    t->active = 0;
}

/* ── Atomics ──────────────────────────────────────────────────────── */

int axiom_atomic_load(volatile int *ptr)  {
    return InterlockedCompareExchange((volatile LONG *)ptr, 0, 0);
}

void axiom_atomic_store(volatile int *ptr, int val) {
    InterlockedExchange((volatile LONG *)ptr, val);
}

int axiom_atomic_add(volatile int *ptr, int val) {
    return InterlockedExchangeAdd((volatile LONG *)ptr, val);
}

int axiom_atomic_cas(volatile int *ptr, int expected, int desired) {
    return InterlockedCompareExchange((volatile LONG *)ptr, desired, expected);
}

/* ── Mutex ────────────────────────────────────────────────────────── */

void *axiom_mutex_create(void) {
    CRITICAL_SECTION *cs = (CRITICAL_SECTION *)malloc(sizeof(CRITICAL_SECTION));
    if (!cs) return NULL;
    InitializeCriticalSection(cs);
    return cs;
}

void axiom_mutex_lock(void *mtx)    { EnterCriticalSection((CRITICAL_SECTION *)mtx); }
void axiom_mutex_unlock(void *mtx)  { LeaveCriticalSection((CRITICAL_SECTION *)mtx); }

void axiom_mutex_destroy(void *mtx) {
    if (!mtx) return;
    DeleteCriticalSection((CRITICAL_SECTION *)mtx);
    free(mtx);
}

/* ── Job system (thread pool) ─────────────────────────────────────── */

#define AXIOM_JOB_MAX_WORKERS 32
#define AXIOM_JOB_QUEUE_SIZE  256

typedef void (*AxiomJobFunc)(void*, int, int);

typedef struct {
    AxiomJobFunc func;
    void        *data;
    int          start;
    int          end;
} AxiomJob;

static HANDLE            axiom_worker_threads[AXIOM_JOB_MAX_WORKERS];
static int               axiom_num_workers = 0;
static volatile int      axiom_jobs_running = 0;  /* 1 = pool active */

static CRITICAL_SECTION  axiom_job_queue_cs;
static CONDITION_VARIABLE axiom_job_queue_cv;
static CONDITION_VARIABLE axiom_job_done_cv;

static AxiomJob          axiom_job_queue[AXIOM_JOB_QUEUE_SIZE];
static volatile int      axiom_job_head = 0;
static volatile int      axiom_job_tail = 0;
static volatile int      axiom_jobs_pending = 0;

static DWORD WINAPI axiom_worker_func(LPVOID param) {
    (void)param;
    while (1) {
        AxiomJob job;
        int got = 0;

        EnterCriticalSection(&axiom_job_queue_cs);
        while (axiom_job_head == axiom_job_tail && axiom_jobs_running) {
            SleepConditionVariableCS(&axiom_job_queue_cv, &axiom_job_queue_cs, INFINITE);
        }
        if (!axiom_jobs_running && axiom_job_head == axiom_job_tail) {
            LeaveCriticalSection(&axiom_job_queue_cs);
            return 0;
        }
        if (axiom_job_head != axiom_job_tail) {
            job = axiom_job_queue[axiom_job_head % AXIOM_JOB_QUEUE_SIZE];
            axiom_job_head++;
            got = 1;
        }
        LeaveCriticalSection(&axiom_job_queue_cs);

        if (got) {
            job.func(job.data, job.start, job.end);
            InterlockedDecrement((volatile LONG *)&axiom_jobs_pending);
            WakeConditionVariable(&axiom_job_done_cv);
        }
    }
}

void axiom_jobs_init(int num_workers) {
    int i;
    if (num_workers < 1) num_workers = 1;
    if (num_workers > AXIOM_JOB_MAX_WORKERS) num_workers = AXIOM_JOB_MAX_WORKERS;

    InitializeCriticalSection(&axiom_job_queue_cs);
    InitializeConditionVariable(&axiom_job_queue_cv);
    InitializeConditionVariable(&axiom_job_done_cv);

    axiom_job_head = 0;
    axiom_job_tail = 0;
    axiom_jobs_pending = 0;
    axiom_jobs_running = 1;
    axiom_num_workers = num_workers;

    for (i = 0; i < num_workers; i++) {
        axiom_worker_threads[i] = CreateThread(
            NULL, 0, axiom_worker_func, NULL, 0, NULL
        );
    }
}

void axiom_job_dispatch(AxiomJobFunc func, void *data, int total_items) {
    int chunk, start, end;
    if (total_items <= 0 || axiom_num_workers <= 0) return;

    chunk = (total_items + axiom_num_workers - 1) / axiom_num_workers;

    EnterCriticalSection(&axiom_job_queue_cs);
    for (start = 0; start < total_items; start += chunk) {
        end = start + chunk;
        if (end > total_items) end = total_items;

        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].func  = func;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].data  = data;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].start = start;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].end   = end;
        axiom_job_tail++;
        InterlockedIncrement((volatile LONG *)&axiom_jobs_pending);
    }
    LeaveCriticalSection(&axiom_job_queue_cs);
    WakeAllConditionVariable(&axiom_job_queue_cv);
}

void axiom_job_wait(void) {
    EnterCriticalSection(&axiom_job_queue_cs);
    while (axiom_jobs_pending > 0) {
        SleepConditionVariableCS(&axiom_job_done_cv, &axiom_job_queue_cs, INFINITE);
    }
    LeaveCriticalSection(&axiom_job_queue_cs);
}

void axiom_jobs_shutdown(void) {
    int i;
    EnterCriticalSection(&axiom_job_queue_cs);
    axiom_jobs_running = 0;
    LeaveCriticalSection(&axiom_job_queue_cs);
    WakeAllConditionVariable(&axiom_job_queue_cv);

    for (i = 0; i < axiom_num_workers; i++) {
        WaitForSingleObject(axiom_worker_threads[i], INFINITE);
        CloseHandle(axiom_worker_threads[i]);
        axiom_worker_threads[i] = NULL;
    }
    DeleteCriticalSection(&axiom_job_queue_cs);
    axiom_num_workers = 0;
}

int axiom_num_cores(void) {
    SYSTEM_INFO si;
    GetSystemInfo(&si);
    return (int)si.dwNumberOfProcessors;
}

#else /* POSIX ----------------------------------------------------------- */

#include <pthread.h>

/* ── Thread create / join ─────────────────────────────────────────── */

#define AXIOM_MAX_THREADS 64

typedef struct {
    pthread_t    thread;
    int          active;
    AxiomThreadFunc func;
    void        *arg;
} AxiomThread;

static AxiomThread axiom_threads[AXIOM_MAX_THREADS];

static void *axiom_thread_entry(void *param) {
    int id = (int)(intptr_t)param;
    AxiomThread *t = &axiom_threads[id];
    t->func(t->arg);
    return NULL;
}

int axiom_thread_create(AxiomThreadFunc func, void *arg) {
    int i;
    for (i = 0; i < AXIOM_MAX_THREADS; i++) {
        if (!axiom_threads[i].active) {
            axiom_threads[i].active = 1;
            axiom_threads[i].func   = func;
            axiom_threads[i].arg    = arg;
            if (pthread_create(&axiom_threads[i].thread, NULL, axiom_thread_entry,
                               (void *)(intptr_t)i) != 0) {
                axiom_threads[i].active = 0;
                return -1;
            }
            return i;
        }
    }
    return -1;
}

void axiom_thread_join(int handle) {
    if (handle < 0 || handle >= AXIOM_MAX_THREADS) return;
    AxiomThread *t = &axiom_threads[handle];
    if (!t->active) return;
    pthread_join(t->thread, NULL);
    t->active = 0;
}

/* ── Atomics ──────────────────────────────────────────────────────── */

int axiom_atomic_load(volatile int *ptr) {
    return __atomic_load_n(ptr, __ATOMIC_SEQ_CST);
}

void axiom_atomic_store(volatile int *ptr, int val) {
    __atomic_store_n(ptr, val, __ATOMIC_SEQ_CST);
}

int axiom_atomic_add(volatile int *ptr, int val) {
    return __atomic_fetch_add(ptr, val, __ATOMIC_SEQ_CST);
}

int axiom_atomic_cas(volatile int *ptr, int expected, int desired) {
    int old = expected;
    __atomic_compare_exchange_n(ptr, &old, desired, 0,
                                __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
    return old;
}

/* ── Mutex ────────────────────────────────────────────────────────── */

void *axiom_mutex_create(void) {
    pthread_mutex_t *mtx = (pthread_mutex_t *)malloc(sizeof(pthread_mutex_t));
    if (!mtx) return NULL;
    pthread_mutex_init(mtx, NULL);
    return mtx;
}

void axiom_mutex_lock(void *mtx)    { pthread_mutex_lock((pthread_mutex_t *)mtx); }
void axiom_mutex_unlock(void *mtx)  { pthread_mutex_unlock((pthread_mutex_t *)mtx); }

void axiom_mutex_destroy(void *mtx) {
    if (!mtx) return;
    pthread_mutex_destroy((pthread_mutex_t *)mtx);
    free(mtx);
}

/* ── Job system (thread pool) ─────────────────────────────────────── */

#define AXIOM_JOB_MAX_WORKERS 32
#define AXIOM_JOB_QUEUE_SIZE  256

typedef void (*AxiomJobFunc)(void*, int, int);

typedef struct {
    AxiomJobFunc func;
    void        *data;
    int          start;
    int          end;
} AxiomJob;

static pthread_t         axiom_worker_threads[AXIOM_JOB_MAX_WORKERS];
static int               axiom_num_workers = 0;
static volatile int      axiom_jobs_running = 0;

static pthread_mutex_t   axiom_job_queue_mtx = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t    axiom_job_queue_cv  = PTHREAD_COND_INITIALIZER;
static pthread_cond_t    axiom_job_done_cv   = PTHREAD_COND_INITIALIZER;

static AxiomJob          axiom_job_queue[AXIOM_JOB_QUEUE_SIZE];
static volatile int      axiom_job_head = 0;
static volatile int      axiom_job_tail = 0;
static volatile int      axiom_jobs_pending = 0;

static void *axiom_worker_func(void *param) {
    (void)param;
    while (1) {
        AxiomJob job;
        int got = 0;

        pthread_mutex_lock(&axiom_job_queue_mtx);
        while (axiom_job_head == axiom_job_tail && axiom_jobs_running) {
            pthread_cond_wait(&axiom_job_queue_cv, &axiom_job_queue_mtx);
        }
        if (!axiom_jobs_running && axiom_job_head == axiom_job_tail) {
            pthread_mutex_unlock(&axiom_job_queue_mtx);
            return NULL;
        }
        if (axiom_job_head != axiom_job_tail) {
            job = axiom_job_queue[axiom_job_head % AXIOM_JOB_QUEUE_SIZE];
            axiom_job_head++;
            got = 1;
        }
        pthread_mutex_unlock(&axiom_job_queue_mtx);

        if (got) {
            job.func(job.data, job.start, job.end);
            __atomic_sub_fetch(&axiom_jobs_pending, 1, __ATOMIC_SEQ_CST);
            pthread_cond_signal(&axiom_job_done_cv);
        }
    }
}

void axiom_jobs_init(int num_workers) {
    int i;
    if (num_workers < 1) num_workers = 1;
    if (num_workers > AXIOM_JOB_MAX_WORKERS) num_workers = AXIOM_JOB_MAX_WORKERS;

    pthread_mutex_init(&axiom_job_queue_mtx, NULL);
    pthread_cond_init(&axiom_job_queue_cv, NULL);
    pthread_cond_init(&axiom_job_done_cv, NULL);

    axiom_job_head = 0;
    axiom_job_tail = 0;
    axiom_jobs_pending = 0;
    axiom_jobs_running = 1;
    axiom_num_workers = num_workers;

    for (i = 0; i < num_workers; i++) {
        pthread_create(&axiom_worker_threads[i], NULL, axiom_worker_func, NULL);
    }
}

void axiom_job_dispatch(AxiomJobFunc func, void *data, int total_items) {
    int chunk, start, end;
    if (total_items <= 0 || axiom_num_workers <= 0) return;

    chunk = (total_items + axiom_num_workers - 1) / axiom_num_workers;

    pthread_mutex_lock(&axiom_job_queue_mtx);
    for (start = 0; start < total_items; start += chunk) {
        end = start + chunk;
        if (end > total_items) end = total_items;

        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].func  = func;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].data  = data;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].start = start;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].end   = end;
        axiom_job_tail++;
        __atomic_add_fetch(&axiom_jobs_pending, 1, __ATOMIC_SEQ_CST);
    }
    pthread_mutex_unlock(&axiom_job_queue_mtx);
    pthread_cond_broadcast(&axiom_job_queue_cv);
}

void axiom_job_wait(void) {
    pthread_mutex_lock(&axiom_job_queue_mtx);
    while (axiom_jobs_pending > 0) {
        pthread_cond_wait(&axiom_job_done_cv, &axiom_job_queue_mtx);
    }
    pthread_mutex_unlock(&axiom_job_queue_mtx);
}

void axiom_jobs_shutdown(void) {
    int i;
    pthread_mutex_lock(&axiom_job_queue_mtx);
    axiom_jobs_running = 0;
    pthread_mutex_unlock(&axiom_job_queue_mtx);
    pthread_cond_broadcast(&axiom_job_queue_cv);

    for (i = 0; i < axiom_num_workers; i++) {
        pthread_join(axiom_worker_threads[i], NULL);
    }
    pthread_mutex_destroy(&axiom_job_queue_mtx);
    pthread_cond_destroy(&axiom_job_queue_cv);
    pthread_cond_destroy(&axiom_job_done_cv);
    axiom_num_workers = 0;
}

int axiom_num_cores(void) {
    long n = sysconf(_SC_NPROCESSORS_ONLN);
    return n > 0 ? (int)n : 1;
}

#endif /* _WIN32 / POSIX -- threading */

/* ── Renderer API ────────────────────────────────────────────────────── */
/*
 * Provides a rendering API that AXIOM programs call to create windows,
 * load SPIR-V shaders (compiled by Lux), build pipelines, and draw
 * geometry.
 *
 * On Windows: real windowed renderer using Win32 API + software
 * rasterization.  Creates an actual window, maintains a pixel
 * framebuffer, blits via StretchDIBits.  Implements edge-function
 * triangle rasterization and point drawing.
 *
 * On other platforms: headless stub that prints lifecycle events.
 *
 * API summary:
 *   axiom_renderer_create(w, h, title) -> ptr     Create a renderer context
 *   axiom_renderer_destroy(r)                      Destroy the renderer
 *   axiom_renderer_begin_frame(r) -> i32           Begin a frame (1=ok, 0=fail)
 *   axiom_renderer_end_frame(r)                    End a frame (present)
 *   axiom_renderer_should_close(r) -> i32          1 if window should close
 *   axiom_renderer_clear(r, color)                 Clear framebuffer
 *   axiom_renderer_draw_triangles(r, pos, col, n)  Draw n vertices as tris
 *   axiom_renderer_draw_points(r, x, y, col, n)   Draw n colored points
 *   axiom_renderer_get_time(r) -> f64              Elapsed time in seconds
 *   axiom_shader_load(r, path, stage) -> ptr       Load SPIR-V shader module
 *   axiom_pipeline_create(r, vert, frag) -> ptr    Create a graphics pipeline
 *   axiom_renderer_bind_pipeline(r, p)             Bind a pipeline for drawing
 */

/* Shader stage constants (matches Vulkan VkShaderStageFlagBits layout). */
#define AXIOM_SHADER_STAGE_VERTEX   0
#define AXIOM_SHADER_STAGE_FRAGMENT 1

/* Maximum number of loaded shader modules. */
#define AXIOM_MAX_SHADERS   64

/* Maximum number of pipelines. */
#define AXIOM_MAX_PIPELINES 32

/* ---- Shader module (loaded SPIR-V) ------------------------------------- */

typedef struct {
    int   active;
    int   stage;          /* 0 = vertex, 1 = fragment */
    char  path[512];
} AxiomShaderModule;

/* ---- Graphics pipeline ------------------------------------------------- */

typedef struct {
    int   active;
    int   vert_index;     /* index into shader_modules[] */
    int   frag_index;     /* index into shader_modules[] */
} AxiomPipeline;

static AxiomShaderModule axiom_shader_modules[AXIOM_MAX_SHADERS];
static AxiomPipeline     axiom_pipelines[AXIOM_MAX_PIPELINES];

/* ======================================================================== */
/* Win32 windowed software renderer                                         */
/* ======================================================================== */

#if defined(_WIN32)

/* windows.h is already included above for coroutines/threading. */

/* ---- Renderer state ---------------------------------------------------- */

typedef struct {
    int           width;
    int           height;
    char          title[256];
    int           should_close;
    int           frame_count;
    long long     start_time_ns;
    /* Win32 windowing */
    HWND          hwnd;
    HDC           hdc;
    BITMAPINFO    bmi;
    /* Software framebuffer: BGRA pixel array (0xAARRGGBB in little-endian) */
    unsigned int *framebuffer;
} AxiomRenderer;

/* Global renderer pointer for the window procedure callback. */
static AxiomRenderer *axiom_renderer_global = NULL;

static LRESULT CALLBACK axiom_wnd_proc(HWND hwnd, UINT msg,
                                        WPARAM wParam, LPARAM lParam) {
    switch (msg) {
    case WM_CLOSE:
    case WM_DESTROY:
        if (axiom_renderer_global) {
            axiom_renderer_global->should_close = 1;
        }
        return 0;
    case WM_KEYDOWN:
        if (wParam == VK_ESCAPE) {
            if (axiom_renderer_global) {
                axiom_renderer_global->should_close = 1;
            }
        }
        return 0;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

void *axiom_renderer_create(int width, int height, const char *title) {
    AxiomRenderer *r = (AxiomRenderer *)calloc(1, sizeof(AxiomRenderer));
    if (!r) return NULL;

    r->width  = width;
    r->height = height;
    r->should_close = 0;
    r->frame_count  = 0;
    r->start_time_ns = axiom_clock_ns();

    /* Copy title. */
    if (title) {
        size_t len = strlen(title);
        if (len >= sizeof(r->title)) len = sizeof(r->title) - 1;
        memcpy(r->title, title, len);
        r->title[len] = '\0';
    } else {
        strcpy(r->title, "AXIOM");
    }

    /* Allocate framebuffer. */
    r->framebuffer = (unsigned int *)calloc((size_t)(width * height),
                                            sizeof(unsigned int));
    if (!r->framebuffer) {
        free(r);
        return NULL;
    }

    /* Register window class (idempotent -- RegisterClassW returns 0 if
       already registered, but that is fine). */
    WNDCLASSW wc;
    memset(&wc, 0, sizeof(wc));
    wc.lpfnWndProc   = axiom_wnd_proc;
    wc.hInstance      = GetModuleHandleW(NULL);
    wc.lpszClassName  = L"AxiomRendererClass";
    wc.hCursor        = LoadCursorW(NULL, (LPCWSTR)IDC_ARROW);
    wc.hbrBackground  = (HBRUSH)GetStockObject(BLACK_BRUSH);
    RegisterClassW(&wc);

    /* Convert title to wide string. */
    wchar_t wtitle[256];
    MultiByteToWideChar(CP_UTF8, 0, r->title, -1, wtitle, 256);

    /* Compute window rect that gives us the desired *client* area. */
    RECT wr = { 0, 0, width, height };
    AdjustWindowRectEx(&wr, WS_OVERLAPPEDWINDOW, FALSE, 0);

    r->hwnd = CreateWindowExW(
        0, L"AxiomRendererClass", wtitle,
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        CW_USEDEFAULT, CW_USEDEFAULT,
        wr.right - wr.left, wr.bottom - wr.top,
        NULL, NULL, GetModuleHandleW(NULL), NULL
    );

    if (!r->hwnd) {
        free(r->framebuffer);
        free(r);
        return NULL;
    }

    r->hdc = GetDC(r->hwnd);
    axiom_renderer_global = r;

    /* Setup BITMAPINFO for StretchDIBits blitting. */
    memset(&r->bmi, 0, sizeof(BITMAPINFO));
    r->bmi.bmiHeader.biSize        = sizeof(BITMAPINFOHEADER);
    r->bmi.bmiHeader.biWidth       = width;
    r->bmi.bmiHeader.biHeight      = -height; /* negative = top-down */
    r->bmi.bmiHeader.biPlanes      = 1;
    r->bmi.bmiHeader.biBitCount    = 32;
    r->bmi.bmiHeader.biCompression = BI_RGB;

    printf("[AXIOM Renderer] Created %dx%d window: \"%s\" (Win32 software)\n",
           width, height, r->title);

    return r;
}

void axiom_renderer_destroy(void *renderer) {
    if (!renderer) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;

    printf("[AXIOM Renderer] Destroyed after %d frames: \"%s\"\n",
           r->frame_count, r->title);

    if (r->hdc && r->hwnd) {
        ReleaseDC(r->hwnd, r->hdc);
    }
    if (r->hwnd) {
        DestroyWindow(r->hwnd);
    }
    if (axiom_renderer_global == r) {
        axiom_renderer_global = NULL;
    }
    free(r->framebuffer);
    free(r);
}

/* ---- Frame operations -------------------------------------------------- */

int axiom_renderer_begin_frame(void *renderer) {
    if (!renderer) return 0;
    AxiomRenderer *r = (AxiomRenderer *)renderer;

    /* Pump Win32 message queue so the window stays responsive. */
    MSG msg;
    while (PeekMessageW(&msg, NULL, 0, 0, PM_REMOVE)) {
        if (msg.message == WM_QUIT) {
            r->should_close = 1;
            return 0;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    if (r->should_close) return 0;
    return 1;
}

void axiom_renderer_end_frame(void *renderer) {
    if (!renderer) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;

    /* Blit the software framebuffer to the window. */
    StretchDIBits(
        r->hdc,
        0, 0, r->width, r->height,          /* dest rect */
        0, 0, r->width, r->height,          /* src rect */
        r->framebuffer,
        &r->bmi,
        DIB_RGB_COLORS,
        SRCCOPY
    );
    GdiFlush();

    r->frame_count++;

    /* Print progress for first few frames and periodically. */
    if (r->frame_count <= 3 || r->frame_count % 50 == 0) {
        printf("[AXIOM Renderer] Frame %d presented\n", r->frame_count);
    }
}

int axiom_renderer_should_close(void *renderer) {
    if (!renderer) return 1;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    return r->should_close;
}

/* ---- Clear -------------------------------------------------------------- */

void axiom_renderer_clear(void *renderer, unsigned int color) {
    if (!renderer) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    int total = r->width * r->height;
    int i;
    /* Fast path for black (color == 0). */
    if (color == 0) {
        memset(r->framebuffer, 0, (size_t)total * sizeof(unsigned int));
    } else {
        for (i = 0; i < total; i++) {
            r->framebuffer[i] = color;
        }
    }
}

/* ---- Drawing: points ---------------------------------------------------- */

/* Draw colored points.  x_arr and y_arr are arrays of f64 positions,
   colors is an array of u32 (0xRRGGBB), count is the number of points. */
void axiom_renderer_draw_points(void *renderer,
                                const double *x_arr,
                                const double *y_arr,
                                const unsigned int *colors,
                                int count) {
    if (!renderer || !x_arr || !y_arr || !colors) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    int w = r->width;
    int h = r->height;
    unsigned int *fb = r->framebuffer;
    int i;

    for (i = 0; i < count; i++) {
        int px = (int)(x_arr[i] + 0.5);
        int py = (int)(y_arr[i] + 0.5);
        /* Draw a 2x2 point for visibility. */
        if (px >= 0 && px < w - 1 && py >= 0 && py < h - 1) {
            unsigned int c = colors[i] | 0xFF000000u; /* ensure opaque */
            fb[py * w + px]           = c;
            fb[py * w + px + 1]       = c;
            fb[(py + 1) * w + px]     = c;
            fb[(py + 1) * w + px + 1] = c;
        } else if (px >= 0 && px < w && py >= 0 && py < h) {
            /* Edge pixel: draw single point. */
            fb[py * w + px] = colors[i] | 0xFF000000u;
        }
    }
}

/* ---- Drawing: triangles ------------------------------------------------- */

/* Helper: integer min/max of 3 values. */
static int axiom_min3i(int a, int b, int c) {
    int m = a < b ? a : b;
    return m < c ? m : c;
}
static int axiom_max3i(int a, int b, int c) {
    int m = a > b ? a : b;
    return m > c ? m : c;
}

/* Edge function for triangle rasterization.
   Returns positive if (px,py) is on the left side of edge (ax,ay)->(bx,by). */
static int axiom_edge_func(int ax, int ay, int bx, int by, int px, int py) {
    return (bx - ax) * (py - ay) - (by - ay) * (px - ax);
}

void axiom_renderer_draw_triangles(void *renderer,
                                   const float *positions,
                                   const float *colors_f,
                                   int vertex_count) {
    if (!renderer || !positions) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    int w = r->width;
    int h = r->height;
    unsigned int *fb = r->framebuffer;

    /* Each triangle is 3 vertices, each vertex has 2 floats (x, y)
       in the positions array, and 3 floats (r, g, b) in the colors array. */
    int tri_count = vertex_count / 3;
    int t;
    for (t = 0; t < tri_count; t++) {
        int base_p = t * 6;  /* 3 vertices * 2 coords */
        int base_c = t * 9;  /* 3 vertices * 3 color channels */

        int x0 = (int)(positions[base_p + 0] + 0.5f);
        int y0 = (int)(positions[base_p + 1] + 0.5f);
        int x1 = (int)(positions[base_p + 2] + 0.5f);
        int y1 = (int)(positions[base_p + 3] + 0.5f);
        int x2 = (int)(positions[base_p + 4] + 0.5f);
        int y2 = (int)(positions[base_p + 5] + 0.5f);

        /* Flat color from first vertex (for simplicity). */
        unsigned int cr = 255, cg = 255, cb = 255;
        if (colors_f) {
            cr = (unsigned int)(colors_f[base_c + 0] * 255.0f);
            cg = (unsigned int)(colors_f[base_c + 1] * 255.0f);
            cb = (unsigned int)(colors_f[base_c + 2] * 255.0f);
            if (cr > 255) cr = 255;
            if (cg > 255) cg = 255;
            if (cb > 255) cb = 255;
        }
        unsigned int color = 0xFF000000u | (cr << 16) | (cg << 8) | cb;

        /* Bounding box, clipped to screen. */
        int minX = axiom_min3i(x0, x1, x2);
        int minY = axiom_min3i(y0, y1, y2);
        int maxX = axiom_max3i(x0, x1, x2);
        int maxY = axiom_max3i(y0, y1, y2);
        if (minX < 0) minX = 0;
        if (minY < 0) minY = 0;
        if (maxX >= w) maxX = w - 1;
        if (maxY >= h) maxY = h - 1;

        /* Compute twice the triangle area (for winding check). */
        int area2 = axiom_edge_func(x0, y0, x1, y1, x2, y2);
        if (area2 == 0) continue; /* degenerate triangle */

        /* Rasterize via edge functions. */
        int py, px;
        for (py = minY; py <= maxY; py++) {
            for (px = minX; px <= maxX; px++) {
                int e0 = axiom_edge_func(x0, y0, x1, y1, px, py);
                int e1 = axiom_edge_func(x1, y1, x2, y2, px, py);
                int e2 = axiom_edge_func(x2, y2, x0, y0, px, py);
                /* Accept pixel if all edge functions have same sign. */
                if ((e0 >= 0 && e1 >= 0 && e2 >= 0) ||
                    (e0 <= 0 && e1 <= 0 && e2 <= 0)) {
                    fb[py * w + px] = color;
                }
            }
        }
    }
}

/* ---- Time -------------------------------------------------------------- */

double axiom_renderer_get_time(void *renderer) {
    if (!renderer) return 0.0;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    long long now = axiom_clock_ns();
    return (double)(now - r->start_time_ns) / 1000000000.0;
}

/* ---- Shader loading (SPIR-V from Lux) --------------------------------- */

void *axiom_shader_load(void *renderer, const char *spv_path, int stage) {
    if (!renderer || !spv_path) return NULL;
    (void)renderer;

    int i;
    for (i = 0; i < AXIOM_MAX_SHADERS; i++) {
        if (!axiom_shader_modules[i].active) {
            AxiomShaderModule *s = &axiom_shader_modules[i];
            s->active = 1;
            s->stage  = stage;

            size_t len = strlen(spv_path);
            if (len >= sizeof(s->path)) len = sizeof(s->path) - 1;
            memcpy(s->path, spv_path, len);
            s->path[len] = '\0';

            const char *stage_name = (stage == AXIOM_SHADER_STAGE_VERTEX)
                                         ? "vertex"
                                         : (stage == AXIOM_SHADER_STAGE_FRAGMENT)
                                               ? "fragment"
                                               : "unknown";

            printf("[AXIOM Renderer] Loaded %s shader: \"%s\" (slot %d)\n",
                   stage_name, spv_path, i);
            return s;
        }
    }

    printf("[AXIOM Renderer] ERROR: no free shader slots\n");
    return NULL;
}

/* ---- Pipeline creation ------------------------------------------------- */

void *axiom_pipeline_create(void *renderer, void *vert_shader, void *frag_shader) {
    if (!renderer) return NULL;
    (void)renderer;

    int i;
    for (i = 0; i < AXIOM_MAX_PIPELINES; i++) {
        if (!axiom_pipelines[i].active) {
            AxiomPipeline *p = &axiom_pipelines[i];
            p->active = 1;

            if (vert_shader) {
                p->vert_index = (int)(((AxiomShaderModule *)vert_shader)
                                      - axiom_shader_modules);
            } else {
                p->vert_index = -1;
            }
            if (frag_shader) {
                p->frag_index = (int)(((AxiomShaderModule *)frag_shader)
                                      - axiom_shader_modules);
            } else {
                p->frag_index = -1;
            }

            printf("[AXIOM Renderer] Created pipeline %d "
                   "(vert=%d, frag=%d)\n",
                   i, p->vert_index, p->frag_index);
            return p;
        }
    }

    printf("[AXIOM Renderer] ERROR: no free pipeline slots\n");
    return NULL;
}

void axiom_renderer_bind_pipeline(void *renderer, void *pipeline) {
    if (!renderer || !pipeline) return;
    (void)renderer;
    (void)pipeline;
}

#else /* !_WIN32 -- headless stub for non-Windows platforms */

/* ---- Renderer state (headless stub) ------------------------------------ */

typedef struct {
    int   width;
    int   height;
    char  title[256];
    int   should_close;
    int   frame_count;
    long long start_time_ns;
} AxiomRenderer;

void *axiom_renderer_create(int width, int height, const char *title) {
    AxiomRenderer *r = (AxiomRenderer *)calloc(1, sizeof(AxiomRenderer));
    if (!r) return NULL;

    r->width  = width;
    r->height = height;
    r->should_close = 0;
    r->frame_count  = 0;
    r->start_time_ns = axiom_clock_ns();

    if (title) {
        size_t len = strlen(title);
        if (len >= sizeof(r->title)) len = sizeof(r->title) - 1;
        memcpy(r->title, title, len);
        r->title[len] = '\0';
    } else {
        r->title[0] = '\0';
    }

    printf("[AXIOM Renderer] Created %dx%d window: \"%s\" (headless stub)\n",
           width, height, r->title);
    return r;
}

void axiom_renderer_destroy(void *renderer) {
    if (!renderer) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    printf("[AXIOM Renderer] Destroyed after %d frames: \"%s\"\n",
           r->frame_count, r->title);
    free(r);
}

int axiom_renderer_begin_frame(void *renderer) {
    if (!renderer) return 0;
    return 1;
}

void axiom_renderer_end_frame(void *renderer) {
    if (!renderer) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    r->frame_count++;
    if (r->frame_count <= 3 || r->frame_count % 50 == 0) {
        printf("[AXIOM Renderer] Frame %d complete\n", r->frame_count);
    }
}

int axiom_renderer_should_close(void *renderer) {
    if (!renderer) return 1;
    return ((AxiomRenderer *)renderer)->should_close;
}

void axiom_renderer_clear(void *renderer, unsigned int color) {
    (void)renderer; (void)color;
}

void axiom_renderer_draw_points(void *renderer,
                                const double *x_arr,
                                const double *y_arr,
                                const unsigned int *colors,
                                int count) {
    (void)renderer; (void)x_arr; (void)y_arr; (void)colors; (void)count;
}

void axiom_renderer_draw_triangles(void *renderer,
                                   const float *positions,
                                   const float *colors,
                                   int vertex_count) {
    if (!renderer) return;
    (void)positions; (void)colors;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    if (r->frame_count == 0) {
        printf("[AXIOM Renderer] draw_triangles: %d vertices (stub)\n",
               vertex_count);
    }
}

double axiom_renderer_get_time(void *renderer) {
    if (!renderer) return 0.0;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    long long now = axiom_clock_ns();
    return (double)(now - r->start_time_ns) / 1000000000.0;
}

void *axiom_shader_load(void *renderer, const char *spv_path, int stage) {
    if (!renderer || !spv_path) return NULL;
    (void)renderer;
    int i;
    for (i = 0; i < AXIOM_MAX_SHADERS; i++) {
        if (!axiom_shader_modules[i].active) {
            AxiomShaderModule *s = &axiom_shader_modules[i];
            s->active = 1;
            s->stage  = stage;
            size_t len = strlen(spv_path);
            if (len >= sizeof(s->path)) len = sizeof(s->path) - 1;
            memcpy(s->path, spv_path, len);
            s->path[len] = '\0';
            printf("[AXIOM Renderer] Loaded %s shader: \"%s\" (slot %d, stub)\n",
                   (stage == 0) ? "vertex" : "fragment", spv_path, i);
            return s;
        }
    }
    return NULL;
}

void *axiom_pipeline_create(void *renderer, void *vert_shader, void *frag_shader) {
    if (!renderer) return NULL;
    (void)renderer;
    int i;
    for (i = 0; i < AXIOM_MAX_PIPELINES; i++) {
        if (!axiom_pipelines[i].active) {
            AxiomPipeline *p = &axiom_pipelines[i];
            p->active = 1;
            p->vert_index = vert_shader
                ? (int)(((AxiomShaderModule *)vert_shader) - axiom_shader_modules)
                : -1;
            p->frag_index = frag_shader
                ? (int)(((AxiomShaderModule *)frag_shader) - axiom_shader_modules)
                : -1;
            printf("[AXIOM Renderer] Created pipeline %d (stub)\n", i);
            return p;
        }
    }
    return NULL;
}

void axiom_renderer_bind_pipeline(void *renderer, void *pipeline) {
    (void)renderer; (void)pipeline;
}

#endif /* _WIN32 / headless stub */
