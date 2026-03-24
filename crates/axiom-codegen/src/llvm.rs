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
    HirType, PrimitiveType, UnaryOp,
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
    /// Whether the function is annotated with `@lifetime(scope)`.
    is_lifetime_scope: bool,
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

        let func_annots = extract_func_annotations(&func.annotations, &func.params);

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

/// Extract optimization annotation flags from a function's annotations.
fn extract_func_annotations(
    annotations: &[axiom_hir::HirAnnotation],
    params: &[HirParam],
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
            _ => {}
        }
    }
    // Check if any parameter is a pointer/array/struct type (meaning the function reads arg memory).
    annots.reads_arg_memory = params.iter().any(|p| {
        matches!(
            p.ty,
            HirType::Array { .. }
                | HirType::Ptr { .. }
                | HirType::Slice { .. }
                | HirType::UserDefined(_)
        )
    });
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
    ctx.block_terminated = false;
    ctx.current_return_type = String::new();

    let ret_type = match hir_type_to_llvm(&func.return_type) {
        Ok(t) => t,
        Err(e) => {
            ctx.errors.push(e);
            return;
        }
    };

    ctx.current_return_type = ret_type.clone();

    // Extract optimization annotations for the current function.
    let func_annots = extract_func_annotations(&func.annotations, &func.params);
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

    ctx.emit_raw("}");

    // Reset per-function optimization state.
    ctx.current_func_is_pure = false;
    ctx.current_func_is_const = false;
    ctx.current_func_is_vectorizable = false;
    ctx.current_func_reads_argmem = false;
}

/// Build the function attribute suffix string (e.g., ` #0`).
///
/// For `@pure` functions: memory(none) or memory(argmem: read) + nounwind
/// For `@const` functions: memory(none) + nounwind + speculatable
fn build_func_attr_suffix(ctx: &mut CodegenContext, annots: &FuncAnnotations) -> String {
    if !annots.is_pure && !annots.is_const {
        return String::new();
    }

    let mut attrs = Vec::new();

    if annots.is_const {
        // @const implies no memory access, speculatable.
        attrs.push("memory(none)");
        attrs.push("nounwind");
        attrs.push("willreturn");
        attrs.push("nosync");
        attrs.push("speculatable");
    } else if annots.is_pure {
        if annots.reads_arg_memory {
            // @pure with pointer args: reads argument memory only.
            attrs.push("memory(argmem: read)");
        } else {
            // @pure without pointer args: no memory access at all.
            attrs.push("memory(none)");
        }
        attrs.push("nounwind");
        attrs.push("willreturn");
        attrs.push("nosync");
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
        match hir_type_to_llvm_param(&param.ty) {
            Ok(llvm_type) => {
                if llvm_type == "ptr" {
                    // AXIOM has no pointer aliasing — emit noalias on all ptr params.
                    parts.push(format!("ptr noalias %{}", param.name));
                } else {
                    parts.push(format!("{llvm_type} %{}", param.name));
                }
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
        } => emit_for(ctx, var, var_type, iterable, body),
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
fn emit_for(
    ctx: &mut CodegenContext,
    var: &str,
    var_type: &HirType,
    iterable: &HirExpr,
    body: &HirBlock,
) {
    let loop_type = match hir_type_to_llvm(var_type) {
        Ok(t) => t,
        Err(e) => {
            ctx.errors.push(e);
            return;
        }
    };

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

        // Optimization #7: Loop vectorization hints for @vectorizable functions.
        if ctx.current_func_is_vectorizable {
            let loop_md_id = ctx.fresh_metadata_id();
            let vec_enable_id = ctx.fresh_metadata_id();
            ctx.metadata_entries.push(format!(
                "!{loop_md_id} = distinct !{{!{loop_md_id}, !{vec_enable_id}}}"
            ));
            ctx.metadata_entries.push(format!(
                "!{vec_enable_id} = !{{!\"llvm.loop.vectorize.enable\", i1 true}}"
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
                    if let Some(result) = try_const_eval_body(&const_func, &int_args) {
                        return LlvmValue {
                            reg: format!("{result}"),
                            ty: func_info.return_type,
                        };
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
            ctx.emit(&format!("{cc}call void @{name}({args_str})"));
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

/// Try to evaluate a `@const` function at compile time given the HIR function body.
///
/// This is called during codegen for modules where we have access to the function body.
/// For single-expression return functions with basic arithmetic on parameters,
/// we can compute the result directly.
fn try_const_eval_body(
    func: &HirFunction,
    int_args: &[i128],
) -> Option<i128> {
    // Only handle functions with a single return statement in the body.
    if func.body.stmts.len() != 1 {
        return None;
    }
    if let HirStmtKind::Return { ref value } = func.body.stmts[0].kind {
        let param_map: HashMap<String, i128> = func
            .params
            .iter()
            .zip(int_args.iter())
            .map(|(p, &v)| (p.name.clone(), v))
            .collect();
        eval_const_expr(value, &param_map)
    } else {
        None
    }
}

/// Evaluate a constant expression given parameter bindings.
fn eval_const_expr(expr: &HirExpr, params: &HashMap<String, i128>) -> Option<i128> {
    match &expr.kind {
        HirExprKind::IntLiteral { value } => Some(*value),
        HirExprKind::Ident { name } => params.get(name).copied(),
        HirExprKind::BinaryOp { op, lhs, rhs } => {
            let l = eval_const_expr(lhs, params)?;
            let r = eval_const_expr(rhs, params)?;
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
                _ => None,
            }
        }
        HirExprKind::UnaryOp { op, operand } => {
            let v = eval_const_expr(operand, params)?;
            match op {
                UnaryOp::Neg => Some(-v),
                UnaryOp::Not => None, // Not meaningful for integers.
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_hir::{
        HirAnnotation, HirAnnotationKind, HirBlock, HirExpr, HirExprKind,
        HirExternFunction, HirFunction, HirModule, HirParam, HirStmt, HirStmtKind,
        HirType, NodeId, PrimitiveType, SPAN_DUMMY,
    };

    /// Helper: create a dummy span.
    fn span() -> axiom_lexer::Span {
        SPAN_DUMMY
    }

    /// Helper: create a dummy node ID.
    fn nid(n: u32) -> NodeId {
        NodeId(n)
    }

    /// Helper: create an integer literal expression.
    fn int_lit(value: i128) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::IntLiteral { value },
            span: span(),
        }
    }

    /// Helper: create a float literal expression.
    fn float_lit(value: f64) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::FloatLiteral { value },
            span: span(),
        }
    }

    /// Helper: create a bool literal expression.
    fn bool_lit(value: bool) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::BoolLiteral { value },
            span: span(),
        }
    }

    /// Helper: create a string literal expression.
    fn str_lit(value: &str) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::StringLiteral {
                value: value.to_string(),
            },
            span: span(),
        }
    }

    /// Helper: create an identifier expression.
    fn ident(name: &str) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::Ident {
                name: name.to_string(),
            },
            span: span(),
        }
    }

    /// Helper: create a binary op expression.
    fn binop(op: BinOp, lhs: HirExpr, rhs: HirExpr) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::BinaryOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span: span(),
        }
    }

    /// Helper: create a function call expression.
    fn call(func_name: &str, args: Vec<HirExpr>) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::Call {
                func: Box::new(ident(func_name)),
                args,
            },
            span: span(),
        }
    }

    /// Helper: create a unary op expression.
    fn unaryop(op: UnaryOp, operand: HirExpr) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::UnaryOp {
                op,
                operand: Box::new(operand),
            },
            span: span(),
        }
    }

    /// Helper: create a block with statements.
    fn block(stmts: Vec<HirStmt>) -> HirBlock {
        HirBlock {
            id: nid(0),
            annotations: vec![],
            stmts,
            span: span(),
        }
    }

    /// Helper: create a statement.
    fn stmt(kind: HirStmtKind) -> HirStmt {
        HirStmt {
            id: nid(0),
            kind,
            span: span(),
            annotations: Vec::new(),
        }
    }

    /// Helper: create a statement with annotations (e.g., `@lifetime(scope)` on a let binding).
    fn stmt_with_annotations(kind: HirStmtKind, annotations: Vec<HirAnnotation>) -> HirStmt {
        HirStmt {
            id: nid(0),
            kind,
            span: span(),
            annotations,
        }
    }

    /// Helper: create a function.
    fn func(
        name: &str,
        params: Vec<HirParam>,
        return_type: HirType,
        body: HirBlock,
    ) -> HirFunction {
        HirFunction {
            id: nid(0),
            name: name.to_string(),
            name_span: span(),
            annotations: vec![],
            params,
            return_type,
            body,
            span: span(),
        }
    }

    /// Helper: create a param.
    fn param(name: &str, ty: HirType) -> HirParam {
        HirParam {
            id: nid(0),
            name: name.to_string(),
            name_span: span(),
            ty,
            annotations: vec![],
        }
    }

    /// Helper: create a module with functions.
    fn module(name: Option<&str>, functions: Vec<HirFunction>) -> HirModule {
        HirModule {
            name: name.map(|s| s.to_string()),
            annotations: vec![],
            functions,
            extern_functions: vec![],
            structs: vec![],
            type_aliases: vec![],
            imports: vec![],
        }
    }

    /// Helper: create a module with functions and extern functions.
    fn module_with_externs(
        name: Option<&str>,
        functions: Vec<HirFunction>,
        extern_functions: Vec<HirExternFunction>,
    ) -> HirModule {
        HirModule {
            name: name.map(|s| s.to_string()),
            annotations: vec![],
            functions,
            extern_functions,
            structs: vec![],
            type_aliases: vec![],
            imports: vec![],
        }
    }

    /// Helper: create a module with functions and struct definitions.
    fn module_with_structs(
        name: Option<&str>,
        functions: Vec<HirFunction>,
        structs: Vec<HirStruct>,
    ) -> HirModule {
        HirModule {
            name: name.map(|s| s.to_string()),
            annotations: vec![],
            functions,
            extern_functions: vec![],
            structs,
            type_aliases: vec![],
            imports: vec![],
        }
    }

    /// Helper: create an HirStruct.
    fn hir_struct(name: &str, fields: Vec<axiom_hir::HirStructField>) -> HirStruct {
        HirStruct {
            id: nid(0),
            name: name.to_string(),
            name_span: span(),
            annotations: vec![],
            fields,
            span: span(),
        }
    }

    /// Helper: create an HirStructField.
    fn struct_field(name: &str, ty: HirType) -> axiom_hir::HirStructField {
        axiom_hir::HirStructField {
            id: nid(0),
            name: name.to_string(),
            name_span: span(),
            ty,
            annotations: vec![],
        }
    }

    /// Helper: create a field access expression.
    fn field_access(base: HirExpr, field: &str) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::FieldAccess {
                expr: Box::new(base),
                field: field.to_string(),
            },
            span: span(),
        }
    }

    // -----------------------------------------------------------------------
    // Type mapping tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_numeric_types() {
        assert_eq!(primitive_to_llvm(PrimitiveType::I8), "i8");
        assert_eq!(primitive_to_llvm(PrimitiveType::I16), "i16");
        assert_eq!(primitive_to_llvm(PrimitiveType::I32), "i32");
        assert_eq!(primitive_to_llvm(PrimitiveType::I64), "i64");
        assert_eq!(primitive_to_llvm(PrimitiveType::I128), "i128");
        assert_eq!(primitive_to_llvm(PrimitiveType::U8), "i8");
        assert_eq!(primitive_to_llvm(PrimitiveType::U16), "i16");
        assert_eq!(primitive_to_llvm(PrimitiveType::U32), "i32");
        assert_eq!(primitive_to_llvm(PrimitiveType::U64), "i64");
        assert_eq!(primitive_to_llvm(PrimitiveType::U128), "i128");
        assert_eq!(primitive_to_llvm(PrimitiveType::F16), "half");
        assert_eq!(primitive_to_llvm(PrimitiveType::Bf16), "bfloat");
        assert_eq!(primitive_to_llvm(PrimitiveType::F32), "float");
        assert_eq!(primitive_to_llvm(PrimitiveType::F64), "double");
        assert_eq!(primitive_to_llvm(PrimitiveType::Bool), "i1");
    }

    // -----------------------------------------------------------------------
    // Basic function tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_main_return_zero() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![stmt(HirStmtKind::Return {
                    value: int_lit(0),
                })]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("define i32 @main()"), "should define main");
        assert!(ir.contains("ret i32 0"), "should return 0");
    }

    #[test]
    fn test_function_params() {
        let m = module(
            Some("test"),
            vec![func(
                "add",
                vec![
                    param("a", HirType::Primitive(PrimitiveType::I32)),
                    param("b", HirType::Primitive(PrimitiveType::I32)),
                ],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![stmt(HirStmtKind::Return {
                    value: binop(BinOp::Add, ident("a"), ident("b")),
                })]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("@add(i32 %a, i32 %b)"),
            "should define add with params"
        );
        assert!(ir.contains("%a.addr = alloca i32"), "should alloca param a");
        assert!(
            ir.contains("store i32 %a, ptr %a.addr"),
            "should store param a"
        );
        assert!(ir.contains("%b.addr = alloca i32"), "should alloca param b");
        assert!(
            ir.contains("store i32 %b, ptr %b.addr"),
            "should store param b"
        );
        assert!(ir.contains("add nsw i32"), "should add with nsw");
    }

    // -----------------------------------------------------------------------
    // Let binding and assignment tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_let_binding() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(42)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("x"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("alloca i32"), "should alloca x");
        assert!(ir.contains("store i32 42, ptr %x"), "should store 42");
        assert!(ir.contains("load i32, ptr %x"), "should load x");
    }

    #[test]
    fn test_assignment() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(0)),
                        mutable: true,
                    }),
                    stmt(HirStmtKind::Assign {
                        target: ident("x"),
                        value: int_lit(42),
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("x"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("store i32 42, ptr %x"), "should store 42 to x");
    }

    // -----------------------------------------------------------------------
    // If/else tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_if_no_else() {
        let m = module(
            Some("test"),
            vec![func(
                "test_fn",
                vec![param("x", HirType::Primitive(PrimitiveType::I32))],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::If {
                        condition: binop(BinOp::Gt, ident("x"), int_lit(0)),
                        then_block: block(vec![stmt(HirStmtKind::Return {
                            value: int_lit(1),
                        })]),
                        else_block: None,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("icmp sgt"), "should have comparison");
        assert!(ir.contains("br i1"), "should have conditional branch");
        assert!(ir.contains("then."), "should have then label");
        assert!(ir.contains("merge."), "should have merge label");
    }

    #[test]
    fn test_if_else() {
        let m = module(
            Some("test"),
            vec![func(
                "test_fn",
                vec![param("x", HirType::Primitive(PrimitiveType::I32))],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![stmt(HirStmtKind::If {
                    condition: binop(BinOp::Gt, ident("x"), int_lit(0)),
                    then_block: block(vec![stmt(HirStmtKind::Return {
                        value: int_lit(1),
                    })]),
                    else_block: Some(block(vec![stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    })])),
                })]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("icmp sgt"), "should have comparison");
        assert!(ir.contains("br i1"), "should have conditional branch");
        assert!(ir.contains("then."), "should have then label");
        assert!(ir.contains("else."), "should have else label");
        assert!(ir.contains("merge."), "should have merge label");
    }

    // -----------------------------------------------------------------------
    // For loop tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_for_loop() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "sum".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(0)),
                        mutable: true,
                    }),
                    stmt(HirStmtKind::For {
                        var: "i".to_string(),
                        var_span: span(),
                        var_type: HirType::Primitive(PrimitiveType::I32),
                        iterable: call("range", vec![int_lit(0), int_lit(10)]),
                        body: block(vec![stmt(HirStmtKind::Assign {
                            target: ident("sum"),
                            value: binop(BinOp::Add, ident("sum"), ident("i")),
                        })]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("sum"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("alloca i32"), "should alloca loop var");
        assert!(ir.contains("icmp slt"), "should have loop comparison");
        assert!(ir.contains("for.cond."), "should have for.cond label");
        assert!(ir.contains("for.body."), "should have for.body label");
        assert!(ir.contains("for.end."), "should have for.end label");
        assert!(ir.contains("add nsw i32"), "should have nsw increment");
        assert!(
            ir.contains("br label %for.cond."),
            "should branch back to cond"
        );
    }

    // -----------------------------------------------------------------------
    // Function call tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_function_call() {
        let m = module(
            Some("test"),
            vec![
                func(
                    "fib",
                    vec![param("n", HirType::Primitive(PrimitiveType::I32))],
                    HirType::Primitive(PrimitiveType::I64),
                    block(vec![stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    })]),
                ),
                func(
                    "main",
                    vec![],
                    HirType::Primitive(PrimitiveType::I32),
                    block(vec![
                        stmt(HirStmtKind::Let {
                            name: "result".to_string(),
                            name_span: span(),
                            ty: HirType::Primitive(PrimitiveType::I64),
                            value: Some(call("fib", vec![int_lit(40)])),
                            mutable: false,
                        }),
                        stmt(HirStmtKind::Return {
                            value: int_lit(0),
                        }),
                    ]),
                ),
            ],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("@fib(i32 40)"), "should call fib");
    }

    // -----------------------------------------------------------------------
    // Built-in function tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_print_string() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Expr {
                        expr: call("print", vec![str_lit("hello")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("c\"hello\\00\""), "should have string constant");
        assert!(ir.contains("call i32 @puts"), "should call puts");
        assert!(ir.contains("declare i32 @puts(ptr)"), "should declare puts");
    }

    #[test]
    fn test_print_i64() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: Some(int_lit(42)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("print_i64", vec![ident("x")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("@.fmt.i64"), "should have format string");
        assert!(
            ir.contains("call i32 (ptr, ...) @printf"),
            "should call printf"
        );
        assert!(
            ir.contains("declare i32 @printf(ptr, ...)"),
            "should declare printf"
        );
    }

    #[test]
    fn test_widen() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(5)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: Some(call("widen", vec![ident("x")])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("sext i32"), "should have sext");
        assert!(ir.contains("to i64"), "should extend to i64");
    }

    // -----------------------------------------------------------------------
    // Boolean tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_bool_literal() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::Bool),
                        value: Some(bool_lit(true)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::Bool),
                        value: Some(bool_lit(false)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("store i1 1, ptr %x"), "true should be i1 1");
        assert!(ir.contains("store i1 0, ptr %y"), "false should be i1 0");
    }

    // -----------------------------------------------------------------------
    // Unary op tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_unary_neg() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(5)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(unaryop(UnaryOp::Neg, ident("x"))),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("sub i32 0,"), "neg should be sub 0, x");
    }

    #[test]
    fn test_unary_not() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::Bool),
                        value: Some(bool_lit(true)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::Bool),
                        value: Some(unaryop(UnaryOp::Not, ident("x"))),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("xor i1"), "not should be xor i1");
    }

    // -----------------------------------------------------------------------
    // Float tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_float_operations() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "a".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(float_lit(1.5)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "b".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(float_lit(2.5)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "c".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(binop(BinOp::Add, ident("a"), ident("b"))),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("fadd double"), "should use fadd for f64 add");
    }

    // -----------------------------------------------------------------------
    // Error tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_unsupported_type_error() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Tensor {
                            element: Box::new(HirType::Primitive(PrimitiveType::F32)),
                            dims: vec![],
                        },
                        value: Some(int_lit(0)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let result = codegen(&m);
        assert!(result.is_err(), "should error on unsupported type");
    }

    // -----------------------------------------------------------------------
    // Integration tests: full programs
    // -----------------------------------------------------------------------

    #[test]
    fn test_hello() {
        let source = std::fs::read_to_string("../../tests/samples/hello.axm")
            .expect("should read hello.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "hello.axm should parse without errors"
        );
        let hir_module =
            axiom_hir::lower(&parse_result.module).expect("hello.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("hello.axm should codegen");

        assert!(
            ir.contains("define i32 @main()"),
            "should define main: {ir}"
        );
        assert!(
            ir.contains("Hello from AXIOM!"),
            "should contain string: {ir}"
        );
        assert!(ir.contains("call i32 @puts"), "should call puts: {ir}");
        assert!(
            ir.contains("declare i32 @puts(ptr)"),
            "should declare puts: {ir}"
        );
        assert!(ir.contains("ret i32 0"), "should return 0: {ir}");
    }

    #[test]
    fn test_fibonacci() {
        let source = std::fs::read_to_string("../../tests/samples/fibonacci.axm")
            .expect("should read fibonacci.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "fibonacci.axm should parse without errors"
        );
        let hir_module =
            axiom_hir::lower(&parse_result.module).expect("fibonacci.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("fibonacci.axm should codegen");

        assert!(
            ir.contains("@fib(i32"),
            "should define fib: {ir}"
        );
        assert!(
            ir.contains("define i32 @main()"),
            "should define main: {ir}"
        );
        assert!(ir.contains("sext i32"), "should have sext (widen): {ir}");
        assert!(
            ir.contains("icmp slt i32"),
            "should have icmp slt (range loop): {ir}"
        );
        assert!(ir.contains("@fib("), "should call fib: {ir}");
        assert!(
            ir.contains("call i32 (ptr, ...) @printf"),
            "should call printf: {ir}"
        );
        assert!(
            ir.contains("declare i32 @printf(ptr, ...)"),
            "should declare printf: {ir}"
        );
        assert!(ir.contains("ret i32 0"), "should return 0 from main: {ir}");
        assert!(ir.contains("ret i64"), "should return i64 from fib: {ir}");
    }

    #[test]
    fn test_empty_module() {
        let m = module(Some("empty"), vec![]);
        let ir = codegen(&m).expect("empty module should codegen");
        assert!(ir.contains("; ModuleID = 'empty'"), "should have module ID");
        assert!(
            ir.contains("source_filename = \"empty\""),
            "should have source_filename"
        );
        assert!(!ir.contains("define "), "should have no function defs");
    }

    #[test]
    fn test_multiple_functions() {
        let m = module(
            Some("test"),
            vec![
                func(
                    "helper",
                    vec![param("x", HirType::Primitive(PrimitiveType::I32))],
                    HirType::Primitive(PrimitiveType::I32),
                    block(vec![stmt(HirStmtKind::Return {
                        value: ident("x"),
                    })]),
                ),
                func(
                    "main",
                    vec![],
                    HirType::Primitive(PrimitiveType::I32),
                    block(vec![stmt(HirStmtKind::Return {
                        value: call("helper", vec![int_lit(42)]),
                    })]),
                ),
            ],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("@helper("), "should define helper");
        assert!(ir.contains("define i32 @main()"), "should define main");
        assert!(
            ir.contains("@helper(i32 42)"),
            "should call helper"
        );
    }

    #[test]
    fn test_while_loop() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(0)),
                        mutable: true,
                    }),
                    stmt(HirStmtKind::While {
                        condition: binop(BinOp::Lt, ident("x"), int_lit(10)),
                        body: block(vec![stmt(HirStmtKind::Assign {
                            target: ident("x"),
                            value: binop(BinOp::Add, ident("x"), int_lit(1)),
                        })]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("x"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("while.cond."), "should have while.cond label");
        assert!(ir.contains("while.body."), "should have while.body label");
        assert!(ir.contains("while.end."), "should have while.end label");
    }

    #[test]
    fn test_nested_expressions() {
        // a + b * c should emit mul first, then add.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "a".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(1)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "b".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(2)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "c".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(3)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "result".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        // a + (b * c)  -- parser already handles precedence in AST
                        value: Some(binop(
                            BinOp::Add,
                            ident("a"),
                            binop(BinOp::Mul, ident("b"), ident("c")),
                        )),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // Mul should appear before add in the IR (with nsw flags).
        let mul_pos = ir.find("mul nsw i32").expect("should have mul nsw");
        let add_pos = ir
            .rfind("add nsw i32")
            .expect("should have add nsw");
        assert!(
            mul_pos < add_pos,
            "mul should come before add in the IR"
        );
    }

    #[test]
    fn test_string_escaping() {
        assert_eq!(escape_llvm_string("hello"), "hello");
        assert_eq!(escape_llvm_string("hello\nworld"), "hello\\0Aworld");
        assert_eq!(escape_llvm_string("tab\there"), "tab\\09here");
        assert_eq!(escape_llvm_string("quote\"here"), "quote\\22here");
        assert_eq!(escape_llvm_string("back\\slash"), "back\\5Cslash");
    }

    #[test]
    fn test_float_formatting() {
        assert_eq!(format_float(0.0), "0.0");
        assert_eq!(format_float(1.5), "1.5");
        assert_eq!(format_float(42.0), "42.0");
        assert_eq!(format_float(-3.14), "-3.14");
    }

    // -----------------------------------------------------------------------
    // Standard library built-in tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_math_builtins() {
        // Test abs, min, max, sqrt, pow
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // abs(x: i32) -> i32
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(-5)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "a".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("abs", vec![ident("x")])),
                        mutable: false,
                    }),
                    // abs_f64(x: f64) -> f64
                    stmt(HirStmtKind::Let {
                        name: "fx".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(float_lit(-3.14)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "fa".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(call("abs_f64", vec![ident("fx")])),
                        mutable: false,
                    }),
                    // min(a: i32, b: i32) -> i32
                    stmt(HirStmtKind::Let {
                        name: "mn".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("min", vec![int_lit(3), int_lit(7)])),
                        mutable: false,
                    }),
                    // max(a: i32, b: i32) -> i32
                    stmt(HirStmtKind::Let {
                        name: "mx".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("max", vec![int_lit(3), int_lit(7)])),
                        mutable: false,
                    }),
                    // min_f64(a: f64, b: f64) -> f64
                    stmt(HirStmtKind::Let {
                        name: "fmn".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(call("min_f64", vec![float_lit(1.5), float_lit(2.5)])),
                        mutable: false,
                    }),
                    // max_f64(a: f64, b: f64) -> f64
                    stmt(HirStmtKind::Let {
                        name: "fmx".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(call("max_f64", vec![float_lit(1.5), float_lit(2.5)])),
                        mutable: false,
                    }),
                    // sqrt(x: f64) -> f64
                    stmt(HirStmtKind::Let {
                        name: "sq".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(call("sqrt", vec![float_lit(4.0)])),
                        mutable: false,
                    }),
                    // pow(base: f64, exp: f64) -> f64
                    stmt(HirStmtKind::Let {
                        name: "pw".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(call("pow", vec![float_lit(2.0), float_lit(3.0)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // abs: @llvm.abs.i32
        assert!(
            ir.contains("call i32 @llvm.abs.i32(i32"),
            "should call llvm.abs.i32: {ir}"
        );
        assert!(
            ir.contains("declare i32 @llvm.abs.i32(i32, i1)"),
            "should declare llvm.abs.i32: {ir}"
        );

        // abs_f64: @llvm.fabs.f64
        assert!(
            ir.contains("call double @llvm.fabs.f64(double"),
            "should call llvm.fabs.f64: {ir}"
        );
        assert!(
            ir.contains("declare double @llvm.fabs.f64(double)"),
            "should declare llvm.fabs.f64: {ir}"
        );

        // min: icmp slt + select
        assert!(
            ir.contains("icmp slt i32"),
            "min should use icmp slt: {ir}"
        );
        assert!(
            ir.contains("select i1"),
            "min/max should use select: {ir}"
        );

        // max: icmp sgt + select
        assert!(
            ir.contains("icmp sgt i32"),
            "max should use icmp sgt: {ir}"
        );

        // min_f64: fcmp olt + select
        assert!(
            ir.contains("fcmp olt double"),
            "min_f64 should use fcmp olt: {ir}"
        );

        // max_f64: fcmp ogt + select
        assert!(
            ir.contains("fcmp ogt double"),
            "max_f64 should use fcmp ogt: {ir}"
        );

        // sqrt: @llvm.sqrt.f64
        assert!(
            ir.contains("call double @llvm.sqrt.f64(double"),
            "should call llvm.sqrt.f64: {ir}"
        );
        assert!(
            ir.contains("declare double @llvm.sqrt.f64(double)"),
            "should declare llvm.sqrt.f64: {ir}"
        );

        // pow: @llvm.pow.f64
        assert!(
            ir.contains("call double @llvm.pow.f64(double"),
            "should call llvm.pow.f64: {ir}"
        );
        assert!(
            ir.contains("declare double @llvm.pow.f64(double, double)"),
            "should declare llvm.pow.f64: {ir}"
        );
    }

    #[test]
    fn test_conversion_builtins() {
        // Test narrow, truncate
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // narrow(x: i64) -> i32
                    stmt(HirStmtKind::Let {
                        name: "wide".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: Some(int_lit(42)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "narrow_val".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("narrow", vec![ident("wide")])),
                        mutable: false,
                    }),
                    // truncate(x: f64) -> i32
                    stmt(HirStmtKind::Let {
                        name: "fval".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(float_lit(3.14)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "trunc_val".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("truncate", vec![ident("fval")])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // narrow: trunc i64 to i32
        assert!(
            ir.contains("trunc i64"),
            "narrow should use trunc: {ir}"
        );
        assert!(
            ir.contains("to i32"),
            "narrow should truncate to i32: {ir}"
        );

        // truncate: fptosi double to i32
        assert!(
            ir.contains("fptosi double"),
            "truncate should use fptosi: {ir}"
        );
        assert!(
            ir.contains("to i32"),
            "truncate should convert to i32: {ir}"
        );
    }

    #[test]
    fn test_io_builtins() {
        // Test print_f64, print_i32
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // print_i32
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(42)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("print_i32", vec![ident("x")]),
                    }),
                    // print_f64
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(float_lit(3.14)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("print_f64", vec![ident("y")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // print_i32: format string + printf call
        assert!(
            ir.contains("@.fmt.i32"),
            "should have i32 format string: {ir}"
        );
        assert!(
            ir.contains("call i32 (ptr, ...) @printf(ptr @.fmt.i32, i32"),
            "should call printf with i32 format: {ir}"
        );

        // print_f64: format string + printf call
        assert!(
            ir.contains("@.fmt.f64"),
            "should have f64 format string: {ir}"
        );
        assert!(
            ir.contains("call i32 (ptr, ...) @printf(ptr @.fmt.f64, double"),
            "should call printf with f64 format: {ir}"
        );

        // Should declare printf
        assert!(
            ir.contains("declare i32 @printf(ptr, ...)"),
            "should declare printf: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // FFI / Extern function tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_extern_decl() {
        let ef = HirExternFunction {
            id: nid(0),
            name: "sin".to_string(),
            name_span: span(),
            annotations: vec![],
            params: vec![param("x", HirType::Primitive(PrimitiveType::F64))],
            return_type: HirType::Primitive(PrimitiveType::F64),
            span: span(),
        };

        let m = module_with_externs(Some("test"), vec![], vec![ef]);
        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("declare double @sin(double)"),
            "should declare extern sin: {ir}"
        );
    }

    #[test]
    fn test_extern_call() {
        let ef = HirExternFunction {
            id: nid(0),
            name: "clock".to_string(),
            name_span: span(),
            annotations: vec![],
            params: vec![],
            return_type: HirType::Primitive(PrimitiveType::I64),
            span: span(),
        };

        let main_func = func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "t".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(call("clock", vec![])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: int_lit(0),
                }),
            ]),
        );

        let m = module_with_externs(Some("test"), vec![main_func], vec![ef]);
        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("declare i64 @clock()"),
            "should declare extern clock: {ir}"
        );
        assert!(
            ir.contains("call i64 @clock()"),
            "should call clock: {ir}"
        );
    }

    #[test]
    fn test_export_function() {
        let export_ann = HirAnnotation {
            kind: HirAnnotationKind::Export,
            span: SPAN_DUMMY,
        };

        let add_func = HirFunction {
            id: nid(0),
            name: "add".to_string(),
            name_span: span(),
            annotations: vec![export_ann],
            params: vec![
                param("a", HirType::Primitive(PrimitiveType::I32)),
                param("b", HirType::Primitive(PrimitiveType::I32)),
            ],
            return_type: HirType::Primitive(PrimitiveType::I32),
            body: block(vec![stmt(HirStmtKind::Return {
                value: binop(BinOp::Add, ident("a"), ident("b")),
            })]),
            span: span(),
        };

        let m = module(Some("test"), vec![add_func]);
        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("define dso_local i32 @add(i32 %a, i32 %b)"),
            "should define exported function with dso_local: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // Self-hosting bootstrap tests (M5.1)
    // -----------------------------------------------------------------------

    /// Integration test: self-hosting lexer example compiles through the full
    /// pipeline (parse -> HIR -> LLVM IR).
    #[test]
    fn test_self_host_lexer() {
        let source = std::fs::read_to_string("../../examples/self_host/lexer.axm")
            .expect("should read lexer.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "lexer.axm should parse without errors: {:?}",
            parse_result.errors
        );
        let hir_module =
            axiom_hir::lower(&parse_result.module).expect("lexer.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("lexer.axm should codegen");

        // Verify the classify_char function is emitted.
        assert!(
            ir.contains("@classify_char(i32"),
            "should define classify_char: {ir}"
        );
        // Verify main is emitted.
        assert!(
            ir.contains("define i32 @main()"),
            "should define main: {ir}"
        );
        // Verify classify_char is called with ASCII character codes.
        assert!(
            ir.contains("@classify_char(i32 49)"),
            "should call classify_char with '1' (49): {ir}"
        );
        assert!(
            ir.contains("@classify_char(i32 43)"),
            "should call classify_char with '+' (43): {ir}"
        );
        // Verify printf is used for output.
        assert!(
            ir.contains("call i32 (ptr, ...) @printf"),
            "should call printf for output: {ir}"
        );
        // Verify the `and` logic is compiled (digit range check: c >= 48 and c <= 57).
        assert!(
            ir.contains("and i1"),
            "should have logical AND for range check: {ir}"
        );
        assert!(ir.contains("ret i32 0"), "main should return 0: {ir}");
    }

    /// Integration test: self-hosting token counter compiles through the full
    /// pipeline (parse -> HIR -> LLVM IR).
    #[test]
    fn test_self_host_token_counter() {
        let source = std::fs::read_to_string("../../examples/self_host/token_counter.axm")
            .expect("should read token_counter.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "token_counter.axm should parse without errors: {:?}",
            parse_result.errors
        );
        let hir_module = axiom_hir::lower(&parse_result.module)
            .expect("token_counter.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("token_counter.axm should codegen");

        // Verify classify_char function.
        assert!(
            ir.contains("@classify_char(i32"),
            "should define classify_char: {ir}"
        );
        // Verify main with mutable counters.
        assert!(
            ir.contains("define i32 @main()"),
            "should define main: {ir}"
        );
        // Verify alloca for mutable counter variables.
        assert!(
            ir.contains("alloca i32") && ir.contains("numbers"),
            "should have numbers counter: {ir}"
        );
        assert!(
            ir.contains("alloca i32") && ir.contains("operators"),
            "should have operators counter: {ir}"
        );
        // Verify if/else branches for counting logic.
        assert!(ir.contains("then."), "should have then branches: {ir}");
        assert!(ir.contains("else."), "should have else branches: {ir}");
        // Verify printf calls for output.
        assert!(
            ir.contains("call i32 (ptr, ...) @printf"),
            "should call printf: {ir}"
        );
        assert!(ir.contains("ret i32 0"), "main should return 0: {ir}");
    }

    // -----------------------------------------------------------------------
    // to_f64 / to_f64_i64 conversion builtin tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_to_f64() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(42)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(call("to_f64", vec![ident("x")])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("sitofp i32"),
            "should have sitofp i32: {ir}"
        );
        assert!(
            ir.contains("to double"),
            "should convert to double: {ir}"
        );
    }

    #[test]
    fn test_to_f64_i64() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: Some(int_lit(100)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(call("to_f64_i64", vec![ident("x")])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("sitofp i64"),
            "should have sitofp i64: {ir}"
        );
        assert!(
            ir.contains("to double"),
            "should convert to double: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // Benchmark program integration tests
    // -----------------------------------------------------------------------

    /// Integration test: recursive fibonacci benchmark compiles through the
    /// full pipeline (parse -> HIR -> LLVM IR).
    #[test]
    fn test_benchmark_fib() {
        let source = std::fs::read_to_string("../../benchmarks/fib/fib.axm")
            .expect("should read fib.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "fib.axm should parse without errors: {:?}",
            parse_result.errors
        );
        let hir_module = axiom_hir::lower(&parse_result.module)
            .expect("fib.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("fib.axm should codegen");

        // Verify recursive fib function with i64 params.
        assert!(
            ir.contains("@fib(i64 %n)"),
            "should define fib with i64 param: {ir}"
        );
        // Verify recursive calls.
        assert!(
            ir.contains("@fib(i64"),
            "should have recursive call: {ir}"
        );
        // Verify i64 comparison.
        assert!(
            ir.contains("icmp sle i64"),
            "should have i64 comparison: {ir}"
        );
        // Verify i64 subtraction (with nsw flag from @pure function).
        assert!(
            ir.contains("sub nsw i64"),
            "should have i64 subtraction with nsw: {ir}"
        );
        // Verify i64 addition (with nsw flag from @pure function).
        assert!(
            ir.contains("add nsw i64"),
            "should have i64 addition with nsw: {ir}"
        );
        // Verify main calls fib(47).
        assert!(
            ir.contains("@fib(i64 47)"),
            "should call fib(47): {ir}"
        );
    }

    /// Integration test: Leibniz Pi benchmark compiles through the full
    /// pipeline (parse -> HIR -> LLVM IR).
    #[test]
    fn test_benchmark_leibniz() {
        let source = std::fs::read_to_string("../../benchmarks/leibniz/leibniz.axm")
            .expect("should read leibniz.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "leibniz.axm should parse without errors: {:?}",
            parse_result.errors
        );
        let hir_module = axiom_hir::lower(&parse_result.module)
            .expect("leibniz.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("leibniz.axm should codegen");

        // Verify main function.
        assert!(
            ir.contains("define i32 @main()"),
            "should define main: {ir}"
        );
        // Verify sitofp for to_f64 builtin.
        assert!(
            ir.contains("sitofp i32"),
            "should have sitofp for to_f64: {ir}"
        );
        // Verify float division for 1.0/d.
        assert!(
            ir.contains("fdiv double"),
            "should have float division: {ir}"
        );
        // Verify float subtraction and addition.
        assert!(
            ir.contains("fsub double"),
            "should have float subtraction: {ir}"
        );
        assert!(
            ir.contains("fadd double"),
            "should have float addition: {ir}"
        );
        // Verify for loop structure.
        assert!(
            ir.contains("for.cond."),
            "should have for loop condition: {ir}"
        );
        // Verify printf for f64 output.
        assert!(
            ir.contains("@.fmt.f64"),
            "should have f64 format string: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // Array support tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_array_type() {
        // Verify that array type generates the correct LLVM type string.
        let arr_ty = HirType::Array {
            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
            size: 100,
        };
        let llvm_ty = hir_type_to_llvm(&arr_ty).expect("array type should convert");
        assert_eq!(llvm_ty, "[100 x i32]");

        let arr_ty_f64 = HirType::Array {
            element: Box::new(HirType::Primitive(PrimitiveType::F64)),
            size: 50,
        };
        let llvm_ty_f64 = hir_type_to_llvm(&arr_ty_f64).expect("f64 array type should convert");
        assert_eq!(llvm_ty_f64, "[50 x double]");
    }

    #[test]
    fn test_array_param_type() {
        // Verify that array params become ptr in function signatures.
        let arr_ty = HirType::Array {
            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
            size: 10,
        };
        let param_ty = hir_type_to_llvm_param(&arr_ty).expect("array param type should convert");
        assert_eq!(param_ty, "ptr");
    }

    #[test]
    fn test_array_alloca() {
        // Test that array_zeros creates alloca + memset.
        let m = module(
            Some("arr_test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "arr".to_string(),
                        name_span: span(),
                        ty: HirType::Array {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                            size: 10,
                        },
                        value: Some(HirExpr {
                            id: nid(0),
                            kind: HirExprKind::ArrayZeros {
                                element_type: HirType::Primitive(PrimitiveType::I32),
                                size: 10,
                            },
                            span: span(),
                        }),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );
        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("alloca [10 x i32]"),
            "should have array alloca: {ir}"
        );
        assert!(
            ir.contains("i8 0, i64 40, i1 false"),
            "should have memset for 10 * 4 = 40 bytes: {ir}"
        );
        assert!(
            ir.contains("declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)"),
            "should declare memset intrinsic: {ir}"
        );
    }

    #[test]
    fn test_array_index_read() {
        // Test array index read: arr[5].
        let m = module(
            Some("arr_read"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "arr".to_string(),
                        name_span: span(),
                        ty: HirType::Array {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                            size: 10,
                        },
                        value: Some(HirExpr {
                            id: nid(0),
                            kind: HirExprKind::ArrayZeros {
                                element_type: HirType::Primitive(PrimitiveType::I32),
                                size: 10,
                            },
                            span: span(),
                        }),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(HirExpr {
                            id: nid(0),
                            kind: HirExprKind::Index {
                                expr: Box::new(ident("arr")),
                                indices: vec![int_lit(5)],
                            },
                            span: span(),
                        }),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("x"),
                    }),
                ]),
            )],
        );
        let ir = codegen(&m).expect("codegen should succeed");
        // Should have GEP + load for index read.
        assert!(
            ir.contains("getelementptr inbounds [10 x i32], ptr %arr"),
            "should have GEP for array index: {ir}"
        );
        assert!(
            ir.contains("load i32, ptr"),
            "should load element from GEP pointer: {ir}"
        );
    }

    #[test]
    fn test_array_index_write() {
        // Test array index write: arr[5] = 42.
        let m = module(
            Some("arr_write"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "arr".to_string(),
                        name_span: span(),
                        ty: HirType::Array {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                            size: 10,
                        },
                        value: Some(HirExpr {
                            id: nid(0),
                            kind: HirExprKind::ArrayZeros {
                                element_type: HirType::Primitive(PrimitiveType::I32),
                                size: 10,
                            },
                            span: span(),
                        }),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Assign {
                        target: HirExpr {
                            id: nid(0),
                            kind: HirExprKind::Index {
                                expr: Box::new(ident("arr")),
                                indices: vec![int_lit(5)],
                            },
                            span: span(),
                        },
                        value: int_lit(42),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );
        let ir = codegen(&m).expect("codegen should succeed");
        // Should have GEP + store for index write.
        assert!(
            ir.contains("getelementptr inbounds [10 x i32], ptr %arr"),
            "should have GEP for array index write: {ir}"
        );
        assert!(
            ir.contains("store i32 42, ptr"),
            "should store value at GEP pointer: {ir}"
        );
    }

    #[test]
    fn test_array_program() {
        // Full array program: create, fill with squares, sum them.
        let source = r#"
@module array_test;
fn main() -> i32 {
    let arr: array[i32, 10] = array_zeros[i32, 10];
    for i: i32 in range(0, 10) {
        arr[i] = i * i;
    }
    let sum: i32 = 0;
    for i: i32 in range(0, 10) {
        sum = sum + arr[i];
    }
    print_i32(sum);
    return 0;
}
"#;
        let parse_result = axiom_parser::parse(source);
        assert!(
            !parse_result.has_errors(),
            "parse should succeed: {:?}",
            parse_result.errors
        );
        let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
        let ir = codegen(&hir).expect("codegen should succeed");

        // Verify key patterns in the generated IR.
        assert!(
            ir.contains("alloca [10 x i32]"),
            "should have array alloca: {ir}"
        );
        assert!(
            ir.contains("@llvm.memset.p0.i64"),
            "should use memset: {ir}"
        );
        assert!(
            ir.contains("getelementptr inbounds [10 x i32]"),
            "should have GEP: {ir}"
        );
        // Should have at least one store to array and one load from array.
        assert!(
            ir.contains("store i32"),
            "should have store to array: {ir}"
        );
        assert!(
            ir.contains("load i32"),
            "should have load from array: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // LLVM optimization hint tests
    // -----------------------------------------------------------------------

    /// Helper: create a function with annotations.
    fn func_with_annotations(
        name: &str,
        params: Vec<HirParam>,
        return_type: HirType,
        body: HirBlock,
        annotations: Vec<HirAnnotation>,
    ) -> HirFunction {
        HirFunction {
            id: nid(0),
            name: name.to_string(),
            name_span: span(),
            annotations,
            params,
            return_type,
            body,
            span: span(),
        }
    }

    /// Helper: create a @pure annotation.
    fn pure_ann() -> HirAnnotation {
        HirAnnotation {
            kind: HirAnnotationKind::Pure,
            span: SPAN_DUMMY,
        }
    }

    /// Helper: create a @const annotation.
    fn const_ann() -> HirAnnotation {
        HirAnnotation {
            kind: HirAnnotationKind::Const,
            span: SPAN_DUMMY,
        }
    }

    /// Helper: create a @vectorizable annotation.
    fn vectorizable_ann() -> HirAnnotation {
        HirAnnotation {
            kind: HirAnnotationKind::Vectorizable(vec![]),
            span: SPAN_DUMMY,
        }
    }

    // --- Test #1: noalias on all pointer parameters ---

    #[test]
    fn test_noalias_params() {
        // Function with an array (ptr) parameter should get noalias.
        let sum_func = func_with_annotations(
            "sum_arr",
            vec![
                param(
                    "arr",
                    HirType::Array {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        size: 10,
                    },
                ),
                param("n", HirType::Primitive(PrimitiveType::I32)),
            ],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![stmt(HirStmtKind::Return {
                value: int_lit(0),
            })]),
            vec![],
        );

        let m = module(Some("test"), vec![sum_func]);
        let ir = codegen(&m).expect("codegen should succeed");

        // The ptr parameter should have noalias.
        assert!(
            ir.contains("ptr noalias %arr"),
            "ptr params should have noalias: {ir}"
        );
        // Non-ptr params should NOT have noalias.
        assert!(
            ir.contains("i32 %n"),
            "non-ptr params should not have noalias: {ir}"
        );
    }

    // --- Test #2: @pure function attributes (readnone/readonly) ---

    #[test]
    fn test_pure_function_attrs_readnone() {
        // @pure function with no pointer params -> memory(none).
        let fib_func = func_with_annotations(
            "fib",
            vec![param("n", HirType::Primitive(PrimitiveType::I64))],
            HirType::Primitive(PrimitiveType::I64),
            block(vec![stmt(HirStmtKind::Return {
                value: ident("n"),
            })]),
            vec![pure_ann()],
        );

        let m = module(Some("test"), vec![fib_func]);
        let ir = codegen(&m).expect("codegen should succeed");

        // Should have function attribute group reference.
        assert!(
            ir.contains("#0"),
            "pure function should have attribute group ref: {ir}"
        );
        // Should have memory(none) in the attribute group.
        assert!(
            ir.contains("memory(none)"),
            "pure function without ptr params should get memory(none): {ir}"
        );
        // Should have nounwind.
        assert!(
            ir.contains("nounwind"),
            "pure function should have nounwind: {ir}"
        );
        // Should have willreturn.
        assert!(
            ir.contains("willreturn"),
            "pure function should have willreturn: {ir}"
        );
    }

    #[test]
    fn test_pure_function_attrs_argmem_read() {
        // @pure function with pointer params -> memory(argmem: read).
        let sum_func = func_with_annotations(
            "sum_arr",
            vec![
                param(
                    "arr",
                    HirType::Array {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        size: 10,
                    },
                ),
                param("n", HirType::Primitive(PrimitiveType::I32)),
            ],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![stmt(HirStmtKind::Return {
                value: int_lit(0),
            })]),
            vec![pure_ann()],
        );

        let m = module(Some("test"), vec![sum_func]);
        let ir = codegen(&m).expect("codegen should succeed");

        // Should have memory(argmem: read) for @pure with ptr params.
        assert!(
            ir.contains("memory(argmem: read)"),
            "pure function with ptr params should get memory(argmem: read): {ir}"
        );
    }

    // --- Test #3: nsw/nuw flags on arithmetic ---

    #[test]
    fn test_nsw_arithmetic() {
        // Integer add/sub/mul should get nsw flag.
        let m = module(
            Some("test"),
            vec![func(
                "compute",
                vec![
                    param("a", HirType::Primitive(PrimitiveType::I32)),
                    param("b", HirType::Primitive(PrimitiveType::I32)),
                ],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "sum".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(binop(BinOp::Add, ident("a"), ident("b"))),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "diff".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(binop(BinOp::Sub, ident("a"), ident("b"))),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "prod".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(binop(BinOp::Mul, ident("a"), ident("b"))),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("sum"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("add nsw i32"),
            "integer add should have nsw: {ir}"
        );
        assert!(
            ir.contains("sub nsw i32"),
            "integer sub should have nsw: {ir}"
        );
        assert!(
            ir.contains("mul nsw i32"),
            "integer mul should have nsw: {ir}"
        );
    }

    #[test]
    fn test_wrap_ops_no_nsw() {
        // AddWrap/SubWrap/MulWrap should NOT get nsw flag.
        let m = module(
            Some("test"),
            vec![func(
                "wrap_ops",
                vec![
                    param("a", HirType::Primitive(PrimitiveType::I32)),
                    param("b", HirType::Primitive(PrimitiveType::I32)),
                ],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(binop(BinOp::AddWrap, ident("a"), ident("b"))),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("x"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // AddWrap should produce plain `add` without nsw.
        // We check that it contains "add i32" but not "add nsw i32" at the same position.
        assert!(
            ir.contains("add i32"),
            "AddWrap should produce plain add: {ir}"
        );
        // The wrap add line should not have nsw.
        for line in ir.lines() {
            if line.contains("add i32") && line.contains("= add") {
                assert!(
                    !line.contains("nsw"),
                    "AddWrap should NOT have nsw: {line}"
                );
            }
        }
    }

    // --- Test #4: fast flag on float ops in @pure context ---

    #[test]
    fn test_fast_float_in_pure() {
        // Float operations in @pure function should get `fast` flag.
        let compute_func = func_with_annotations(
            "compute",
            vec![
                param("a", HirType::Primitive(PrimitiveType::F64)),
                param("b", HirType::Primitive(PrimitiveType::F64)),
            ],
            HirType::Primitive(PrimitiveType::F64),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "sum".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(binop(BinOp::Add, ident("a"), ident("b"))),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "prod".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(binop(BinOp::Mul, ident("a"), ident("b"))),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: ident("sum"),
                }),
            ]),
            vec![pure_ann()],
        );

        let m = module(Some("test"), vec![compute_func]);
        let ir = codegen(&m).expect("codegen should succeed");

        assert!(
            ir.contains("fadd fast double"),
            "float add in @pure should have fast flag: {ir}"
        );
        assert!(
            ir.contains("fmul fast double"),
            "float mul in @pure should have fast flag: {ir}"
        );
    }

    #[test]
    fn test_no_fast_float_outside_pure() {
        // Float operations in non-@pure function should NOT get `fast` flag.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "a".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(float_lit(1.5)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "b".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(float_lit(2.5)),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "c".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(binop(BinOp::Add, ident("a"), ident("b"))),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // Should have plain fadd without fast.
        assert!(
            ir.contains("fadd double"),
            "should have fadd: {ir}"
        );
        assert!(
            !ir.contains("fadd fast"),
            "non-pure function should NOT have fast flag: {ir}"
        );
    }

    // --- Test #5: @const compile-time evaluation ---

    #[test]
    fn test_const_eval_simple() {
        // @const function called with all-literal args should be evaluated at compile time.
        let square_func = func_with_annotations(
            "square",
            vec![param("n", HirType::Primitive(PrimitiveType::I32))],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![stmt(HirStmtKind::Return {
                value: binop(BinOp::Mul, ident("n"), ident("n")),
            })]),
            vec![const_ann()],
        );

        let main_func = func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![stmt(HirStmtKind::Return {
                value: call("square", vec![int_lit(42)]),
            })]),
        );

        let m = module(Some("test"), vec![square_func, main_func]);
        let ir = codegen(&m).expect("codegen should succeed");

        // The call to square(42) should be replaced with the literal 1764.
        assert!(
            ir.contains("1764"),
            "const eval should compute square(42) = 1764: {ir}"
        );
        // Main should NOT contain a call to square.
        let main_section = ir.split("define i32 @main").nth(1).unwrap_or("");
        assert!(
            !main_section.contains("@square"),
            "const call should be eliminated from main: {ir}"
        );
    }

    #[test]
    fn test_const_function_attributes() {
        // @const functions should get speculatable + memory(none).
        let square_func = func_with_annotations(
            "square",
            vec![param("n", HirType::Primitive(PrimitiveType::I32))],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![stmt(HirStmtKind::Return {
                value: binop(BinOp::Mul, ident("n"), ident("n")),
            })]),
            vec![const_ann()],
        );

        let m = module(Some("test"), vec![square_func]);
        let ir = codegen(&m).expect("codegen should succeed");

        assert!(
            ir.contains("speculatable"),
            "@const should have speculatable: {ir}"
        );
        assert!(
            ir.contains("memory(none)"),
            "@const should have memory(none): {ir}"
        );
    }

    // --- Test #6: branch prediction hints ---

    #[test]
    fn test_branch_prediction_hints() {
        // @pure function with `if n <= 1` should get branch weight metadata.
        let fib_func = func_with_annotations(
            "fib",
            vec![param("n", HirType::Primitive(PrimitiveType::I64))],
            HirType::Primitive(PrimitiveType::I64),
            block(vec![
                stmt(HirStmtKind::If {
                    condition: binop(BinOp::LtEq, ident("n"), int_lit(1)),
                    then_block: block(vec![stmt(HirStmtKind::Return {
                        value: ident("n"),
                    })]),
                    else_block: Some(block(vec![stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    })])),
                }),
            ]),
            vec![pure_ann()],
        );

        let m = module(Some("test"), vec![fib_func]);
        let ir = codegen(&m).expect("codegen should succeed");

        // Should have !prof metadata on the branch.
        assert!(
            ir.contains("!prof !"),
            "base case branch should have !prof metadata: {ir}"
        );
        // Should have branch_weights metadata.
        assert!(
            ir.contains("branch_weights"),
            "should have branch_weights metadata: {ir}"
        );
        // Then-branch (base case) should be unlikely (weight 1).
        assert!(
            ir.contains("i32 1, i32 2000"),
            "base case should be unlikely: {ir}"
        );
    }

    #[test]
    fn test_no_branch_hints_in_non_pure() {
        // Non-@pure function should NOT get branch prediction hints.
        let m = module(
            Some("test"),
            vec![func(
                "test_fn",
                vec![param("n", HirType::Primitive(PrimitiveType::I32))],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::If {
                        condition: binop(BinOp::LtEq, ident("n"), int_lit(1)),
                        then_block: block(vec![stmt(HirStmtKind::Return {
                            value: int_lit(1),
                        })]),
                        else_block: None,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // Should NOT have !prof metadata.
        assert!(
            !ir.contains("!prof"),
            "non-pure function should not have branch hints: {ir}"
        );
    }

    // --- Test #7: loop vectorization hints ---

    #[test]
    fn test_loop_vectorization_hints() {
        // @vectorizable function with a for loop should get vectorization metadata.
        let sum_func = func_with_annotations(
            "vec_sum",
            vec![
                param(
                    "arr",
                    HirType::Array {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        size: 100,
                    },
                ),
                param("n", HirType::Primitive(PrimitiveType::I32)),
            ],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "sum".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(int_lit(0)),
                    mutable: true,
                }),
                stmt(HirStmtKind::For {
                    var: "i".to_string(),
                    var_span: span(),
                    var_type: HirType::Primitive(PrimitiveType::I32),
                    iterable: call("range", vec![int_lit(0), ident("n")]),
                    body: block(vec![stmt(HirStmtKind::Assign {
                        target: ident("sum"),
                        value: binop(BinOp::Add, ident("sum"), int_lit(1)),
                    })]),
                }),
                stmt(HirStmtKind::Return {
                    value: ident("sum"),
                }),
            ]),
            vec![vectorizable_ann()],
        );

        let m = module(Some("test"), vec![sum_func]);
        let ir = codegen(&m).expect("codegen should succeed");

        // Should have loop vectorization metadata.
        assert!(
            ir.contains("!llvm.loop"),
            "vectorizable loop should have !llvm.loop metadata: {ir}"
        );
        assert!(
            ir.contains("llvm.loop.vectorize.enable"),
            "should have vectorize.enable metadata: {ir}"
        );
    }

    #[test]
    fn test_no_vectorization_without_annotation() {
        // Regular function loops should NOT get vectorization metadata.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "sum".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(int_lit(0)),
                        mutable: true,
                    }),
                    stmt(HirStmtKind::For {
                        var: "i".to_string(),
                        var_span: span(),
                        var_type: HirType::Primitive(PrimitiveType::I32),
                        iterable: call("range", vec![int_lit(0), int_lit(10)]),
                        body: block(vec![stmt(HirStmtKind::Assign {
                            target: ident("sum"),
                            value: binop(BinOp::Add, ident("sum"), ident("i")),
                        })]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("sum"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            !ir.contains("llvm.loop.vectorize"),
            "non-vectorizable function should not have vectorize metadata: {ir}"
        );
    }

    // --- Test: fibonacci benchmark generates all optimization hints ---

    #[test]
    fn test_fibonacci_optimizations() {
        // Full integration test: fibonacci with @pure generates all expected hints.
        let source = std::fs::read_to_string("../../benchmarks/fib/fib.axm")
            .expect("should read fib.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "fib.axm should parse: {:?}",
            parse_result.errors
        );
        let hir_module = axiom_hir::lower(&parse_result.module)
            .expect("should lower");
        let ir = codegen(&hir_module).expect("should codegen");

        // 1. noalias is not applicable (no ptr params) -- that's correct.
        // 2. @pure attributes.
        assert!(
            ir.contains("memory(none)"),
            "fib should have memory(none): {ir}"
        );
        assert!(
            ir.contains("nounwind"),
            "fib should have nounwind: {ir}"
        );
        // 3. nsw on arithmetic.
        assert!(
            ir.contains("sub nsw i64"),
            "fib should have nsw on sub: {ir}"
        );
        assert!(
            ir.contains("add nsw i64"),
            "fib should have nsw on add: {ir}"
        );
        // 6. Branch prediction.
        assert!(
            ir.contains("!prof"),
            "fib base case should have branch prediction: {ir}"
        );
        assert!(
            ir.contains("branch_weights"),
            "fib should have branch_weights: {ir}"
        );
    }

    // --- Test: noalias on call-site arguments ---

    #[test]
    fn test_noalias_call_args() {
        // When calling a function with ptr params, the call-site should also
        // have noalias on the pointer arguments.
        let sum_func = func(
            "sum_arr",
            vec![
                param(
                    "arr",
                    HirType::Array {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        size: 10,
                    },
                ),
                param("n", HirType::Primitive(PrimitiveType::I32)),
            ],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![stmt(HirStmtKind::Return {
                value: int_lit(0),
            })]),
        );

        let main_func = func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "arr".to_string(),
                    name_span: span(),
                    ty: HirType::Array {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        size: 10,
                    },
                    value: Some(HirExpr {
                        id: nid(0),
                        kind: HirExprKind::ArrayZeros {
                            element_type: HirType::Primitive(PrimitiveType::I32),
                            size: 10,
                        },
                        span: span(),
                    }),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "result".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("sum_arr", vec![ident("arr"), int_lit(10)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: ident("result"),
                }),
            ]),
        );

        let m = module(Some("test"), vec![sum_func, main_func]);
        let ir = codegen(&m).expect("codegen should succeed");

        // Call-site should have noalias on ptr arg.
        assert!(
            ir.contains("ptr noalias %arr"),
            "call-site should have noalias on ptr args: {ir}"
        );
    }

    // --- Test: loop increment has nsw ---

    #[test]
    fn test_loop_increment_nsw() {
        // The for-loop increment should have nsw flag.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::For {
                        var: "i".to_string(),
                        var_span: span(),
                        var_type: HirType::Primitive(PrimitiveType::I32),
                        iterable: call("range", vec![int_lit(0), int_lit(10)]),
                        body: block(vec![]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // The loop increment should have nsw.
        assert!(
            ir.contains("add nsw i32"),
            "loop increment should have nsw: {ir}"
        );
    }

    // --- Test: bitwise builtins ---

    #[test]
    fn test_bitwise_builtins() {
        // Test band, bor, bxor, shl, shr, lshr, bnot
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // band(0xFF, 0x0F) = 15
                    stmt(HirStmtKind::Let {
                        name: "b_and".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("band", vec![int_lit(0xFF), int_lit(0x0F)])),
                        mutable: false,
                    }),
                    // bor(0xF0, 0x0F) = 255
                    stmt(HirStmtKind::Let {
                        name: "b_or".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("bor", vec![int_lit(0xF0), int_lit(0x0F)])),
                        mutable: false,
                    }),
                    // bxor(0xFF, 0x0F) = 240
                    stmt(HirStmtKind::Let {
                        name: "b_xor".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("bxor", vec![int_lit(0xFF), int_lit(0x0F)])),
                        mutable: false,
                    }),
                    // shl(1, 8) = 256
                    stmt(HirStmtKind::Let {
                        name: "shifted_l".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("shl", vec![int_lit(1), int_lit(8)])),
                        mutable: false,
                    }),
                    // shr(256, 4) = 16
                    stmt(HirStmtKind::Let {
                        name: "shifted_r".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("shr", vec![int_lit(256), int_lit(4)])),
                        mutable: false,
                    }),
                    // lshr(256, 4) = 16
                    stmt(HirStmtKind::Let {
                        name: "shifted_lr".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("lshr", vec![int_lit(256), int_lit(4)])),
                        mutable: false,
                    }),
                    // bnot(0) = -1
                    stmt(HirStmtKind::Let {
                        name: "b_not".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("bnot", vec![int_lit(0)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // band: LLVM `and`
        assert!(
            ir.contains("= and i32"),
            "band should emit LLVM `and`: {ir}"
        );
        // bor: LLVM `or`
        assert!(
            ir.contains("= or i32"),
            "bor should emit LLVM `or`: {ir}"
        );
        // bxor: LLVM `xor` (for values, not bnot)
        assert!(
            ir.contains("= xor i32") && ir.contains("255, 15"),
            "bxor should emit LLVM `xor`: {ir}"
        );
        // shl: LLVM `shl`
        assert!(
            ir.contains("= shl i32"),
            "shl should emit LLVM `shl`: {ir}"
        );
        // shr: LLVM `ashr`
        assert!(
            ir.contains("= ashr i32"),
            "shr should emit LLVM `ashr`: {ir}"
        );
        // lshr: LLVM `lshr`
        assert!(
            ir.contains("= lshr i32"),
            "lshr should emit LLVM `lshr`: {ir}"
        );
        // bnot: LLVM `xor %val, -1`
        assert!(
            ir.contains("xor i32"),
            "bnot should emit LLVM `xor %val, -1`: {ir}"
        );
    }

    #[test]
    fn test_bitwise_rotate_builtins() {
        // Test rotl and rotr which use LLVM funnel shift intrinsics.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // rotl(0x80000001, 1) = 3
                    stmt(HirStmtKind::Let {
                        name: "rot_l".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("rotl", vec![int_lit(0x80000001_u32 as i128), int_lit(1)])),
                        mutable: false,
                    }),
                    // rotr(3, 1) = 0x80000001
                    stmt(HirStmtKind::Let {
                        name: "rot_r".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("rotr", vec![int_lit(3), int_lit(1)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // rotl: @llvm.fshl.i32
        assert!(
            ir.contains("call i32 @llvm.fshl.i32(i32"),
            "rotl should call llvm.fshl.i32: {ir}"
        );
        assert!(
            ir.contains("declare i32 @llvm.fshl.i32(i32, i32, i32)"),
            "should declare llvm.fshl.i32: {ir}"
        );

        // rotr: @llvm.fshr.i32
        assert!(
            ir.contains("call i32 @llvm.fshr.i32(i32"),
            "rotr should call llvm.fshr.i32: {ir}"
        );
        assert!(
            ir.contains("declare i32 @llvm.fshr.i32(i32, i32, i32)"),
            "should declare llvm.fshr.i32: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // Heap allocation builtin tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_heap_alloc() {
        // heap_alloc(100, 4) should emit sext + mul + call malloc with noalias
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "p".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("heap_alloc", vec![int_lit(100), int_lit(4)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call noalias ptr @malloc(i64"),
            "heap_alloc should emit malloc call with noalias: {ir}"
        );
        assert!(
            ir.contains("declare noalias ptr @malloc(i64) #"),
            "should declare malloc: {ir}"
        );
    }

    #[test]
    fn test_heap_alloc_zeroed() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "p".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("heap_alloc_zeroed", vec![int_lit(50), int_lit(4)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call noalias ptr @calloc(i64"),
            "heap_alloc_zeroed should emit calloc call: {ir}"
        );
        assert!(
            ir.contains("declare noalias ptr @calloc(i64, i64) #"),
            "should declare calloc: {ir}"
        );
    }

    #[test]
    fn test_heap_free() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "p".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("heap_free", vec![ident("p")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call void @free(ptr"),
            "heap_free should emit free call: {ir}"
        );
        assert!(
            ir.contains("declare void @free(ptr allocptr) #"),
            "should declare free: {ir}"
        );
    }

    #[test]
    fn test_heap_realloc() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "p".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                        mutable: true,
                    }),
                    stmt(HirStmtKind::Assign {
                        target: ident("p"),
                        value: call(
                            "heap_realloc",
                            vec![ident("p"), int_lit(20), int_lit(4)],
                        ),
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("heap_free", vec![ident("p")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call noalias ptr @realloc(ptr"),
            "heap_realloc should emit realloc call: {ir}"
        );
        assert!(
            ir.contains("declare noalias ptr @realloc(ptr, i64) #"),
            "should declare realloc: {ir}"
        );
    }

    #[test]
    fn test_ptr_read_write_i32() {
        // ptr_write_i32(p, 0, 42); ptr_read_i32(p, 0) should emit GEP + store/load
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "p".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call(
                            "ptr_write_i32",
                            vec![ident("p"), int_lit(0), int_lit(42)],
                        ),
                    }),
                    stmt(HirStmtKind::Let {
                        name: "val".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("ptr_read_i32", vec![ident("p"), int_lit(0)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("heap_free", vec![ident("p")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("val"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // ptr_write: GEP + store
        assert!(
            ir.contains("getelementptr i32, ptr"),
            "ptr_write should emit GEP: {ir}"
        );
        assert!(
            ir.contains("store i32"),
            "ptr_write should emit store: {ir}"
        );
        // ptr_read: GEP + load
        assert!(
            ir.contains("load i32, ptr"),
            "ptr_read should emit load: {ir}"
        );
    }

    #[test]
    fn test_ptr_read_write_f64() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "p".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::F64)),
                        },
                        value: Some(call("heap_alloc", vec![int_lit(10), int_lit(8)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call(
                            "ptr_write_f64",
                            vec![ident("p"), int_lit(0), float_lit(3.14)],
                        ),
                    }),
                    stmt(HirStmtKind::Let {
                        name: "val".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(call("ptr_read_f64", vec![ident("p"), int_lit(0)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("heap_free", vec![ident("p")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("getelementptr double, ptr"),
            "ptr_write_f64 should emit GEP with double: {ir}"
        );
        assert!(
            ir.contains("store double"),
            "ptr_write_f64 should emit store double: {ir}"
        );
        assert!(
            ir.contains("load double, ptr"),
            "ptr_read_f64 should emit load double: {ir}"
        );
    }

    #[test]
    fn test_ptr_read_write_i64() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "p".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I64)),
                        },
                        value: Some(call("heap_alloc", vec![int_lit(10), int_lit(8)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call(
                            "ptr_write_i64",
                            vec![ident("p"), int_lit(0), int_lit(999)],
                        ),
                    }),
                    stmt(HirStmtKind::Let {
                        name: "val".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: Some(call("ptr_read_i64", vec![ident("p"), int_lit(0)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("heap_free", vec![ident("p")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("getelementptr i64, ptr"),
            "ptr_write_i64 should emit GEP with i64: {ir}"
        );
        assert!(
            ir.contains("store i64"),
            "ptr_write_i64 should emit store i64: {ir}"
        );
        assert!(
            ir.contains("load i64, ptr"),
            "ptr_read_i64 should emit load i64: {ir}"
        );
    }

    #[test]
    fn test_heap_program_full_integration() {
        // A full program: alloc, write, read, sum, free -- mirrors heap_test.axm
        let m = module(
            Some("heap_int_test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // let data: ptr[i32] = heap_alloc(10, 4);
                    stmt(HirStmtKind::Let {
                        name: "data".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                        mutable: false,
                    }),
                    // for i in range(0,10) { ptr_write_i32(data, i, i*i); }
                    stmt(HirStmtKind::For {
                        var: "i".to_string(),
                        var_span: span(),
                        var_type: HirType::Primitive(PrimitiveType::I32),
                        iterable: call("range", vec![int_lit(0), int_lit(10)]),
                        body: block(vec![stmt(HirStmtKind::Expr {
                            expr: call(
                                "ptr_write_i32",
                                vec![
                                    ident("data"),
                                    ident("i"),
                                    binop(BinOp::Mul, ident("i"), ident("i")),
                                ],
                            ),
                        })]),
                    }),
                    // let sum: i64 = 0;
                    stmt(HirStmtKind::Let {
                        name: "sum".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: Some(int_lit(0)),
                        mutable: true,
                    }),
                    // for i in range(0,10) { sum = sum + widen(ptr_read_i32(data, i)); }
                    stmt(HirStmtKind::For {
                        var: "i".to_string(),
                        var_span: span(),
                        var_type: HirType::Primitive(PrimitiveType::I32),
                        iterable: call("range", vec![int_lit(0), int_lit(10)]),
                        body: block(vec![stmt(HirStmtKind::Assign {
                            target: ident("sum"),
                            value: binop(
                                BinOp::Add,
                                ident("sum"),
                                call(
                                    "widen",
                                    vec![call("ptr_read_i32", vec![ident("data"), ident("i")])],
                                ),
                            ),
                        })]),
                    }),
                    // heap_free(data);
                    stmt(HirStmtKind::Expr {
                        expr: call("heap_free", vec![ident("data")]),
                    }),
                    // print_i64(sum);
                    stmt(HirStmtKind::Expr {
                        expr: call("print_i64", vec![ident("sum")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // Verify all key components are present.
        assert!(
            ir.contains("call noalias ptr @malloc(i64"),
            "should emit malloc: {ir}"
        );
        assert!(
            ir.contains("getelementptr i32, ptr"),
            "should emit GEP for ptr_write/read: {ir}"
        );
        assert!(
            ir.contains("call void @free(ptr"),
            "should emit free: {ir}"
        );
        assert!(
            ir.contains("sext i32"),
            "should widen i32 index to i64: {ir}"
        );
        assert!(
            ir.contains("declare noalias ptr @malloc(i64) #"),
            "should declare malloc: {ir}"
        );
        assert!(
            ir.contains("declare void @free(ptr allocptr) #"),
            "should declare free: {ir}"
        );
    }

    #[test]
    fn test_no_malloc_decl_when_unused() {
        // A program that doesn't use heap builtins should NOT declare malloc/free.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![stmt(HirStmtKind::Return {
                    value: int_lit(0),
                })]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            !ir.contains("@malloc"),
            "should not declare malloc when unused: {ir}"
        );
        assert!(
            !ir.contains("@free"),
            "should not declare free when unused: {ir}"
        );
        assert!(
            !ir.contains("@calloc"),
            "should not declare calloc when unused: {ir}"
        );
        assert!(
            !ir.contains("@realloc"),
            "should not declare realloc when unused: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // LLVM allocator attribute tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_malloc_alloc_attrs() {
        // Verify that malloc/calloc/realloc/free declarations include
        // LLVM allockind and alloc-family attributes for optimizer integration.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "p".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                        mutable: true,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "q".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("heap_alloc_zeroed", vec![int_lit(10), int_lit(4)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Assign {
                        target: ident("p"),
                        value: call(
                            "heap_realloc",
                            vec![ident("p"), int_lit(20), int_lit(4)],
                        ),
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("heap_free", vec![ident("p")]),
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("heap_free", vec![ident("q")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // malloc declaration should have allockind("alloc,uninitialized") attribute group.
        assert!(
            ir.contains("declare noalias ptr @malloc(i64) #"),
            "malloc should have attribute group reference: {ir}"
        );
        assert!(
            ir.contains("allockind(\"alloc,uninitialized\")"),
            "malloc attribute group should have allockind(alloc,uninitialized): {ir}"
        );

        // calloc declaration should have allockind("alloc,zeroed") attribute group.
        assert!(
            ir.contains("declare noalias ptr @calloc(i64, i64) #"),
            "calloc should have attribute group reference: {ir}"
        );
        assert!(
            ir.contains("allockind(\"alloc,zeroed\")"),
            "calloc attribute group should have allockind(alloc,zeroed): {ir}"
        );

        // realloc declaration should have allockind("realloc") attribute group.
        assert!(
            ir.contains("declare noalias ptr @realloc(ptr, i64) #"),
            "realloc should have attribute group reference: {ir}"
        );
        assert!(
            ir.contains("allockind(\"realloc\")"),
            "realloc attribute group should have allockind(realloc): {ir}"
        );

        // free declaration should have allocptr parameter attribute and allockind("free").
        assert!(
            ir.contains("declare void @free(ptr allocptr) #"),
            "free should have allocptr param attribute and attr group ref: {ir}"
        );
        assert!(
            ir.contains("allockind(\"free\")"),
            "free attribute group should have allockind(free): {ir}"
        );

        // All allocator functions should be in the same alloc-family.
        let family_count = ir.matches("\"alloc-family\"=\"malloc\"").count();
        assert!(
            family_count >= 4,
            "all 4 allocator declarations should have alloc-family=malloc (got {family_count}): {ir}"
        );
    }

    #[test]
    fn test_escape_analysis_hint() {
        // A @pure function that uses heap_alloc should get both @pure function
        // attributes (memory(none), nounwind, etc.) and the LLVM allocator
        // attributes on the malloc declaration. Together these enable LLVM's
        // HeapToStackPass to promote the allocation to the stack.
        let m = module(
            Some("test"),
            vec![func_with_annotations(
                "compute",
                vec![param("n", HirType::Primitive(PrimitiveType::I32))],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "buf".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("heap_alloc", vec![ident("n"), int_lit(4)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("heap_free", vec![ident("buf")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(42),
                    }),
                ]),
                vec![pure_ann()],
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // The function should have @pure attributes (memory + nounwind).
        // Note: @pure with a ptr result from heap_alloc uses argmem:read if
        // there are ptr params. But compute takes only i32, so memory(none).
        assert!(
            ir.contains("memory(none)"),
            "@pure function should have memory(none): {ir}"
        );
        assert!(
            ir.contains("nounwind"),
            "@pure function should have nounwind: {ir}"
        );

        // The malloc declaration should have allocator attributes.
        assert!(
            ir.contains("allockind(\"alloc,uninitialized\")"),
            "malloc should have allockind for heap-to-stack optimization: {ir}"
        );
        assert!(
            ir.contains("\"alloc-family\"=\"malloc\""),
            "should have alloc-family for malloc/free pairing: {ir}"
        );

        // The free declaration should have allocator attributes.
        assert!(
            ir.contains("allockind(\"free\")"),
            "free should have allockind for dead-free elimination: {ir}"
        );
    }

    #[test]
    fn test_lifetime_scope() {
        // @lifetime(scope) on a let binding with heap_alloc should emit alloca
        // instead of malloc, promoting the heap allocation to the stack.
        let lifetime_scope_ann = HirAnnotation {
            kind: HirAnnotationKind::Lifetime("scope".to_string()),
            span: SPAN_DUMMY,
        };

        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt_with_annotations(
                        HirStmtKind::Let {
                            name: "buf".to_string(),
                            name_span: span(),
                            ty: HirType::Ptr {
                                element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                            },
                            value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                            mutable: false,
                        },
                        vec![lifetime_scope_ann],
                    ),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // The allocation should be promoted to alloca (stack).
        assert!(
            ir.contains("alloca i8, i64"),
            "@lifetime(scope) should promote heap_alloc to alloca: {ir}"
        );

        // No malloc should be called — the allocation is on the stack.
        assert!(
            !ir.contains("call noalias ptr @malloc"),
            "@lifetime(scope) should NOT call malloc: {ir}"
        );

        // No malloc declaration needed since it was promoted.
        assert!(
            !ir.contains("@malloc"),
            "@lifetime(scope) should not need malloc declaration: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // Arena (bump) allocator builtin tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_arena_create() {
        // arena_create(1048576) should emit two malloc calls + stores for struct fields.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "arena".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("arena_create", vec![int_lit(1048576)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // Should allocate 24 bytes for the arena struct.
        assert!(
            ir.contains("call noalias ptr @malloc(i64 24)"),
            "arena_create should malloc 24 bytes for arena struct: {ir}"
        );
        // Should allocate the backing buffer.
        assert!(
            ir.contains("call noalias ptr @malloc(i64 %"),
            "arena_create should malloc the backing buffer: {ir}"
        );
        // Should store offset = 0.
        assert!(
            ir.contains("store i64 0, ptr %"),
            "arena_create should store offset = 0: {ir}"
        );
        // Should emit GEP for offset field at +8.
        assert!(
            ir.contains("getelementptr i8, ptr %"),
            "arena_create should emit GEP for struct fields: {ir}"
        );
        // Should declare malloc.
        assert!(
            ir.contains("declare noalias ptr @malloc(i64) #"),
            "arena_create should declare malloc: {ir}"
        );
    }

    #[test]
    fn test_arena_alloc() {
        // arena_alloc(arena, 100, 4) should emit GEP + load offset + bump + store.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "arena".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("arena_create", vec![int_lit(4096)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "data".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call(
                            "arena_alloc",
                            vec![ident("arena"), int_lit(100), int_lit(4)],
                        )),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // Should load the base pointer.
        assert!(
            ir.contains("load ptr, ptr %"),
            "arena_alloc should load base pointer: {ir}"
        );
        // Should load the offset.
        assert!(
            ir.contains("load i64, ptr %"),
            "arena_alloc should load offset: {ir}"
        );
        // Should compute total = count * elem_size.
        assert!(
            ir.contains("mul i64"),
            "arena_alloc should compute total bytes: {ir}"
        );
        // Should compute new_offset = offset + total.
        assert!(
            ir.contains("add i64"),
            "arena_alloc should bump the offset: {ir}"
        );
        // Should compute result pointer = base + offset via GEP.
        let gep_count = ir.matches("getelementptr i8, ptr %").count();
        assert!(
            gep_count >= 2,
            "arena_alloc should emit GEP for struct fields and result pointer (got {gep_count}): {ir}"
        );
    }

    #[test]
    fn test_arena_reset() {
        // arena_reset(arena) should store 0 to the offset field.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "arena".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("arena_create", vec![int_lit(4096)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("arena_reset", vec![ident("arena")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // The reset emits a GEP to arena+8 then store i64 0.
        // Count "store i64 0" -- arena_create stores one (offset=0), arena_reset stores another.
        let store_zero_count = ir.matches("store i64 0, ptr %").count();
        assert!(
            store_zero_count >= 2,
            "arena_reset should store 0 to offset (found {store_zero_count} store-zero ops): {ir}"
        );
    }

    #[test]
    fn test_arena_destroy() {
        // arena_destroy(arena) should free the base buffer and the arena struct.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "arena".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("arena_create", vec![int_lit(4096)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("arena_destroy", vec![ident("arena")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // Should emit two free calls (one for base, one for arena struct).
        let free_count = ir.matches("call void @free(ptr").count();
        assert!(
            free_count == 2,
            "arena_destroy should emit exactly 2 free calls (got {free_count}): {ir}"
        );
        // Should declare free.
        assert!(
            ir.contains("declare void @free(ptr allocptr) #"),
            "arena_destroy should declare free: {ir}"
        );
    }

    #[test]
    fn test_arena_program() {
        // Full integration: create arena, alloc, write, read, reset, reuse, destroy.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // let arena: ptr[i32] = arena_create(4096);
                    stmt(HirStmtKind::Let {
                        name: "arena".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("arena_create", vec![int_lit(4096)])),
                        mutable: false,
                    }),
                    // let data: ptr[i32] = arena_alloc(arena, 10, 4);
                    stmt(HirStmtKind::Let {
                        name: "data".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call(
                            "arena_alloc",
                            vec![ident("arena"), int_lit(10), int_lit(4)],
                        )),
                        mutable: false,
                    }),
                    // ptr_write_i32(data, 0, 42);
                    stmt(HirStmtKind::Expr {
                        expr: call(
                            "ptr_write_i32",
                            vec![ident("data"), int_lit(0), int_lit(42)],
                        ),
                    }),
                    // let val: i32 = ptr_read_i32(data, 0);
                    stmt(HirStmtKind::Let {
                        name: "val".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("ptr_read_i32", vec![ident("data"), int_lit(0)])),
                        mutable: false,
                    }),
                    // arena_reset(arena);
                    stmt(HirStmtKind::Expr {
                        expr: call("arena_reset", vec![ident("arena")]),
                    }),
                    // let reused: ptr[i32] = arena_alloc(arena, 5, 4);
                    stmt(HirStmtKind::Let {
                        name: "reused".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call(
                            "arena_alloc",
                            vec![ident("arena"), int_lit(5), int_lit(4)],
                        )),
                        mutable: false,
                    }),
                    // arena_destroy(arena);
                    stmt(HirStmtKind::Expr {
                        expr: call("arena_destroy", vec![ident("arena")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // Verify all key components are present.
        assert!(
            ir.contains("call noalias ptr @malloc(i64 24)"),
            "should emit malloc for arena struct: {ir}"
        );
        assert!(
            ir.contains("getelementptr i8, ptr"),
            "should emit GEP for arena struct fields: {ir}"
        );
        assert!(
            ir.contains("load ptr, ptr"),
            "should load base pointer: {ir}"
        );
        assert!(
            ir.contains("load i64, ptr"),
            "should load offset: {ir}"
        );
        assert!(
            ir.contains("mul i64"),
            "should compute total bytes: {ir}"
        );
        assert!(
            ir.contains("add i64"),
            "should bump offset: {ir}"
        );
        let free_count = ir.matches("call void @free(ptr").count();
        assert!(
            free_count == 2,
            "should emit 2 free calls for destroy (got {free_count}): {ir}"
        );
        assert!(
            ir.contains("declare noalias ptr @malloc(i64) #"),
            "should declare malloc: {ir}"
        );
        assert!(
            ir.contains("declare void @free(ptr allocptr) #"),
            "should declare free: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // Struct codegen tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_struct_type_definition() {
        let vec3 = hir_struct(
            "Vec3",
            vec![
                struct_field("x", HirType::Primitive(PrimitiveType::F64)),
                struct_field("y", HirType::Primitive(PrimitiveType::F64)),
                struct_field("z", HirType::Primitive(PrimitiveType::F64)),
            ],
        );
        let m = module_with_structs(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![stmt(HirStmtKind::Return {
                    value: int_lit(0),
                })]),
            )],
            vec![vec3],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("%struct.Vec3 = type { double, double, double }"),
            "should emit struct type definition: {ir}"
        );
    }

    #[test]
    fn test_struct_field_access() {
        let point = hir_struct(
            "Point",
            vec![
                struct_field("x", HirType::Primitive(PrimitiveType::F64)),
                struct_field("y", HirType::Primitive(PrimitiveType::F64)),
            ],
        );
        let m = module_with_structs(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // let p: Point;
                    stmt(HirStmtKind::Let {
                        name: "p".to_string(),
                        name_span: span(),
                        ty: HirType::UserDefined("Point".to_string()),
                        value: None,
                        mutable: false,
                    }),
                    // p.x = 1.0;
                    stmt(HirStmtKind::Assign {
                        target: field_access(ident("p"), "x"),
                        value: float_lit(1.0),
                    }),
                    // p.y = 2.0;
                    stmt(HirStmtKind::Assign {
                        target: field_access(ident("p"), "y"),
                        value: float_lit(2.0),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
            vec![point],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // Struct type should be emitted.
        assert!(
            ir.contains("%struct.Point = type { double, double }"),
            "should emit struct type: {ir}"
        );
        // Alloca for struct.
        assert!(
            ir.contains("alloca %struct.Point"),
            "should alloca struct: {ir}"
        );
        // Memset zero-init.
        assert!(
            ir.contains("call void @llvm.memset.p0.i64(ptr %p.0, i8 0, i64 16, i1 false)"),
            "should zero-init struct: {ir}"
        );
        // GEP for field x (index 0).
        assert!(
            ir.contains("getelementptr inbounds %struct.Point, ptr %p.0, i32 0, i32 0"),
            "should GEP field x (index 0): {ir}"
        );
        // GEP for field y (index 1).
        assert!(
            ir.contains("getelementptr inbounds %struct.Point, ptr %p.0, i32 0, i32 1"),
            "should GEP field y (index 1): {ir}"
        );
        // Store values.
        assert!(
            ir.contains("store double 1.0"),
            "should store field x: {ir}"
        );
        assert!(
            ir.contains("store double 2.0"),
            "should store field y: {ir}"
        );
    }

    #[test]
    fn test_struct_as_param() {
        let vec2 = hir_struct(
            "Vec2",
            vec![
                struct_field("x", HirType::Primitive(PrimitiveType::F64)),
                struct_field("y", HirType::Primitive(PrimitiveType::F64)),
            ],
        );
        let m = module_with_structs(
            Some("test"),
            vec![func(
                "get_x",
                vec![param("v", HirType::UserDefined("Vec2".to_string()))],
                HirType::Primitive(PrimitiveType::F64),
                block(vec![stmt(HirStmtKind::Return {
                    value: field_access(ident("v"), "x"),
                })]),
            )],
            vec![vec2],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // Struct param should be ptr.
        assert!(
            ir.contains("@get_x(ptr noalias %v)"),
            "should pass struct as ptr noalias: {ir}"
        );
        // Should alloca ptr for param.
        assert!(
            ir.contains("%v.addr = alloca ptr"),
            "should alloca ptr for struct param: {ir}"
        );
        // Should store param ptr.
        assert!(
            ir.contains("store ptr %v, ptr %v.addr"),
            "should store struct param ptr: {ir}"
        );
        // Should GEP for field access on param.
        assert!(
            ir.contains("getelementptr inbounds %struct.Vec2"),
            "should GEP for field access: {ir}"
        );
    }

    #[test]
    fn test_struct_program() {
        // Full integration test using parse → lower → codegen pipeline.
        let source = r#"
@module struct_prog;

struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

@pure
fn vec3_dot(a: Vec3, b: Vec3) -> f64 {
    return a.x * b.x + a.y * b.y + a.z * b.z;
}

fn main() -> i32 {
    let a: Vec3;
    a.x = 3.0;
    a.y = 4.0;
    a.z = 0.0;

    let dot: f64 = vec3_dot(a, a);
    print_f64(dot);
    return 0;
}
"#;
        let parse_result = axiom_parser::parse(source);
        assert!(
            parse_result.errors.is_empty(),
            "parse errors: {:?}",
            parse_result.errors
        );
        let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
        let ir = codegen(&hir).expect("codegen should succeed");

        // Struct type definition.
        assert!(
            ir.contains("%struct.Vec3 = type { double, double, double }"),
            "should define Vec3: {ir}"
        );
        // Struct alloca + memset.
        assert!(
            ir.contains("alloca %struct.Vec3"),
            "should alloca Vec3: {ir}"
        );
        assert!(
            ir.contains("call void @llvm.memset.p0.i64"),
            "should zero-init: {ir}"
        );
        // Pure function with struct params gets memory(argmem: read).
        assert!(
            ir.contains("memory(argmem: read)"),
            "should have argmem read for @pure with struct params: {ir}"
        );
        // Field stores.
        assert!(ir.contains("store double 3.0"), "should store x: {ir}");
        assert!(ir.contains("store double 4.0"), "should store y: {ir}");
        // Dot product call.
        assert!(
            ir.contains("call fastcc double @vec3_dot"),
            "should call vec3_dot: {ir}"
        );
    }

    #[test]
    fn test_struct_field_read() {
        let point = hir_struct(
            "Point",
            vec![
                struct_field("x", HirType::Primitive(PrimitiveType::F64)),
                struct_field("y", HirType::Primitive(PrimitiveType::F64)),
            ],
        );
        let m = module_with_structs(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // let p: Point;
                    stmt(HirStmtKind::Let {
                        name: "p".to_string(),
                        name_span: span(),
                        ty: HirType::UserDefined("Point".to_string()),
                        value: None,
                        mutable: false,
                    }),
                    // p.x = 5.0;
                    stmt(HirStmtKind::Assign {
                        target: field_access(ident("p"), "x"),
                        value: float_lit(5.0),
                    }),
                    // let val: f64 = p.x;
                    stmt(HirStmtKind::Let {
                        name: "val".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: Some(field_access(ident("p"), "x")),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
            vec![point],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // Should load field value for the let binding.
        assert!(
            ir.contains("load double, ptr"),
            "should load field value: {ir}"
        );
    }

    #[test]
    fn test_let_without_initializer() {
        // Test that `let x: i32;` (no initializer) works with zero-init.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: None,
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("x"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("alloca i32"),
            "should alloca: {ir}"
        );
        assert!(
            ir.contains("store i32 0, ptr"),
            "should zero-init: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // I/O runtime builtin tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_clock_ns_builtin() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "t".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: Some(call("clock_ns", vec![])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call i64 @axiom_clock_ns()"),
            "should call axiom_clock_ns: {ir}"
        );
        assert!(
            ir.contains("declare i64 @axiom_clock_ns()"),
            "should declare axiom_clock_ns: {ir}"
        );
        assert!(needs_runtime(&ir), "IR using clock_ns should need runtime");
    }

    #[test]
    fn test_file_read_builtin() {
        // file_read takes two ptr args; we use heap_alloc to get a ptr for out_size.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "sz_ptr".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I8)),
                        },
                        value: Some(call("heap_alloc", vec![int_lit(1), int_lit(8)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "buf".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I8)),
                        },
                        value: Some(call(
                            "file_read",
                            vec![str_lit("test.txt"), ident("sz_ptr")],
                        )),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call ptr @axiom_file_read(ptr"),
            "should call axiom_file_read: {ir}"
        );
        assert!(
            ir.contains("declare ptr @axiom_file_read(ptr, ptr)"),
            "should declare axiom_file_read: {ir}"
        );
    }

    #[test]
    fn test_file_size_builtin() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "sz".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: Some(call("file_size", vec![str_lit("test.txt")])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call i64 @axiom_file_size(ptr"),
            "should call axiom_file_size: {ir}"
        );
        assert!(
            ir.contains("declare i64 @axiom_file_size(ptr)"),
            "should declare axiom_file_size: {ir}"
        );
    }

    #[test]
    fn test_get_argc_builtin() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "n".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("get_argc", vec![])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call i32 @axiom_get_argc()"),
            "should call axiom_get_argc: {ir}"
        );
        assert!(
            ir.contains("declare i32 @axiom_get_argc()"),
            "should declare axiom_get_argc: {ir}"
        );
    }

    #[test]
    fn test_get_argv_builtin() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "arg0".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I8)),
                        },
                        value: Some(call("get_argv", vec![int_lit(0)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call ptr @axiom_get_argv(i32"),
            "should call axiom_get_argv: {ir}"
        );
        assert!(
            ir.contains("declare ptr @axiom_get_argv(i32)"),
            "should declare axiom_get_argv: {ir}"
        );
    }

    #[test]
    fn test_runtime_declarations_only_when_needed() {
        // A module that doesn't use I/O builtins should not emit runtime declarations.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![stmt(HirStmtKind::Return {
                    value: int_lit(0),
                })]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            !ir.contains("@axiom_"),
            "should NOT contain runtime declarations: {ir}"
        );
        assert!(
            !needs_runtime(&ir),
            "IR without I/O builtins should not need runtime"
        );
    }

    #[test]
    fn test_file_write_builtin() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Expr {
                        expr: call(
                            "file_write",
                            vec![str_lit("out.bin"), str_lit("data"), int_lit(4)],
                        ),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call void @axiom_file_write(ptr"),
            "should call axiom_file_write: {ir}"
        );
        assert!(
            ir.contains("declare void @axiom_file_write(ptr, ptr, i64)"),
            "should declare axiom_file_write: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // Coroutine builtin tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_coro_resume_builtin() {
        // coro_resume(handle: i32) -> i32
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "val".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("coro_resume", vec![int_lit(0)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call i32 @axiom_coro_resume(i32"),
            "should call axiom_coro_resume: {ir}"
        );
        assert!(
            ir.contains("declare i32 @axiom_coro_resume(i32)"),
            "should declare axiom_coro_resume: {ir}"
        );
        assert!(
            needs_runtime(&ir),
            "IR using coro_resume should need runtime"
        );
    }

    #[test]
    fn test_coro_yield_builtin() {
        // coro_yield(value: i32)
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Expr {
                        expr: call("coro_yield", vec![int_lit(42)]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call void @axiom_coro_yield(i32"),
            "should call axiom_coro_yield: {ir}"
        );
        assert!(
            ir.contains("declare void @axiom_coro_yield(i32)"),
            "should declare axiom_coro_yield: {ir}"
        );
    }

    #[test]
    fn test_coro_is_done_builtin() {
        // coro_is_done(handle: i32) -> i32
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "done".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: Some(call("coro_is_done", vec![int_lit(0)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call i32 @axiom_coro_is_done(i32"),
            "should call axiom_coro_is_done: {ir}"
        );
        assert!(
            ir.contains("declare i32 @axiom_coro_is_done(i32)"),
            "should declare axiom_coro_is_done: {ir}"
        );
    }

    #[test]
    fn test_coro_destroy_builtin() {
        // coro_destroy(handle: i32)
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Expr {
                        expr: call("coro_destroy", vec![int_lit(0)]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("call void @axiom_coro_destroy(i32"),
            "should call axiom_coro_destroy: {ir}"
        );
        assert!(
            ir.contains("declare void @axiom_coro_destroy(i32)"),
            "should declare axiom_coro_destroy: {ir}"
        );
    }

    #[test]
    fn test_coro_all_declarations_emitted() {
        // When any coroutine builtin is used, all 5 extern declarations should be emitted.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Expr {
                        expr: call("coro_yield", vec![int_lit(1)]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("declare i32 @axiom_coro_create(ptr, i32)"),
            "should declare axiom_coro_create: {ir}"
        );
        assert!(
            ir.contains("declare i32 @axiom_coro_resume(i32)"),
            "should declare axiom_coro_resume: {ir}"
        );
        assert!(
            ir.contains("declare void @axiom_coro_yield(i32)"),
            "should declare axiom_coro_yield: {ir}"
        );
        assert!(
            ir.contains("declare i32 @axiom_coro_is_done(i32)"),
            "should declare axiom_coro_is_done: {ir}"
        );
        assert!(
            ir.contains("declare void @axiom_coro_destroy(i32)"),
            "should declare axiom_coro_destroy: {ir}"
        );
    }

    #[test]
    fn test_coro_needs_runtime() {
        // Coroutine builtins should trigger needs_runtime detection.
        assert!(needs_runtime("declare i32 @axiom_coro_create(ptr, i32)"));
        assert!(needs_runtime("declare i32 @axiom_coro_resume(i32)"));
        assert!(needs_runtime("declare void @axiom_coro_yield(i32)"));
        assert!(needs_runtime("declare i32 @axiom_coro_is_done(i32)"));
        assert!(needs_runtime("declare void @axiom_coro_destroy(i32)"));
        // Non-coroutine, non-runtime IR should not trigger.
        assert!(!needs_runtime("define i32 @main() { ret i32 0 }"));
    }
}
