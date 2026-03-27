//! Native binary compilation -- invokes an external C/LLVM compiler on LLVM IR text.
//!
//! This module encapsulates compiler discovery and invocation. It writes LLVM IR
//! to a temporary `.ll` file and invokes `clang` (or a versioned variant) to
//! produce a native executable.
//!
//! When the generated IR references `@axiom_*` runtime helpers, the tiny C
//! runtime (`runtime/axiom_rt.c`) is compiled and linked alongside the `.ll`
//! file automatically.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The kind of compiler discovered on the system.
enum CompilerKind {
    /// `clang` -- can compile `.ll` directly to an executable.
    Clang(PathBuf),
}

/// Options controlling the native compilation step.
///
/// These options allow the caller to influence the target architecture and
/// optimization level used by the backend compiler (clang).
#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    /// Target architecture for `-march=`. When `None` (the default), the
    /// compiler uses `-march=native` to auto-detect the host CPU features
    /// (AVX2, AVX-512, etc.).  When `Some("x86-64-v4")` for example, the
    /// specified architecture is used verbatim.
    pub target_arch: Option<String>,

    /// Optimization profile derived from `@constraint { optimize_for: X }`.
    /// Maps to different clang `-O` flags:
    /// - `"performance"` -> `-O3`
    /// - `"memory"` -> `-Os`
    /// - `"size"` -> `-Oz`
    /// - `"latency"` -> `-O3 -fno-exceptions`
    /// - anything else / `None` -> default `-O2`
    pub optimize_for: Option<String>,

    /// The LLVM IR text, used to detect which features are needed (e.g.,
    /// renderer functions) for linking decisions.
    pub ir_text: Option<String>,

    /// Additional library search directories (passed as `-L` to clang).
    pub link_dirs: Vec<String>,

    /// When true, pass `-fsave-optimization-record` to clang to produce
    /// a `.opt.yaml` file alongside the output binary. The YAML file
    /// contains LLVM optimization remarks (applied and missed).
    pub opt_report: bool,

    /// Sanitizer to enable: `"address"`, `"thread"`, `"undefined"`, or `"memory"`.
    /// When set, `-fsanitize=<value>` and `-g` are passed to clang.
    pub sanitize: Option<String>,

    /// When true, compile the C runtime with `-DAXIOM_DEBUG_MODE` to enable
    /// the crash handler (stack traces on crash), and add `-g` and
    /// `-fno-omit-frame-pointer` for reliable stack walking.
    /// On Windows, also links `-ldbghelp`.
    pub debug_mode: bool,
}

/// Compile LLVM IR text to a native executable binary with default options.
///
/// Writes the IR to a temporary file, invokes the discovered compiler, and
/// cleans up the temp file on success. On failure the temp file is left in
/// place for debugging and its path is printed to stderr.
pub fn compile_to_binary(llvm_ir: &str, output_path: &str) -> miette::Result<()> {
    compile_to_binary_with_options(llvm_ir, output_path, &CompileOptions::default())
}

/// Compile LLVM IR text to a native executable binary with explicit options.
///
/// This is the full-featured entry point that respects `@constraint` and
/// `@target` annotations via [`CompileOptions`].
pub fn compile_to_binary_with_options(
    llvm_ir: &str,
    output_path: &str,
    options: &CompileOptions,
) -> miette::Result<()> {
    let temp_ll = temp_ll_path();
    std::fs::write(&temp_ll, llvm_ir)
        .map_err(|e| miette::miette!("failed to write temp file {}: {}", temp_ll.display(), e))?;

    let compiler = find_compiler()?;

    let needs_rt = axiom_codegen::needs_runtime(llvm_ir);
    let rt_path = if needs_rt {
        Some(write_runtime_c()?)
    } else {
        None
    };

    let result = match &compiler {
        CompilerKind::Clang(path) => invoke_clang(path, &temp_ll, output_path, rt_path.as_deref(), options),
    };

    // Clean up temp files on success; leave them on failure for debugging.
    if result.is_ok() {
        let _ = std::fs::remove_file(&temp_ll);
        if let Some(ref rp) = rt_path {
            let _ = std::fs::remove_file(rp);
        }
    } else {
        eprintln!("note: LLVM IR written to {} for debugging", temp_ll.display());
    }

    result
}

/// Search for a C/LLVM compiler on `PATH`.
///
/// Returns `Some(name)` with the first compiler name found, or `None`.
#[cfg(test)]
fn find_compiler_name() -> Option<String> {
    let candidates = [
        "clang",
        "clang-19",
        "clang-18",
        "clang-17",
        "clang-16",
        "clang-15",
        "gcc",
        "cc",
    ];
    for name in &candidates {
        if compiler_exists(name) {
            return Some((*name).to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// The C runtime source files, embedded at compile time.
/// The main file `axiom_rt.c` includes the split files via `#include`.
const AXIOM_RT_C: &str = include_str!("../runtime/axiom_rt.c");
const AXIOM_RT_CORE_C: &str = include_str!("../runtime/axiom_rt_core.c");
const AXIOM_RT_IO_C: &str = include_str!("../runtime/axiom_rt_io.c");
const AXIOM_RT_COROUTINES_C: &str = include_str!("../runtime/axiom_rt_coroutines.c");
const AXIOM_RT_THREADING_C: &str = include_str!("../runtime/axiom_rt_threading.c");
const AXIOM_RT_STRINGS_C: &str = include_str!("../runtime/axiom_rt_strings.c");
const AXIOM_RT_VEC_C: &str = include_str!("../runtime/axiom_rt_vec.c");


/// Write the embedded C runtime (and its split include files) to a temp
/// directory and return the path to the main `axiom_rt.c` file.
fn write_runtime_c() -> miette::Result<PathBuf> {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let tid = std::thread::current().id();
    let rt_dir = dir.join(format!("axiom_rt_{pid}_{tid:?}"));
    std::fs::create_dir_all(&rt_dir)
        .map_err(|e| miette::miette!("failed to create runtime dir {}: {}", rt_dir.display(), e))?;

    // Write all split files into the same directory so #include works.
    let files: &[(&str, &str)] = &[
        ("axiom_rt.c", AXIOM_RT_C),
        ("axiom_rt_core.c", AXIOM_RT_CORE_C),
        ("axiom_rt_io.c", AXIOM_RT_IO_C),
        ("axiom_rt_coroutines.c", AXIOM_RT_COROUTINES_C),
        ("axiom_rt_threading.c", AXIOM_RT_THREADING_C),
        ("axiom_rt_strings.c", AXIOM_RT_STRINGS_C),
        ("axiom_rt_vec.c", AXIOM_RT_VEC_C),
    ];
    for (name, content) in files {
        let path = rt_dir.join(name);
        std::fs::write(&path, content)
            .map_err(|e| miette::miette!("failed to write runtime file {}: {}", path.display(), e))?;
    }

    Ok(rt_dir.join("axiom_rt.c"))
}

/// Check whether a compiler is available by running `<name> --version`.
fn compiler_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Discover a suitable compiler. Tries `clang`, then versioned variants, then
/// `gcc`/`cc` (the latter two can also handle `.ll` files when they are
/// actually clang symlinks, but we try clang first for clarity).
fn find_compiler() -> miette::Result<CompilerKind> {
    let candidates = [
        "clang",
        "clang-19",
        "clang-18",
        "clang-17",
        "clang-16",
        "clang-15",
        "gcc",
        "cc",
    ];
    for name in &candidates {
        if compiler_exists(name) {
            return Ok(CompilerKind::Clang(PathBuf::from(name)));
        }
    }
    Err(miette::miette!(
        "no LLVM/C compiler found on PATH\n\n{}",
        install_instructions()
    ))
}

/// Invoke clang (or compatible compiler) to compile a `.ll` file to a native
/// binary, optionally linking the C runtime alongside.
fn invoke_clang(
    clang: &Path,
    ll_file: &Path,
    output: &str,
    runtime_c: Option<&Path>,
    options: &CompileOptions,
) -> miette::Result<()> {
    // First try linking with mimalloc for faster heap allocation.
    // If mimalloc is not installed, fall back to system malloc.
    if try_invoke_clang_with_mimalloc(clang, ll_file, output, runtime_c, options) {
        return Ok(());
    }
    invoke_clang_core(clang, ll_file, output, runtime_c, &[], options)
}

/// Try to compile with `-lmimalloc`. Returns `true` on success.
fn try_invoke_clang_with_mimalloc(
    clang: &Path,
    ll_file: &Path,
    output: &str,
    runtime_c: Option<&Path>,
    options: &CompileOptions,
) -> bool {
    invoke_clang_core(clang, ll_file, output, runtime_c, &["-lmimalloc"], options).is_ok()
}

/// Resolve the `-O` flag from the `optimize_for` constraint.
///
/// Maps well-known constraint values to clang optimization flags:
/// - `"performance"` -> `-O3`
/// - `"memory"` -> `-Os`
/// - `"size"` -> `-Oz`
/// - `"latency"` -> `-O3` (with `-fno-exceptions` added separately)
/// - anything else / `None` -> `-O2`
fn resolve_opt_level(optimize_for: Option<&str>) -> &'static str {
    match optimize_for {
        Some("performance") => "-O3",
        Some("memory") => "-Os",
        Some("size") => "-Oz",
        Some("latency") => "-O3",
        _ => "-O2",
    }
}

/// Core clang invocation with optional extra linker flags.
fn invoke_clang_core(
    clang: &Path,
    ll_file: &Path,
    output: &str,
    runtime_c: Option<&Path>,
    extra_args: &[&str],
    options: &CompileOptions,
) -> miette::Result<()> {
    let opt_level = resolve_opt_level(options.optimize_for.as_deref());

    let mut cmd = Command::new(clang);
    cmd.arg(opt_level)
        .arg("-Wno-override-module")
        .arg(ll_file);

    // P1: Pass -march flag for target CPU feature selection.
    // When no target arch is specified, use -march=native to auto-detect host
    // CPU features (AVX2, AVX-512, etc.). When a specific arch is given (e.g.
    // via `axiom compile --target=x86-64-v4`), use that verbatim.
    let march = match &options.target_arch {
        Some(arch) => format!("-march={arch}"),
        None => "-march=native".to_string(),
    };
    cmd.arg(&march);

    // P4: For "latency" constraint, also add -fno-exceptions to minimize overhead.
    if options.optimize_for.as_deref() == Some("latency") {
        cmd.arg("-fno-exceptions");
    }

    // Pass sanitizer flags if requested.
    if let Some(ref san) = options.sanitize {
        cmd.arg(format!("-fsanitize={san}"));
        cmd.arg("-g");  // Sanitizers need debug info for useful output.
    }

    // Debug mode: enable crash handler in C runtime, debug symbols, and
    // reliable stack walking.
    if options.debug_mode {
        cmd.arg("-DAXIOM_DEBUG_MODE");
        cmd.arg("-g");
        cmd.arg("-fno-omit-frame-pointer");
        #[cfg(target_os = "windows")]
        cmd.arg("-ldbghelp");
    }

    // Link the C runtime if needed.
    if let Some(rt) = runtime_c {
        cmd.arg(rt);
        #[cfg(not(target_os = "windows"))]
        cmd.arg("-lpthread");
        #[cfg(target_os = "windows")]
        {
            cmd.arg("-lgdi32");
            cmd.arg("-luser32");
        }
    }

    // Scan IR for @axiom_link comments and add -l flags for each library.
    if let Some(ref ir_text) = options.ir_text {
        let mut link_dirs_added = std::collections::HashSet::new();
        for line in ir_text.lines() {
            if let Some(rest) = line.strip_prefix("; @axiom_link: ") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if let Some(lib_name) = parts.first() {
                    cmd.arg(format!("-l{lib_name}"));
                }
            }
        }

        // Auto-discover library search paths:
        // 1. Directory of the compiler binary (target/release/)
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let dir = exe_dir.to_string_lossy().to_string();
                if link_dirs_added.insert(dir.clone()) {
                    cmd.arg(format!("-L{dir}"));
                }
            }
        }
        // 2. target/release/ relative to cwd (common for development)
        for search_dir in &["target/release", "target/debug", "."] {
            let path = std::path::Path::new(search_dir);
            if path.is_dir() {
                let dir = path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
                    .to_string_lossy().to_string();
                if link_dirs_added.insert(dir.clone()) {
                    cmd.arg(format!("-L{dir}"));
                }
            }
        }
        // 3. Directory of the source file being compiled
        if let Some(source_dir) = std::path::Path::new(output).parent() {
            if source_dir.is_dir() {
                let dir = source_dir.to_string_lossy().to_string();
                if link_dirs_added.insert(dir.clone()) {
                    cmd.arg(format!("-L{dir}"));
                }
            }
        }
    }

    // Add user-specified library search paths
    for dir in &options.link_dirs {
        cmd.arg(format!("-L{dir}"));
    }

    // OPT-REPORT: pass -fsave-optimization-record to emit a .opt.yaml file
    // containing LLVM optimization remarks (applied, missed, analysis).
    // Use -foptimization-record-file= to place the yaml next to the output binary.
    if options.opt_report {
        cmd.arg("-fsave-optimization-record");
        let yaml_path = format!("{}.opt.yaml", output.trim_end_matches(".exe"));
        cmd.arg(format!("-foptimization-record-file={yaml_path}"));
    }

    cmd.arg("-o")
        .arg(output);

    for arg in extra_args {
        cmd.arg(arg);
    }

    // AXIOM uses stack-allocated arrays (alloca). Large arrays need a bigger
    // stack than the default 1MB on Windows. Request 64MB.
    #[cfg(target_os = "windows")]
    cmd.arg("-Wl,/STACK:67108864");

    let child = cmd
        .output()
        .map_err(|e| miette::miette!("failed to run {}: {}", clang.display(), e))?;

    if !child.status.success() {
        let stderr = String::from_utf8_lossy(&child.stderr);
        return Err(miette::miette!(
            "{} exited with {}:\n{}",
            clang.display(),
            child.status,
            stderr
        ));
    }
    Ok(())
}

/// Generate a unique temp file path for the LLVM IR.
fn temp_ll_path() -> PathBuf {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    // Include thread id for additional uniqueness when tests run in parallel.
    let tid = std::thread::current().id();
    dir.join(format!("axiom_{pid}_{tid:?}.ll"))
}

/// Return platform-specific instructions for installing clang.
fn install_instructions() -> &'static str {
    if cfg!(target_os = "windows") {
        "Install LLVM/clang:\n\
         \x20 winget install LLVM.LLVM\n\
         \x20 Or download from https://github.com/llvm/llvm-project/releases\n\n\
         After installing, ensure clang.exe is on your PATH."
    } else if cfg!(target_os = "macos") {
        "Install clang:\n\
         \x20 xcode-select --install\n\
         \x20 Or: brew install llvm"
    } else {
        "Install clang:\n\
         \x20 Ubuntu/Debian: sudo apt install clang\n\
         \x20 Fedora: sudo dnf install clang\n\
         \x20 Arch: sudo pacman -S clang"
    }
}

// ---------------------------------------------------------------------------
// L4: PGO (Profile-Guided Optimization) Bootstrap
// ---------------------------------------------------------------------------

/// Compile an AXIOM program with Profile-Guided Optimization.
///
/// Steps:
/// 1. Compile with `-fprofile-generate` to instrument the binary
/// 2. Run the instrumented binary with `training_args` to generate profile data
/// 3. Merge profile data (llvm-profdata if available)
/// 4. Recompile with `-fprofile-use` for optimized output
///
/// Returns (output_path, speedup_message).
pub fn compile_with_pgo(
    llvm_ir: &str,
    output_path: &str,
    training_args: &[String],
) -> miette::Result<String> {
    let compiler = find_compiler()?;
    let clang_path = match &compiler {
        CompilerKind::Clang(p) => p.clone(),
    };

    let temp_ll = temp_ll_path();
    std::fs::write(&temp_ll, llvm_ir)
        .map_err(|e| miette::miette!("failed to write temp file {}: {}", temp_ll.display(), e))?;

    let needs_rt = axiom_codegen::needs_runtime(llvm_ir);
    let rt_path = if needs_rt {
        Some(write_runtime_c()?)
    } else {
        None
    };

    let prof_dir = std::env::temp_dir().join(format!("axiom_pgo_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&prof_dir);

    let instrumented_bin = if cfg!(windows) {
        prof_dir.join("instrumented.exe")
    } else {
        prof_dir.join("instrumented")
    };

    // Step 1: Compile with -fprofile-generate
    eprintln!("[PGO] Step 1: Compiling with instrumentation...");
    let mut cmd1 = Command::new(&clang_path);
    cmd1.arg("-O2")
        .arg("-Wno-override-module")
        .arg("-march=native")
        .arg(&format!("-fprofile-generate={}", prof_dir.display()))
        .arg(&temp_ll);
    if let Some(ref rt) = rt_path {
        cmd1.arg(rt);
        #[cfg(not(target_os = "windows"))]
        cmd1.arg("-lpthread");
        #[cfg(target_os = "windows")]
        {
            cmd1.arg("-lgdi32");
            cmd1.arg("-luser32");
        }
    }
    cmd1.arg("-o").arg(&instrumented_bin);
    #[cfg(target_os = "windows")]
    cmd1.arg("-Wl,/STACK:67108864");

    let out1 = cmd1.output()
        .map_err(|e| miette::miette!("PGO step 1 failed to run: {e}"))?;
    if !out1.status.success() {
        let stderr = String::from_utf8_lossy(&out1.stderr);
        return Err(miette::miette!("PGO instrumentation build failed:\n{stderr}"));
    }

    // Step 2: Run instrumented binary to generate profile data
    eprintln!("[PGO] Step 2: Running instrumented binary for profiling...");
    let t_start = std::time::Instant::now();
    let mut cmd2 = Command::new(&instrumented_bin);
    for arg in training_args {
        cmd2.arg(arg);
    }
    let out2 = cmd2.output()
        .map_err(|e| miette::miette!("PGO training run failed: {e}"))?;
    let training_time = t_start.elapsed();
    if !out2.status.success() {
        eprintln!("[PGO] Warning: training run exited with {}", out2.status);
    }
    eprintln!("[PGO] Training run completed in {:.3}s", training_time.as_secs_f64());

    // Step 3: Merge profile data if llvm-profdata is available
    let prof_data = prof_dir.join("default.profdata");
    // Look for .profraw files
    let profraw_files: Vec<_> = std::fs::read_dir(&prof_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "profraw"))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();

    if !profraw_files.is_empty() {
        // Try llvm-profdata merge
        let profdata_tool = find_profdata_tool();
        if let Some(tool) = profdata_tool {
            eprintln!("[PGO] Step 2.5: Merging profile data with {tool}...");
            let mut merge_cmd = Command::new(&tool);
            merge_cmd.arg("merge").arg("-output").arg(&prof_data);
            for pf in &profraw_files {
                merge_cmd.arg(pf);
            }
            let _ = merge_cmd.output(); // Best-effort
        }
    }

    // Step 4: Recompile with profile data
    eprintln!("[PGO] Step 3: Recompiling with profile data...");
    let t_opt_start = std::time::Instant::now();
    let mut cmd3 = Command::new(&clang_path);
    cmd3.arg("-O2")
        .arg("-Wno-override-module")
        .arg("-march=native");

    // Use -fprofile-use if merged profdata exists, otherwise -fprofile-use=dir
    if prof_data.exists() {
        cmd3.arg(&format!("-fprofile-use={}", prof_data.display()));
    } else {
        cmd3.arg(&format!("-fprofile-use={}", prof_dir.display()));
    }

    cmd3.arg(&temp_ll);
    if let Some(ref rt) = rt_path {
        cmd3.arg(rt);
        #[cfg(not(target_os = "windows"))]
        cmd3.arg("-lpthread");
        #[cfg(target_os = "windows")]
        {
            cmd3.arg("-lgdi32");
            cmd3.arg("-luser32");
        }
    }
    cmd3.arg("-o").arg(output_path);
    #[cfg(target_os = "windows")]
    cmd3.arg("-Wl,/STACK:67108864");

    let out3 = cmd3.output()
        .map_err(|e| miette::miette!("PGO optimized build failed to run: {e}"))?;
    let opt_time = t_opt_start.elapsed();

    if !out3.status.success() {
        let stderr = String::from_utf8_lossy(&out3.stderr);
        return Err(miette::miette!("PGO optimized build failed:\n{stderr}"));
    }

    // Clean up
    let _ = std::fs::remove_file(&temp_ll);
    if let Some(ref rp) = rt_path {
        let _ = std::fs::remove_file(rp);
    }
    let _ = std::fs::remove_dir_all(&prof_dir);

    let message = format!(
        "[PGO] Complete. Training: {:.3}s, Optimized build: {:.3}s\n\
         [PGO] Output: {output_path}",
        training_time.as_secs_f64(),
        opt_time.as_secs_f64(),
    );
    Ok(message)
}

/// Try to find llvm-profdata on PATH.
fn find_profdata_tool() -> Option<String> {
    let candidates = [
        "llvm-profdata",
        "llvm-profdata-19",
        "llvm-profdata-18",
        "llvm-profdata-17",
    ];
    for name in &candidates {
        if compiler_exists(name) {
            return Some((*name).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temp_ll_path_has_ll_extension() {
        let p = temp_ll_path();
        assert_eq!(p.extension().and_then(|e| e.to_str()), Some("ll"));
    }

    #[test]
    fn test_temp_ll_path_is_in_temp_dir() {
        let p = temp_ll_path();
        let temp = std::env::temp_dir();
        assert!(p.starts_with(&temp));
    }

    #[test]
    fn test_install_instructions_not_empty() {
        let msg = install_instructions();
        assert!(!msg.is_empty());
        // On any platform the message should mention "clang".
        assert!(msg.contains("clang") || msg.contains("LLVM"));
    }

    #[test]
    fn test_find_compiler_name_returns_option() {
        // We cannot guarantee a compiler is present in CI, but the function
        // should not panic regardless.
        let _result = find_compiler_name();
    }

    #[test]
    fn test_needs_runtime_detection() {
        // IR without runtime functions should not need the runtime.
        let ir_no_rt = "declare i32 @puts(ptr)\ndefine i32 @main() {\n  ret i32 0\n}\n";
        assert!(!axiom_codegen::needs_runtime(ir_no_rt));

        // IR with a runtime function declaration should need the runtime.
        let ir_with_rt = "declare i64 @axiom_clock_ns()\ndefine i32 @main() {\n  ret i32 0\n}\n";
        assert!(axiom_codegen::needs_runtime(ir_with_rt));

        // IR with coroutine declarations should also need the runtime.
        let ir_with_coro = "declare i32 @axiom_coro_create(ptr, i32)\ndefine i32 @main() {\n  ret i32 0\n}\n";
        assert!(axiom_codegen::needs_runtime(ir_with_coro));
    }

    #[test]
    fn test_runtime_c_is_embedded() {
        // The embedded C runtime should contain our function names.
        assert!(AXIOM_RT_C.contains("axiom_file_read"));
        assert!(AXIOM_RT_C.contains("axiom_file_write"));
        assert!(AXIOM_RT_C.contains("axiom_file_size"));
        assert!(AXIOM_RT_C.contains("axiom_clock_ns"));
        assert!(AXIOM_RT_C.contains("axiom_get_argc"));
        assert!(AXIOM_RT_C.contains("axiom_get_argv"));
        // Coroutine functions should also be present.
        assert!(AXIOM_RT_C.contains("axiom_coro_create"));
        assert!(AXIOM_RT_C.contains("axiom_coro_resume"));
        assert!(AXIOM_RT_C.contains("axiom_coro_yield"));
        assert!(AXIOM_RT_C.contains("axiom_coro_is_done"));
        assert!(AXIOM_RT_C.contains("axiom_coro_destroy"));
    }

    #[test]
    fn test_resolve_opt_level_default() {
        assert_eq!(resolve_opt_level(None), "-O2");
    }

    #[test]
    fn test_resolve_opt_level_performance() {
        assert_eq!(resolve_opt_level(Some("performance")), "-O3");
    }

    #[test]
    fn test_resolve_opt_level_memory() {
        assert_eq!(resolve_opt_level(Some("memory")), "-Os");
    }

    #[test]
    fn test_resolve_opt_level_size() {
        assert_eq!(resolve_opt_level(Some("size")), "-Oz");
    }

    #[test]
    fn test_resolve_opt_level_latency() {
        assert_eq!(resolve_opt_level(Some("latency")), "-O3");
    }

    #[test]
    fn test_resolve_opt_level_unknown() {
        assert_eq!(resolve_opt_level(Some("unknown")), "-O2");
    }

    #[test]
    fn test_compile_options_default() {
        let opts = CompileOptions::default();
        assert!(opts.target_arch.is_none());
        assert!(opts.optimize_for.is_none());
    }
}
