//! LLVM IR text generator.
//!
//! Generates valid LLVM IR text from an [`HirModule`]. Uses an alloca-based
//! strategy for local variables (not SSA phi nodes). LLVM's `mem2reg` pass
//! promotes allocas to SSA registers when optimizing.
//!
//! # Example
//!
//! ```ignore
//! let hir_module = axiom_hir::lower(&ast_module)?;
//! let llvm_ir = axiom_codegen::codegen(&hir_module)?;
//! println!("{llvm_ir}");
//! ```

use std::collections::HashMap;
use std::fmt::Write as _;

use axiom_hir::{
    BinOp, HirAnnotation, HirAnnotationKind, HirBlock, HirExpr, HirExprKind,
    HirExternFunction, HirFunction, HirModule, HirParam, HirStmt, HirStmtKind, HirStruct,
    HirType, InlineHint, PrimitiveType, UnaryOp,
};

use crate::error::CodegenError;

/// Information about an alloca'd variable.
#[derive(Debug, Clone)]
struct VarInfo {
    /// The LLVM name of the alloca (e.g., `%a`).
    alloca_name: String,
    /// The LLVM type (e.g., `i64`).
    llvm_type: String,
    /// For array variables: the element type and fixed size.
    /// Local arrays have `alloca [N x T]` — the alloca IS the array pointer.
    /// Array parameters have `alloca ptr` — the alloca stores a pointer to the array.
    array_info: Option<ArrayVarInfo>,
}

/// Extra info for array variables, enabling correct GEP codegen.
#[derive(Debug, Clone)]
struct ArrayVarInfo {
    /// LLVM element type (e.g., `i32`).
    element_type: String,
    /// Fixed array size.
    size: usize,
    /// If true, the alloca stores the array directly (`alloca [N x T]`).
    /// If false, the alloca stores a pointer to the array (`alloca ptr`).
    is_local: bool,
}

/// Optimization-relevant annotation flags for a function.
#[derive(Debug, Clone, Default)]
struct FuncAnnotations {
    /// Whether the function is annotated with `@pure`.
    is_pure: bool,
    /// Whether the function is annotated with `@const`.
    is_const: bool,
    /// Whether the function is annotated with `@vectorizable`.
    is_vectorizable: bool,
    /// Whether the function has any array/pointer parameter reads (for memory attribute).
    reads_arg_memory: bool,
    /// Whether the function body writes through pointers (ptr_write_* calls).
    writes_arg_memory: bool,
    /// Whether the function is annotated with `@lifetime(scope)`.
    is_lifetime_scope: bool,
    /// Inline hint from `@inline(always|never|hint)`.
    inline_hint: Option<InlineHint>,
}

/// Information about a function's signature.
#[derive(Debug, Clone)]
struct FuncInfo {
    /// LLVM return type.
    return_type: String,
    /// LLVM parameter types.
    param_types: Vec<String>,
    /// Whether this function uses fastcc (internal, non-main, non-export).
    uses_fastcc: bool,
    /// Optimization annotation flags.
    annotations: FuncAnnotations,
}

/// Codegen metadata for a user-defined struct type.
#[derive(Debug, Clone)]
struct StructInfo {
    /// LLVM named type (e.g., `%struct.Vec3`).
    llvm_name: String,
    /// Field names and their LLVM types, in declaration order.
    fields: Vec<(String, String)>,
    /// Total size in bytes (sum of field sizes, no padding for now).
    total_size: u64,
}

/// Result of emitting an expression -- an SSA register name or immediate.
#[derive(Debug, Clone)]
struct LlvmValue {
    /// The register name (e.g., `%3`) or literal (e.g., `42`).
    reg: String,
    /// The LLVM type (e.g., `i32`, `i64`, `float`, `double`).
    ty: String,
}

/// Mutable state for IR generation.
struct CodegenContext {
    /// Accumulated LLVM IR text.
    output: String,
    /// Next SSA register number for unnamed temporaries.
    next_reg: u32,
    /// Next label number for basic blocks.
    next_label: u32,
    /// Maps variable names to their alloca info.
    variables: HashMap<String, VarInfo>,
    /// Maps function names to their signatures.
    functions: HashMap<String, FuncInfo>,
    /// Collected string constants for global declarations.
    string_literals: Vec<String>,
    /// Whether an i64 format string is needed.
    needs_printf_i64: bool,
    /// Whether an i32 format string is needed.
    needs_printf_i32: bool,
    /// Whether an f64 format string is needed.
    needs_printf_f64: bool,
    /// Whether puts is needed.
    needs_puts: bool,
    /// Whether printf is needed.
    needs_printf: bool,
    /// Whether the `@llvm.sqrt.f64` intrinsic is needed.
    needs_sqrt_f64: bool,
    /// Whether the `@llvm.pow.f64` intrinsic is needed.
    needs_pow_f64: bool,
    /// Whether the `@llvm.abs.i32` intrinsic is needed.
    needs_abs_i32: bool,
    /// Whether the `@llvm.fabs.f64` intrinsic is needed.
    needs_fabs_f64: bool,
    /// Whether the `@llvm.memset.p0.i64` intrinsic is needed.
    needs_memset: bool,
    /// Whether the `@llvm.fshl.i32` intrinsic is needed (rotate left).
    needs_fshl_i32: bool,
    /// Whether the `@llvm.fshr.i32` intrinsic is needed (rotate right).
    needs_fshr_i32: bool,
    /// Whether `@malloc` is needed (heap_alloc).
    needs_malloc: bool,
    /// Whether `@calloc` is needed (heap_alloc_zeroed).
    needs_calloc: bool,
    /// Whether `@realloc` is needed (heap_realloc).
    needs_realloc: bool,
    /// Whether `@free` is needed (heap_free).
    needs_free: bool,
    /// Whether arena builtins are used (arena_create/arena_alloc/etc.).
    /// When true, `@malloc` and `@free` declarations are also emitted.
    needs_arena: bool,
    /// Whether the AXIOM C runtime is needed (file I/O, clock, argc/argv, coroutines).
    /// When true, `axiom_rt.c` must be linked alongside the `.ll` file.
    needs_runtime: bool,
    /// Whether coroutine builtins are used (coro_create/coro_resume/etc.).
    /// When true, coroutine extern declarations are emitted and the runtime is linked.
    needs_coroutines: bool,
    /// Whether threading/job-system builtins are used (thread_create, jobs_init, etc.).
    /// When true, threading extern declarations are emitted and the runtime is linked.
    needs_threading: bool,
    /// Whether renderer builtins are used (renderer_create, shader_load, etc.).
    /// When true, renderer extern declarations are emitted and the runtime is linked.
    needs_renderer: bool,
    /// Whether GPU PBR/glTF builtins are used (gpu_init, gpu_load_gltf, etc.).
    /// When true, gpu_* extern declarations are emitted and axiom-renderer is linked.
    needs_gpu: bool,
    /// Whether Vec (dynamic array) builtins are used (vec_new, vec_push_*, etc.).
    /// When true, Vec extern declarations are emitted and the runtime is linked.
    needs_vec: bool,
    /// Whether string builtins are used (string_from_literal, string_len, etc.).
    /// When true, string extern declarations are emitted and the runtime is linked.
    needs_strings: bool,
    /// Registry of struct types (name → StructInfo).
    struct_registry: HashMap<String, StructInfo>,
    /// Collected errors.
    errors: Vec<CodegenError>,
    /// Whether the current basic block has been terminated (ret or br).
    block_terminated: bool,
    /// The LLVM return type of the current function being emitted.
    current_return_type: String,
    /// Whether the current function is `@pure`.
    current_func_is_pure: bool,
    /// Whether the current function is `@const`.
    current_func_is_const: bool,
    /// Whether the current function is `@vectorizable`.
    current_func_is_vectorizable: bool,
    /// Whether the current function reads argument memory (has ptr/array params).
    current_func_reads_argmem: bool,
    /// Next metadata ID for branch weights, loop hints, etc.
    next_metadata_id: u32,
    /// Collected function attribute group strings (e.g., `attributes #0 = { ... }`).
    attribute_groups: Vec<String>,
    /// Collected metadata entries for the footer.
    metadata_entries: Vec<String>,
    /// Map from attribute group string to its group number.
    attr_group_map: HashMap<String, u32>,
    /// Next attribute group number.
    next_attr_group: u32,
    /// Bodies of @const functions for compile-time evaluation.
    const_func_bodies: HashMap<String, HirFunction>,
    /// Non-fatal warnings emitted during codegen (e.g., aliasing detection).
    warnings: Vec<String>,
    /// Maps parameter names to their ownership kind for the current function.
    /// Used to validate readonly_ptr / writeonly_ptr access at codegen time.
    param_ownership: HashMap<String, PtrOwnership>,
}

/// Ownership kind for pointer parameters, used for access validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PtrOwnership {
    /// `readonly_ptr[T]` — only ptr_read_* is allowed.
    Readonly,
    /// `writeonly_ptr[T]` — only ptr_write_* is allowed.
    Writeonly,
}

impl CodegenContext {
    /// Create a new codegen context.
    fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            next_reg: 0,
            next_label: 0,
            variables: HashMap::new(),
            functions: HashMap::new(),
            string_literals: Vec::new(),
            needs_printf_i64: false,
            needs_printf_i32: false,
            needs_printf_f64: false,
            needs_puts: false,
            needs_printf: false,
            needs_sqrt_f64: false,
            needs_pow_f64: false,
            needs_abs_i32: false,
            needs_fabs_f64: false,
            needs_memset: false,
            needs_fshl_i32: false,
            needs_fshr_i32: false,
            needs_malloc: false,
            needs_calloc: false,
            needs_realloc: false,
            needs_free: false,
            needs_arena: false,
            needs_runtime: false,
            needs_coroutines: false,
            needs_threading: false,
            needs_renderer: false,
            needs_gpu: false,
            needs_vec: false,
            needs_strings: false,
            struct_registry: HashMap::new(),
            errors: Vec::new(),
            block_terminated: false,
            current_return_type: String::new(),
            current_func_is_pure: false,
            current_func_is_const: false,
            current_func_is_vectorizable: false,
            current_func_reads_argmem: false,
            next_metadata_id: 0,
            attribute_groups: Vec::new(),
            metadata_entries: Vec::new(),
            attr_group_map: HashMap::new(),
            next_attr_group: 0,
            const_func_bodies: HashMap::new(),
            warnings: Vec::new(),
            param_ownership: HashMap::new(),
        }
    }

    /// Get or create an attribute group number for the given attributes string.
    fn get_or_create_attr_group(&mut self, attrs: &str) -> u32 {
        if let Some(&id) = self.attr_group_map.get(attrs) {
            return id;
        }
        let id = self.next_attr_group;
        self.next_attr_group += 1;
        self.attr_group_map.insert(attrs.to_string(), id);
        self.attribute_groups
            .push(format!("attributes #{id} = {{ {attrs} }}"));
        id
    }

    /// Allocate the next metadata ID.
    fn fresh_metadata_id(&mut self) -> u32 {
        let id = self.next_metadata_id;
        self.next_metadata_id += 1;
        id
    }

    /// Return the next numbered register like `%0`, `%1`, `%2`.
    fn fresh_reg(&mut self) -> String {
        let reg = format!("%{}", self.next_reg);
        self.next_reg += 1;
        reg
    }

    /// Return the next numbered label like `then.0`, `else.0`, `merge.0`.
    fn fresh_label(&mut self, prefix: &str) -> String {
        let label = format!("{prefix}.{}", self.next_label);
        self.next_label += 1;
        label
    }

    /// Emit a line of LLVM IR with indentation.
    fn emit(&mut self, line: &str) {
        let _ = writeln!(self.output, "  {line}");
    }

    /// Emit a line of LLVM IR without indentation (for labels, define, etc.).
    fn emit_raw(&mut self, line: &str) {
        let _ = writeln!(self.output, "{line}");
    }

    /// Emit a blank line.
    fn emit_blank(&mut self) {
        let _ = writeln!(self.output);
    }
}

/// Generate LLVM IR text from an HIR module.
///
/// Returns the complete `.ll` file content on success, or a list of errors.
pub fn codegen(module: &HirModule) -> Result<String, Vec<CodegenError>> {
    let mut ctx = CodegenContext::new();

    // Register all struct types in the struct registry.
    for s in &module.structs {
        register_struct(&mut ctx, s);
    }

    // Register all function signatures.
    for func in &module.functions {
        let ret_type = match hir_type_to_llvm(&func.return_type) {
            Ok(t) => t,
            Err(e) => {
                ctx.errors.push(e);
                continue;
            }
        };
        let mut param_types = Vec::new();
        for param in &func.params {
            match hir_type_to_llvm_param(&param.ty) {
                Ok(t) => param_types.push(t),
                Err(e) => ctx.errors.push(e),
            }
        }
        let is_export = func
            .annotations
            .iter()
            .any(|a| matches!(a.kind, HirAnnotationKind::Export));
        let uses_fastcc = func.name != "main" && !is_export;

        let func_annots = extract_func_annotations(&func.annotations, &func.params, &func.body);

        ctx.functions.insert(
            func.name.clone(),
            FuncInfo {
                return_type: ret_type,
                param_types,
                uses_fastcc,
                annotations: func_annots,
            },
        );
    }

    // Register extern function signatures.
    for ef in &module.extern_functions {
        let ret_type = match hir_type_to_llvm(&ef.return_type) {
            Ok(t) => t,
            Err(e) => {
                ctx.errors.push(e);
                continue;
            }
        };
        let mut param_types = Vec::new();
        for param in &ef.params {
            match hir_type_to_llvm_param(&param.ty) {
                Ok(t) => param_types.push(t),
                Err(e) => ctx.errors.push(e),
            }
        }
        ctx.functions.insert(
            ef.name.clone(),
            FuncInfo {
                return_type: ret_type,
                param_types,
                uses_fastcc: false,
                annotations: FuncAnnotations::default(),
            },
        );
    }

    // Store @const function bodies for compile-time evaluation.
    for func in &module.functions {
        let is_const = func
            .annotations
            .iter()
            .any(|a| matches!(a.kind, HirAnnotationKind::Const));
        if is_const {
            ctx.const_func_bodies
                .insert(func.name.clone(), func.clone());
        }
    }

    // First pass: emit all functions to a buffer (so we know what globals are needed).
    let mut func_output = String::with_capacity(4096);
    for func in &module.functions {
        let saved_output = std::mem::take(&mut ctx.output);
        emit_function(&mut ctx, func);
        let _ = writeln!(func_output, "{}", ctx.output);
        ctx.output = saved_output;
    }

    // Emit module header.
    let module_name = module.name.as_deref().unwrap_or("axiom_module");
    let _ = writeln!(ctx.output, "; ModuleID = '{module_name}'");
    let _ = writeln!(ctx.output, "source_filename = \"{module_name}\"");

    let target_triple = get_target_triple();
    let _ = writeln!(ctx.output, "target triple = \"{target_triple}\"");
    ctx.emit_blank();

    // Emit struct type definitions (`%struct.Name = type { ... }`).
    if !ctx.struct_registry.is_empty() {
        // Collect struct defs sorted by name for deterministic output.
        let mut struct_defs: Vec<_> = ctx.struct_registry.values().collect();
        struct_defs.sort_by(|a, b| a.llvm_name.cmp(&b.llvm_name));
        for info in &struct_defs {
            let field_types: Vec<&str> = info.fields.iter().map(|(_, t)| t.as_str()).collect();
            let _ = writeln!(
                ctx.output,
                "{} = type {{ {} }}",
                info.llvm_name,
                field_types.join(", ")
            );
        }
        ctx.emit_blank();
    }

    // Emit string literal globals.
    for (i, s) in ctx.string_literals.iter().enumerate() {
        let escaped = escape_llvm_string(s);
        let len = s.len() + 1; // +1 for null terminator
        let _ = writeln!(
            ctx.output,
            "@.str.{i} = private unnamed_addr constant [{len} x i8] c\"{escaped}\\00\""
        );
    }

    // Emit format string globals.
    if ctx.needs_printf_i64 {
        let _ = writeln!(
            ctx.output,
            "@.fmt.i64 = private unnamed_addr constant [6 x i8] c\"%lld\\0A\\00\""
        );
    }

    if ctx.needs_printf_i32 {
        let _ = writeln!(
            ctx.output,
            "@.fmt.i32 = private unnamed_addr constant [4 x i8] c\"%d\\0A\\00\""
        );
    }

    if ctx.needs_printf_f64 {
        let _ = writeln!(
            ctx.output,
            "@.fmt.f64 = private unnamed_addr constant [4 x i8] c\"%f\\0A\\00\""
        );
    }

    let has_globals = !ctx.string_literals.is_empty()
        || ctx.needs_printf_i64
        || ctx.needs_printf_i32
        || ctx.needs_printf_f64;
    if has_globals {
        ctx.emit_blank();
    }

    // Emit function definitions.
    ctx.output.push_str(&func_output);

    // Emit user-declared extern function declarations.
    for ef in &module.extern_functions {
        emit_extern_function_decl(&mut ctx, ef);
    }

    // Emit external declarations for built-in C functions.
    if ctx.needs_puts {
        let _ = writeln!(ctx.output, "declare i32 @puts(ptr)");
    }
    if ctx.needs_printf {
        let _ = writeln!(ctx.output, "declare i32 @printf(ptr, ...)");
    }
    if ctx.needs_sqrt_f64 {
        let _ = writeln!(ctx.output, "declare double @llvm.sqrt.f64(double)");
    }
    if ctx.needs_pow_f64 {
        let _ = writeln!(ctx.output, "declare double @llvm.pow.f64(double, double)");
    }
    if ctx.needs_abs_i32 {
        let _ = writeln!(ctx.output, "declare i32 @llvm.abs.i32(i32, i1)");
    }
    if ctx.needs_fabs_f64 {
        let _ = writeln!(ctx.output, "declare double @llvm.fabs.f64(double)");
    }
    if ctx.needs_memset {
        let _ = writeln!(
            ctx.output,
            "declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)"
        );
    }
    if ctx.needs_fshl_i32 {
        let _ = writeln!(
            ctx.output,
            "declare i32 @llvm.fshl.i32(i32, i32, i32)"
        );
    }
    if ctx.needs_fshr_i32 {
        let _ = writeln!(
            ctx.output,
            "declare i32 @llvm.fshr.i32(i32, i32, i32)"
        );
    }
    // Emit allocator function declarations with LLVM allocator attributes.
    // These attributes (allockind, alloc-family) enable LLVM's optimizer to:
    // - Eliminate dead allocations (unused malloc results)
    // - Promote heap-to-stack (HeapToStackPass) when allocation doesn't escape
    // - Merge/hoist allocations out of loops
    // - Eliminate redundant memset after calloc (zeroed attribute)
    // - Pair malloc/free for dead-free elimination (alloc-family)
    if ctx.needs_malloc || ctx.needs_arena {
        let malloc_group =
            ctx.get_or_create_attr_group("allockind(\"alloc,uninitialized\") \"alloc-family\"=\"malloc\"");
        let _ = writeln!(
            ctx.output,
            "declare noalias ptr @malloc(i64) #{malloc_group}"
        );
    }
    if ctx.needs_calloc {
        let calloc_group =
            ctx.get_or_create_attr_group("allockind(\"alloc,zeroed\") \"alloc-family\"=\"malloc\"");
        let _ = writeln!(
            ctx.output,
            "declare noalias ptr @calloc(i64, i64) #{calloc_group}"
        );
    }
    if ctx.needs_realloc {
        let realloc_group =
            ctx.get_or_create_attr_group("allockind(\"realloc\") \"alloc-family\"=\"malloc\"");
        let _ = writeln!(
            ctx.output,
            "declare noalias ptr @realloc(ptr, i64) #{realloc_group}"
        );
    }
    if ctx.needs_free || ctx.needs_arena {
        let free_group =
            ctx.get_or_create_attr_group("allockind(\"free\") \"alloc-family\"=\"malloc\"");
        let _ = writeln!(
            ctx.output,
            "declare void @free(ptr allocptr) #{free_group}"
        );
    }

    // Emit AXIOM C runtime extern declarations.
    if ctx.needs_runtime {
        let _ = writeln!(ctx.output, "declare ptr @axiom_file_read(ptr, ptr)");
        let _ = writeln!(ctx.output, "declare void @axiom_file_write(ptr, ptr, i64)");
        let _ = writeln!(ctx.output, "declare i64 @axiom_file_size(ptr)");
        let _ = writeln!(ctx.output, "declare i64 @axiom_clock_ns()");
        let _ = writeln!(ctx.output, "declare i32 @axiom_get_argc()");
        let _ = writeln!(ctx.output, "declare ptr @axiom_get_argv(i32)");
        let _ = writeln!(ctx.output, "declare i32 @axiom_cpu_features()");
    }

    // Emit coroutine extern declarations (also part of axiom_rt.c).
    if ctx.needs_coroutines {
        let _ = writeln!(ctx.output, "declare i32 @axiom_coro_create(ptr, i32)");
        let _ = writeln!(ctx.output, "declare i32 @axiom_coro_resume(i32)");
        let _ = writeln!(ctx.output, "declare void @axiom_coro_yield(i32)");
        let _ = writeln!(ctx.output, "declare i32 @axiom_coro_is_done(i32)");
        let _ = writeln!(ctx.output, "declare void @axiom_coro_destroy(i32)");
    }

    // Emit threading + job system extern declarations (also part of axiom_rt.c).
    if ctx.needs_threading {
        // Thread creation / join
        let _ = writeln!(ctx.output, "declare i32 @axiom_thread_create(ptr, ptr)");
        let _ = writeln!(ctx.output, "declare void @axiom_thread_join(i32)");
        // Atomics
        let _ = writeln!(ctx.output, "declare i32 @axiom_atomic_load(ptr)");
        let _ = writeln!(ctx.output, "declare void @axiom_atomic_store(ptr, i32)");
        let _ = writeln!(ctx.output, "declare i32 @axiom_atomic_add(ptr, i32)");
        let _ = writeln!(
            ctx.output,
            "declare i32 @axiom_atomic_cas(ptr, i32, i32)"
        );
        // Mutex
        let _ = writeln!(ctx.output, "declare ptr @axiom_mutex_create()");
        let _ = writeln!(ctx.output, "declare void @axiom_mutex_lock(ptr)");
        let _ = writeln!(ctx.output, "declare void @axiom_mutex_unlock(ptr)");
        let _ = writeln!(ctx.output, "declare void @axiom_mutex_destroy(ptr)");
        // Job system
        let _ = writeln!(ctx.output, "declare void @axiom_jobs_init(i32)");
        let _ = writeln!(
            ctx.output,
            "declare void @axiom_job_dispatch(ptr, ptr, i32)"
        );
        let _ = writeln!(ctx.output, "declare void @axiom_job_wait()");
        let _ = writeln!(ctx.output, "declare void @axiom_jobs_shutdown()");
        let _ = writeln!(ctx.output, "declare i32 @axiom_num_cores()");
        // Job handle & dependency graph
        let _ = writeln!(
            ctx.output,
            "declare i32 @axiom_job_dispatch_handle(ptr, ptr, i32)"
        );
        let _ = writeln!(
            ctx.output,
            "declare i32 @axiom_job_dispatch_after(ptr, ptr, i32, i32)"
        );
        let _ = writeln!(ctx.output, "declare void @axiom_job_wait_handle(i32)");
    }

    // Emit renderer / Vulkan FFI extern declarations (also part of axiom_rt.c).
    if ctx.needs_renderer {
        // Renderer lifecycle
        let _ = writeln!(
            ctx.output,
            "declare ptr @axiom_renderer_create(i32, i32, ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare void @axiom_renderer_destroy(ptr)"
        );
        // Frame operations
        let _ = writeln!(
            ctx.output,
            "declare i32 @axiom_renderer_begin_frame(ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare void @axiom_renderer_end_frame(ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare i32 @axiom_renderer_should_close(ptr)"
        );
        // Clear framebuffer
        let _ = writeln!(
            ctx.output,
            "declare void @axiom_renderer_clear(ptr, i32)"
        );
        // Drawing
        let _ = writeln!(
            ctx.output,
            "declare void @axiom_renderer_draw_triangles(ptr, ptr, ptr, i32)"
        );
        // Point drawing (for particle systems: x_arr, y_arr as f64*, colors as i32*)
        let _ = writeln!(
            ctx.output,
            "declare void @axiom_renderer_draw_points(ptr, ptr, ptr, ptr, i32)"
        );
        // Time
        let _ = writeln!(
            ctx.output,
            "declare double @axiom_renderer_get_time(ptr)"
        );
        // Shader loading (SPIR-V from Lux)
        let _ = writeln!(
            ctx.output,
            "declare ptr @axiom_shader_load(ptr, ptr, i32)"
        );
        // Pipeline
        let _ = writeln!(
            ctx.output,
            "declare ptr @axiom_pipeline_create(ptr, ptr, ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare void @axiom_renderer_bind_pipeline(ptr, ptr)"
        );
        // G2: Input System
        let _ = writeln!(ctx.output, "declare i32 @axiom_is_key_down(i32)");
        let _ = writeln!(ctx.output, "declare i32 @axiom_get_mouse_x()");
        let _ = writeln!(ctx.output, "declare i32 @axiom_get_mouse_y()");
        let _ = writeln!(ctx.output, "declare i32 @axiom_is_mouse_down(i32)");
        // G3: Audio
        let _ = writeln!(ctx.output, "declare void @axiom_play_beep(i32, i32)");
        let _ = writeln!(ctx.output, "declare void @axiom_play_sound(ptr)");
    }

    // Emit GPU PBR / glTF extern declarations (axiom-renderer gpu_* functions).
    if ctx.needs_gpu {
        let _ = writeln!(
            ctx.output,
            "declare ptr @gpu_init(i32, i32, ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare void @gpu_shutdown(ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare i32 @gpu_begin_frame(ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare void @gpu_end_frame(ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare i32 @gpu_should_close(ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare i32 @gpu_load_gltf(ptr, ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare void @gpu_set_camera(ptr, double, double, double, double, double, double, double)"
        );
        let _ = writeln!(
            ctx.output,
            "declare void @gpu_render(ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare double @gpu_get_frame_time(ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare ptr @gpu_get_gpu_name(ptr)"
        );
        let _ = writeln!(
            ctx.output,
            "declare i32 @gpu_screenshot(ptr, ptr)"
        );
    }

    // Emit Vec (dynamic array) runtime extern declarations.
    if ctx.needs_vec {
        let _ = writeln!(ctx.output, "declare ptr @axiom_vec_new(i32)");
        let _ = writeln!(ctx.output, "declare void @axiom_vec_push_i32(ptr, i32)");
        let _ = writeln!(ctx.output, "declare void @axiom_vec_push_f64(ptr, double)");
        let _ = writeln!(ctx.output, "declare i32 @axiom_vec_get_i32(ptr, i32)");
        let _ = writeln!(ctx.output, "declare double @axiom_vec_get_f64(ptr, i32)");
        let _ = writeln!(ctx.output, "declare void @axiom_vec_set_i32(ptr, i32, i32)");
        let _ = writeln!(
            ctx.output,
            "declare void @axiom_vec_set_f64(ptr, i32, double)"
        );
        let _ = writeln!(ctx.output, "declare i32 @axiom_vec_len(ptr)");
        let _ = writeln!(ctx.output, "declare void @axiom_vec_free(ptr)");
    }

    // Emit string runtime extern declarations.
    if ctx.needs_strings {
        let _ = writeln!(ctx.output, "declare i64 @axiom_string_from_literal(ptr)");
        let _ = writeln!(ctx.output, "declare i32 @axiom_string_len(i64)");
        let _ = writeln!(ctx.output, "declare ptr @axiom_string_ptr(i64)");
        let _ = writeln!(ctx.output, "declare i32 @axiom_string_eq(i64, i64)");
        let _ = writeln!(ctx.output, "declare void @axiom_string_print(i64)");
    }

    // Emit attribute groups.
    if !ctx.attribute_groups.is_empty() {
        ctx.emit_blank();
        for ag in &ctx.attribute_groups {
            let _ = writeln!(ctx.output, "{ag}");
        }
    }

    // Emit metadata entries.
    if !ctx.metadata_entries.is_empty() {
        ctx.emit_blank();
        for md in &ctx.metadata_entries {
            let _ = writeln!(ctx.output, "{md}");
        }
    }

    // E2: Emit basic DWARF debug info (compile unit metadata).
    // This provides minimal debug information so debuggers can identify the source.
    {
        let dbg_file_id = ctx.fresh_metadata_id();
        let dbg_cu_id = ctx.fresh_metadata_id();
        let flags_id = ctx.fresh_metadata_id();
        ctx.emit_blank();
        let _ = writeln!(
            ctx.output,
            "!llvm.dbg.cu = !{{!{dbg_cu_id}}}"
        );
        let _ = writeln!(
            ctx.output,
            "!llvm.module.flags = !{{!{flags_id}}}"
        );
        let _ = writeln!(
            ctx.output,
            "!{dbg_cu_id} = distinct !DICompileUnit(language: DW_LANG_C, file: !{dbg_file_id}, producer: \"axiom\", isOptimized: false, emissionKind: LineTablesOnly)"
        );
        let _ = writeln!(
            ctx.output,
            "!{dbg_file_id} = !DIFile(filename: \"{module_name}.axm\", directory: \".\")"
        );
        let _ = writeln!(
            ctx.output,
            "!{flags_id} = !{{i32 2, !\"Debug Info Version\", i32 3}}"
        );
    }

    // Emit warnings as LLVM IR comments so they appear in the output but
    // do not affect the IR semantics.
    if !ctx.warnings.is_empty() {
        ctx.emit_blank();
        for w in &ctx.warnings {
            let _ = writeln!(ctx.output, "; {w}");
        }
    }

    if !ctx.errors.is_empty() {
        return Err(ctx.errors);
    }

    Ok(ctx.output)
}

/// Check whether the generated LLVM IR requires the AXIOM C runtime to be
/// linked.  Returns `true` when the IR contains declarations for any
/// `@axiom_*` runtime helper function.
pub fn needs_runtime(ir: &str) -> bool {
    ir.contains("@axiom_file_read")
        || ir.contains("@axiom_file_write")
        || ir.contains("@axiom_file_size")
        || ir.contains("@axiom_clock_ns")
        || ir.contains("@axiom_get_argc")
        || ir.contains("@axiom_get_argv")
        || ir.contains("@axiom_coro_create")
        || ir.contains("@axiom_coro_resume")
        || ir.contains("@axiom_coro_yield")
        || ir.contains("@axiom_coro_is_done")
        || ir.contains("@axiom_coro_destroy")
        || ir.contains("@axiom_thread_create")
        || ir.contains("@axiom_thread_join")
        || ir.contains("@axiom_atomic_load")
        || ir.contains("@axiom_atomic_store")
        || ir.contains("@axiom_atomic_add")
        || ir.contains("@axiom_atomic_cas")
        || ir.contains("@axiom_mutex_create")
        || ir.contains("@axiom_mutex_lock")
        || ir.contains("@axiom_mutex_unlock")
        || ir.contains("@axiom_mutex_destroy")
        || ir.contains("@axiom_jobs_init")
        || ir.contains("@axiom_job_dispatch")
        || ir.contains("@axiom_job_wait")
        || ir.contains("@axiom_jobs_shutdown")
        || ir.contains("@axiom_num_cores")
        || ir.contains("@axiom_job_dispatch_handle")
        || ir.contains("@axiom_job_dispatch_after")
        || ir.contains("@axiom_job_wait_handle")
        || ir.contains("@axiom_renderer_create")
        || ir.contains("@axiom_renderer_destroy")
        || ir.contains("@axiom_renderer_begin_frame")
        || ir.contains("@axiom_renderer_end_frame")
        || ir.contains("@axiom_renderer_should_close")
        || ir.contains("@axiom_renderer_clear")
        || ir.contains("@axiom_renderer_draw_triangles")
        || ir.contains("@axiom_renderer_draw_points")
        || ir.contains("@axiom_renderer_get_time")
        || ir.contains("@axiom_shader_load")
        || ir.contains("@axiom_pipeline_create")
        || ir.contains("@axiom_renderer_bind_pipeline")
        // GPU PBR / glTF builtins
        || ir.contains("@gpu_init")
        || ir.contains("@gpu_shutdown")
        || ir.contains("@gpu_begin_frame")
        || ir.contains("@gpu_end_frame")
        || ir.contains("@gpu_should_close")
        || ir.contains("@gpu_load_gltf")
        || ir.contains("@gpu_set_camera")
        || ir.contains("@gpu_render(")
        || ir.contains("@gpu_get_frame_time")
        || ir.contains("@gpu_get_gpu_name")
        || ir.contains("@gpu_screenshot")
        // Vec builtins
        || ir.contains("@axiom_vec_new")
        || ir.contains("@axiom_vec_push_i32")
        || ir.contains("@axiom_vec_push_f64")
        || ir.contains("@axiom_vec_get_i32")
        || ir.contains("@axiom_vec_get_f64")
        || ir.contains("@axiom_vec_set_i32")
        || ir.contains("@axiom_vec_set_f64")
        || ir.contains("@axiom_vec_len")
        || ir.contains("@axiom_vec_free")
        // String builtins
        || ir.contains("@axiom_string_from_literal")
        || ir.contains("@axiom_string_len")
        || ir.contains("@axiom_string_ptr")
        || ir.contains("@axiom_string_eq")
        || ir.contains("@axiom_string_print")
        // CPUID feature detection
        || ir.contains("@axiom_cpu_features")
        // G2: Input System
        || ir.contains("@axiom_is_key_down")
        || ir.contains("@axiom_get_mouse_x")
        || ir.contains("@axiom_get_mouse_y")
        || ir.contains("@axiom_is_mouse_down")
        // G3: Audio
        || ir.contains("@axiom_play_beep")
        || ir.contains("@axiom_play_sound")
}

/// Register a struct type in the codegen context.
///
/// Builds the [`StructInfo`] from the HIR struct definition, computing field
/// types and total byte size for `memset` zero-initialization.
fn register_struct(ctx: &mut CodegenContext, s: &HirStruct) {
    let llvm_name = format!("%struct.{}", s.name);
    let mut fields = Vec::new();
    let mut total_size: u64 = 0;
    for field in &s.fields {
        match hir_type_to_llvm(&field.ty) {
            Ok(llvm_ty) => {
                total_size += llvm_type_size(&llvm_ty);
                fields.push((field.name.clone(), llvm_ty));
            }
            Err(e) => {
                ctx.errors.push(e);
            }
        }
    }
    ctx.struct_registry.insert(
        s.name.clone(),
        StructInfo {
            llvm_name,
            fields,
            total_size,
        },
    );
}

/// Scan a function body for writes through pointers (array index assignments or ptr_write_* calls).
///
/// Walks the HIR block recursively, checking for:
/// - calls to `ptr_write_i32`, `ptr_write_i64`, or `ptr_write_f64` builtins
/// - array index assignments like `arr[i] = val` (which write through a pointer to arg memory)
///
/// Returns `true` if any such write is found.
fn function_writes_through_ptrs(body: &HirBlock) -> bool {
    fn expr_has_ptr_write(expr: &HirExpr) -> bool {
        match &expr.kind {
            HirExprKind::Call { func, args } => {
                // Check if the callee is a ptr_write_* builtin.
                if let HirExprKind::Ident { name } = &func.kind {
                    if name == "ptr_write_i32"
                        || name == "ptr_write_i64"
                        || name == "ptr_write_f64"
                    {
                        return true;
                    }
                }
                // Also check callee expression and arguments recursively.
                if expr_has_ptr_write(func) {
                    return true;
                }
                args.iter().any(expr_has_ptr_write)
            }
            HirExprKind::BinaryOp { lhs, rhs, .. } => {
                expr_has_ptr_write(lhs) || expr_has_ptr_write(rhs)
            }
            HirExprKind::UnaryOp { operand, .. } => expr_has_ptr_write(operand),
            HirExprKind::Index { expr, indices } => {
                expr_has_ptr_write(expr) || indices.iter().any(expr_has_ptr_write)
            }
            HirExprKind::FieldAccess { expr, .. } => expr_has_ptr_write(expr),
            HirExprKind::MethodCall { expr, args, .. } => {
                expr_has_ptr_write(expr) || args.iter().any(expr_has_ptr_write)
            }
            _ => false,
        }
    }

    fn block_has_ptr_write(blk: &HirBlock) -> bool {
        blk.stmts.iter().any(|s| stmt_has_ptr_write(&s.kind))
    }

    fn stmt_has_ptr_write(kind: &HirStmtKind) -> bool {
        match kind {
            HirStmtKind::Let { value, .. } => {
                value.as_ref().is_some_and(expr_has_ptr_write)
            }
            HirStmtKind::Assign { target, value } => {
                // Array index assignment (arr[i] = val) writes through a pointer.
                if matches!(target.kind, HirExprKind::Index { .. }) {
                    return true;
                }
                expr_has_ptr_write(target) || expr_has_ptr_write(value)
            }
            HirStmtKind::Return { value } => expr_has_ptr_write(value),
            HirStmtKind::If {
                condition,
                then_block,
                else_block,
            } => {
                expr_has_ptr_write(condition)
                    || block_has_ptr_write(then_block)
                    || else_block.as_ref().is_some_and(block_has_ptr_write)
            }
            HirStmtKind::For {
                iterable, body, ..
            } => expr_has_ptr_write(iterable) || block_has_ptr_write(body),
            HirStmtKind::While { condition, body } => {
                expr_has_ptr_write(condition) || block_has_ptr_write(body)
            }
            HirStmtKind::Expr { expr } => expr_has_ptr_write(expr),
        }
    }

    block_has_ptr_write(body)
}

/// Extract optimization annotation flags from a function's annotations.
fn extract_func_annotations(
    annotations: &[axiom_hir::HirAnnotation],
    params: &[HirParam],
    body: &HirBlock,
) -> FuncAnnotations {
    let mut annots = FuncAnnotations::default();
    for ann in annotations {
        match &ann.kind {
            HirAnnotationKind::Pure => annots.is_pure = true,
            HirAnnotationKind::Const => annots.is_const = true,
            HirAnnotationKind::Vectorizable(_) => annots.is_vectorizable = true,
            HirAnnotationKind::Lifetime(s) if s == "scope" => {
                annots.is_lifetime_scope = true;
            }
            HirAnnotationKind::Inline(hint) => {
                annots.inline_hint = Some(hint.clone());
            }
            _ => {}
        }
    }
    // Check if any parameter is a pointer/array/struct type (meaning the function reads arg memory).
    annots.reads_arg_memory = params.iter().any(|p| {
        matches!(
            p.ty,
            HirType::Array { .. }
                | HirType::Ptr { .. }
                | HirType::ReadonlyPtr { .. }
                | HirType::WriteonlyPtr { .. }
                | HirType::Slice { .. }
                | HirType::UserDefined(_)
        )
    });
    // Scan function body for ptr_write_* calls to determine if the function writes through pointers.
    if annots.reads_arg_memory {
        annots.writes_arg_memory = function_writes_through_ptrs(body);
    }
    annots
}

/// Check if an LLVM type string represents a signed integer type.
///
/// In AXIOM, i8/i16/i32/i64/i128 are signed integers (separate from u8/u16/etc.),
/// so we can safely add `nsw` (no signed wrap) to operations on these types.
fn is_signed_int_type_str(ty: &str) -> bool {
    matches!(ty, "i8" | "i16" | "i32" | "i64" | "i128")
}

/// Emit a function definition.
fn emit_function(ctx: &mut CodegenContext, func: &HirFunction) {
    // Reset per-function state.
    ctx.next_reg = 0;
    ctx.next_label = 0;
    ctx.variables.clear();
    ctx.param_ownership.clear();
    ctx.block_terminated = false;
    ctx.current_return_type = String::new();

    // Populate ownership map for readonly_ptr / writeonly_ptr parameters.
    for param in &func.params {
        match &param.ty {
            HirType::ReadonlyPtr { .. } => {
                ctx.param_ownership
                    .insert(param.name.clone(), PtrOwnership::Readonly);
            }
            HirType::WriteonlyPtr { .. } => {
                ctx.param_ownership
                    .insert(param.name.clone(), PtrOwnership::Writeonly);
            }
            _ => {}
        }
    }

    let ret_type = match hir_type_to_llvm(&func.return_type) {
        Ok(t) => t,
        Err(e) => {
            ctx.errors.push(e);
            return;
        }
    };

    ctx.current_return_type = ret_type.clone();

    // Extract optimization annotations for the current function.
    let func_annots = extract_func_annotations(&func.annotations, &func.params, &func.body);
    ctx.current_func_is_pure = func_annots.is_pure;
    ctx.current_func_is_const = func_annots.is_const;
    ctx.current_func_is_vectorizable = func_annots.is_vectorizable;
    ctx.current_func_reads_argmem = func_annots.reads_arg_memory;

    // Build parameter list string (with noalias on ptr params).
    let params_str = build_params_str(ctx, &func.params);

    // Build function attribute group for @pure/@const functions.
    let attr_suffix = build_func_attr_suffix(ctx, &func_annots);

    // Check if function has @export annotation.
    let is_export = func
        .annotations
        .iter()
        .any(|a| matches!(a.kind, HirAnnotationKind::Export));

    let is_main = func.name == "main";
    if is_export {
        ctx.emit_raw(&format!(
            "define dso_local {ret_type} @{}({params_str}){attr_suffix} {{",
            func.name
        ));
    } else if is_main {
        ctx.emit_raw(&format!(
            "define {ret_type} @{}({params_str}){attr_suffix} {{",
            func.name
        ));
    } else {
        // Internal functions use fastcc for better performance on recursive calls
        ctx.emit_raw(&format!(
            "define internal fastcc {ret_type} @{}({params_str}){attr_suffix} {{",
            func.name
        ));
    }
    ctx.emit_raw("entry:");

    // Alloca + store for each parameter.
    emit_param_allocas(ctx, &func.params);

    // Emit function body.
    emit_block(ctx, &func.body);

    // If the function is void and the block didn't end with a terminator,
    // add an implicit `ret void`.
    if !ctx.block_terminated {
        if ret_type == "void" {
            ctx.emit("ret void");
        } else {
            // Non-void function without return — emit unreachable as safety net.
            ctx.emit("unreachable");
        }
    }

    ctx.emit_raw("}");

    // Reset per-function optimization state.
    ctx.current_func_is_pure = false;
    ctx.current_func_is_const = false;
    ctx.current_func_is_vectorizable = false;
    ctx.current_func_reads_argmem = false;
}

/// Build the function attribute suffix string (e.g., ` #0`).
///
/// For `@pure` functions (no willreturn, no nosync):
///
/// - No ptr params: `memory(none) nounwind`
/// - Ptr params, no writes: `memory(argmem: read) nounwind`
/// - Ptr params, has writes: `memory(argmem: readwrite) nounwind`
///
/// For `@const` functions: `memory(none) nounwind willreturn nosync speculatable`
fn build_func_attr_suffix(ctx: &mut CodegenContext, annots: &FuncAnnotations) -> String {
    let mut attrs = Vec::new();

    if annots.is_const {
        // @const implies no memory access, speculatable.
        // @const functions MUST terminate (verified during const-eval), so willreturn is safe.
        // memory(none) makes synchronization impossible, so nosync is trivially true.
        attrs.push("memory(none)");
        attrs.push("nounwind");
        attrs.push("willreturn");
        attrs.push("nosync");
        attrs.push("speculatable");
    } else if annots.is_pure {
        if annots.reads_arg_memory {
            if annots.writes_arg_memory {
                // @pure with pointer args that writes through pointers.
                attrs.push("memory(argmem: readwrite)");
            } else {
                // @pure with pointer args: reads argument memory only.
                attrs.push("memory(argmem: read)");
            }
        } else {
            // @pure without pointer args: no memory access at all.
            attrs.push("memory(none)");
        }
        attrs.push("nounwind");
        // NOTE: no willreturn — cannot prove termination for @pure functions with loops.
        // NOTE: no nosync — @pure functions may be called from parallel worker threads.
    }

    // @inline attribute: alwaysinline, noinline, or inlinehint.
    match &annots.inline_hint {
        Some(InlineHint::Always) => attrs.push("alwaysinline"),
        Some(InlineHint::Never) => attrs.push("noinline"),
        Some(InlineHint::Hint) => attrs.push("inlinehint"),
        None => {}
    }

    if attrs.is_empty() {
        return String::new();
    }

    let attrs_str = attrs.join(" ");
    let group_id = ctx.get_or_create_attr_group(&attrs_str);
    format!(" #{group_id}")
}

/// Emit an extern function declaration (`declare`).
fn emit_extern_function_decl(ctx: &mut CodegenContext, ef: &HirExternFunction) {
    let ret_type = match hir_type_to_llvm(&ef.return_type) {
        Ok(t) => t,
        Err(e) => {
            ctx.errors.push(e);
            return;
        }
    };

    let mut param_types_str = Vec::new();
    for param in &ef.params {
        match hir_type_to_llvm_param(&param.ty) {
            Ok(t) => param_types_str.push(t),
            Err(e) => ctx.errors.push(e),
        }
    }

    let params_str = param_types_str.join(", ");
    let _ = writeln!(
        ctx.output,
        "declare {ret_type} @{}({params_str})",
        ef.name
    );
}

/// Build the parameter list string for a function definition.
///
/// Adds `noalias` to all `ptr` parameters — AXIOM guarantees no pointer aliasing
/// by design (every array parameter is a unique allocation). This is the key
/// reason Fortran beats C in numerical code.
fn build_params_str(ctx: &mut CodegenContext, params: &[HirParam]) -> String {
    let mut parts = Vec::new();
    for param in params {
        match hir_type_to_llvm_param_with_attrs(&param.ty) {
            Ok(llvm_param_str) => {
                parts.push(format!("{llvm_param_str} %{}", param.name));
            }
            Err(e) => ctx.errors.push(e),
        }
    }
    parts.join(", ")
}

/// Emit alloca + store for function parameters.
fn emit_param_allocas(ctx: &mut CodegenContext, params: &[HirParam]) {
    for param in params {
        let alloca_name = format!("%{}.addr", param.name);

        // Array parameters are passed as ptr — store the pointer.
        if let HirType::Array {
            ref element, size, ..
        } = param.ty
        {
            let elem_llvm = match hir_type_to_llvm(element) {
                Ok(t) => t,
                Err(_) => continue,
            };
            ctx.emit(&format!("{alloca_name} = alloca ptr"));
            ctx.emit(&format!(
                "store ptr %{}, ptr {alloca_name}",
                param.name
            ));
            ctx.variables.insert(
                param.name.clone(),
                VarInfo {
                    alloca_name,
                    llvm_type: "ptr".to_string(),
                    array_info: Some(ArrayVarInfo {
                        element_type: elem_llvm,
                        size,
                        is_local: false,
                    }),
                },
            );
        } else if let HirType::UserDefined(ref struct_name) = param.ty {
            // Struct parameters are passed by pointer. Store the pointer in
            // an alloca so that field access GEPs can load from it.
            let llvm_type = format!("%struct.{struct_name}");
            ctx.emit(&format!("{alloca_name} = alloca ptr"));
            ctx.emit(&format!(
                "store ptr %{}, ptr {alloca_name}",
                param.name
            ));
            ctx.variables.insert(
                param.name.clone(),
                VarInfo {
                    alloca_name,
                    llvm_type,
                    array_info: None,
                },
            );
        } else {
            let llvm_type = match hir_type_to_llvm(&param.ty) {
                Ok(t) => t,
                Err(_) => continue,
            };
            ctx.emit(&format!("{alloca_name} = alloca {llvm_type}"));
            ctx.emit(&format!(
                "store {llvm_type} %{}, ptr {alloca_name}",
                param.name
            ));
            ctx.variables.insert(
                param.name.clone(),
                VarInfo {
                    alloca_name,
                    llvm_type,
                    array_info: None,
                },
            );
        }
    }
}

/// Emit statements in a block.
fn emit_block(ctx: &mut CodegenContext, block: &HirBlock) {
    for stmt in &block.stmts {
        if ctx.block_terminated {
            // Don't emit code after a terminator in the same basic block.
            break;
        }
        emit_stmt(ctx, stmt);
    }
}

/// Emit a single statement.
fn emit_stmt(ctx: &mut CodegenContext, stmt: &HirStmt) {
    match &stmt.kind {
        HirStmtKind::Let {
            name,
            ty,
            value,
            ..
        } => emit_let(ctx, name, ty, value.as_ref(), &stmt.annotations),
        HirStmtKind::Assign { target, value } => emit_assign(ctx, target, value),
        HirStmtKind::Return { value } => emit_return(ctx, value),
        HirStmtKind::If {
            condition,
            then_block,
            else_block,
        } => emit_if(ctx, condition, then_block, else_block.as_ref()),
        HirStmtKind::For {
            var,
            var_type,
            iterable,
            body,
            ..
        } => emit_for(ctx, var, var_type, iterable, body, &stmt.annotations),
        HirStmtKind::While { condition, body } => emit_while(ctx, condition, body),
        HirStmtKind::Expr { expr } => {
            emit_expr(ctx, expr, None);
        }
    }
}

/// Check whether the given annotations include `@lifetime(scope)`.
fn has_lifetime_scope(annotations: &[HirAnnotation]) -> bool {
    annotations
        .iter()
        .any(|a| matches!(&a.kind, HirAnnotationKind::Lifetime(s) if s == "scope"))
}

/// Check whether an expression is a `heap_alloc(count, elem_size)` call.
///
/// Returns `true` if the expression is a `Call` to the built-in `heap_alloc` function.
fn is_heap_alloc_call(expr: &HirExpr) -> bool {
    if let HirExprKind::Call { func, .. } = &expr.kind {
        if let HirExprKind::Ident { name } = &func.kind {
            return name == "heap_alloc";
        }
    }
    false
}

/// Emit a let binding: alloca + optional store.
///
/// When the let binding has `@lifetime(scope)` and the value is a `heap_alloc` call,
/// the allocation is promoted from `malloc` to `alloca` (stack allocation), which
/// eliminates the need for `free` and enables further LLVM optimizations.
///
/// When `value` is `None` (e.g., `let v: Vec3;`), the variable is zero-initialized
/// via `memset`. This is the standard initialization for struct-typed locals.
fn emit_let(
    ctx: &mut CodegenContext,
    name: &str,
    ty: &HirType,
    value: Option<&HirExpr>,
    annotations: &[HirAnnotation],
) {
    // Special handling for array types: alloca [N x T] + memset.
    if let HirType::Array {
        ref element, size, ..
    } = ty
    {
        let elem_llvm = match hir_type_to_llvm(element) {
            Ok(t) => t,
            Err(e) => {
                ctx.errors.push(e);
                return;
            }
        };
        let array_llvm = format!("[{size} x {elem_llvm}]");
        let uid = ctx.next_reg;
        ctx.next_reg += 1;
        let alloca_name = format!("%{name}.{uid}");
        ctx.emit(&format!("{alloca_name} = alloca {array_llvm}, align 16"));

        // Check if the initializer is ArrayZeros — emit memset.
        if value.is_some_and(|v| matches!(v.kind, HirExprKind::ArrayZeros { .. })) {
            let elem_size = llvm_type_size(&elem_llvm);
            let total_bytes = elem_size * (*size as u64);
            ctx.needs_memset = true;
            ctx.emit(&format!(
                "call void @llvm.memset.p0.i64(ptr {alloca_name}, i8 0, i64 {total_bytes}, i1 false)"
            ));
        }

        ctx.variables.insert(
            name.to_string(),
            VarInfo {
                alloca_name,
                llvm_type: array_llvm,
                array_info: Some(ArrayVarInfo {
                    element_type: elem_llvm,
                    size: *size,
                    is_local: true,
                }),
            },
        );
        return;
    }

    // --- @lifetime(scope) escape analysis: promote heap_alloc to alloca ---
    //
    // When a let binding has `@lifetime(scope)` and initializes with `heap_alloc`,
    // we emit a stack allocation (`alloca`) instead of calling `malloc`. This is safe
    // because `@lifetime(scope)` guarantees the pointer's lifetime matches the current
    // scope, so it cannot escape. The corresponding `heap_free` becomes a no-op
    // (the stack frame cleanup handles deallocation).
    if let Some(value) = value {
        if has_lifetime_scope(annotations) && is_heap_alloc_call(value) {
            if let HirExprKind::Call { args, .. } = &value.kind {
                if args.len() == 2 {
                    let llvm_type = match hir_type_to_llvm(ty) {
                        Ok(t) => t,
                        Err(e) => {
                            ctx.errors.push(e);
                            return;
                        }
                    };

                    // Evaluate count and elem_size arguments.
                    let count = emit_expr(ctx, &args[0], Some("i32"));
                    let elem_size = emit_expr(ctx, &args[1], Some("i32"));

                    // Widen to i64 for the multiplication.
                    let count64 = ctx.fresh_reg();
                    ctx.emit(&format!("{count64} = sext i32 {} to i64", count.reg));
                    let elem64 = ctx.fresh_reg();
                    ctx.emit(&format!("{elem64} = sext i32 {} to i64", elem_size.reg));
                    let total = ctx.fresh_reg();
                    ctx.emit(&format!("{total} = mul i64 {count64}, {elem64}"));

                    // Emit alloca instead of malloc — stack allocation.
                    let buf_reg = ctx.fresh_reg();
                    ctx.emit(&format!(
                        "{buf_reg} = alloca i8, i64 {total}, align 16"
                    ));

                    // Store the pointer into the variable's alloca slot.
                    let uid = ctx.next_reg;
                    ctx.next_reg += 1;
                    let alloca_name = format!("%{name}.{uid}");
                    ctx.emit(&format!("{alloca_name} = alloca {llvm_type}"));
                    ctx.emit(&format!(
                        "store {llvm_type} {buf_reg}, ptr {alloca_name}"
                    ));

                    ctx.variables.insert(
                        name.to_string(),
                        VarInfo {
                            alloca_name,
                            llvm_type,
                            array_info: None,
                        },
                    );
                    return;
                }
            }
        }
    }

    // Special handling for struct types: alloca + memset zero-initialization.
    if let HirType::UserDefined(ref struct_name) = ty {
        let struct_info = match ctx.struct_registry.get(struct_name) {
            Some(info) => info.clone(),
            None => {
                ctx.errors.push(CodegenError::UnsupportedType {
                    ty: struct_name.clone(),
                    context: "let binding (unknown struct)".to_string(),
                });
                return;
            }
        };
        let llvm_type = struct_info.llvm_name.clone();
        let uid = ctx.next_reg;
        ctx.next_reg += 1;
        let alloca_name = format!("%{name}.{uid}");
        ctx.emit(&format!("{alloca_name} = alloca {llvm_type}"));

        // Zero-initialize the struct via memset.
        ctx.needs_memset = true;
        ctx.emit(&format!(
            "call void @llvm.memset.p0.i64(ptr {alloca_name}, i8 0, i64 {}, i1 false)",
            struct_info.total_size
        ));

        ctx.variables.insert(
            name.to_string(),
            VarInfo {
                alloca_name,
                llvm_type,
                array_info: None,
            },
        );

        // If there is an explicit initializer, emit the store (though struct
        // literals are not yet supported, this handles future expansion).
        if let Some(value) = value {
            let val = emit_expr(ctx, value, Some(&struct_info.llvm_name));
            // For now we ignore the value for struct types since we don't have
            // struct literal expressions yet. The struct is already zero-initialized.
            let _ = val;
        }

        return;
    }

    let llvm_type = match hir_type_to_llvm(ty) {
        Ok(t) => t,
        Err(e) => {
            ctx.errors.push(e);
            return;
        }
    };

    // Use unique suffix to avoid collisions when the same variable name
    // appears in multiple scopes (e.g., `sum` in nested blocks).
    let uid = ctx.next_reg;
    ctx.next_reg += 1;
    let alloca_name = format!("%{name}.{uid}");
    ctx.emit(&format!("{alloca_name} = alloca {llvm_type}"));

    if let Some(value) = value {
        let val = emit_expr(ctx, value, Some(&llvm_type));
        ctx.emit(&format!(
            "store {llvm_type} {}, ptr {alloca_name}",
            val.reg
        ));
    } else {
        // No initializer — zero-initialize primitive types too.
        // This handles `let x: i32;` → alloca + store 0.
        ctx.emit(&format!(
            "store {llvm_type} 0, ptr {alloca_name}"
        ));
    }

    ctx.variables.insert(
        name.to_string(),
        VarInfo {
            alloca_name,
            llvm_type,
            array_info: None,
        },
    );
}

/// Emit an assignment: store to existing alloca or array index.
fn emit_assign(ctx: &mut CodegenContext, target: &HirExpr, value: &HirExpr) {
    if let HirExprKind::Ident { name } = &target.kind {
        let var_info = match ctx.variables.get(name) {
            Some(v) => v.clone(),
            None => {
                ctx.errors.push(CodegenError::UndefinedVariable {
                    name: name.clone(),
                });
                return;
            }
        };
        let val = emit_expr(ctx, value, Some(&var_info.llvm_type));
        ctx.emit(&format!(
            "store {} {}, ptr {}",
            var_info.llvm_type, val.reg, var_info.alloca_name
        ));
    } else if let HirExprKind::Index {
        expr: ref arr_expr,
        ref indices,
    } = target.kind
    {
        // Array index assignment: arr[i] = val
        if let HirExprKind::Ident { name } = &arr_expr.kind {
            let var_info = match ctx.variables.get(name.as_str()) {
                Some(v) => v.clone(),
                None => {
                    ctx.errors.push(CodegenError::UndefinedVariable {
                        name: name.clone(),
                    });
                    return;
                }
            };
            if let Some(ref ainfo) = var_info.array_info {
                let elem_type = ainfo.element_type.clone();
                let arr_size = ainfo.size;
                let is_local = ainfo.is_local;

                if indices.len() != 1 {
                    ctx.errors.push(CodegenError::UnsupportedExpression {
                        expr: "multi-dimensional array index".to_string(),
                        context: "array index assignment".to_string(),
                    });
                    return;
                }
                let idx_val = emit_expr(ctx, &indices[0], Some("i64"));
                // Ensure index is i64 for GEP.
                let idx_i64 = if idx_val.ty != "i64" {
                    let ext_reg = ctx.fresh_reg();
                    ctx.emit(&format!("{ext_reg} = sext {} {} to i64", idx_val.ty, idx_val.reg));
                    ext_reg
                } else {
                    idx_val.reg.clone()
                };

                let array_llvm = format!("[{arr_size} x {elem_type}]");
                let base_ptr = if is_local {
                    var_info.alloca_name.clone()
                } else {
                    // Load the pointer from the alloca.
                    let load_reg = ctx.fresh_reg();
                    ctx.emit(&format!(
                        "{load_reg} = load ptr, ptr {}",
                        var_info.alloca_name
                    ));
                    load_reg
                };

                let gep_reg = ctx.fresh_reg();
                ctx.emit(&format!(
                    "{gep_reg} = getelementptr inbounds {array_llvm}, ptr {base_ptr}, i64 0, i64 {idx_i64}"
                ));

                let rhs_val = emit_expr(ctx, value, Some(&elem_type));
                ctx.emit(&format!(
                    "store {elem_type} {}, ptr {gep_reg}",
                    rhs_val.reg
                ));
            } else {
                ctx.errors.push(CodegenError::UnsupportedExpression {
                    expr: "index assignment on non-array variable".to_string(),
                    context: "assignment".to_string(),
                });
            }
        } else {
            ctx.errors.push(CodegenError::UnsupportedExpression {
                expr: "index assignment on non-ident base".to_string(),
                context: "assignment".to_string(),
            });
        }
    } else if let HirExprKind::FieldAccess {
        expr: ref base_expr,
        ref field,
    } = target.kind
    {
        // Struct field assignment: v.x = val
        if let HirExprKind::Ident { name } = &base_expr.kind {
            let var_info = match ctx.variables.get(name.as_str()) {
                Some(v) => v.clone(),
                None => {
                    ctx.errors.push(CodegenError::UndefinedVariable {
                        name: name.clone(),
                    });
                    return;
                }
            };

            // Determine the struct name from the LLVM type.
            let struct_name = if var_info.llvm_type.starts_with("%struct.") {
                var_info
                    .llvm_type
                    .strip_prefix("%struct.")
                    .map(|s| s.to_string())
            } else {
                None
            };

            let struct_name = match struct_name {
                Some(n) => n,
                None => {
                    ctx.errors.push(CodegenError::UnsupportedExpression {
                        expr: format!(
                            "field assignment on non-struct type `{}`",
                            var_info.llvm_type
                        ),
                        context: "assignment".to_string(),
                    });
                    return;
                }
            };

            let struct_info = match ctx.struct_registry.get(&struct_name) {
                Some(info) => info.clone(),
                None => {
                    ctx.errors.push(CodegenError::UnsupportedType {
                        ty: struct_name,
                        context: "field assignment (unknown struct)".to_string(),
                    });
                    return;
                }
            };

            // Find field index and type.
            let field_idx = struct_info
                .fields
                .iter()
                .position(|(fname, _)| fname == field.as_str());
            let (field_index, field_type) = match field_idx {
                Some(idx) => (idx, struct_info.fields[idx].1.clone()),
                None => {
                    ctx.errors.push(CodegenError::UnsupportedExpression {
                        expr: format!("unknown field `{field}` on struct `{struct_name}`"),
                        context: "field assignment".to_string(),
                    });
                    return;
                }
            };

            // Get the base pointer to the struct.
            let base_ptr = get_struct_base_ptr(ctx, &var_info);

            // GEP to the field.
            let gep_reg = ctx.fresh_reg();
            ctx.emit(&format!(
                "{gep_reg} = getelementptr inbounds {}, ptr {base_ptr}, i32 0, i32 {field_index}",
                struct_info.llvm_name
            ));

            // Emit the value and store it.
            let rhs_val = emit_expr(ctx, value, Some(&field_type));
            ctx.emit(&format!(
                "store {field_type} {}, ptr {gep_reg}",
                rhs_val.reg
            ));
        } else {
            ctx.errors.push(CodegenError::UnsupportedExpression {
                expr: "field assignment on non-ident base".to_string(),
                context: "assignment".to_string(),
            });
        }
    } else {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "non-ident assignment target".to_string(),
            context: "assignment".to_string(),
        });
    }
}

/// Emit a return statement.
fn emit_return(ctx: &mut CodegenContext, value: &HirExpr) {
    let ret_type = ctx.current_return_type.clone();
    let expected = if ret_type.is_empty() {
        None
    } else {
        Some(ret_type.as_str())
    };
    let val = emit_expr(ctx, value, expected);
    let ty = if !ret_type.is_empty() {
        &ret_type
    } else {
        &val.ty
    };
    ctx.emit(&format!("ret {ty} {}", val.reg));
    ctx.block_terminated = true;
}

/// Emit if/else control flow.
fn emit_if(
    ctx: &mut CodegenContext,
    condition: &HirExpr,
    then_block: &HirBlock,
    else_block: Option<&HirBlock>,
) {
    let cond = emit_expr(ctx, condition, Some("i1"));

    let then_label = ctx.fresh_label("then");
    let merge_label = ctx.fresh_label("merge");

    // Optimization #6: Branch prediction hints.
    // Detect base-case patterns like `n <= 1` or `n < 2` in recursive/@pure functions
    // and add !prof metadata indicating the then-branch (base case) is unlikely.
    let branch_weights = if ctx.current_func_is_pure || ctx.current_func_is_const {
        detect_base_case_pattern(condition)
    } else {
        None
    };

    if let Some(else_blk) = else_block {
        let else_label = ctx.fresh_label("else");

        if let Some((then_weight, else_weight)) = branch_weights {
            // Add branch weight metadata.
            let md_id = ctx.fresh_metadata_id();
            ctx.metadata_entries.push(format!(
                "!{md_id} = !{{!\"branch_weights\", i32 {then_weight}, i32 {else_weight}}}"
            ));
            ctx.emit(&format!(
                "br i1 {}, label %{then_label}, label %{else_label}, !prof !{md_id}",
                cond.reg
            ));
        } else {
            ctx.emit(&format!(
                "br i1 {}, label %{then_label}, label %{else_label}",
                cond.reg
            ));
        }

        // Then block.
        ctx.emit_blank();
        ctx.emit_raw(&format!("{then_label}:"));
        ctx.block_terminated = false;
        emit_block(ctx, then_block);
        let then_terminated = ctx.block_terminated;
        if !then_terminated {
            ctx.emit(&format!("br label %{merge_label}"));
        }

        // Else block.
        ctx.emit_blank();
        ctx.emit_raw(&format!("{else_label}:"));
        ctx.block_terminated = false;
        emit_block(ctx, else_blk);
        let else_terminated = ctx.block_terminated;
        if !else_terminated {
            ctx.emit(&format!("br label %{merge_label}"));
        }

        // Merge block.
        ctx.emit_blank();
        ctx.emit_raw(&format!("{merge_label}:"));
        // If both branches terminated, the merge block is unreachable.
        ctx.block_terminated = then_terminated && else_terminated;
    } else {
        if let Some((then_weight, else_weight)) = branch_weights {
            let md_id = ctx.fresh_metadata_id();
            ctx.metadata_entries.push(format!(
                "!{md_id} = !{{!\"branch_weights\", i32 {then_weight}, i32 {else_weight}}}"
            ));
            ctx.emit(&format!(
                "br i1 {}, label %{then_label}, label %{merge_label}, !prof !{md_id}",
                cond.reg
            ));
        } else {
            ctx.emit(&format!(
                "br i1 {}, label %{then_label}, label %{merge_label}",
                cond.reg
            ));
        }

        // Then block.
        ctx.emit_blank();
        ctx.emit_raw(&format!("{then_label}:"));
        ctx.block_terminated = false;
        emit_block(ctx, then_block);
        if !ctx.block_terminated {
            ctx.emit(&format!("br label %{merge_label}"));
        }

        // Merge block.
        ctx.emit_blank();
        ctx.emit_raw(&format!("{merge_label}:"));
        ctx.block_terminated = false;
    }
}

/// Detect base-case patterns in conditions for branch prediction.
///
/// Returns `Some((then_weight, else_weight))` if the condition looks like a
/// base case (e.g., `n <= 1`, `n < 2`), indicating the then-branch is rarely taken.
fn detect_base_case_pattern(condition: &HirExpr) -> Option<(u32, u32)> {
    if let HirExprKind::BinaryOp { op, lhs, rhs } = &condition.kind {
        // Check for patterns like `n <= SMALL_CONST` or `n < SMALL_CONST`.
        let is_base_case_op = matches!(op, BinOp::LtEq | BinOp::Lt | BinOp::Eq);
        if !is_base_case_op {
            return None;
        }

        // LHS should be an identifier, RHS should be a small integer literal.
        let rhs_is_small_lit = matches!(
            &rhs.kind,
            HirExprKind::IntLiteral { value } if *value >= 0 && *value <= 3
        );
        let lhs_is_ident = matches!(&lhs.kind, HirExprKind::Ident { .. });

        if lhs_is_ident && rhs_is_small_lit {
            // Base case: then-branch is unlikely (weight 1), else is likely (weight 2000).
            return Some((1, 2000));
        }
    }
    None
}

/// Emit a for loop with range() recognition.
/// Check whether the given annotations include `@parallel_for`.
///
/// Returns `true` if at least one annotation is `HirAnnotationKind::ParallelFor`.
fn has_parallel_for(annotations: &[HirAnnotation]) -> bool {
    annotations
        .iter()
        .any(|a| matches!(&a.kind, HirAnnotationKind::ParallelFor(_)))
}

fn emit_for(
    ctx: &mut CodegenContext,
    var: &str,
    var_type: &HirType,
    iterable: &HirExpr,
    body: &HirBlock,
    annotations: &[HirAnnotation],
) {
    let loop_type = match hir_type_to_llvm(var_type) {
        Ok(t) => t,
        Err(e) => {
            ctx.errors.push(e);
            return;
        }
    };

    // Detect @parallel_for annotation on this for loop.
    let is_parallel = has_parallel_for(annotations);

    // Recognize range(start, end) or range(end) pattern.
    let (start_expr, end_expr) = match &iterable.kind {
        HirExprKind::Call { func, args } => {
            if let HirExprKind::Ident { name } = &func.kind {
                if name == "range" {
                    match args.len() {
                        1 => (None, Some(&args[0])),
                        2 => (Some(&args[0]), Some(&args[1])),
                        _ => {
                            ctx.errors.push(CodegenError::UnsupportedExpression {
                                expr: "range() with wrong number of arguments".to_string(),
                                context: "for loop".to_string(),
                            });
                            return;
                        }
                    }
                } else {
                    ctx.errors.push(CodegenError::UnsupportedExpression {
                        expr: format!("for-in with non-range iterable `{name}`"),
                        context: "for loop".to_string(),
                    });
                    return;
                }
            } else {
                ctx.errors.push(CodegenError::UnsupportedExpression {
                    expr: "for-in with non-ident callee".to_string(),
                    context: "for loop".to_string(),
                });
                return;
            }
        }
        _ => {
            ctx.errors.push(CodegenError::UnsupportedExpression {
                expr: "for-in with non-range iterable".to_string(),
                context: "for loop".to_string(),
            });
            return;
        }
    };

    // Emit start value.
    let start_val = match start_expr {
        Some(expr) => emit_expr(ctx, expr, Some(&loop_type)),
        None => LlvmValue {
            reg: "0".to_string(),
            ty: loop_type.clone(),
        },
    };

    // Emit end value (once, before the loop).
    let end_val = emit_expr(ctx, end_expr.expect("end_expr should be Some"), Some(&loop_type));

    // Alloca for loop variable — use unique name to avoid collisions with
    // multiple loops using the same variable name.
    let unique_id = ctx.next_reg;
    ctx.next_reg += 1;
    let alloca_name = format!("%{var}.{unique_id}");
    ctx.emit(&format!("{alloca_name} = alloca {loop_type}"));
    ctx.emit(&format!(
        "store {loop_type} {}, ptr {alloca_name}",
        start_val.reg
    ));

    // Save old variable info if shadowed.
    let old_var = ctx.variables.remove(var);

    ctx.variables.insert(
        var.to_string(),
        VarInfo {
            alloca_name: alloca_name.clone(),
            llvm_type: loop_type.clone(),
            array_info: None,
        },
    );

    let cond_label = ctx.fresh_label("for.cond");
    let body_label = ctx.fresh_label("for.body");
    let end_label = ctx.fresh_label("for.end");

    ctx.emit(&format!("br label %{cond_label}"));

    // Condition block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{cond_label}:"));
    ctx.block_terminated = false;
    let load_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{load_reg} = load {loop_type}, ptr {alloca_name}"
    ));
    let cmp_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{cmp_reg} = icmp slt {loop_type} {load_reg}, {}",
        end_val.reg
    ));
    ctx.emit(&format!(
        "br i1 {cmp_reg}, label %{body_label}, label %{end_label}"
    ));

    // Body block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{body_label}:"));
    ctx.block_terminated = false;
    emit_block(ctx, body);

    // Increment loop variable with nsw (loop induction variable doesn't wrap).
    if !ctx.block_terminated {
        let inc_load = ctx.fresh_reg();
        ctx.emit(&format!(
            "{inc_load} = load {loop_type}, ptr {alloca_name}"
        ));
        let inc_add = ctx.fresh_reg();
        ctx.emit(&format!("{inc_add} = add nsw {loop_type} {inc_load}, 1"));
        ctx.emit(&format!(
            "store {loop_type} {inc_add}, ptr {alloca_name}"
        ));

        // Emit LLVM loop metadata.
        // @parallel_for loops get parallel access metadata + vectorize hints.
        // @vectorizable functions get vectorize hints on all loops.
        let needs_parallel_md = is_parallel;
        let needs_vec_md = ctx.current_func_is_vectorizable || is_parallel;

        if needs_parallel_md || needs_vec_md {
            let loop_md_id = ctx.fresh_metadata_id();
            let mut md_operands = vec![format!("!{loop_md_id}")];

            if needs_parallel_md {
                // Emit access group metadata for parallel loop.
                let access_group_id = ctx.fresh_metadata_id();
                let parallel_accesses_id = ctx.fresh_metadata_id();
                ctx.metadata_entries.push(format!(
                    "!{access_group_id} = distinct !{{}}"
                ));
                ctx.metadata_entries.push(format!(
                    "!{parallel_accesses_id} = !{{!\"llvm.loop.parallel_accesses\", !{access_group_id}}}"
                ));
                md_operands.push(format!("!{parallel_accesses_id}"));
            }

            if needs_vec_md {
                let vec_enable_id = ctx.fresh_metadata_id();
                ctx.metadata_entries.push(format!(
                    "!{vec_enable_id} = !{{!\"llvm.loop.vectorize.enable\", i1 true}}"
                ));
                md_operands.push(format!("!{vec_enable_id}"));

                // P3: Hint preferred SIMD width (8 lanes) for @vectorizable loops.
                let vec_width_id = ctx.fresh_metadata_id();
                ctx.metadata_entries.push(format!(
                    "!{vec_width_id} = !{{!\"llvm.loop.vectorize.width\", i32 8}}"
                ));
                md_operands.push(format!("!{vec_width_id}"));
            }

            if is_parallel {
                // Also hint that loop iterations are independent (safe to reorder).
                let distribute_id = ctx.fresh_metadata_id();
                ctx.metadata_entries.push(format!(
                    "!{distribute_id} = !{{!\"llvm.loop.distribute.enable\", i1 true}}"
                ));
                md_operands.push(format!("!{distribute_id}"));
            }

            ctx.metadata_entries.push(format!(
                "!{loop_md_id} = distinct !{{{}}}",
                md_operands.join(", ")
            ));
            ctx.emit(&format!(
                "br label %{cond_label}, !llvm.loop !{loop_md_id}"
            ));
        } else {
            ctx.emit(&format!("br label %{cond_label}"));
        }
    }

    // End block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{end_label}:"));
    ctx.block_terminated = false;

    // Restore old variable if shadowed.
    if let Some(old) = old_var {
        ctx.variables.insert(var.to_string(), old);
    }
}

/// Emit a while loop.
fn emit_while(ctx: &mut CodegenContext, condition: &HirExpr, body: &HirBlock) {
    let cond_label = ctx.fresh_label("while.cond");
    let body_label = ctx.fresh_label("while.body");
    let end_label = ctx.fresh_label("while.end");

    ctx.emit(&format!("br label %{cond_label}"));

    // Condition block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{cond_label}:"));
    ctx.block_terminated = false;
    let cond = emit_expr(ctx, condition, Some("i1"));
    ctx.emit(&format!(
        "br i1 {}, label %{body_label}, label %{end_label}",
        cond.reg
    ));

    // Body block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{body_label}:"));
    ctx.block_terminated = false;
    emit_block(ctx, body);
    if !ctx.block_terminated {
        ctx.emit(&format!("br label %{cond_label}"));
    }

    // End block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{end_label}:"));
    ctx.block_terminated = false;
}

/// Emit an expression. Returns the LLVM value (register or literal) and type.
///
/// `expected_type` is a hint for literals that don't carry their own type.
fn emit_expr(ctx: &mut CodegenContext, expr: &HirExpr, expected_type: Option<&str>) -> LlvmValue {
    match &expr.kind {
        HirExprKind::IntLiteral { value } => {
            let ty = expected_type.unwrap_or("i64").to_string();
            LlvmValue {
                reg: format!("{value}"),
                ty,
            }
        }
        HirExprKind::FloatLiteral { value } => {
            let ty = expected_type.unwrap_or("double").to_string();
            // LLVM requires decimal format with digit on each side of the dot.
            let formatted = format_float(*value);
            LlvmValue {
                reg: formatted,
                ty,
            }
        }
        HirExprKind::BoolLiteral { value } => LlvmValue {
            reg: if *value {
                "1".to_string()
            } else {
                "0".to_string()
            },
            ty: "i1".to_string(),
        },
        HirExprKind::StringLiteral { value } => {
            let idx = ctx.string_literals.len();
            ctx.string_literals.push(value.clone());
            LlvmValue {
                reg: format!("@.str.{idx}"),
                ty: "ptr".to_string(),
            }
        }
        HirExprKind::Ident { name } => emit_ident(ctx, name),
        HirExprKind::BinaryOp { op, lhs, rhs } => emit_binary_op(ctx, *op, lhs, rhs),
        HirExprKind::UnaryOp { op, operand } => emit_unary_op(ctx, *op, operand, expected_type),
        HirExprKind::Call { func, args } => emit_call(ctx, func, args),
        HirExprKind::Index {
            expr: arr_expr,
            indices,
        } => emit_array_index_read(ctx, arr_expr, indices),
        HirExprKind::FieldAccess {
            expr: base_expr,
            field,
        } => emit_field_access(ctx, base_expr, field),
        HirExprKind::ArrayZeros {
            element_type,
            size,
        } => {
            // ArrayZeros is handled at the let-binding level (emit_let).
            // If we reach here, it means ArrayZeros was used in a non-let context.
            // Return a dummy value — the alloca+memset was already emitted by emit_let.
            let elem_llvm = match hir_type_to_llvm(element_type) {
                Ok(t) => t,
                Err(e) => {
                    ctx.errors.push(e);
                    return LlvmValue {
                        reg: "0".to_string(),
                        ty: "i32".to_string(),
                    };
                }
            };
            LlvmValue {
                reg: "zeroinitializer".to_string(),
                ty: format!("[{size} x {elem_llvm}]"),
            }
        }
        _ => {
            ctx.errors.push(CodegenError::UnsupportedExpression {
                expr: format!("{:?}", expr.kind),
                context: "expression".to_string(),
            });
            LlvmValue {
                reg: "0".to_string(),
                ty: "i32".to_string(),
            }
        }
    }
}

/// Emit a variable reference (load from alloca).
fn emit_ident(ctx: &mut CodegenContext, name: &str) -> LlvmValue {
    let var_info = match ctx.variables.get(name) {
        Some(v) => v.clone(),
        None => {
            ctx.errors.push(CodegenError::UndefinedVariable {
                name: name.to_string(),
            });
            return LlvmValue {
                reg: "0".to_string(),
                ty: "i32".to_string(),
            };
        }
    };

    // For array variables, return the pointer to the array data (for passing to functions).
    if let Some(ref ainfo) = var_info.array_info {
        if ainfo.is_local {
            // Local array: alloca IS the pointer.
            return LlvmValue {
                reg: var_info.alloca_name,
                ty: "ptr".to_string(),
            };
        } else {
            // Parameter array: load the stored pointer.
            let reg = ctx.fresh_reg();
            ctx.emit(&format!(
                "{reg} = load ptr, ptr {}",
                var_info.alloca_name
            ));
            return LlvmValue {
                reg,
                ty: "ptr".to_string(),
            };
        }
    }

    // For struct variables, return a pointer (structs are passed by pointer).
    if var_info.llvm_type.starts_with("%struct.") {
        let base_ptr = get_struct_base_ptr(ctx, &var_info);
        return LlvmValue {
            reg: base_ptr,
            ty: "ptr".to_string(),
        };
    }

    let reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{reg} = load {}, ptr {}",
        var_info.llvm_type, var_info.alloca_name
    ));
    LlvmValue {
        reg,
        ty: var_info.llvm_type,
    }
}

/// Emit an array index read: arr[i] -> GEP + load.
fn emit_array_index_read(
    ctx: &mut CodegenContext,
    arr_expr: &HirExpr,
    indices: &[HirExpr],
) -> LlvmValue {
    if let HirExprKind::Ident { name } = &arr_expr.kind {
        let var_info = match ctx.variables.get(name.as_str()) {
            Some(v) => v.clone(),
            None => {
                ctx.errors.push(CodegenError::UndefinedVariable {
                    name: name.clone(),
                });
                return LlvmValue {
                    reg: "0".to_string(),
                    ty: "i32".to_string(),
                };
            }
        };
        if let Some(ref ainfo) = var_info.array_info {
            if indices.len() != 1 {
                ctx.errors.push(CodegenError::UnsupportedExpression {
                    expr: "multi-dimensional array index".to_string(),
                    context: "array index read".to_string(),
                });
                return LlvmValue {
                    reg: "0".to_string(),
                    ty: "i32".to_string(),
                };
            }
            let elem_type = ainfo.element_type.clone();
            let arr_size = ainfo.size;
            let is_local = ainfo.is_local;

            let idx_val = emit_expr(ctx, &indices[0], Some("i64"));
            // Ensure index is i64 for GEP.
            let idx_i64 = if idx_val.ty != "i64" {
                let ext_reg = ctx.fresh_reg();
                ctx.emit(&format!(
                    "{ext_reg} = sext {} {} to i64",
                    idx_val.ty, idx_val.reg
                ));
                ext_reg
            } else {
                idx_val.reg.clone()
            };

            let array_llvm = format!("[{arr_size} x {elem_type}]");
            let base_ptr = if is_local {
                var_info.alloca_name.clone()
            } else {
                let load_reg = ctx.fresh_reg();
                ctx.emit(&format!(
                    "{load_reg} = load ptr, ptr {}",
                    var_info.alloca_name
                ));
                load_reg
            };

            let gep_reg = ctx.fresh_reg();
            ctx.emit(&format!(
                "{gep_reg} = getelementptr inbounds {array_llvm}, ptr {base_ptr}, i64 0, i64 {idx_i64}"
            ));

            let load_reg = ctx.fresh_reg();
            ctx.emit(&format!(
                "{load_reg} = load {elem_type}, ptr {gep_reg}"
            ));

            return LlvmValue {
                reg: load_reg,
                ty: elem_type,
            };
        }
    }

    ctx.errors.push(CodegenError::UnsupportedExpression {
        expr: "index expression on non-array".to_string(),
        context: "index read".to_string(),
    });
    LlvmValue {
        reg: "0".to_string(),
        ty: "i32".to_string(),
    }
}

/// Get the base pointer for a struct variable (handles both locals and params).
///
/// For local struct variables, the alloca IS the struct (alloca %struct.Name),
/// so the alloca_name is the pointer to the struct.
/// For struct parameters, the alloca stores a pointer to the struct (alloca ptr),
/// so we need to load the pointer first.
fn get_struct_base_ptr(ctx: &mut CodegenContext, var_info: &VarInfo) -> String {
    // If the alloca type is a struct type (%struct.X), the alloca IS the struct.
    // If the alloca type is "ptr", we need to load the pointer.
    if var_info.llvm_type.starts_with("%struct.") {
        // Check if this is a local (alloca %struct.X) or parameter (alloca ptr).
        // For parameters, the alloca_name holds `%name.addr` which stores a `ptr`.
        // For locals, the alloca_name holds `%name.N` which is an `alloca %struct.X`.
        // We distinguish by checking if `.addr` suffix is present.
        if var_info.alloca_name.ends_with(".addr") {
            // Parameter: alloca ptr → load ptr
            let load_reg = ctx.fresh_reg();
            ctx.emit(&format!(
                "{load_reg} = load ptr, ptr {}",
                var_info.alloca_name
            ));
            load_reg
        } else {
            // Local: alloca %struct.X → the alloca itself is the pointer.
            var_info.alloca_name.clone()
        }
    } else {
        // Generic ptr (shouldn't happen for well-typed code, but handle gracefully).
        var_info.alloca_name.clone()
    }
}

/// Emit a struct field access (read): `v.x` → GEP + load.
///
/// Resolves the struct type from the variable's LLVM type, looks up the field
/// index and type in the struct registry, then emits a `getelementptr` + `load`.
fn emit_field_access(
    ctx: &mut CodegenContext,
    base_expr: &HirExpr,
    field: &str,
) -> LlvmValue {
    // The base expression should be an identifier referring to a struct variable.
    if let HirExprKind::Ident { name } = &base_expr.kind {
        let var_info = match ctx.variables.get(name.as_str()) {
            Some(v) => v.clone(),
            None => {
                ctx.errors.push(CodegenError::UndefinedVariable {
                    name: name.clone(),
                });
                return LlvmValue {
                    reg: "0".to_string(),
                    ty: "i32".to_string(),
                };
            }
        };

        // Determine the struct name from the LLVM type.
        let struct_name = if var_info.llvm_type.starts_with("%struct.") {
            var_info.llvm_type.strip_prefix("%struct.").map(|s| s.to_string())
        } else {
            None
        };

        let struct_name = match struct_name {
            Some(n) => n,
            None => {
                ctx.errors.push(CodegenError::UnsupportedExpression {
                    expr: format!("field access on non-struct type `{}`", var_info.llvm_type),
                    context: "field access".to_string(),
                });
                return LlvmValue {
                    reg: "0".to_string(),
                    ty: "i32".to_string(),
                };
            }
        };

        let struct_info = match ctx.struct_registry.get(&struct_name) {
            Some(info) => info.clone(),
            None => {
                ctx.errors.push(CodegenError::UnsupportedType {
                    ty: struct_name,
                    context: "field access (unknown struct)".to_string(),
                });
                return LlvmValue {
                    reg: "0".to_string(),
                    ty: "i32".to_string(),
                };
            }
        };

        // Find field index and type.
        let field_idx = struct_info
            .fields
            .iter()
            .position(|(fname, _)| fname == field);
        let (field_index, field_type) = match field_idx {
            Some(idx) => (idx, struct_info.fields[idx].1.clone()),
            None => {
                ctx.errors.push(CodegenError::UnsupportedExpression {
                    expr: format!("unknown field `{field}` on struct `{struct_name}`"),
                    context: "field access".to_string(),
                });
                return LlvmValue {
                    reg: "0".to_string(),
                    ty: "i32".to_string(),
                };
            }
        };

        // Get the base pointer to the struct.
        let base_ptr = get_struct_base_ptr(ctx, &var_info);

        // GEP to the field.
        let gep_reg = ctx.fresh_reg();
        ctx.emit(&format!(
            "{gep_reg} = getelementptr inbounds {}, ptr {base_ptr}, i32 0, i32 {field_index}",
            struct_info.llvm_name
        ));

        // Load the field value.
        let load_reg = ctx.fresh_reg();
        ctx.emit(&format!(
            "{load_reg} = load {field_type}, ptr {gep_reg}"
        ));

        return LlvmValue {
            reg: load_reg,
            ty: field_type,
        };
    }

    ctx.errors.push(CodegenError::UnsupportedExpression {
        expr: "field access on non-ident expression".to_string(),
        context: "field access".to_string(),
    });
    LlvmValue {
        reg: "0".to_string(),
        ty: "i32".to_string(),
    }
}

/// Emit a binary operation.
fn emit_binary_op(
    ctx: &mut CodegenContext,
    op: BinOp,
    lhs: &HirExpr,
    rhs: &HirExpr,
) -> LlvmValue {
    let mut lhs_val = emit_expr(ctx, lhs, None);
    let mut rhs_val = emit_expr(ctx, rhs, Some(&lhs_val.ty));

    // If types mismatch (e.g., literal defaulted to i64 but variable is i32),
    // coerce the literal side to match the variable side.
    if lhs_val.ty != rhs_val.ty {
        let lhs_is_literal = is_literal_reg(&lhs_val.reg);
        let rhs_is_literal = is_literal_reg(&rhs_val.reg);
        if lhs_is_literal && !rhs_is_literal {
            // LHS is a literal — adopt RHS type
            lhs_val.ty = rhs_val.ty.clone();
        } else if rhs_is_literal && !lhs_is_literal {
            // RHS is a literal — adopt LHS type
            rhs_val.ty = lhs_val.ty.clone();
        } else if !lhs_is_literal && !rhs_is_literal {
            // Both are registers with different types — cast smaller to larger
            let lhs_bits = type_bits(&lhs_val.ty);
            let rhs_bits = type_bits(&rhs_val.ty);
            if lhs_bits < rhs_bits {
                let cast_reg = ctx.fresh_reg();
                ctx.emit(&format!(
                    "{cast_reg} = sext {} {} to {}",
                    lhs_val.ty, lhs_val.reg, rhs_val.ty
                ));
                lhs_val.reg = cast_reg;
                lhs_val.ty = rhs_val.ty.clone();
            } else if rhs_bits < lhs_bits {
                let cast_reg = ctx.fresh_reg();
                ctx.emit(&format!(
                    "{cast_reg} = sext {} {} to {}",
                    rhs_val.ty, rhs_val.reg, lhs_val.ty
                ));
                rhs_val.reg = cast_reg;
                rhs_val.ty = lhs_val.ty.clone();
            }
        }
    }

    let is_float = is_float_type(&lhs_val.ty);
    let is_int = is_signed_int_type_str(&lhs_val.ty);
    let in_pure = ctx.current_func_is_pure || ctx.current_func_is_const;
    let result_reg = ctx.fresh_reg();

    let instruction = match (op, is_float) {
        // Integer arithmetic — add nsw/nuw flags.
        // Regular ops (Add, Sub, Mul) get nsw (no signed wrap) since AXIOM defines
        // overflow as UB for non-Wrap variants.
        // Wrap variants (AddWrap, SubWrap, MulWrap) explicitly allow wrapping — no flags.
        (BinOp::Add, false) => {
            if is_int { "add nsw" } else { "add" }
        }
        (BinOp::Sub, false) => {
            if is_int { "sub nsw" } else { "sub" }
        }
        (BinOp::Mul, false) => {
            if is_int { "mul nsw" } else { "mul" }
        }
        (BinOp::AddWrap, false) => "add",
        (BinOp::SubWrap, false) => "sub",
        (BinOp::MulWrap, false) => "mul",
        (BinOp::Div, false) => "sdiv",
        (BinOp::Mod, false) => "srem",
        // Float arithmetic — add `fast` flag in @pure/@const functions.
        (BinOp::Add, true) => {
            if in_pure { "fadd fast" } else { "fadd" }
        }
        (BinOp::Sub, true) => {
            if in_pure { "fsub fast" } else { "fsub" }
        }
        (BinOp::Mul, true) => {
            if in_pure { "fmul fast" } else { "fmul" }
        }
        (BinOp::Div, true) => {
            if in_pure { "fdiv fast" } else { "fdiv" }
        }
        (BinOp::Mod, true) => {
            if in_pure { "frem fast" } else { "frem" }
        }
        // Logical.
        (BinOp::And, _) => "and",
        (BinOp::Or, _) => "or",
        // Comparisons (integer).
        (BinOp::Eq, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp eq {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::NotEq, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp ne {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::Lt, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp slt {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::Gt, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp sgt {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::LtEq, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp sle {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::GtEq, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp sge {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        // Comparisons (float).
        (BinOp::Eq, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp oeq {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::NotEq, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp one {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::Lt, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp olt {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::Gt, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp ogt {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::LtEq, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp ole {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::GtEq, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp oge {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        // Unsupported ops.
        (op, _) => {
            ctx.errors.push(CodegenError::UnsupportedExpression {
                expr: format!("{op:?}"),
                context: "binary operation".to_string(),
            });
            return LlvmValue {
                reg: "0".to_string(),
                ty: lhs_val.ty,
            };
        }
    };

    ctx.emit(&format!(
        "{result_reg} = {instruction} {} {}, {}",
        lhs_val.ty, lhs_val.reg, rhs_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: lhs_val.ty,
    }
}

/// Emit a unary operation.
fn emit_unary_op(
    ctx: &mut CodegenContext,
    op: UnaryOp,
    operand: &HirExpr,
    expected_type: Option<&str>,
) -> LlvmValue {
    let val = emit_expr(ctx, operand, expected_type);
    let result_reg = ctx.fresh_reg();

    match op {
        UnaryOp::Neg => {
            if is_float_type(&val.ty) {
                ctx.emit(&format!(
                    "{result_reg} = fneg {} {}",
                    val.ty, val.reg
                ));
            } else {
                ctx.emit(&format!(
                    "{result_reg} = sub {} 0, {}",
                    val.ty, val.reg
                ));
            }
            LlvmValue {
                reg: result_reg,
                ty: val.ty,
            }
        }
        UnaryOp::Not => {
            ctx.emit(&format!("{result_reg} = xor i1 {}, 1", val.reg));
            LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            }
        }
    }
}

/// Emit a function call expression.
fn emit_call(ctx: &mut CodegenContext, func: &HirExpr, args: &[HirExpr]) -> LlvmValue {
    if let HirExprKind::Ident { name } = &func.kind {
        // Check for built-in functions first.
        match name.as_str() {
            "print" => return emit_builtin_print(ctx, args),
            "print_i64" => return emit_builtin_print_i64(ctx, args),
            "print_i32" => return emit_builtin_print_i32(ctx, args),
            "print_f64" => return emit_builtin_print_f64(ctx, args),
            "widen" => return emit_builtin_widen(ctx, args),
            "narrow" => return emit_builtin_narrow(ctx, args),
            "truncate" => return emit_builtin_truncate(ctx, args),
            "abs" => return emit_builtin_abs(ctx, args),
            "abs_f64" => return emit_builtin_abs_f64(ctx, args),
            "min" => return emit_builtin_min(ctx, args),
            "min_f64" => return emit_builtin_min_f64(ctx, args),
            "max" => return emit_builtin_max(ctx, args),
            "max_f64" => return emit_builtin_max_f64(ctx, args),
            "sqrt" => return emit_builtin_sqrt(ctx, args),
            "pow" => return emit_builtin_pow(ctx, args),
            "to_f64" => return emit_builtin_to_f64(ctx, args),
            "to_f64_i64" => return emit_builtin_to_f64_i64(ctx, args),
            "band" => return emit_builtin_band(ctx, args),
            "bor" => return emit_builtin_bor(ctx, args),
            "bxor" => return emit_builtin_bxor(ctx, args),
            "shl" => return emit_builtin_shl(ctx, args),
            "shr" => return emit_builtin_shr(ctx, args),
            "lshr" => return emit_builtin_lshr(ctx, args),
            "bnot" => return emit_builtin_bnot(ctx, args),
            "rotl" => return emit_builtin_rotl(ctx, args),
            "rotr" => return emit_builtin_rotr(ctx, args),
            "heap_alloc" => return emit_builtin_heap_alloc(ctx, args),
            "heap_alloc_zeroed" => return emit_builtin_heap_alloc_zeroed(ctx, args),
            "heap_free" => return emit_builtin_heap_free(ctx, args),
            "heap_realloc" => return emit_builtin_heap_realloc(ctx, args),
            "ptr_read_i32" => return emit_builtin_ptr_read(ctx, args, "i32"),
            "ptr_read_i64" => return emit_builtin_ptr_read(ctx, args, "i64"),
            "ptr_read_f64" => return emit_builtin_ptr_read(ctx, args, "double"),
            "ptr_write_i32" => return emit_builtin_ptr_write(ctx, args, "i32"),
            "ptr_write_i64" => return emit_builtin_ptr_write(ctx, args, "i64"),
            "ptr_write_f64" => return emit_builtin_ptr_write(ctx, args, "double"),
            "arena_create" => return emit_builtin_arena_create(ctx, args),
            "arena_alloc" => return emit_builtin_arena_alloc(ctx, args),
            "arena_reset" => return emit_builtin_arena_reset(ctx, args),
            "arena_destroy" => return emit_builtin_arena_destroy(ctx, args),
            // I/O runtime builtins (axiom_rt.c)
            "file_read" => return emit_builtin_file_read(ctx, args),
            "file_write" => return emit_builtin_file_write(ctx, args),
            "file_size" => return emit_builtin_file_size(ctx, args),
            "clock_ns" => return emit_builtin_clock_ns(ctx, args),
            "get_argc" => return emit_builtin_get_argc(ctx, args),
            "get_argv" => return emit_builtin_get_argv(ctx, args),
            // Coroutine builtins (axiom_rt.c -- fibers/ucontext)
            "coro_create" => return emit_builtin_coro_create(ctx, args),
            "coro_resume" => return emit_builtin_coro_resume(ctx, args),
            "coro_yield" => return emit_builtin_coro_yield(ctx, args),
            "coro_is_done" => return emit_builtin_coro_is_done(ctx, args),
            "coro_destroy" => return emit_builtin_coro_destroy(ctx, args),
            // Threading primitives (axiom_rt.c)
            "thread_create" => return emit_builtin_thread_create(ctx, args),
            "thread_join" => return emit_builtin_thread_join(ctx, args),
            // Atomics (axiom_rt.c)
            "atomic_load" => return emit_builtin_atomic_load(ctx, args),
            "atomic_store" => return emit_builtin_atomic_store(ctx, args),
            "atomic_add" => return emit_builtin_atomic_add(ctx, args),
            "atomic_cas" => return emit_builtin_atomic_cas(ctx, args),
            // Mutex (axiom_rt.c)
            "mutex_create" => return emit_builtin_mutex_create(ctx, args),
            "mutex_lock" => return emit_builtin_mutex_lock(ctx, args),
            "mutex_unlock" => return emit_builtin_mutex_unlock(ctx, args),
            "mutex_destroy" => return emit_builtin_mutex_destroy(ctx, args),
            // Job system (axiom_rt.c -- thread pool)
            "jobs_init" => return emit_builtin_jobs_init(ctx, args),
            "job_dispatch" => return emit_builtin_job_dispatch(ctx, args),
            "job_wait" => return emit_builtin_job_wait(ctx, args),
            "jobs_shutdown" => return emit_builtin_jobs_shutdown(ctx, args),
            "num_cores" => return emit_builtin_num_cores(ctx, args),
            "job_dispatch_handle" => return emit_builtin_job_dispatch_handle(ctx, args),
            "job_dispatch_after" => return emit_builtin_job_dispatch_after(ctx, args),
            "job_wait_handle" => return emit_builtin_job_wait_handle(ctx, args),
            // Renderer / Vulkan FFI builtins (axiom_rt.c -- stub/Vulkan)
            "renderer_create" => return emit_builtin_renderer_create(ctx, args),
            "renderer_destroy" => return emit_builtin_renderer_destroy(ctx, args),
            "renderer_begin_frame" => return emit_builtin_renderer_begin_frame(ctx, args),
            "renderer_end_frame" => return emit_builtin_renderer_end_frame(ctx, args),
            "renderer_should_close" => return emit_builtin_renderer_should_close(ctx, args),
            "renderer_clear" => return emit_builtin_renderer_clear(ctx, args),
            "renderer_draw_triangles" => return emit_builtin_renderer_draw_triangles(ctx, args),
            "renderer_draw_points" => return emit_builtin_renderer_draw_points(ctx, args),
            "renderer_get_time" => return emit_builtin_renderer_get_time(ctx, args),
            "shader_load" => return emit_builtin_shader_load(ctx, args),
            "pipeline_create" => return emit_builtin_pipeline_create(ctx, args),
            "renderer_bind_pipeline" => return emit_builtin_renderer_bind_pipeline(ctx, args),
            // GPU PBR / glTF builtins (axiom-renderer gpu_* functions)
            "gpu_init" => return emit_builtin_gpu_init(ctx, args),
            "gpu_shutdown" => return emit_builtin_gpu_shutdown(ctx, args),
            "gpu_begin_frame" => return emit_builtin_gpu_begin_frame(ctx, args),
            "gpu_end_frame" => return emit_builtin_gpu_end_frame(ctx, args),
            "gpu_should_close" => return emit_builtin_gpu_should_close(ctx, args),
            "gpu_load_gltf" => return emit_builtin_gpu_load_gltf(ctx, args),
            "gpu_set_camera" => return emit_builtin_gpu_set_camera(ctx, args),
            "gpu_render" => return emit_builtin_gpu_render(ctx, args),
            "gpu_get_frame_time" => return emit_builtin_gpu_get_frame_time(ctx, args),
            "gpu_get_gpu_name" => return emit_builtin_gpu_get_gpu_name(ctx, args),
            "gpu_screenshot" => return emit_builtin_gpu_screenshot(ctx, args),
            // Option (sum type) builtins -- tagged union packed into i64
            "option_none" => return emit_builtin_option_none(ctx, args),
            "option_some" => return emit_builtin_option_some(ctx, args),
            "option_is_some" => return emit_builtin_option_is_some(ctx, args),
            "option_is_none" => return emit_builtin_option_is_none(ctx, args),
            "option_unwrap" => return emit_builtin_option_unwrap(ctx, args),
            // String builtins -- fat pointer (ptr, len) via axiom_rt.c
            "string_from_literal" => return emit_builtin_string_from_literal(ctx, args),
            "string_len" => return emit_builtin_string_len(ctx, args),
            "string_ptr" => return emit_builtin_string_ptr(ctx, args),
            "string_eq" => return emit_builtin_string_eq(ctx, args),
            "string_print" => return emit_builtin_string_print(ctx, args),
            // Vec (dynamic array) builtins -- axiom_rt.c runtime
            "vec_new" => return emit_builtin_vec_new(ctx, args),
            "vec_push_i32" => return emit_builtin_vec_push_i32(ctx, args),
            "vec_push_f64" => return emit_builtin_vec_push_f64(ctx, args),
            "vec_get_i32" => return emit_builtin_vec_get_i32(ctx, args),
            "vec_get_f64" => return emit_builtin_vec_get_f64(ctx, args),
            "vec_set_i32" => return emit_builtin_vec_set_i32(ctx, args),
            "vec_set_f64" => return emit_builtin_vec_set_f64(ctx, args),
            "vec_len" => return emit_builtin_vec_len(ctx, args),
            "vec_free" => return emit_builtin_vec_free(ctx, args),
            // Function pointer builtins
            "fn_ptr" => return emit_builtin_fn_ptr(ctx, args),
            "call_fn_ptr_i32" => return emit_builtin_call_fn_ptr_i32(ctx, args),
            "call_fn_ptr_f64" => return emit_builtin_call_fn_ptr_f64(ctx, args),
            // Result (error handling) builtins -- tagged union packed into i64
            "result_ok" => return emit_builtin_result_ok(ctx, args),
            "result_err" => return emit_builtin_result_err(ctx, args),
            "result_is_ok" => return emit_builtin_result_is_ok(ctx, args),
            "result_is_err" => return emit_builtin_result_is_err(ctx, args),
            "result_unwrap" => return emit_builtin_result_unwrap(ctx, args),
            "result_err_code" => return emit_builtin_result_err_code(ctx, args),
            // CPUID feature detection (axiom_rt.c)
            "cpu_features" => return emit_builtin_cpu_features(ctx, args),
            // G2: Input System builtins
            "is_key_down" => return emit_builtin_is_key_down(ctx, args),
            "get_mouse_x" => return emit_builtin_get_mouse_x(ctx, args),
            "get_mouse_y" => return emit_builtin_get_mouse_y(ctx, args),
            "is_mouse_down" => return emit_builtin_is_mouse_down(ctx, args),
            // G3: Audio builtins
            "play_beep" => return emit_builtin_play_beep(ctx, args),
            "play_sound" => return emit_builtin_play_sound(ctx, args),
            _ => {}
        }

        // Regular function call.
        let func_info = match ctx.functions.get(name.as_str()) {
            Some(f) => f.clone(),
            None => {
                ctx.errors.push(CodegenError::UndefinedFunction {
                    name: name.clone(),
                });
                return LlvmValue {
                    reg: "0".to_string(),
                    ty: "i32".to_string(),
                };
            }
        };

        // Optimization #5: @const compile-time evaluation.
        // If the callee is @const and all arguments are integer/float literals,
        // try to evaluate at compile time by interpreting the function body.
        if func_info.annotations.is_const {
            // First try the simple pattern match.
            if let Some(result) = try_const_eval(name, args) {
                return LlvmValue {
                    reg: result,
                    ty: func_info.return_type,
                };
            }
            // Then try evaluating the function body if we have it.
            // Uses the full evaluator with access to all @const function bodies
            // for recursive call support.
            if let Some(const_func) = ctx.const_func_bodies.get(name.as_str()).cloned() {
                // Check if all args are integer literals.
                let int_args: Option<Vec<i128>> = args
                    .iter()
                    .map(|a| {
                        if let HirExprKind::IntLiteral { value } = &a.kind {
                            Some(*value)
                        } else {
                            None
                        }
                    })
                    .collect();
                if let Some(int_args) = int_args {
                    if let Some(result) = try_const_eval_body_with_funcs(
                        &const_func,
                        &int_args,
                        &ctx.const_func_bodies,
                        CONST_EVAL_MAX_DEPTH,
                    ) {
                        return LlvmValue {
                            reg: format!("{result}"),
                            ty: func_info.return_type,
                        };
                    }
                }
            }
        }

        // Check for obvious pointer aliasing at this call site.
        // AXIOM language rule: pointer parameters must not alias each other.
        // Emit a warning if the same variable is passed as two different pointer arguments.
        {
            let mut ptr_args: Vec<(usize, &str)> = Vec::new();
            for (i, arg) in args.iter().enumerate() {
                let is_ptr = func_info
                    .param_types
                    .get(i)
                    .map(|t| t == "ptr")
                    .unwrap_or(false);
                if is_ptr {
                    if let HirExprKind::Ident { name: arg_name } = &arg.kind {
                        ptr_args.push((i, arg_name.as_str()));
                    }
                }
            }
            for i in 0..ptr_args.len() {
                for j in (i + 1)..ptr_args.len() {
                    if ptr_args[i].1 == ptr_args[j].1 {
                        ctx.warnings.push(format!(
                            "warning: pointer argument '{}' passed as both param {} and param {} \
                             to '{}'. AXIOM requires pointer parameters to be non-aliasing.",
                            ptr_args[i].1, ptr_args[i].0, ptr_args[j].0, name
                        ));
                    }
                }
            }
        }

        // Emit arguments with type hints from the function signature.
        // Add noalias to pointer arguments (AXIOM guarantees no aliasing).
        let mut arg_strs = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let expected_ty = func_info.param_types.get(i).map(|s| s.as_str());
            let val = emit_expr(ctx, arg, expected_ty);
            let arg_ty = if let Some(pt) = func_info.param_types.get(i) {
                pt.clone()
            } else {
                val.ty.clone()
            };
            if arg_ty == "ptr" {
                arg_strs.push(format!("ptr noalias {}", val.reg));
            } else {
                arg_strs.push(format!("{arg_ty} {}", val.reg));
            }
        }

        let result_reg = ctx.fresh_reg();
        let args_str = arg_strs.join(", ");

        let cc = if func_info.uses_fastcc { "fastcc " } else { "" };
        if func_info.return_type == "void" {
            ctx.emit(&format!("call {cc}void @{name}({args_str})"));
            LlvmValue {
                reg: "0".to_string(),
                ty: "void".to_string(),
            }
        } else {
            ctx.emit(&format!(
                "{result_reg} = call {cc}{} @{name}({args_str})",
                func_info.return_type
            ));
            LlvmValue {
                reg: result_reg,
                ty: func_info.return_type,
            }
        }
    } else {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "indirect function call".to_string(),
            context: "call expression".to_string(),
        });
        LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        }
    }
}

/// Emit built-in `print(msg)` -- calls C `puts()`.
fn emit_builtin_print(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_puts = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "print() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("ptr"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!("{result_reg} = call i32 @puts(ptr {})", val.reg));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `print_i64(n)` -- calls C `printf("%lld\n", n)`.
fn emit_builtin_print_i64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_printf = true;
    ctx.needs_printf_i64 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "print_i64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i64"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 (ptr, ...) @printf(ptr @.fmt.i64, i64 {})",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `widen(n)` -- sign-extends i32 to i64.
fn emit_builtin_widen(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "widen() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i64".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i32"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = sext i32 {} to i64",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i64".to_string(),
    }
}

/// Emit built-in `print_i32(n)` -- calls C `printf("%d\n", n)`.
fn emit_builtin_print_i32(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_printf = true;
    ctx.needs_printf_i32 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "print_i32() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i32"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 (ptr, ...) @printf(ptr @.fmt.i32, i32 {})",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `print_f64(x)` -- calls C `printf("%f\n", x)`.
fn emit_builtin_print_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_printf = true;
    ctx.needs_printf_f64 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "print_f64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("double"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 (ptr, ...) @printf(ptr @.fmt.f64, double {})",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `narrow(n)` -- truncates i64 to i32.
fn emit_builtin_narrow(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "narrow() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i64"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = trunc i64 {} to i32",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `truncate(x)` -- converts f64 to i32 via fptosi.
fn emit_builtin_truncate(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "truncate() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("double"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = fptosi double {} to i32",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `abs(x: i32) -> i32` -- uses `@llvm.abs.i32`.
fn emit_builtin_abs(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_abs_i32 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "abs() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i32"));
    let result_reg = ctx.fresh_reg();
    // The second argument (i1 false) means INT_MIN is not poison.
    ctx.emit(&format!(
        "{result_reg} = call i32 @llvm.abs.i32(i32 {}, i1 false)",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `abs_f64(x: f64) -> f64` -- uses `@llvm.fabs.f64`.
fn emit_builtin_abs_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_fabs_f64 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "abs_f64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("double"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call double @llvm.fabs.f64(double {})",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `min(a: i32, b: i32) -> i32` -- uses icmp + select.
fn emit_builtin_min(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "min() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let b = emit_expr(ctx, &args[1], Some("i32"));
    let cmp_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{cmp_reg} = icmp slt i32 {}, {}",
        a.reg, b.reg
    ));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = select i1 {cmp_reg}, i32 {}, i32 {}",
        a.reg, b.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `min_f64(a: f64, b: f64) -> f64` -- uses fcmp + select.
fn emit_builtin_min_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "min_f64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("double"));
    let b = emit_expr(ctx, &args[1], Some("double"));
    let cmp_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{cmp_reg} = fcmp olt double {}, {}",
        a.reg, b.reg
    ));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = select i1 {cmp_reg}, double {}, double {}",
        a.reg, b.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `max(a: i32, b: i32) -> i32` -- uses icmp + select.
fn emit_builtin_max(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "max() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let b = emit_expr(ctx, &args[1], Some("i32"));
    let cmp_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{cmp_reg} = icmp sgt i32 {}, {}",
        a.reg, b.reg
    ));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = select i1 {cmp_reg}, i32 {}, i32 {}",
        a.reg, b.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `max_f64(a: f64, b: f64) -> f64` -- uses fcmp + select.
fn emit_builtin_max_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "max_f64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("double"));
    let b = emit_expr(ctx, &args[1], Some("double"));
    let cmp_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{cmp_reg} = fcmp ogt double {}, {}",
        a.reg, b.reg
    ));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = select i1 {cmp_reg}, double {}, double {}",
        a.reg, b.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `sqrt(x: f64) -> f64` -- uses `@llvm.sqrt.f64`.
fn emit_builtin_sqrt(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_sqrt_f64 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "sqrt() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("double"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call double @llvm.sqrt.f64(double {})",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `pow(base: f64, exp: f64) -> f64` -- uses `@llvm.pow.f64`.
fn emit_builtin_pow(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_pow_f64 = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "pow() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let base = emit_expr(ctx, &args[0], Some("double"));
    let exp = emit_expr(ctx, &args[1], Some("double"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call double @llvm.pow.f64(double {}, double {})",
        base.reg, exp.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `to_f64(x: i32) -> f64` -- converts i32 to f64 via sitofp.
fn emit_builtin_to_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "to_f64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i32"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = sitofp i32 {} to double",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `to_f64_i64(x: i64) -> f64` -- converts i64 to f64 via sitofp.
fn emit_builtin_to_f64_i64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "to_f64_i64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i64"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = sitofp i64 {} to double",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

// ── Bitwise builtins ────────────────────────────────────────────────

/// Emit built-in `band(a, b)` -- bitwise AND.
fn emit_builtin_band(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "band() requires exactly 2 arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    // Default to i32 for bitwise ops (most common width for bit manipulation).
    let a = emit_expr(ctx, &args[0], Some("i32"));
    let b = emit_expr(ctx, &args[1], Some(&a.ty));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!("{result_reg} = and {} {}, {}", a.ty, a.reg, b.reg));
    LlvmValue {
        reg: result_reg,
        ty: a.ty,
    }
}

/// Emit built-in `bor(a, b)` -- bitwise OR.
fn emit_builtin_bor(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "bor() requires exactly 2 arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let b = emit_expr(ctx, &args[1], Some(&a.ty));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!("{result_reg} = or {} {}, {}", a.ty, a.reg, b.reg));
    LlvmValue {
        reg: result_reg,
        ty: a.ty,
    }
}

/// Emit built-in `bxor(a, b)` -- bitwise XOR.
fn emit_builtin_bxor(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "bxor() requires exactly 2 arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let b = emit_expr(ctx, &args[1], Some(&a.ty));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!("{result_reg} = xor {} {}, {}", a.ty, a.reg, b.reg));
    LlvmValue {
        reg: result_reg,
        ty: a.ty,
    }
}

/// Emit built-in `shl(a, n)` -- shift left.
fn emit_builtin_shl(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "shl() requires exactly 2 arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let n = emit_expr(ctx, &args[1], Some(&a.ty));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!("{result_reg} = shl {} {}, {}", a.ty, a.reg, n.reg));
    LlvmValue {
        reg: result_reg,
        ty: a.ty,
    }
}

/// Emit built-in `shr(a, n)` -- arithmetic shift right (sign-preserving).
fn emit_builtin_shr(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "shr() requires exactly 2 arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let n = emit_expr(ctx, &args[1], Some(&a.ty));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = ashr {} {}, {}",
        a.ty, a.reg, n.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: a.ty,
    }
}

/// Emit built-in `lshr(a, n)` -- logical shift right (zero-fill).
fn emit_builtin_lshr(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "lshr() requires exactly 2 arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let n = emit_expr(ctx, &args[1], Some(&a.ty));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = lshr {} {}, {}",
        a.ty, a.reg, n.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: a.ty,
    }
}

/// Emit built-in `bnot(a)` -- bitwise NOT (xor with -1).
fn emit_builtin_bnot(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "bnot() requires exactly 1 argument".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = xor {} {}, -1",
        a.ty, a.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: a.ty,
    }
}

/// Emit built-in `rotl(a, n)` -- rotate left using `@llvm.fshl.i32`.
fn emit_builtin_rotl(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_fshl_i32 = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "rotl() requires exactly 2 arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let n = emit_expr(ctx, &args[1], Some("i32"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @llvm.fshl.i32(i32 {}, i32 {}, i32 {})",
        a.reg, a.reg, n.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `rotr(a, n)` -- rotate right using `@llvm.fshr.i32`.
fn emit_builtin_rotr(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_fshr_i32 = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "rotr() requires exactly 2 arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let n = emit_expr(ctx, &args[1], Some("i32"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @llvm.fshr.i32(i32 {}, i32 {}, i32 {})",
        a.reg, a.reg, n.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Heap allocation builtins
// ---------------------------------------------------------------------------

/// Emit built-in `heap_alloc(count: i32, elem_size: i32) -> ptr`.
///
/// Calls `malloc(count * elem_size)` with `noalias` on the result.
fn emit_builtin_heap_alloc(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_malloc = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "heap_alloc() requires exactly 2 arguments (count, elem_size)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let count = emit_expr(ctx, &args[0], Some("i32"));
    let elem_size = emit_expr(ctx, &args[1], Some("i32"));

    // Widen both to i64 for the multiplication.
    let count64 = ctx.fresh_reg();
    ctx.emit(&format!(
        "{count64} = sext i32 {} to i64",
        count.reg
    ));
    let elem64 = ctx.fresh_reg();
    ctx.emit(&format!(
        "{elem64} = sext i32 {} to i64",
        elem_size.reg
    ));
    let total = ctx.fresh_reg();
    ctx.emit(&format!("{total} = mul i64 {count64}, {elem64}"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call noalias ptr @malloc(i64 {total})"
    ));
    LlvmValue {
        reg: result_reg,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `heap_alloc_zeroed(count: i32, elem_size: i32) -> ptr`.
///
/// Calls `calloc(count, elem_size)` with `noalias` on the result.
fn emit_builtin_heap_alloc_zeroed(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_calloc = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "heap_alloc_zeroed() requires exactly 2 arguments (count, elem_size)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let count = emit_expr(ctx, &args[0], Some("i32"));
    let elem_size = emit_expr(ctx, &args[1], Some("i32"));

    // Widen both to i64 for calloc.
    let count64 = ctx.fresh_reg();
    ctx.emit(&format!(
        "{count64} = sext i32 {} to i64",
        count.reg
    ));
    let elem64 = ctx.fresh_reg();
    ctx.emit(&format!(
        "{elem64} = sext i32 {} to i64",
        elem_size.reg
    ));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call noalias ptr @calloc(i64 {count64}, i64 {elem64})"
    ));
    LlvmValue {
        reg: result_reg,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `heap_free(p: ptr)`.
///
/// Calls `free(p)`.
fn emit_builtin_heap_free(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_free = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "heap_free() requires exactly 1 argument (ptr)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let ptr_val = emit_expr(ctx, &args[0], Some("ptr"));
    ctx.emit(&format!("call void @free(ptr {})", ptr_val.reg));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `heap_realloc(p: ptr, new_count: i32, elem_size: i32) -> ptr`.
///
/// Calls `realloc(p, new_count * elem_size)` with `noalias` on the result.
fn emit_builtin_heap_realloc(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_realloc = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "heap_realloc() requires exactly 3 arguments (ptr, new_count, elem_size)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let ptr_val = emit_expr(ctx, &args[0], Some("ptr"));
    let new_count = emit_expr(ctx, &args[1], Some("i32"));
    let elem_size = emit_expr(ctx, &args[2], Some("i32"));

    // Widen to i64 for the multiplication.
    let count64 = ctx.fresh_reg();
    ctx.emit(&format!(
        "{count64} = sext i32 {} to i64",
        new_count.reg
    ));
    let elem64 = ctx.fresh_reg();
    ctx.emit(&format!(
        "{elem64} = sext i32 {} to i64",
        elem_size.reg
    ));
    let total = ctx.fresh_reg();
    ctx.emit(&format!("{total} = mul i64 {count64}, {elem64}"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call noalias ptr @realloc(ptr {}, i64 {total})",
        ptr_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "ptr".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Arena (bump) allocator builtins
// ---------------------------------------------------------------------------
//
// Arena layout: { ptr base, i64 offset, i64 capacity }  (24 bytes)
//   offset 0:  ptr to backing memory (from malloc)
//   offset 8:  current bump offset (i64)
//   offset 16: total capacity in bytes (i64)
//
// All arena builtins are emitted inline as LLVM IR -- no runtime library needed.

/// Emit built-in `arena_create(size_bytes: i32) -> ptr`.
///
/// Allocates a 24-byte arena struct and a backing buffer of `size_bytes`.
/// Returns a pointer to the arena struct.
fn emit_builtin_arena_create(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_arena = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "arena_create() requires exactly 1 argument (size_bytes)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let size = emit_expr(ctx, &args[0], Some("i32"));

    // Widen size to i64.
    let size64 = ctx.fresh_reg();
    ctx.emit(&format!("{size64} = sext i32 {} to i64", size.reg));

    // Allocate the arena struct (24 bytes: ptr + i64 + i64).
    let arena = ctx.fresh_reg();
    ctx.emit(&format!("{arena} = call noalias ptr @malloc(i64 24)"));

    // Allocate the backing buffer.
    let base = ctx.fresh_reg();
    ctx.emit(&format!("{base} = call noalias ptr @malloc(i64 {size64})"));

    // Store base pointer at arena+0.
    ctx.emit(&format!("store ptr {base}, ptr {arena}"));

    // Store offset = 0 at arena+8.
    let off_ptr = ctx.fresh_reg();
    ctx.emit(&format!(
        "{off_ptr} = getelementptr i8, ptr {arena}, i64 8"
    ));
    ctx.emit(&format!("store i64 0, ptr {off_ptr}"));

    // Store capacity at arena+16.
    let cap_ptr = ctx.fresh_reg();
    ctx.emit(&format!(
        "{cap_ptr} = getelementptr i8, ptr {arena}, i64 16"
    ));
    ctx.emit(&format!("store i64 {size64}, ptr {cap_ptr}"));

    LlvmValue {
        reg: arena,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `arena_alloc(arena: ptr, count: i32, elem_size: i32) -> ptr`.
///
/// Bump-allocates `count * elem_size` bytes from the arena. Returns a pointer
/// to the allocated region. No bounds checking (by design -- the arena is
/// pre-sized by the programmer).
fn emit_builtin_arena_alloc(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_arena = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "arena_alloc() requires exactly 3 arguments (arena, count, elem_size)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let arena_val = emit_expr(ctx, &args[0], Some("ptr"));
    let count = emit_expr(ctx, &args[1], Some("i32"));
    let elem_size = emit_expr(ctx, &args[2], Some("i32"));

    // Widen count and elem_size to i64.
    let count64 = ctx.fresh_reg();
    ctx.emit(&format!("{count64} = sext i32 {} to i64", count.reg));
    let elem64 = ctx.fresh_reg();
    ctx.emit(&format!("{elem64} = sext i32 {} to i64", elem_size.reg));

    // total = count * elem_size
    let total = ctx.fresh_reg();
    ctx.emit(&format!("{total} = mul i64 {count64}, {elem64}"));

    // Load base pointer from arena+0.
    let base = ctx.fresh_reg();
    ctx.emit(&format!("{base} = load ptr, ptr {}", arena_val.reg));

    // Load current offset from arena+8.
    let off_ptr = ctx.fresh_reg();
    ctx.emit(&format!(
        "{off_ptr} = getelementptr i8, ptr {}, i64 8",
        arena_val.reg
    ));
    let offset = ctx.fresh_reg();
    ctx.emit(&format!("{offset} = load i64, ptr {off_ptr}"));

    // result = base + offset
    let result = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result} = getelementptr i8, ptr {base}, i64 {offset}"
    ));

    // new_offset = offset + total
    let new_offset = ctx.fresh_reg();
    ctx.emit(&format!("{new_offset} = add i64 {offset}, {total}"));

    // Store new_offset back at arena+8.
    ctx.emit(&format!("store i64 {new_offset}, ptr {off_ptr}"));

    LlvmValue {
        reg: result,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `arena_reset(arena: ptr)`.
///
/// Resets the arena offset to 0, instantly "freeing" all allocations.
fn emit_builtin_arena_reset(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_arena = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "arena_reset() requires exactly 1 argument (arena)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let arena_val = emit_expr(ctx, &args[0], Some("ptr"));

    // Store 0 to offset at arena+8.
    let off_ptr = ctx.fresh_reg();
    ctx.emit(&format!(
        "{off_ptr} = getelementptr i8, ptr {}, i64 8",
        arena_val.reg
    ));
    ctx.emit(&format!("store i64 0, ptr {off_ptr}"));

    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `arena_destroy(arena: ptr)`.
///
/// Frees the backing buffer and the arena struct itself.
fn emit_builtin_arena_destroy(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_arena = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "arena_destroy() requires exactly 1 argument (arena)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let arena_val = emit_expr(ctx, &args[0], Some("ptr"));

    // Load base pointer from arena+0.
    let base = ctx.fresh_reg();
    ctx.emit(&format!("{base} = load ptr, ptr {}", arena_val.reg));

    // Free the backing buffer.
    ctx.emit(&format!("call void @free(ptr {base})"));

    // Free the arena struct.
    ctx.emit(&format!("call void @free(ptr {})", arena_val.reg));

    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

// ---------------------------------------------------------------------------
// I/O runtime builtins (axiom_rt.c)
// ---------------------------------------------------------------------------

/// Emit built-in `file_read(path: ptr, out_size: ptr) -> ptr`.
///
/// Calls `axiom_file_read(path, out_size)` which reads an entire file into a
/// malloc'd buffer and writes the byte count to `*out_size`.
fn emit_builtin_file_read(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "file_read() requires exactly 2 arguments (path, out_size_ptr)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let path_val = emit_expr(ctx, &args[0], Some("ptr"));
    let out_size_val = emit_expr(ctx, &args[1], Some("ptr"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call ptr @axiom_file_read(ptr {}, ptr {})",
        path_val.reg, out_size_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `file_write(path: ptr, data: ptr, len: i64)`.
///
/// Calls `axiom_file_write(path, data, len)` which writes `len` bytes to the
/// file at `path`.
fn emit_builtin_file_write(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "file_write() requires exactly 3 arguments (path, data, len)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let path_val = emit_expr(ctx, &args[0], Some("ptr"));
    let data_val = emit_expr(ctx, &args[1], Some("ptr"));
    let len_val = emit_expr(ctx, &args[2], Some("i64"));

    ctx.emit(&format!(
        "call void @axiom_file_write(ptr {}, ptr {}, i64 {})",
        path_val.reg, data_val.reg, len_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `file_size(path: ptr) -> i64`.
///
/// Calls `axiom_file_size(path)` which returns the file size in bytes, or -1
/// on error.
fn emit_builtin_file_size(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "file_size() requires exactly 1 argument (path)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i64".to_string(),
        };
    }

    let path_val = emit_expr(ctx, &args[0], Some("ptr"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i64 @axiom_file_size(ptr {})",
        path_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i64".to_string(),
    }
}

/// Emit built-in `clock_ns() -> i64`.
///
/// Calls `axiom_clock_ns()` which returns the current monotonic clock value in
/// nanoseconds.
fn emit_builtin_clock_ns(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;

    if !args.is_empty() {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "clock_ns() takes no arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i64".to_string(),
        };
    }

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!("{result_reg} = call i64 @axiom_clock_ns()"));
    LlvmValue {
        reg: result_reg,
        ty: "i64".to_string(),
    }
}

/// Emit built-in `get_argc() -> i32`.
///
/// Calls `axiom_get_argc()` which returns the number of command-line arguments.
fn emit_builtin_get_argc(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;

    if !args.is_empty() {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "get_argc() takes no arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!("{result_reg} = call i32 @axiom_get_argc()"));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `get_argv(i: i32) -> ptr`.
///
/// Calls `axiom_get_argv(i)` which returns a pointer to the i-th command-line
/// argument string, or an empty string if out of range.
fn emit_builtin_get_argv(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "get_argv() requires exactly 1 argument (index)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let idx_val = emit_expr(ctx, &args[0], Some("i32"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call ptr @axiom_get_argv(i32 {})",
        idx_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "ptr".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Coroutine builtins (axiom_rt.c -- OS fibers / ucontext)
// ---------------------------------------------------------------------------

/// Emit built-in `coro_create(func: ptr, arg: i32) -> i32`.
///
/// Creates a new coroutine that will run `func(arg)` when resumed.
/// Returns a handle (non-negative) on success, or -1 on failure.
/// The `func` argument must be a function name (resolved to a function pointer).
fn emit_builtin_coro_create(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_coroutines = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "coro_create() requires exactly 2 arguments (func, arg)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "-1".to_string(),
            ty: "i32".to_string(),
        };
    }

    // The first argument should be a function name (identifier).
    // We emit it as a function pointer cast to the expected type.
    let func_val = emit_expr(ctx, &args[0], Some("ptr"));
    let arg_val = emit_expr(ctx, &args[1], Some("i32"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_coro_create(ptr {}, i32 {})",
        func_val.reg, arg_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `coro_resume(handle: i32) -> i32`.
///
/// Resumes the coroutine identified by `handle`. Returns the value that was
/// passed to `coro_yield`, or -1 if the coroutine is already done.
fn emit_builtin_coro_resume(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_coroutines = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "coro_resume() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "-1".to_string(),
            ty: "i32".to_string(),
        };
    }

    let handle_val = emit_expr(ctx, &args[0], Some("i32"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_coro_resume(i32 {})",
        handle_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `coro_yield(value: i32)`.
///
/// Suspends the currently running coroutine and passes `value` back to the
/// caller (the code that called `coro_resume`).
fn emit_builtin_coro_yield(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_coroutines = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "coro_yield() requires exactly 1 argument (value)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let value_val = emit_expr(ctx, &args[0], Some("i32"));

    ctx.emit(&format!(
        "call void @axiom_coro_yield(i32 {})",
        value_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `coro_is_done(handle: i32) -> i32`.
///
/// Returns 1 if the coroutine has finished executing, 0 otherwise.
fn emit_builtin_coro_is_done(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_coroutines = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "coro_is_done() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "1".to_string(),
            ty: "i32".to_string(),
        };
    }

    let handle_val = emit_expr(ctx, &args[0], Some("i32"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_coro_is_done(i32 {})",
        handle_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `coro_destroy(handle: i32)`.
///
/// Frees the resources (stack/fiber) associated with the coroutine.
fn emit_builtin_coro_destroy(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_coroutines = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "coro_destroy() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let handle_val = emit_expr(ctx, &args[0], Some("i32"));

    ctx.emit(&format!(
        "call void @axiom_coro_destroy(i32 {})",
        handle_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Threading primitives (axiom_rt.c -- threads, atomics, mutexes)
// ---------------------------------------------------------------------------

/// Emit built-in `thread_create(func: ptr, arg: ptr) -> i32`.
///
/// Creates a new OS thread that runs `func(arg)`. Returns a handle.
fn emit_builtin_thread_create(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "thread_create() requires exactly 2 arguments (func, arg)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "-1".to_string(),
            ty: "i32".to_string(),
        };
    }

    let func_val = emit_expr(ctx, &args[0], Some("ptr"));
    let arg_val = emit_expr(ctx, &args[1], Some("ptr"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_thread_create(ptr {}, ptr {})",
        func_val.reg, arg_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `thread_join(handle: i32)`.
///
/// Waits for the thread identified by `handle` to finish.
fn emit_builtin_thread_join(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "thread_join() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let handle_val = emit_expr(ctx, &args[0], Some("i32"));

    ctx.emit(&format!(
        "call void @axiom_thread_join(i32 {})",
        handle_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `atomic_load(ptr: ptr) -> i32`.
///
/// Atomically loads an i32 from the given pointer.
fn emit_builtin_atomic_load(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "atomic_load() requires exactly 1 argument (ptr)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let ptr_val = emit_expr(ctx, &args[0], Some("ptr"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_atomic_load(ptr {})",
        ptr_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `atomic_store(ptr: ptr, val: i32)`.
///
/// Atomically stores an i32 to the given pointer.
fn emit_builtin_atomic_store(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "atomic_store() requires exactly 2 arguments (ptr, val)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let ptr_val = emit_expr(ctx, &args[0], Some("ptr"));
    let val = emit_expr(ctx, &args[1], Some("i32"));

    ctx.emit(&format!(
        "call void @axiom_atomic_store(ptr {}, i32 {})",
        ptr_val.reg, val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `atomic_add(ptr: ptr, val: i32) -> i32`.
///
/// Atomically adds `val` to the i32 at `ptr`. Returns the old value.
fn emit_builtin_atomic_add(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "atomic_add() requires exactly 2 arguments (ptr, val)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let ptr_val = emit_expr(ctx, &args[0], Some("ptr"));
    let val = emit_expr(ctx, &args[1], Some("i32"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_atomic_add(ptr {}, i32 {})",
        ptr_val.reg, val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `atomic_cas(ptr: ptr, expected: i32, desired: i32) -> i32`.
///
/// Atomically compares `*ptr` with `expected`; if equal, stores `desired`.
/// Returns the old value (useful for checking if the CAS succeeded).
fn emit_builtin_atomic_cas(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "atomic_cas() requires exactly 3 arguments (ptr, expected, desired)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let ptr_val = emit_expr(ctx, &args[0], Some("ptr"));
    let expected_val = emit_expr(ctx, &args[1], Some("i32"));
    let desired_val = emit_expr(ctx, &args[2], Some("i32"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_atomic_cas(ptr {}, i32 {}, i32 {})",
        ptr_val.reg, expected_val.reg, desired_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `mutex_create() -> ptr`.
///
/// Creates and returns a new mutex handle.
fn emit_builtin_mutex_create(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if !args.is_empty() {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "mutex_create() takes no arguments".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call ptr @axiom_mutex_create()"
    ));
    LlvmValue {
        reg: result_reg,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `mutex_lock(mtx: ptr)`.
///
/// Acquires the mutex.
fn emit_builtin_mutex_lock(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "mutex_lock() requires exactly 1 argument (mtx)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let mtx_val = emit_expr(ctx, &args[0], Some("ptr"));

    ctx.emit(&format!(
        "call void @axiom_mutex_lock(ptr {})",
        mtx_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `mutex_unlock(mtx: ptr)`.
///
/// Releases the mutex.
fn emit_builtin_mutex_unlock(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "mutex_unlock() requires exactly 1 argument (mtx)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let mtx_val = emit_expr(ctx, &args[0], Some("ptr"));

    ctx.emit(&format!(
        "call void @axiom_mutex_unlock(ptr {})",
        mtx_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `mutex_destroy(mtx: ptr)`.
///
/// Destroys the mutex and frees its resources.
fn emit_builtin_mutex_destroy(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "mutex_destroy() requires exactly 1 argument (mtx)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let mtx_val = emit_expr(ctx, &args[0], Some("ptr"));

    ctx.emit(&format!(
        "call void @axiom_mutex_destroy(ptr {})",
        mtx_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Job system builtins (axiom_rt.c -- thread pool + parallel dispatch)
// ---------------------------------------------------------------------------

/// Emit built-in `jobs_init(num_workers: i32)`.
///
/// Initializes the thread pool with `num_workers` worker threads.
fn emit_builtin_jobs_init(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "jobs_init() requires exactly 1 argument (num_workers)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let workers_val = emit_expr(ctx, &args[0], Some("i32"));

    ctx.emit(&format!(
        "call void @axiom_jobs_init(i32 {})",
        workers_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `job_dispatch(func: ptr, data: ptr, total_items: i32)`.
///
/// Splits `total_items` into chunks across workers. Each worker calls
/// `func(data, chunk_start, chunk_end)`.
fn emit_builtin_job_dispatch(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "job_dispatch() requires exactly 3 arguments (func, data, total_items)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let func_val = emit_expr(ctx, &args[0], Some("ptr"));
    let data_val = emit_expr(ctx, &args[1], Some("ptr"));
    let total_val = emit_expr(ctx, &args[2], Some("i32"));

    // Release fence: ensure main thread's stores are visible to worker threads.
    ctx.emit("fence release");
    ctx.emit(&format!(
        "call void @axiom_job_dispatch(ptr {}, ptr {}, i32 {})",
        func_val.reg, data_val.reg, total_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `job_wait()`.
///
/// Blocks until all dispatched jobs have completed.
fn emit_builtin_job_wait(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if !args.is_empty() {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "job_wait() takes no arguments".to_string(),
            context: "built-in call".to_string(),
        });
    }

    ctx.emit("call void @axiom_job_wait()");
    // Acquire fence: ensure worker threads' stores are visible to main thread.
    ctx.emit("fence acquire");
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `jobs_shutdown()`.
///
/// Shuts down the thread pool and joins all worker threads.
fn emit_builtin_jobs_shutdown(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if !args.is_empty() {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "jobs_shutdown() takes no arguments".to_string(),
            context: "built-in call".to_string(),
        });
    }

    ctx.emit("call void @axiom_jobs_shutdown()");
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `num_cores() -> i32`.
///
/// Returns the number of hardware threads (logical cores) available.
fn emit_builtin_num_cores(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if !args.is_empty() {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "num_cores() takes no arguments".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!("{result_reg} = call i32 @axiom_num_cores()"));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Job handle & dependency graph builtins
// ---------------------------------------------------------------------------

/// Emit built-in `job_dispatch_handle(func: ptr, data: ptr, total: i32) -> i32`.
///
/// Dispatches a parallel job and returns a handle that can be waited on or
/// used as a dependency for subsequent jobs.
fn emit_builtin_job_dispatch_handle(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "job_dispatch_handle() requires exactly 3 arguments (func, data, total)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let func_val = emit_expr(ctx, &args[0], Some("ptr"));
    let data_val = emit_expr(ctx, &args[1], Some("ptr"));
    let total_val = emit_expr(ctx, &args[2], Some("i32"));

    // Release fence: ensure main thread's stores are visible to worker threads.
    ctx.emit("fence release");
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_job_dispatch_handle(ptr {}, ptr {}, i32 {})",
        func_val.reg, data_val.reg, total_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `job_dispatch_after(func: ptr, data: ptr, total: i32, dep: i32) -> i32`.
///
/// Dispatches a parallel job that waits for a dependency handle to complete first.
/// Returns a new handle.
fn emit_builtin_job_dispatch_after(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 4 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "job_dispatch_after() requires exactly 4 arguments (func, data, total, dep)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let func_val = emit_expr(ctx, &args[0], Some("ptr"));
    let data_val = emit_expr(ctx, &args[1], Some("ptr"));
    let total_val = emit_expr(ctx, &args[2], Some("i32"));
    let dep_val = emit_expr(ctx, &args[3], Some("i32"));

    // Release fence: ensure main thread's stores are visible to worker threads.
    ctx.emit("fence release");
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_job_dispatch_after(ptr {}, ptr {}, i32 {}, i32 {})",
        func_val.reg, data_val.reg, total_val.reg, dep_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `job_wait_handle(handle: i32)`.
///
/// Blocks until the job identified by `handle` (and transitively its
/// dependencies) has completed.
fn emit_builtin_job_wait_handle(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_threading = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "job_wait_handle() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let handle_val = emit_expr(ctx, &args[0], Some("i32"));

    ctx.emit(&format!(
        "call void @axiom_job_wait_handle(i32 {})",
        handle_val.reg
    ));
    // Acquire fence: ensure worker threads' stores are visible to main thread.
    ctx.emit("fence acquire");
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Renderer / Vulkan FFI builtins (axiom_rt.c -- stub/Vulkan)
// ---------------------------------------------------------------------------

/// Emit built-in `renderer_create(width: i32, height: i32, title: ptr) -> ptr`.
///
/// Calls `axiom_renderer_create(w, h, title)` which creates a renderer context.
fn emit_builtin_renderer_create(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "renderer_create() requires exactly 3 arguments (width, height, title)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let width_val = emit_expr(ctx, &args[0], Some("i32"));
    let height_val = emit_expr(ctx, &args[1], Some("i32"));
    let title_val = emit_expr(ctx, &args[2], Some("ptr"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call ptr @axiom_renderer_create(i32 {}, i32 {}, ptr {})",
        width_val.reg, height_val.reg, title_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `renderer_destroy(r: ptr)`.
///
/// Calls `axiom_renderer_destroy(r)` which frees the renderer context.
fn emit_builtin_renderer_destroy(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "renderer_destroy() requires exactly 1 argument (renderer)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let renderer_val = emit_expr(ctx, &args[0], Some("ptr"));
    ctx.emit(&format!(
        "call void @axiom_renderer_destroy(ptr {})",
        renderer_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `renderer_begin_frame(r: ptr) -> i32`.
///
/// Calls `axiom_renderer_begin_frame(r)`.  Returns 1 on success, 0 if the
/// window has been closed or the swapchain is unavailable.
fn emit_builtin_renderer_begin_frame(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "renderer_begin_frame() requires exactly 1 argument (renderer)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let renderer_val = emit_expr(ctx, &args[0], Some("ptr"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_renderer_begin_frame(ptr {})",
        renderer_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `renderer_end_frame(r: ptr)`.
///
/// Calls `axiom_renderer_end_frame(r)` which presents the frame and increments
/// the frame counter.
fn emit_builtin_renderer_end_frame(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "renderer_end_frame() requires exactly 1 argument (renderer)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let renderer_val = emit_expr(ctx, &args[0], Some("ptr"));
    ctx.emit(&format!(
        "call void @axiom_renderer_end_frame(ptr {})",
        renderer_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `renderer_should_close(r: ptr) -> i32`.
///
/// Calls `axiom_renderer_should_close(r)`.  Returns 1 if the window should
/// close (user pressed close, or auto-close threshold reached in stub mode).
fn emit_builtin_renderer_should_close(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "renderer_should_close() requires exactly 1 argument (renderer)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let renderer_val = emit_expr(ctx, &args[0], Some("ptr"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_renderer_should_close(ptr {})",
        renderer_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `renderer_clear(r: ptr, color: i32)`.
///
/// Calls `axiom_renderer_clear(r, color)` which fills the framebuffer with the
/// given 0xRRGGBB color value.
fn emit_builtin_renderer_clear(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "renderer_clear() requires exactly 2 arguments (renderer, color)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let renderer_val = emit_expr(ctx, &args[0], Some("ptr"));
    let color_val = emit_expr(ctx, &args[1], Some("i32"));
    ctx.emit(&format!(
        "call void @axiom_renderer_clear(ptr {}, i32 {})",
        renderer_val.reg, color_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `renderer_draw_points(r: ptr, x_arr: ptr, y_arr: ptr, colors: ptr, count: i32)`.
///
/// Calls `axiom_renderer_draw_points(r, x, y, col, n)` which draws `n` colored
/// points.  `x_arr` and `y_arr` are arrays of f64 positions, `colors` is an
/// array of u32 (0xRRGGBB) values.
fn emit_builtin_renderer_draw_points(
    ctx: &mut CodegenContext,
    args: &[HirExpr],
) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 5 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "renderer_draw_points() requires exactly 5 arguments (renderer, x_arr, y_arr, colors, count)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let renderer_val = emit_expr(ctx, &args[0], Some("ptr"));
    let x_val = emit_expr(ctx, &args[1], Some("ptr"));
    let y_val = emit_expr(ctx, &args[2], Some("ptr"));
    let colors_val = emit_expr(ctx, &args[3], Some("ptr"));
    let count_val = emit_expr(ctx, &args[4], Some("i32"));
    ctx.emit(&format!(
        "call void @axiom_renderer_draw_points(ptr {}, ptr {}, ptr {}, ptr {}, i32 {})",
        renderer_val.reg, x_val.reg, y_val.reg, colors_val.reg, count_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `renderer_draw_triangles(r: ptr, positions: ptr, colors: ptr, count: i32)`.
///
/// Calls `axiom_renderer_draw_triangles(r, pos, col, n)` which draws
/// vertex_count/3 triangles using software rasterization.
fn emit_builtin_renderer_draw_triangles(
    ctx: &mut CodegenContext,
    args: &[HirExpr],
) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 4 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "renderer_draw_triangles() requires exactly 4 arguments (renderer, positions, colors, vertex_count)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let renderer_val = emit_expr(ctx, &args[0], Some("ptr"));
    let positions_val = emit_expr(ctx, &args[1], Some("ptr"));
    let colors_val = emit_expr(ctx, &args[2], Some("ptr"));
    let count_val = emit_expr(ctx, &args[3], Some("i32"));
    ctx.emit(&format!(
        "call void @axiom_renderer_draw_triangles(ptr {}, ptr {}, ptr {}, i32 {})",
        renderer_val.reg, positions_val.reg, colors_val.reg, count_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `renderer_get_time(r: ptr) -> f64`.
///
/// Calls `axiom_renderer_get_time(r)` which returns elapsed time in seconds
/// since the renderer was created.
fn emit_builtin_renderer_get_time(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "renderer_get_time() requires exactly 1 argument (renderer)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let renderer_val = emit_expr(ctx, &args[0], Some("ptr"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call double @axiom_renderer_get_time(ptr {})",
        renderer_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `shader_load(r: ptr, path: ptr, stage: i32) -> ptr`.
///
/// Calls `axiom_shader_load(r, path, stage)` which loads a SPIR-V shader
/// module compiled by Lux.  Stage 0 = vertex, 1 = fragment.
fn emit_builtin_shader_load(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "shader_load() requires exactly 3 arguments (renderer, path, stage)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let renderer_val = emit_expr(ctx, &args[0], Some("ptr"));
    let path_val = emit_expr(ctx, &args[1], Some("ptr"));
    let stage_val = emit_expr(ctx, &args[2], Some("i32"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call ptr @axiom_shader_load(ptr {}, ptr {}, i32 {})",
        renderer_val.reg, path_val.reg, stage_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `pipeline_create(r: ptr, vert: ptr, frag: ptr) -> ptr`.
///
/// Calls `axiom_pipeline_create(r, vert_shader, frag_shader)` which creates a
/// graphics pipeline from vertex and fragment shader modules.
fn emit_builtin_pipeline_create(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "pipeline_create() requires exactly 3 arguments (renderer, vert_shader, frag_shader)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let renderer_val = emit_expr(ctx, &args[0], Some("ptr"));
    let vert_val = emit_expr(ctx, &args[1], Some("ptr"));
    let frag_val = emit_expr(ctx, &args[2], Some("ptr"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call ptr @axiom_pipeline_create(ptr {}, ptr {}, ptr {})",
        renderer_val.reg, vert_val.reg, frag_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `renderer_bind_pipeline(r: ptr, pipeline: ptr)`.
///
/// Calls `axiom_renderer_bind_pipeline(r, p)` which binds a graphics pipeline
/// for subsequent draw calls.
fn emit_builtin_renderer_bind_pipeline(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "renderer_bind_pipeline() requires exactly 2 arguments (renderer, pipeline)"
                .to_string(),
            context: "built-in call".to_string(),
        });
    }

    let renderer_val = emit_expr(ctx, &args[0], Some("ptr"));
    let pipeline_val = emit_expr(ctx, &args[1], Some("ptr"));
    ctx.emit(&format!(
        "call void @axiom_renderer_bind_pipeline(ptr {}, ptr {})",
        renderer_val.reg, pipeline_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

// ---------------------------------------------------------------------------
// GPU PBR / glTF builtins — axiom-renderer gpu_* C ABI functions
// ---------------------------------------------------------------------------

/// Emit built-in `gpu_init(width: i32, height: i32, title: ptr) -> ptr`.
fn emit_builtin_gpu_init(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_gpu = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "gpu_init() requires exactly 3 arguments (width, height, title)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let width_val = emit_expr(ctx, &args[0], Some("i32"));
    let height_val = emit_expr(ctx, &args[1], Some("i32"));
    let title_val = emit_expr(ctx, &args[2], Some("ptr"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call ptr @gpu_init(i32 {}, i32 {}, ptr {})",
        width_val.reg, height_val.reg, title_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `gpu_shutdown(handle: ptr)`.
fn emit_builtin_gpu_shutdown(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_gpu = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "gpu_shutdown() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let handle_val = emit_expr(ctx, &args[0], Some("ptr"));
    ctx.emit(&format!(
        "call void @gpu_shutdown(ptr {})",
        handle_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `gpu_begin_frame(handle: ptr) -> i32`.
fn emit_builtin_gpu_begin_frame(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_gpu = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "gpu_begin_frame() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let handle_val = emit_expr(ctx, &args[0], Some("ptr"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @gpu_begin_frame(ptr {})",
        handle_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `gpu_end_frame(handle: ptr)`.
fn emit_builtin_gpu_end_frame(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_gpu = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "gpu_end_frame() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let handle_val = emit_expr(ctx, &args[0], Some("ptr"));
    ctx.emit(&format!(
        "call void @gpu_end_frame(ptr {})",
        handle_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `gpu_should_close(handle: ptr) -> i32`.
fn emit_builtin_gpu_should_close(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_gpu = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "gpu_should_close() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let handle_val = emit_expr(ctx, &args[0], Some("ptr"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @gpu_should_close(ptr {})",
        handle_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `gpu_load_gltf(handle: ptr, path: ptr) -> i32`.
fn emit_builtin_gpu_load_gltf(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_gpu = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "gpu_load_gltf() requires exactly 2 arguments (handle, path)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let handle_val = emit_expr(ctx, &args[0], Some("ptr"));
    let path_val = emit_expr(ctx, &args[1], Some("ptr"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @gpu_load_gltf(ptr {}, ptr {})",
        handle_val.reg, path_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `gpu_set_camera(handle: ptr, ex, ey, ez, tx, ty, tz, fov: f64)`.
fn emit_builtin_gpu_set_camera(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_gpu = true;

    if args.len() != 8 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "gpu_set_camera() requires exactly 8 arguments (handle, ex, ey, ez, tx, ty, tz, fov)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let handle_val = emit_expr(ctx, &args[0], Some("ptr"));
    let ex_val = emit_expr(ctx, &args[1], Some("double"));
    let ey_val = emit_expr(ctx, &args[2], Some("double"));
    let ez_val = emit_expr(ctx, &args[3], Some("double"));
    let tx_val = emit_expr(ctx, &args[4], Some("double"));
    let ty_val = emit_expr(ctx, &args[5], Some("double"));
    let tz_val = emit_expr(ctx, &args[6], Some("double"));
    let fov_val = emit_expr(ctx, &args[7], Some("double"));

    ctx.emit(&format!(
        "call void @gpu_set_camera(ptr {}, double {}, double {}, double {}, double {}, double {}, double {}, double {})",
        handle_val.reg, ex_val.reg, ey_val.reg, ez_val.reg,
        tx_val.reg, ty_val.reg, tz_val.reg, fov_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `gpu_render(handle: ptr)`.
fn emit_builtin_gpu_render(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_gpu = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "gpu_render() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let handle_val = emit_expr(ctx, &args[0], Some("ptr"));
    ctx.emit(&format!(
        "call void @gpu_render(ptr {})",
        handle_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `gpu_get_frame_time(handle: ptr) -> f64`.
fn emit_builtin_gpu_get_frame_time(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_gpu = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "gpu_get_frame_time() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let handle_val = emit_expr(ctx, &args[0], Some("ptr"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call double @gpu_get_frame_time(ptr {})",
        handle_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `gpu_get_gpu_name(handle: ptr) -> ptr`.
fn emit_builtin_gpu_get_gpu_name(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_gpu = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "gpu_get_gpu_name() requires exactly 1 argument (handle)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let handle_val = emit_expr(ctx, &args[0], Some("ptr"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call ptr @gpu_get_gpu_name(ptr {})",
        handle_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `gpu_screenshot(handle: ptr, path: ptr) -> i32`.
fn emit_builtin_gpu_screenshot(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_gpu = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "gpu_screenshot() requires exactly 2 arguments (handle, path)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let handle_val = emit_expr(ctx, &args[0], Some("ptr"));
    let path_val = emit_expr(ctx, &args[1], Some("ptr"));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @gpu_screenshot(ptr {}, ptr {})",
        handle_val.reg, path_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

// ---------------------------------------------------------------------------
// F1: Option (sum type) builtins -- tagged union packed into i64
// ---------------------------------------------------------------------------
// Encoding: (tag << 32) | (value & 0xFFFFFFFF)
// Tag 0 = None, Tag 1 = Some(value)

/// Emit built-in `option_none() -> i64`.
/// Returns 0 (tag=0, no payload).
fn emit_builtin_option_none(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if !args.is_empty() {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "option_none() takes no arguments".to_string(),
            context: "built-in call".to_string(),
        });
    }
    LlvmValue {
        reg: "0".to_string(),
        ty: "i64".to_string(),
    }
}

/// Emit built-in `option_some(val: i32) -> i64`.
/// Packs tag=1 + val into i64: (1 << 32) | (val & 0xFFFFFFFF).
fn emit_builtin_option_some(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "option_some() requires exactly 1 argument (val: i32)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i64".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i32"));

    // Zero-extend val to i64
    let val64 = ctx.fresh_reg();
    ctx.emit(&format!("{val64} = zext i32 {} to i64", val.reg));

    // Mask to lower 32 bits (in case of sign extension issues)
    let masked = ctx.fresh_reg();
    ctx.emit(&format!("{masked} = and i64 {val64}, 4294967295"));

    // tag = 1 << 32 = 4294967296
    let result = ctx.fresh_reg();
    ctx.emit(&format!("{result} = or i64 {masked}, 4294967296"));

    LlvmValue {
        reg: result,
        ty: "i64".to_string(),
    }
}

/// Emit built-in `option_is_some(opt: i64) -> i32`.
/// Returns 1 if tag != 0 (i.e., upper 32 bits nonzero).
fn emit_builtin_option_is_some(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "option_is_some() requires exactly 1 argument (opt: i64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let opt = emit_expr(ctx, &args[0], Some("i64"));

    // tag = opt >> 32 (logical shift right)
    let tag = ctx.fresh_reg();
    ctx.emit(&format!("{tag} = lshr i64 {}, 32", opt.reg));

    // trunc to i32
    let tag32 = ctx.fresh_reg();
    ctx.emit(&format!("{tag32} = trunc i64 {tag} to i32"));

    // cmp ne 0
    let cmp = ctx.fresh_reg();
    ctx.emit(&format!("{cmp} = icmp ne i32 {tag32}, 0"));

    let result = ctx.fresh_reg();
    ctx.emit(&format!("{result} = zext i1 {cmp} to i32"));

    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `option_is_none(opt: i64) -> i32`.
/// Returns 1 if tag == 0.
fn emit_builtin_option_is_none(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "option_is_none() requires exactly 1 argument (opt: i64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let opt = emit_expr(ctx, &args[0], Some("i64"));

    // tag = opt >> 32
    let tag = ctx.fresh_reg();
    ctx.emit(&format!("{tag} = lshr i64 {}, 32", opt.reg));

    // trunc to i32
    let tag32 = ctx.fresh_reg();
    ctx.emit(&format!("{tag32} = trunc i64 {tag} to i32"));

    // cmp eq 0
    let cmp = ctx.fresh_reg();
    ctx.emit(&format!("{cmp} = icmp eq i32 {tag32}, 0"));

    let result = ctx.fresh_reg();
    ctx.emit(&format!("{result} = zext i1 {cmp} to i32"));

    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `option_unwrap(opt: i64) -> i32`.
/// Extracts the value (lower 32 bits). UB if None.
fn emit_builtin_option_unwrap(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "option_unwrap() requires exactly 1 argument (opt: i64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let opt = emit_expr(ctx, &args[0], Some("i64"));

    // trunc i64 to i32 -- extracts lower 32 bits (the value)
    let result = ctx.fresh_reg();
    ctx.emit(&format!("{result} = trunc i64 {} to i32", opt.reg));

    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

// ---------------------------------------------------------------------------
// F7: Result (error handling) builtins -- tagged union packed into i64
// Packing: (tag << 32) | (value & 0xFFFFFFFF). Tag 1 = Ok, Tag 0 = Err.
// ---------------------------------------------------------------------------

/// Emit built-in `result_ok(val: i32) -> i64`.
/// Packs tag=1 + val into i64: (1 << 32) | (val & 0xFFFFFFFF).
fn emit_builtin_result_ok(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "result_ok() requires exactly 1 argument (val: i32)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i64".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i32"));

    // Zero-extend val to i64
    let val64 = ctx.fresh_reg();
    ctx.emit(&format!("{val64} = zext i32 {} to i64", val.reg));

    // Mask to lower 32 bits
    let masked = ctx.fresh_reg();
    ctx.emit(&format!("{masked} = and i64 {val64}, 4294967295"));

    // tag = 1 << 32 = 4294967296
    let result = ctx.fresh_reg();
    ctx.emit(&format!("{result} = or i64 {masked}, 4294967296"));

    LlvmValue {
        reg: result,
        ty: "i64".to_string(),
    }
}

/// Emit built-in `result_err(code: i32) -> i64`.
/// Packs tag=0 + code into i64: (0 << 32) | (code & 0xFFFFFFFF) = just the code.
fn emit_builtin_result_err(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "result_err() requires exactly 1 argument (code: i32)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i64".to_string(),
        };
    }

    let code = emit_expr(ctx, &args[0], Some("i32"));

    // Zero-extend code to i64 (tag is 0, so upper 32 bits are 0)
    let result = ctx.fresh_reg();
    ctx.emit(&format!("{result} = zext i32 {} to i64", code.reg));

    LlvmValue {
        reg: result,
        ty: "i64".to_string(),
    }
}

/// Emit built-in `result_is_ok(r: i64) -> i32`.
/// Returns 1 if tag == 1 (upper 32 bits == 1).
fn emit_builtin_result_is_ok(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "result_is_ok() requires exactly 1 argument (r: i64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let r = emit_expr(ctx, &args[0], Some("i64"));

    // tag = r >> 32
    let tag = ctx.fresh_reg();
    ctx.emit(&format!("{tag} = lshr i64 {}, 32", r.reg));

    let tag32 = ctx.fresh_reg();
    ctx.emit(&format!("{tag32} = trunc i64 {tag} to i32"));

    // cmp eq 1 (Ok tag)
    let cmp = ctx.fresh_reg();
    ctx.emit(&format!("{cmp} = icmp eq i32 {tag32}, 1"));

    let result = ctx.fresh_reg();
    ctx.emit(&format!("{result} = zext i1 {cmp} to i32"));

    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `result_is_err(r: i64) -> i32`.
/// Returns 1 if tag == 0 (upper 32 bits == 0).
fn emit_builtin_result_is_err(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "result_is_err() requires exactly 1 argument (r: i64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let r = emit_expr(ctx, &args[0], Some("i64"));

    // tag = r >> 32
    let tag = ctx.fresh_reg();
    ctx.emit(&format!("{tag} = lshr i64 {}, 32", r.reg));

    let tag32 = ctx.fresh_reg();
    ctx.emit(&format!("{tag32} = trunc i64 {tag} to i32"));

    // cmp eq 0 (Err tag)
    let cmp = ctx.fresh_reg();
    ctx.emit(&format!("{cmp} = icmp eq i32 {tag32}, 0"));

    let result = ctx.fresh_reg();
    ctx.emit(&format!("{result} = zext i1 {cmp} to i32"));

    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `result_unwrap(r: i64) -> i32`.
/// Extracts the Ok value (lower 32 bits). UB if Err.
fn emit_builtin_result_unwrap(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "result_unwrap() requires exactly 1 argument (r: i64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let r = emit_expr(ctx, &args[0], Some("i64"));

    // trunc i64 to i32 -- extracts lower 32 bits (the value)
    let result = ctx.fresh_reg();
    ctx.emit(&format!("{result} = trunc i64 {} to i32", r.reg));

    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `result_err_code(r: i64) -> i32`.
/// Extracts the Err code (lower 32 bits). Same as unwrap but semantically for errors.
fn emit_builtin_result_err_code(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "result_err_code() requires exactly 1 argument (r: i64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let r = emit_expr(ctx, &args[0], Some("i64"));

    // trunc i64 to i32 -- extracts lower 32 bits (the error code)
    let result = ctx.fresh_reg();
    ctx.emit(&format!("{result} = trunc i64 {} to i32", r.reg));

    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

// ---------------------------------------------------------------------------
// P2: CPUID Feature Detection -- axiom_rt.c runtime
// ---------------------------------------------------------------------------

/// Emit built-in `cpu_features() -> i32`.
/// Calls the runtime's `axiom_cpu_features()` which returns a bitmask:
///   Bit 0: SSE4.2, Bit 1: AVX, Bit 2: AVX2, Bit 3: AVX-512F.
fn emit_builtin_cpu_features(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;

    if !args.is_empty() {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "cpu_features() takes no arguments".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let result = ctx.fresh_reg();
    ctx.emit(&format!("{result} = call i32 @axiom_cpu_features()"));

    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

// ---------------------------------------------------------------------------
// F2: String builtins -- fat pointer (ptr, len) via axiom_rt.c
// ---------------------------------------------------------------------------

/// Emit built-in `string_from_literal(lit: ptr) -> i64`.
/// Creates a packed string from a C string literal.
fn emit_builtin_string_from_literal(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_strings = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "string_from_literal() requires exactly 1 argument (ptr)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i64".to_string(),
        };
    }

    let ptr_val = emit_expr(ctx, &args[0], Some("ptr"));
    let result = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result} = call i64 @axiom_string_from_literal(ptr {})",
        ptr_val.reg
    ));
    LlvmValue {
        reg: result,
        ty: "i64".to_string(),
    }
}

/// Emit built-in `string_len(s: i64) -> i32`.
fn emit_builtin_string_len(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_strings = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "string_len() requires exactly 1 argument (s: i64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let s = emit_expr(ctx, &args[0], Some("i64"));
    let result = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result} = call i32 @axiom_string_len(i64 {})",
        s.reg
    ));
    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `string_ptr(s: i64) -> ptr`.
fn emit_builtin_string_ptr(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_strings = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "string_ptr() requires exactly 1 argument (s: i64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let s = emit_expr(ctx, &args[0], Some("i64"));
    let result = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result} = call ptr @axiom_string_ptr(i64 {})",
        s.reg
    ));
    LlvmValue {
        reg: result,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `string_eq(a: i64, b: i64) -> i32`.
fn emit_builtin_string_eq(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_strings = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "string_eq() requires exactly 2 arguments (a: i64, b: i64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i64"));
    let b = emit_expr(ctx, &args[1], Some("i64"));
    let result = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result} = call i32 @axiom_string_eq(i64 {}, i64 {})",
        a.reg, b.reg
    ));
    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `string_print(s: i64)`.
fn emit_builtin_string_print(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_strings = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "string_print() requires exactly 1 argument (s: i64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let s = emit_expr(ctx, &args[0], Some("i64"));
    ctx.emit(&format!("call void @axiom_string_print(i64 {})", s.reg));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

// ---------------------------------------------------------------------------
// F3: Vec (dynamic array) builtins -- axiom_rt.c runtime
// ---------------------------------------------------------------------------

/// Emit built-in `vec_new(elem_size: i32) -> ptr`.
fn emit_builtin_vec_new(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_vec = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "vec_new() requires exactly 1 argument (elem_size: i32)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    let elem_size = emit_expr(ctx, &args[0], Some("i32"));
    let result = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result} = call ptr @axiom_vec_new(i32 {})",
        elem_size.reg
    ));
    LlvmValue {
        reg: result,
        ty: "ptr".to_string(),
    }
}

/// Emit built-in `vec_push_i32(v: ptr, val: i32)`.
fn emit_builtin_vec_push_i32(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_vec = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "vec_push_i32() requires exactly 2 arguments (v: ptr, val: i32)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let v = emit_expr(ctx, &args[0], Some("ptr"));
    let val = emit_expr(ctx, &args[1], Some("i32"));
    ctx.emit(&format!(
        "call void @axiom_vec_push_i32(ptr {}, i32 {})",
        v.reg, val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `vec_push_f64(v: ptr, val: f64)`.
fn emit_builtin_vec_push_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_vec = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "vec_push_f64() requires exactly 2 arguments (v: ptr, val: f64)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let v = emit_expr(ctx, &args[0], Some("ptr"));
    let val = emit_expr(ctx, &args[1], Some("double"));
    ctx.emit(&format!(
        "call void @axiom_vec_push_f64(ptr {}, double {})",
        v.reg, val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `vec_get_i32(v: ptr, index: i32) -> i32`.
fn emit_builtin_vec_get_i32(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_vec = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "vec_get_i32() requires exactly 2 arguments (v: ptr, index: i32)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let v = emit_expr(ctx, &args[0], Some("ptr"));
    let index = emit_expr(ctx, &args[1], Some("i32"));
    let result = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result} = call i32 @axiom_vec_get_i32(ptr {}, i32 {})",
        v.reg, index.reg
    ));
    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `vec_get_f64(v: ptr, index: i32) -> f64`.
fn emit_builtin_vec_get_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_vec = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "vec_get_f64() requires exactly 2 arguments (v: ptr, index: i32)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0.0".to_string(),
            ty: "double".to_string(),
        };
    }

    let v = emit_expr(ctx, &args[0], Some("ptr"));
    let index = emit_expr(ctx, &args[1], Some("i32"));
    let result = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result} = call double @axiom_vec_get_f64(ptr {}, i32 {})",
        v.reg, index.reg
    ));
    LlvmValue {
        reg: result,
        ty: "double".to_string(),
    }
}

/// Emit built-in `vec_set_i32(v: ptr, index: i32, val: i32)`.
fn emit_builtin_vec_set_i32(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_vec = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "vec_set_i32() requires exactly 3 arguments (v: ptr, index: i32, val: i32)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let v = emit_expr(ctx, &args[0], Some("ptr"));
    let index = emit_expr(ctx, &args[1], Some("i32"));
    let val = emit_expr(ctx, &args[2], Some("i32"));
    ctx.emit(&format!(
        "call void @axiom_vec_set_i32(ptr {}, i32 {}, i32 {})",
        v.reg, index.reg, val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `vec_set_f64(v: ptr, index: i32, val: f64)`.
fn emit_builtin_vec_set_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_vec = true;

    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "vec_set_f64() requires exactly 3 arguments (v: ptr, index: i32, val: f64)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let v = emit_expr(ctx, &args[0], Some("ptr"));
    let index = emit_expr(ctx, &args[1], Some("i32"));
    let val = emit_expr(ctx, &args[2], Some("double"));
    ctx.emit(&format!(
        "call void @axiom_vec_set_f64(ptr {}, i32 {}, double {})",
        v.reg, index.reg, val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `vec_len(v: ptr) -> i32`.
fn emit_builtin_vec_len(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_vec = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "vec_len() requires exactly 1 argument (v: ptr)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let v = emit_expr(ctx, &args[0], Some("ptr"));
    let result = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result} = call i32 @axiom_vec_len(ptr {})",
        v.reg
    ));
    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `vec_free(v: ptr)`.
fn emit_builtin_vec_free(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_vec = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "vec_free() requires exactly 1 argument (v: ptr)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let v = emit_expr(ctx, &args[0], Some("ptr"));
    ctx.emit(&format!("call void @axiom_vec_free(ptr {})", v.reg));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

// ---------------------------------------------------------------------------
// F5: Function pointer builtins
// ---------------------------------------------------------------------------

/// Emit built-in `fn_ptr(func_name) -> ptr`.
/// Returns the address of a named function.
fn emit_builtin_fn_ptr(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "fn_ptr() requires exactly 1 argument (function name as identifier)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        };
    }

    // The argument must be an identifier naming a function.
    if let HirExprKind::Ident { name } = &args[0].kind {
        // Verify the function exists.
        if !ctx.functions.contains_key(name.as_str()) {
            ctx.errors.push(CodegenError::UndefinedFunction {
                name: name.clone(),
            });
            return LlvmValue {
                reg: "null".to_string(),
                ty: "ptr".to_string(),
            };
        }
        // In LLVM IR, a function name IS a pointer (ptr @func_name).
        // We just return the function as a ptr value.
        LlvmValue {
            reg: format!("@{name}"),
            ty: "ptr".to_string(),
        }
    } else {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "fn_ptr() argument must be a function name identifier".to_string(),
            context: "built-in call".to_string(),
        });
        LlvmValue {
            reg: "null".to_string(),
            ty: "ptr".to_string(),
        }
    }
}

/// Emit built-in `call_fn_ptr_i32(fptr: ptr, arg: i32) -> i32`.
/// Calls through a function pointer with one i32 argument, returning i32.
fn emit_builtin_call_fn_ptr_i32(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "call_fn_ptr_i32() requires exactly 2 arguments (fptr: ptr, arg: i32)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let fptr = emit_expr(ctx, &args[0], Some("ptr"));
    let arg = emit_expr(ctx, &args[1], Some("i32"));
    let result = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result} = call i32 {fptr_reg}(i32 {arg_reg})",
        fptr_reg = fptr.reg,
        arg_reg = arg.reg,
    ));
    LlvmValue {
        reg: result,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `call_fn_ptr_f64(fptr: ptr, arg: f64) -> f64`.
/// Calls through a function pointer with one f64 argument, returning f64.
fn emit_builtin_call_fn_ptr_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "call_fn_ptr_f64() requires exactly 2 arguments (fptr: ptr, arg: f64)"
                .to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0.0".to_string(),
            ty: "double".to_string(),
        };
    }

    let fptr = emit_expr(ctx, &args[0], Some("ptr"));
    let arg = emit_expr(ctx, &args[1], Some("double"));
    let result = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result} = call double {fptr_reg}(double {arg_reg})",
        fptr_reg = fptr.reg,
        arg_reg = arg.reg,
    ));
    LlvmValue {
        reg: result,
        ty: "double".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Pointer read/write builtins
// ---------------------------------------------------------------------------

/// Emit built-in `ptr_read_<T>(p: ptr, index: i32) -> T`.
///
/// Emits GEP + load for the given element type.
fn emit_builtin_ptr_read(
    ctx: &mut CodegenContext,
    args: &[HirExpr],
    elem_type: &str,
) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: format!("ptr_read_{elem_type}() requires exactly 2 arguments (ptr, index)"),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: elem_type.to_string(),
        };
    }

    // Ownership validation: cannot read from a writeonly_ptr.
    if let HirExprKind::Ident { ref name } = args[0].kind {
        if ctx.param_ownership.get(name) == Some(&PtrOwnership::Writeonly) {
            ctx.errors.push(CodegenError::UnsupportedExpression {
                expr: format!(
                    "cannot call ptr_read_{elem_type}() on writeonly_ptr parameter '{name}'"
                ),
                context: "ownership violation".to_string(),
            });
        }
    }

    let ptr_val = emit_expr(ctx, &args[0], Some("ptr"));
    let index = emit_expr(ctx, &args[1], Some("i32"));

    // Widen index to i64 for GEP.
    let idx64 = ctx.fresh_reg();
    ctx.emit(&format!(
        "{idx64} = sext i32 {} to i64",
        index.reg
    ));

    let gep_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{gep_reg} = getelementptr {elem_type}, ptr {}, i64 {idx64}",
        ptr_val.reg
    ));

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = load {elem_type}, ptr {gep_reg}"
    ));
    LlvmValue {
        reg: result_reg,
        ty: elem_type.to_string(),
    }
}

/// Emit built-in `ptr_write_<T>(p: ptr, index: i32, val: T)`.
///
/// Emits GEP + store for the given element type.
fn emit_builtin_ptr_write(
    ctx: &mut CodegenContext,
    args: &[HirExpr],
    elem_type: &str,
) -> LlvmValue {
    if args.len() != 3 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: format!(
                "ptr_write_{elem_type}() requires exactly 3 arguments (ptr, index, val)"
            ),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    // Ownership validation: cannot write to a readonly_ptr.
    if let HirExprKind::Ident { ref name } = args[0].kind {
        if ctx.param_ownership.get(name) == Some(&PtrOwnership::Readonly) {
            ctx.errors.push(CodegenError::UnsupportedExpression {
                expr: format!(
                    "cannot call ptr_write_{elem_type}() on readonly_ptr parameter '{name}'"
                ),
                context: "ownership violation".to_string(),
            });
        }
    }

    let ptr_val = emit_expr(ctx, &args[0], Some("ptr"));
    let index = emit_expr(ctx, &args[1], Some("i32"));
    let val = emit_expr(ctx, &args[2], Some(elem_type));

    // Widen index to i64 for GEP.
    let idx64 = ctx.fresh_reg();
    ctx.emit(&format!(
        "{idx64} = sext i32 {} to i64",
        index.reg
    ));

    let gep_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{gep_reg} = getelementptr {elem_type}, ptr {}, i64 {idx64}",
        ptr_val.reg
    ));

    ctx.emit(&format!(
        "store {elem_type} {}, ptr {gep_reg}",
        val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Return the size in bytes of an LLVM primitive type string.
fn llvm_type_size(ty: &str) -> u64 {
    match ty {
        "i1" => 1,
        "i8" => 1,
        "i16" => 2,
        "i32" => 4,
        "i64" => 8,
        "i128" => 16,
        "half" | "bfloat" => 2,
        "float" => 4,
        "double" => 8,
        "ptr" => 8,
        _ => 8, // conservative default
    }
}

/// Convert an HIR type to the LLVM type used for function parameters.
///
/// Arrays are passed by pointer, so `array[T, N]` becomes `ptr`.
fn hir_type_to_llvm_param(ty: &HirType) -> Result<String, CodegenError> {
    if matches!(ty, HirType::Array { .. } | HirType::UserDefined(_)) {
        // Arrays and structs are passed by pointer (C ABI for aggregates).
        Ok("ptr".to_string())
    } else {
        hir_type_to_llvm(ty)
    }
}

/// Returns the LLVM parameter attribute string for a given HIR type.
///
/// - `Ptr` → `"ptr noalias"`
/// - `ReadonlyPtr` → `"ptr noalias readonly"`
/// - `WriteonlyPtr` → `"ptr noalias writeonly"`
/// - everything else → just the LLVM type
fn hir_type_to_llvm_param_with_attrs(ty: &HirType) -> Result<String, CodegenError> {
    match ty {
        HirType::ReadonlyPtr { .. } => Ok("ptr noalias readonly".to_string()),
        HirType::WriteonlyPtr { .. } => Ok("ptr noalias writeonly".to_string()),
        _ => {
            let llvm_type = hir_type_to_llvm_param(ty)?;
            if llvm_type == "ptr" {
                Ok("ptr noalias".to_string())
            } else {
                Ok(llvm_type)
            }
        }
    }
}

/// Convert an HIR type to its LLVM IR type string.
fn hir_type_to_llvm(ty: &HirType) -> Result<String, CodegenError> {
    match ty {
        HirType::Primitive(p) => Ok(primitive_to_llvm(*p)),
        HirType::UserDefined(name) => Ok(format!("%struct.{name}")),
        HirType::Tensor { .. } => Err(CodegenError::UnsupportedType {
            ty: "tensor".to_string(),
            context: "tensor types not yet supported".to_string(),
        }),
        HirType::Array { ref element, size } => {
            let elem_llvm = hir_type_to_llvm(element)?;
            Ok(format!("[{size} x {elem_llvm}]"))
        }
        HirType::Slice { .. } => Err(CodegenError::UnsupportedType {
            ty: "slice".to_string(),
            context: "slice types not yet supported".to_string(),
        }),
        HirType::Ptr { .. } => Ok("ptr".to_string()),
        HirType::ReadonlyPtr { .. } => Ok("ptr".to_string()),
        HirType::WriteonlyPtr { .. } => Ok("ptr".to_string()),
        HirType::Tuple { .. } => Err(CodegenError::UnsupportedType {
            ty: "tuple".to_string(),
            context: "tuple types not yet supported".to_string(),
        }),
        HirType::Fn { .. } => Err(CodegenError::UnsupportedType {
            ty: "fn".to_string(),
            context: "function types not yet supported".to_string(),
        }),
        HirType::Unknown(name) if name == "void" => Ok("void".to_string()),
        HirType::Unknown(name) => Err(CodegenError::UnsupportedType {
            ty: name.clone(),
            context: "unknown type".to_string(),
        }),
    }
}

/// Convert a primitive type to its LLVM IR type string.
fn primitive_to_llvm(p: PrimitiveType) -> String {
    match p {
        PrimitiveType::I8 | PrimitiveType::U8 => "i8".to_string(),
        PrimitiveType::I16 | PrimitiveType::U16 => "i16".to_string(),
        PrimitiveType::I32 | PrimitiveType::U32 => "i32".to_string(),
        PrimitiveType::I64 | PrimitiveType::U64 => "i64".to_string(),
        PrimitiveType::I128 | PrimitiveType::U128 => "i128".to_string(),
        PrimitiveType::F16 => "half".to_string(),
        PrimitiveType::Bf16 => "bfloat".to_string(),
        PrimitiveType::F32 => "float".to_string(),
        PrimitiveType::F64 => "double".to_string(),
        PrimitiveType::Bool => "i1".to_string(),
    }
}

/// Check whether an LLVM type string represents a floating-point type.
fn is_float_type(ty: &str) -> bool {
    matches!(ty, "float" | "double" | "half" | "bfloat")
}

/// Check if a register name is a literal constant (number, not a %register).
fn is_literal_reg(reg: &str) -> bool {
    !reg.starts_with('%') && !reg.starts_with('@')
}

/// Get the bit width of an integer type string.
fn type_bits(ty: &str) -> u32 {
    match ty {
        "i1" => 1,
        "i8" => 8,
        "i16" => 16,
        "i32" => 32,
        "i64" => 64,
        "i128" => 128,
        "float" | "half" | "bfloat" => 32,
        "double" => 64,
        _ => 64,
    }
}

/// Try to evaluate a `@const` function call with all-literal arguments at compile time.
///
/// Returns `Some(literal_string)` if evaluation succeeds, `None` if arguments
/// are not all literals or the function body is too complex for the simple evaluator.
///
/// This is a simple evaluator that handles basic arithmetic. It works by
/// pattern-matching on the argument expressions to extract literal values,
/// then looking up the function body in the module. For the initial implementation,
/// we only handle direct single-expression-return functions with basic arithmetic.
fn try_const_eval(func_name: &str, args: &[HirExpr]) -> Option<String> {
    // Extract all-integer literal arguments.
    let mut int_args = Vec::new();
    let mut float_args = Vec::new();
    let mut all_int = true;
    let mut all_float = true;

    for arg in args {
        match &arg.kind {
            HirExprKind::IntLiteral { value } => {
                int_args.push(*value);
                all_float = false;
            }
            HirExprKind::FloatLiteral { value } => {
                float_args.push(*value);
                all_int = false;
            }
            _ => return None, // Non-literal argument, can't evaluate at compile time.
        }
    }

    if !all_int && !all_float {
        return None; // Mixed types, not handled in simple evaluator.
    }

    // For the simple evaluator, we can't look up the function body from just the
    // name (we'd need the full module). Instead, we provide a mechanism for the
    // most common patterns.
    //
    // The real power comes from the @const annotation telling LLVM this function
    // is speculatable + memory(none), enabling LLVM's own constant folding.
    // Here we handle the simplest case: functions we can recognize by name pattern.
    //
    // This is intentionally conservative — we only fold what we can prove correct.
    // The _function_ name is not enough, but for well-known patterns like
    // single-arg arithmetic, we return None and let LLVM handle it.
    //
    // However, the `@const` attribute group (speculatable + readnone) means LLVM
    // will aggressively constant-fold these calls when possible.
    let _ = (func_name, &int_args, &float_args);

    None
}

/// Maximum recursion depth for compile-time const evaluation.
///
/// Prevents stack overflow from deeply recursive `@const` functions and
/// ensures the compiler terminates even on accidentally infinite recursion.
const CONST_EVAL_MAX_DEPTH: usize = 1000;

/// Try to evaluate a `@const` function at compile time, with access to other
/// `@const` function bodies for recursive call support.
fn try_const_eval_body_with_funcs(
    func: &HirFunction,
    int_args: &[i128],
    all_const_funcs: &HashMap<String, HirFunction>,
    fuel: usize,
) -> Option<i128> {
    if fuel == 0 {
        return None; // Recursion depth exceeded.
    }
    let param_map: HashMap<String, i128> = func
        .params
        .iter()
        .zip(int_args.iter())
        .map(|(p, &v)| (p.name.clone(), v))
        .collect();
    eval_const_block(&func.body, &param_map, all_const_funcs, fuel)
}

/// Evaluate a block of statements, returning the value from the first `return` encountered.
///
/// Supports `let` bindings, `if/else` branching, and `return` statements.
fn eval_const_block(
    block: &HirBlock,
    params: &HashMap<String, i128>,
    funcs: &HashMap<String, HirFunction>,
    fuel: usize,
) -> Option<i128> {
    // Use a mutable scope for local variable bindings.
    let mut locals = params.clone();

    for stmt in &block.stmts {
        match &stmt.kind {
            HirStmtKind::Return { ref value } => {
                return eval_const_expr_full(value, &locals, funcs, fuel);
            }
            HirStmtKind::Let { name, value: Some(init_expr), .. } => {
                let val = eval_const_expr_full(init_expr, &locals, funcs, fuel)?;
                locals.insert(name.clone(), val);
            }
            HirStmtKind::Let { .. } => {
                return None; // Uninitialized let, can't evaluate.
            }
            HirStmtKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let cond_val = eval_const_expr_full(condition, &locals, funcs, fuel)?;
                if cond_val != 0 {
                    // Truthy: execute then_block
                    if let Some(result) = eval_const_block(then_block, &locals, funcs, fuel) {
                        return Some(result);
                    }
                } else if let Some(else_blk) = else_block {
                    // Falsy: execute else_block
                    if let Some(result) = eval_const_block(else_blk, &locals, funcs, fuel) {
                        return Some(result);
                    }
                }
                // Neither branch returned -- continue to next statement.
            }
            HirStmtKind::Assign { target, value } => {
                // Handle simple ident assignment for mutable locals.
                if let HirExprKind::Ident { name } = &target.kind {
                    let val = eval_const_expr_full(value, &locals, funcs, fuel)?;
                    locals.insert(name.clone(), val);
                } else {
                    return None; // Complex assignment target, bail.
                }
            }
            _ => return None, // Unsupported statement kind (for, while, expr stmt).
        }
    }

    None // No return found in block.
}

/// Evaluate a constant expression given parameter/local bindings and access to
/// `@const` function bodies for recursive call support.
fn eval_const_expr_full(
    expr: &HirExpr,
    params: &HashMap<String, i128>,
    funcs: &HashMap<String, HirFunction>,
    fuel: usize,
) -> Option<i128> {
    match &expr.kind {
        HirExprKind::IntLiteral { value } => Some(*value),
        HirExprKind::BoolLiteral { value } => Some(if *value { 1 } else { 0 }),
        HirExprKind::Ident { name } => params.get(name).copied(),
        HirExprKind::BinaryOp { op, lhs, rhs } => {
            let l = eval_const_expr_full(lhs, params, funcs, fuel)?;
            let r = eval_const_expr_full(rhs, params, funcs, fuel)?;
            match op {
                BinOp::Add | BinOp::AddWrap => Some(l.wrapping_add(r)),
                BinOp::Sub | BinOp::SubWrap => Some(l.wrapping_sub(r)),
                BinOp::Mul | BinOp::MulWrap => Some(l.wrapping_mul(r)),
                BinOp::Div => {
                    if r == 0 { None } else { Some(l / r) }
                }
                BinOp::Mod => {
                    if r == 0 { None } else { Some(l % r) }
                }
                BinOp::Eq => Some(if l == r { 1 } else { 0 }),
                BinOp::NotEq => Some(if l != r { 1 } else { 0 }),
                BinOp::Lt => Some(if l < r { 1 } else { 0 }),
                BinOp::Gt => Some(if l > r { 1 } else { 0 }),
                BinOp::LtEq => Some(if l <= r { 1 } else { 0 }),
                BinOp::GtEq => Some(if l >= r { 1 } else { 0 }),
                BinOp::And => Some(if l != 0 && r != 0 { 1 } else { 0 }),
                BinOp::Or => Some(if l != 0 || r != 0 { 1 } else { 0 }),
                _ => None,
            }
        }
        HirExprKind::UnaryOp { op, operand } => {
            let v = eval_const_expr_full(operand, params, funcs, fuel)?;
            match op {
                UnaryOp::Neg => Some(-v),
                UnaryOp::Not => Some(if v == 0 { 1 } else { 0 }),
            }
        }
        HirExprKind::Call { func: callee, args } => {
            // Handle recursive calls to other @const functions.
            if fuel == 0 {
                return None;
            }
            let func_name = match &callee.kind {
                HirExprKind::Ident { name } => name.as_str(),
                _ => return None,
            };
            let callee_func = funcs.get(func_name)?;
            let call_args: Option<Vec<i128>> = args
                .iter()
                .map(|a| eval_const_expr_full(a, params, funcs, fuel))
                .collect();
            let call_args = call_args?;
            try_const_eval_body_with_funcs(callee_func, &call_args, funcs, fuel - 1)
        }
        _ => None,
    }
}

/// Get the target triple for the current host platform.
fn get_target_triple() -> &'static str {
    if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-macosx"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

/// Escape a string for LLVM IR constant syntax.
///
/// LLVM uses `\xx` hex escapes for non-printable characters.
fn escape_llvm_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'\n' => result.push_str("\\0A"),
            b'\r' => result.push_str("\\0D"),
            b'\t' => result.push_str("\\09"),
            b'\0' => result.push_str("\\00"),
            b'\\' => result.push_str("\\5C"),
            b'"' => result.push_str("\\22"),
            0x20..=0x7E => result.push(byte as char),
            _ => {
                let _ = write!(result, "\\{byte:02X}");
            }
        }
    }
    result
}

/// Format a float value for LLVM IR.
///
/// LLVM requires at least one digit on each side of the decimal point.
fn format_float(value: f64) -> String {
    if value == 0.0 {
        return "0.0".to_string();
    }
    let s = format!("{value}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        // Ensure there's a digit after the dot.
        if s.ends_with('.') {
            format!("{s}0")
        } else {
            s
        }
    } else {
        format!("{s}.0")
    }
}

// ---------------------------------------------------------------------------
// G2: Input System builtins
// ---------------------------------------------------------------------------

/// Emit built-in `is_key_down(key_code: i32) -> i32`.
///
/// Returns 1 if the given key is currently pressed, 0 otherwise.
fn emit_builtin_is_key_down(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "is_key_down() requires exactly 1 argument (key_code)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let key_val = emit_expr(ctx, &args[0], Some("i32"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_is_key_down(i32 {})",
        key_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `get_mouse_x() -> i32`.
///
/// Returns the current mouse X position in client coordinates.
fn emit_builtin_get_mouse_x(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if !args.is_empty() {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "get_mouse_x() takes no arguments".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!("{result_reg} = call i32 @axiom_get_mouse_x()"));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `get_mouse_y() -> i32`.
///
/// Returns the current mouse Y position in client coordinates.
fn emit_builtin_get_mouse_y(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if !args.is_empty() {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "get_mouse_y() takes no arguments".to_string(),
            context: "built-in call".to_string(),
        });
    }

    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!("{result_reg} = call i32 @axiom_get_mouse_y()"));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `is_mouse_down(button: i32) -> i32`.
///
/// Returns 1 if the given mouse button is pressed (0=left, 1=right, 2=middle).
fn emit_builtin_is_mouse_down(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "is_mouse_down() requires exactly 1 argument (button)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let btn_val = emit_expr(ctx, &args[0], Some("i32"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 @axiom_is_mouse_down(i32 {})",
        btn_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

// ---------------------------------------------------------------------------
// G3: Audio builtins
// ---------------------------------------------------------------------------

/// Emit built-in `play_beep(freq: i32, duration_ms: i32)`.
///
/// Plays a beep at the given frequency for the given duration (Windows Beep API).
fn emit_builtin_play_beep(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "play_beep() requires exactly 2 arguments (freq, duration_ms)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let freq_val = emit_expr(ctx, &args[0], Some("i32"));
    let dur_val = emit_expr(ctx, &args[1], Some("i32"));
    ctx.emit(&format!(
        "call void @axiom_play_beep(i32 {}, i32 {})",
        freq_val.reg, dur_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

/// Emit built-in `play_sound(path: ptr)`.
///
/// Plays a sound file asynchronously (Windows PlaySound API).
fn emit_builtin_play_sound(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_runtime = true;
    ctx.needs_renderer = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "play_sound() requires exactly 1 argument (path)".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "void".to_string(),
        };
    }

    let path_val = emit_expr(ctx, &args[0], Some("ptr"));
    ctx.emit(&format!(
        "call void @axiom_play_sound(ptr {})",
        path_val.reg
    ));
    LlvmValue {
        reg: "0".to_string(),
        ty: "void".to_string(),
    }
}

#[cfg(test)]
#[path = "llvm_tests.rs"]
mod tests;
