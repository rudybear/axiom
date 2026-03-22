//! End-to-end integration tests for the AXIOM compiler.
//!
//! Tests that require a C/LLVM compiler (clang) check for its availability at
//! runtime and skip gracefully when none is found.

use std::process::Command;

/// Return the path to the `axiom` binary built by cargo.
fn axiom_bin() -> std::path::PathBuf {
    // `cargo test` puts the test binary in target/debug/deps, and the built
    // binaries live in target/debug.
    let mut path = std::env::current_exe()
        .expect("cannot determine test binary path");
    // Go up from target/debug/deps/<test-binary> to target/debug/
    path.pop(); // remove test binary name
    path.pop(); // remove deps/
    path.push(if cfg!(windows) { "axiom.exe" } else { "axiom" });
    path
}

/// Check whether a C/LLVM compiler is available on PATH.
fn has_compiler() -> bool {
    let candidates = ["clang", "clang-19", "clang-18", "clang-17", "clang-16", "clang-15", "gcc", "cc"];
    for name in &candidates {
        let ok = Command::new(name)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return true;
        }
    }
    false
}

/// Path to a sample .axm file relative to the workspace root.
fn sample_path(name: &str) -> String {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // workspace root
    p.push("tests");
    p.push("samples");
    p.push(name);
    p.to_string_lossy().into_owned()
}

// ---- Emit-stage tests (no compiler required) ----

#[test]
fn emit_tokens_hello() {
    let output = Command::new(axiom_bin())
        .args(["compile", "--emit=tokens", &sample_path("hello.axm")])
        .output()
        .expect("failed to run axiom");
    assert!(output.status.success(), "axiom exited with failure");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Token output should contain the `Fn` keyword token.
    assert!(stdout.contains("Fn"), "expected Fn token in output:\n{stdout}");
}

#[test]
fn emit_ast_hello() {
    let output = Command::new(axiom_bin())
        .args(["compile", "--emit=ast", &sample_path("hello.axm")])
        .output()
        .expect("failed to run axiom");
    assert!(output.status.success(), "axiom exited with failure");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Module"),
        "expected Module in AST output:\n{stdout}"
    );
}

#[test]
fn emit_hir_hello() {
    let output = Command::new(axiom_bin())
        .args(["compile", "--emit=hir", &sample_path("hello.axm")])
        .output()
        .expect("failed to run axiom");
    assert!(output.status.success(), "axiom exited with failure");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("fn main"),
        "expected 'fn main' in HIR output:\n{stdout}"
    );
}

#[test]
fn emit_llvm_ir_hello() {
    let output = Command::new(axiom_bin())
        .args(["compile", "--emit=llvm-ir", &sample_path("hello.axm")])
        .output()
        .expect("failed to run axiom");
    assert!(output.status.success(), "axiom exited with failure");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("define"),
        "expected 'define' in LLVM IR output:\n{stdout}"
    );
}

#[test]
fn emit_llvm_ir_fibonacci() {
    let output = Command::new(axiom_bin())
        .args(["compile", "--emit=llvm-ir", &sample_path("fibonacci.axm")])
        .output()
        .expect("failed to run axiom");
    assert!(output.status.success(), "axiom exited with failure");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("define"),
        "expected 'define' in LLVM IR output:\n{stdout}"
    );
}

#[test]
fn missing_input_file_error() {
    let output = Command::new(axiom_bin())
        .args(["compile", "nonexistent_file_that_does_not_exist.axm"])
        .output()
        .expect("failed to run axiom");
    assert!(!output.status.success(), "expected failure for missing file");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Failed to read"),
        "expected 'Failed to read' in error output:\n{stderr}"
    );
}

// ---- E2E compilation tests (require clang) ----

#[test]
fn e2e_hello() {
    if !has_compiler() {
        eprintln!("Skipping e2e_hello: no C/LLVM compiler found on PATH");
        return;
    }

    let temp_dir = std::env::temp_dir();
    let output_name = if cfg!(windows) { "axiom_test_hello.exe" } else { "axiom_test_hello" };
    let output_path = temp_dir.join(output_name);
    let output_str = output_path.to_string_lossy().into_owned();

    // Compile
    let compile = Command::new(axiom_bin())
        .args(["compile", &sample_path("hello.axm"), "-o", &output_str])
        .output()
        .expect("failed to run axiom compile");
    assert!(
        compile.status.success(),
        "axiom compile failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
    );

    // Run the compiled binary
    let run = Command::new(&output_path)
        .output()
        .expect("failed to run compiled hello binary");
    assert!(run.status.success(), "hello binary exited with failure");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("Hello from AXIOM!"),
        "expected 'Hello from AXIOM!' in output:\n{stdout}"
    );

    // Clean up
    let _ = std::fs::remove_file(&output_path);
}

#[test]
fn e2e_fibonacci() {
    if !has_compiler() {
        eprintln!("Skipping e2e_fibonacci: no C/LLVM compiler found on PATH");
        return;
    }

    let temp_dir = std::env::temp_dir();
    let output_name = if cfg!(windows) { "axiom_test_fib.exe" } else { "axiom_test_fib" };
    let output_path = temp_dir.join(output_name);
    let output_str = output_path.to_string_lossy().into_owned();

    // Compile
    let compile = Command::new(axiom_bin())
        .args(["compile", &sample_path("fibonacci.axm"), "-o", &output_str])
        .output()
        .expect("failed to run axiom compile");
    assert!(
        compile.status.success(),
        "axiom compile failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
    );

    // Run the compiled binary
    let run = Command::new(&output_path)
        .output()
        .expect("failed to run compiled fibonacci binary");
    assert!(run.status.success(), "fibonacci binary exited with failure");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("102334155"),
        "expected '102334155' in fibonacci output:\n{stdout}"
    );

    // Clean up
    let _ = std::fs::remove_file(&output_path);
}
