/*
 * axiom_rt_threading.c -- Threading, atomics, mutexes, and job system.
 *
 * Provides: axiom_thread_create, axiom_thread_join, axiom_atomic_*,
 *           axiom_mutex_*, axiom_jobs_init, axiom_job_dispatch, axiom_job_wait,
 *           axiom_jobs_shutdown, axiom_num_cores, axiom_job_dispatch_handle,
 *           axiom_job_dispatch_after, axiom_job_wait_handle
 *
 * Included by axiom_rt.c -- do not compile separately.
 */

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

/* ── Job handles & dependency graph ────────────────────────────────── */

#define AXIOM_MAX_JOB_HANDLES 256

typedef struct {
    volatile LONG complete;     /* 0 = pending, 1 = done */
    int           dependency;   /* -1 = none, else handle index to wait for */
    volatile LONG pending;      /* number of sub-jobs still running */
} AxiomJobHandle;

static AxiomJobHandle axiom_job_handles[AXIOM_MAX_JOB_HANDLES];
static volatile LONG  axiom_next_job_handle = 0;

static int axiom_alloc_handle(int dep) {
    LONG idx = InterlockedIncrement(&axiom_next_job_handle) - 1;
    idx = idx % AXIOM_MAX_JOB_HANDLES;
    axiom_job_handles[idx].complete = 0;
    axiom_job_handles[idx].dependency = dep;
    axiom_job_handles[idx].pending = 0;
    return (int)idx;
}

/* Wrapper that decrements the handle's pending counter and marks complete. */
typedef struct {
    AxiomJobFunc func;
    void        *data;
    int          start;
    int          end;
    int          handle;
} AxiomHandleJob;

static AxiomHandleJob axiom_handle_jobs[AXIOM_JOB_QUEUE_SIZE];
static volatile LONG  axiom_next_handle_job = 0;

static void axiom_handle_job_wrapper(void *arg, int start, int end) {
    AxiomHandleJob *hj = (AxiomHandleJob *)arg;
    hj->func(hj->data, start, end);
    if (InterlockedDecrement(&axiom_job_handles[hj->handle].pending) == 0) {
        InterlockedExchange(&axiom_job_handles[hj->handle].complete, 1);
        WakeAllConditionVariable(&axiom_job_done_cv);
    }
}

int axiom_job_dispatch_handle(AxiomJobFunc func, void *data, int total_items) {
    int handle = axiom_alloc_handle(-1);
    int chunk, start, end;
    if (total_items <= 0 || axiom_num_workers <= 0) {
        InterlockedExchange(&axiom_job_handles[handle].complete, 1);
        return handle;
    }
    chunk = (total_items + axiom_num_workers - 1) / axiom_num_workers;

    EnterCriticalSection(&axiom_job_queue_cs);
    for (start = 0; start < total_items; start += chunk) {
        LONG slot;
        end = start + chunk;
        if (end > total_items) end = total_items;

        InterlockedIncrement(&axiom_job_handles[handle].pending);

        slot = InterlockedIncrement(&axiom_next_handle_job) - 1;
        slot = slot % AXIOM_JOB_QUEUE_SIZE;
        axiom_handle_jobs[slot].func   = func;
        axiom_handle_jobs[slot].data   = data;
        axiom_handle_jobs[slot].start  = start;
        axiom_handle_jobs[slot].end    = end;
        axiom_handle_jobs[slot].handle = handle;

        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].func  = axiom_handle_job_wrapper;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].data  = &axiom_handle_jobs[slot];
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].start = start;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].end   = end;
        axiom_job_tail++;
        InterlockedIncrement((volatile LONG *)&axiom_jobs_pending);
    }
    LeaveCriticalSection(&axiom_job_queue_cs);
    WakeAllConditionVariable(&axiom_job_queue_cv);
    return handle;
}

int axiom_job_dispatch_after(AxiomJobFunc func, void *data, int total_items, int dep) {
    int handle;
    /* Wait for the dependency to complete first. */
    if (dep >= 0 && dep < AXIOM_MAX_JOB_HANDLES) {
        while (!axiom_job_handles[dep].complete) {
            EnterCriticalSection(&axiom_job_queue_cs);
            if (!axiom_job_handles[dep].complete) {
                SleepConditionVariableCS(&axiom_job_done_cv, &axiom_job_queue_cs, INFINITE);
            }
            LeaveCriticalSection(&axiom_job_queue_cs);
        }
    }
    handle = axiom_job_dispatch_handle(func, data, total_items);
    axiom_job_handles[handle].dependency = dep;
    return handle;
}

void axiom_job_wait_handle(int handle) {
    if (handle < 0 || handle >= AXIOM_MAX_JOB_HANDLES) return;
    EnterCriticalSection(&axiom_job_queue_cs);
    while (!axiom_job_handles[handle].complete) {
        SleepConditionVariableCS(&axiom_job_done_cv, &axiom_job_queue_cs, INFINITE);
    }
    LeaveCriticalSection(&axiom_job_queue_cs);
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

/* ── Job handles & dependency graph (POSIX) ────────────────────────── */

#define AXIOM_MAX_JOB_HANDLES 256

typedef struct {
    volatile int complete;      /* 0 = pending, 1 = done */
    int          dependency;    /* -1 = none, else handle index to wait for */
    volatile int pending;       /* number of sub-jobs still running */
} AxiomJobHandle;

static AxiomJobHandle axiom_job_handles[AXIOM_MAX_JOB_HANDLES];
static volatile int   axiom_next_job_handle = 0;

static int axiom_alloc_handle(int dep) {
    int idx = __atomic_fetch_add(&axiom_next_job_handle, 1, __ATOMIC_SEQ_CST);
    idx = idx % AXIOM_MAX_JOB_HANDLES;
    axiom_job_handles[idx].complete = 0;
    axiom_job_handles[idx].dependency = dep;
    axiom_job_handles[idx].pending = 0;
    return idx;
}

typedef struct {
    AxiomJobFunc func;
    void        *data;
    int          start;
    int          end;
    int          handle;
} AxiomHandleJob;

static AxiomHandleJob axiom_handle_jobs[AXIOM_JOB_QUEUE_SIZE];
static volatile int   axiom_next_handle_job = 0;

static void axiom_handle_job_wrapper(void *arg, int start, int end) {
    AxiomHandleJob *hj = (AxiomHandleJob *)arg;
    hj->func(hj->data, start, end);
    if (__atomic_sub_fetch(&axiom_job_handles[hj->handle].pending, 1, __ATOMIC_SEQ_CST) == 0) {
        __atomic_store_n(&axiom_job_handles[hj->handle].complete, 1, __ATOMIC_SEQ_CST);
        pthread_cond_broadcast(&axiom_job_done_cv);
    }
}

int axiom_job_dispatch_handle(AxiomJobFunc func, void *data, int total_items) {
    int handle = axiom_alloc_handle(-1);
    int chunk, start, end;
    if (total_items <= 0 || axiom_num_workers <= 0) {
        __atomic_store_n(&axiom_job_handles[handle].complete, 1, __ATOMIC_SEQ_CST);
        return handle;
    }
    chunk = (total_items + axiom_num_workers - 1) / axiom_num_workers;

    pthread_mutex_lock(&axiom_job_queue_mtx);
    for (start = 0; start < total_items; start += chunk) {
        int slot;
        end = start + chunk;
        if (end > total_items) end = total_items;

        __atomic_add_fetch(&axiom_job_handles[handle].pending, 1, __ATOMIC_SEQ_CST);

        slot = __atomic_fetch_add(&axiom_next_handle_job, 1, __ATOMIC_SEQ_CST);
        slot = slot % AXIOM_JOB_QUEUE_SIZE;
        axiom_handle_jobs[slot].func   = func;
        axiom_handle_jobs[slot].data   = data;
        axiom_handle_jobs[slot].start  = start;
        axiom_handle_jobs[slot].end    = end;
        axiom_handle_jobs[slot].handle = handle;

        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].func  = axiom_handle_job_wrapper;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].data  = &axiom_handle_jobs[slot];
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].start = start;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].end   = end;
        axiom_job_tail++;
        __atomic_add_fetch(&axiom_jobs_pending, 1, __ATOMIC_SEQ_CST);
    }
    pthread_mutex_unlock(&axiom_job_queue_mtx);
    pthread_cond_broadcast(&axiom_job_queue_cv);
    return handle;
}

int axiom_job_dispatch_after(AxiomJobFunc func, void *data, int total_items, int dep) {
    int handle;
    /* Wait for the dependency to complete first. */
    if (dep >= 0 && dep < AXIOM_MAX_JOB_HANDLES) {
        while (!__atomic_load_n(&axiom_job_handles[dep].complete, __ATOMIC_SEQ_CST)) {
            pthread_mutex_lock(&axiom_job_queue_mtx);
            if (!__atomic_load_n(&axiom_job_handles[dep].complete, __ATOMIC_SEQ_CST)) {
                pthread_cond_wait(&axiom_job_done_cv, &axiom_job_queue_mtx);
            }
            pthread_mutex_unlock(&axiom_job_queue_mtx);
        }
    }
    handle = axiom_job_dispatch_handle(func, data, total_items);
    axiom_job_handles[handle].dependency = dep;
    return handle;
}

void axiom_job_wait_handle(int handle) {
    if (handle < 0 || handle >= AXIOM_MAX_JOB_HANDLES) return;
    pthread_mutex_lock(&axiom_job_queue_mtx);
    while (!__atomic_load_n(&axiom_job_handles[handle].complete, __ATOMIC_SEQ_CST)) {
        pthread_cond_wait(&axiom_job_done_cv, &axiom_job_queue_mtx);
    }
    pthread_mutex_unlock(&axiom_job_queue_mtx);
}

#endif /* _WIN32 / POSIX -- threading */
