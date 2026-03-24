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

/// Compile LLVM IR text to a native executable binary.
///
/// Writes the IR to a temporary file, invokes the discovered compiler, and
/// cleans up the temp file on success. On failure the temp file is left in
/// place for debugging and its path is printed to stderr.
pub fn compile_to_binary(llvm_ir: &str, output_path: &str) -> miette::Result<()> {
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
        CompilerKind::Clang(path) => invoke_clang(path, &temp_ll, output_path, rt_path.as_deref()),
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

/// The C runtime source, embedded at compile time.
const AXIOM_RT_C: &str = include_str!("../runtime/axiom_rt.c");

/// Write the embedded C runtime to a temp file and return its path.
fn write_runtime_c() -> miette::Result<PathBuf> {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let tid = std::thread::current().id();
    let path = dir.join(format!("axiom_rt_{pid}_{tid:?}.c"));
    std::fs::write(&path, AXIOM_RT_C)
        .map_err(|e| miette::miette!("failed to write runtime C file {}: {}", path.display(), e))?;
    Ok(path)
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
) -> miette::Result<()> {
    // First try linking with mimalloc for faster heap allocation.
    // If mimalloc is not installed, fall back to system malloc.
    if try_invoke_clang_with_mimalloc(clang, ll_file, output, runtime_c) {
        return Ok(());
    }
    invoke_clang_core(clang, ll_file, output, runtime_c, &[])
}

/// Try to compile with `-lmimalloc`. Returns `true` on success.
fn try_invoke_clang_with_mimalloc(
    clang: &Path,
    ll_file: &Path,
    output: &str,
    runtime_c: Option<&Path>,
) -> bool {
    invoke_clang_core(clang, ll_file, output, runtime_c, &["-lmimalloc"]).is_ok()
}

/// Core clang invocation with optional extra linker flags.
fn invoke_clang_core(
    clang: &Path,
    ll_file: &Path,
    output: &str,
    runtime_c: Option<&Path>,
    extra_args: &[&str],
) -> miette::Result<()> {
    let mut cmd = Command::new(clang);
    cmd.arg("-O2")
        .arg("-Wno-override-module")
        .arg(ll_file);

    // Link the C runtime if needed.
    if let Some(rt) = runtime_c {
        cmd.arg(rt);
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
    }
}
