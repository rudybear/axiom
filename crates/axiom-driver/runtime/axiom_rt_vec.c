/*
 * axiom_rt_vec.c -- Dynamic array (Vec) runtime.
 *
 * Provides: axiom_vec_new, axiom_vec_push_i32, axiom_vec_push_f64,
 *           axiom_vec_get_i32, axiom_vec_get_f64, axiom_vec_set_i32,
 *           axiom_vec_set_f64, axiom_vec_len, axiom_vec_free
 *
 * Included by axiom_rt.c -- do not compile separately.
 */

/* ── Vec (Dynamic Array) ─────────────────────────────────────────── */
/*
 * Growable array backed by heap allocation.
 * Layout: { ptr data, i32 len, i32 cap, i32 elem_size }
 *
 * API:
 *   axiom_vec_new(elem_size)        -> ptr to vec header
 *   axiom_vec_push_i32(v, val)      -> push i32, auto-grow
 *   axiom_vec_push_f64(v, val)      -> push f64, auto-grow
 *   axiom_vec_get_i32(v, index)     -> indexed read (i32)
 *   axiom_vec_get_f64(v, index)     -> indexed read (f64)
 *   axiom_vec_set_i32(v, index, val)-> indexed write (i32)
 *   axiom_vec_set_f64(v, index, val)-> indexed write (f64)
 *   axiom_vec_len(v)                -> current length
 *   axiom_vec_free(v)               -> free data + header
 */

typedef struct {
    void *data;
    int   len;
    int   cap;
    int   elem_size;
} AxiomVec;

#define AXIOM_VEC_INITIAL_CAP 16

static void axiom_vec_grow(AxiomVec *v) {
    int new_cap = v->cap * 2;
    if (new_cap < AXIOM_VEC_INITIAL_CAP) new_cap = AXIOM_VEC_INITIAL_CAP;
    void *new_data = realloc(v->data, (size_t)new_cap * (size_t)v->elem_size);
    if (!new_data) {
        fprintf(stderr, "axiom_vec_grow: out of memory\n");
        abort();
    }
    v->data = new_data;
    v->cap  = new_cap;
}

void *axiom_vec_new(int elem_size) {
    AxiomVec *v = (AxiomVec *)malloc(sizeof(AxiomVec));
    if (!v) {
        fprintf(stderr, "axiom_vec_new: out of memory\n");
        abort();
    }
    v->len       = 0;
    v->cap       = AXIOM_VEC_INITIAL_CAP;
    v->elem_size = elem_size;
    v->data      = malloc((size_t)v->cap * (size_t)elem_size);
    if (!v->data) {
        fprintf(stderr, "axiom_vec_new: out of memory\n");
        free(v);
        abort();
    }
    return v;
}

void axiom_vec_push_i32(void *vec, int val) {
    AxiomVec *v = (AxiomVec *)vec;
    if (v->len >= v->cap) axiom_vec_grow(v);
    ((int *)v->data)[v->len] = val;
    v->len++;
}

void axiom_vec_push_f64(void *vec, double val) {
    AxiomVec *v = (AxiomVec *)vec;
    if (v->len >= v->cap) axiom_vec_grow(v);
    ((double *)v->data)[v->len] = val;
    v->len++;
}

int axiom_vec_get_i32(void *vec, int index) {
    AxiomVec *v = (AxiomVec *)vec;
    if (index < 0 || index >= v->len) {
        fprintf(stderr, "axiom_vec_get_i32: index %d out of bounds (len=%d)\n",
                index, v->len);
        abort();
    }
    return ((int *)v->data)[index];
}

double axiom_vec_get_f64(void *vec, int index) {
    AxiomVec *v = (AxiomVec *)vec;
    if (index < 0 || index >= v->len) {
        fprintf(stderr, "axiom_vec_get_f64: index %d out of bounds (len=%d)\n",
                index, v->len);
        abort();
    }
    return ((double *)v->data)[index];
}

void axiom_vec_set_i32(void *vec, int index, int val) {
    AxiomVec *v = (AxiomVec *)vec;
    if (index < 0 || index >= v->len) {
        fprintf(stderr, "axiom_vec_set_i32: index %d out of bounds (len=%d)\n",
                index, v->len);
        abort();
    }
    ((int *)v->data)[index] = val;
}

void axiom_vec_set_f64(void *vec, int index, double val) {
    AxiomVec *v = (AxiomVec *)vec;
    if (index < 0 || index >= v->len) {
        fprintf(stderr, "axiom_vec_set_f64: index %d out of bounds (len=%d)\n",
                index, v->len);
        abort();
    }
    ((double *)v->data)[index] = val;
}

int axiom_vec_len(void *vec) {
    AxiomVec *v = (AxiomVec *)vec;
    return v->len;
}

void axiom_vec_free(void *vec) {
    AxiomVec *v = (AxiomVec *)vec;
    if (v) {
        free(v->data);
        free(v);
    }
}
