//! Native binary compilation -- invokes an external C/LLVM compiler on LLVM IR text.
//!
//! This module encapsulates compiler discovery and invocation. It writes LLVM IR
//! to a temporary `.ll` file and invokes `clang` (or a versioned variant) to
//! produce a native executable.

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

    let result = match &compiler {
        CompilerKind::Clang(path) => invoke_clang(path, &temp_ll, output_path),
    };

    // Clean up temp file on success; leave it on failure for debugging.
    if result.is_ok() {
        let _ = std::fs::remove_file(&temp_ll);
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
/// binary.
fn invoke_clang(clang: &Path, ll_file: &Path, output: &str) -> miette::Result<()> {
    let child = Command::new(clang)
        .arg("-O2")
        .arg("-Wno-override-module")
        .arg(ll_file)
        .arg("-o")
        .arg(output)
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
}
