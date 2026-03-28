//! Built-in benchmarking harness for AXIOM programs.
//!
//! This module provides a harness that runs a compiled AXIOM binary multiple
//! times, collecting timing metrics (median, mean, min, max, stddev).
//!
//! Two entry points are provided:
//!
//! - [`benchmark_binary`] — benchmark a pre-compiled binary (no compiler needed).
//! - [`benchmark_source`] — compile an AXIOM source file and then benchmark the
//!   resulting binary (requires clang on `PATH`).
//!
//! # Example
//!
//! ```no_run
//! use axiom_optimize::benchmark::{BenchmarkConfig, benchmark_binary};
//!
//! let config = BenchmarkConfig::default();
//! let result = benchmark_binary("./a.out", &config).expect("benchmark succeeds");
//! println!("median: {:.3} ms", result.median_ms);
//! ```

use std::fmt;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Configuration for a benchmark run.
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Number of warmup runs (not measured). Default: 3.
    pub warmup_runs: usize,
    /// Number of measurement runs (timed). Default: 5.
    pub measurement_runs: usize,
    /// Per-run timeout in milliseconds. Default: 30 000 ms (30 s).
    pub timeout_ms: u64,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            warmup_runs: 3,
            measurement_runs: 5,
            timeout_ms: 30_000,
        }
    }
}

/// Results of a benchmark run.
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    /// Individual measurement times in milliseconds.
    pub times_ms: Vec<f64>,
    /// Median of the measurement times.
    pub median_ms: f64,
    /// Arithmetic mean of the measurement times.
    pub mean_ms: f64,
    /// Minimum measurement time.
    pub min_ms: f64,
    /// Maximum measurement time.
    pub max_ms: f64,
    /// Sample standard deviation of the measurement times.
    pub stddev_ms: f64,
}

impl fmt::Display for BenchmarkResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Benchmark results ({} runs):", self.times_ms.len())?;
        writeln!(f, "  median:  {:.3} ms", self.median_ms)?;
        writeln!(f, "  mean:    {:.3} ms", self.mean_ms)?;
        writeln!(f, "  min:     {:.3} ms", self.min_ms)?;
        writeln!(f, "  max:     {:.3} ms", self.max_ms)?;
        write!(f, "  stddev:  {:.3} ms", self.stddev_ms)
    }
}

/// Errors that can occur during benchmarking.
#[derive(Debug, thiserror::Error)]
pub enum BenchmarkError {
    /// The source could not be compiled.
    #[error("compilation failed: {0}")]
    CompilationFailed(String),

    /// The binary was not found at the given path.
    #[error("binary not found: {0}")]
    BinaryNotFound(String),

    /// A run of the binary failed (non-zero exit code or could not be spawned).
    #[error("run failed: {0}")]
    RunFailed(String),

    /// A run exceeded the configured timeout.
    #[error("run timed out after {0} ms")]
    Timeout(u64),

    /// No measurement runs were requested.
    #[error("measurement_runs must be at least 1")]
    NoMeasurementRuns,

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Benchmark an already-compiled binary.
///
/// The binary is run `warmup_runs + measurement_runs` times.  The first
/// `warmup_runs` invocations are not timed.  The remaining invocations are
/// timed and the aggregate statistics are returned.
///
/// # Errors
///
/// Returns [`BenchmarkError`] if the binary cannot be found, any run fails, or
/// a run exceeds the configured timeout.
pub fn benchmark_binary(
    binary_path: &str,
    config: &BenchmarkConfig,
) -> Result<BenchmarkResult, BenchmarkError> {
    if config.measurement_runs == 0 {
        return Err(BenchmarkError::NoMeasurementRuns);
    }

    let path = Path::new(binary_path);
    if !path.exists() {
        return Err(BenchmarkError::BinaryNotFound(binary_path.to_string()));
    }

    // Canonicalize so relative paths work with Command::new.
    let canonical = path
        .canonicalize()
        .map_err(|e| BenchmarkError::BinaryNotFound(format!("{binary_path}: {e}")))?;

    // --- warmup ---
    for i in 0..config.warmup_runs {
        run_once(&canonical, config.timeout_ms)
            .map_err(|e| BenchmarkError::RunFailed(format!("warmup run {}: {e}", i + 1)))?;
    }

    // --- measurement ---
    let mut times_ms = Vec::with_capacity(config.measurement_runs);
    for i in 0..config.measurement_runs {
        let start = Instant::now();
        run_once(&canonical, config.timeout_ms)
            .map_err(|e| BenchmarkError::RunFailed(format!("measurement run {}: {e}", i + 1)))?;
        let elapsed = start.elapsed();
        times_ms.push(elapsed.as_secs_f64() * 1000.0);
    }

    Ok(compute_stats(times_ms))
}

/// Compile an AXIOM source string and benchmark the resulting binary.
///
/// This function goes through the full AXIOM pipeline (parse -> HIR ->
/// codegen -> clang) to produce a temporary binary and then benchmarks it.
/// The temporary binary is cleaned up after the benchmark completes.
///
/// # Errors
///
/// Returns [`BenchmarkError`] if compilation fails (including missing clang),
/// or if any benchmark run fails.
pub fn benchmark_source(
    source: &str,
    config: &BenchmarkConfig,
) -> Result<BenchmarkResult, BenchmarkError> {
    if config.measurement_runs == 0 {
        return Err(BenchmarkError::NoMeasurementRuns);
    }

    // 1. Parse
    let parse_result = axiom_parser::parse(source);
    if parse_result.has_errors() {
        let msgs: Vec<String> = parse_result.errors.iter().map(|e| format!("{e}")).collect();
        return Err(BenchmarkError::CompilationFailed(format!(
            "parse errors: {}",
            msgs.join("; ")
        )));
    }

    // 2. Lower to HIR
    let hir_module = axiom_hir::lower(&parse_result.module).map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
        BenchmarkError::CompilationFailed(format!("HIR lowering errors: {}", msgs.join("; ")))
    })?;

    // 3. Codegen to LLVM IR
    let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
        BenchmarkError::CompilationFailed(format!("codegen errors: {}", msgs.join("; ")))
    })?;

    // 4. Compile LLVM IR to a temporary binary via clang
    let temp_binary = temp_binary_path();
    compile_ir_to_binary(&llvm_ir, &temp_binary)?;

    // 5. Run the benchmark
    let result = benchmark_binary(
        temp_binary
            .to_str()
            .ok_or_else(|| BenchmarkError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "temp binary path is not valid UTF-8",
            )))?,
        config,
    );

    // Clean up temporary binary regardless of benchmark outcome.
    let _ = std::fs::remove_file(&temp_binary);

    result
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// Compute aggregate statistics from a vector of individual times.
fn compute_stats(mut times_ms: Vec<f64>) -> BenchmarkResult {
    // Sort for median computation.
    times_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = times_ms.len();
    let min_ms = times_ms[0];
    let max_ms = times_ms[n - 1];
    let mean_ms = times_ms.iter().sum::<f64>() / n as f64;

    let median_ms = if n.is_multiple_of(2) {
        (times_ms[n / 2 - 1] + times_ms[n / 2]) / 2.0
    } else {
        times_ms[n / 2]
    };

    let stddev_ms = if n > 1 {
        let variance =
            times_ms.iter().map(|t| (t - mean_ms).powi(2)).sum::<f64>() / (n - 1) as f64;
        variance.sqrt()
    } else {
        0.0
    };

    BenchmarkResult {
        times_ms,
        median_ms,
        mean_ms,
        min_ms,
        max_ms,
        stddev_ms,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Run a binary once, enforcing a timeout.
///
/// Spawns the binary in a background thread and polls for completion,
/// killing the process if it exceeds `timeout_ms`.
fn run_once(binary: &Path, timeout_ms: u64) -> Result<(), BenchmarkError> {
    let mut child = Command::new(binary)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| BenchmarkError::RunFailed(format!("failed to spawn: {e}")))?;

    let deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let poll_interval = std::time::Duration::from_millis(10);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(BenchmarkError::RunFailed(format!(
                        "process exited with {status}"
                    )))
                };
            }
            Ok(None) => {
                // Still running — check timeout.
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(BenchmarkError::Timeout(timeout_ms));
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                return Err(BenchmarkError::RunFailed(format!("wait failed: {e}")));
            }
        }
    }
}

/// Generate a unique temporary path for a compiled binary.
fn temp_binary_path() -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let tid = std::thread::current().id();
    let ext = if cfg!(windows) { ".exe" } else { "" };
    dir.join(format!("axiom_bench_{pid}_{tid:?}{ext}"))
}

/// Compile LLVM IR text to a binary using clang.
fn compile_ir_to_binary(
    llvm_ir: &str,
    output: &std::path::Path,
) -> Result<(), BenchmarkError> {
    // Write IR to a temp .ll file.
    let ll_path = {
        let dir = std::env::temp_dir();
        let pid = std::process::id();
        let tid = std::thread::current().id();
        dir.join(format!("axiom_bench_{pid}_{tid:?}.ll"))
    };

    std::fs::write(&ll_path, llvm_ir)?;

    let clang = find_clang().ok_or_else(|| {
        // Clean up the .ll file before returning.
        let _ = std::fs::remove_file(&ll_path);
        BenchmarkError::CompilationFailed(
            "no clang found on PATH — install clang to benchmark from source".to_string(),
        )
    })?;

    // Check if the IR needs the AXIOM C runtime (contains @axiom_* calls).
    let needs_rt = axiom_codegen::needs_runtime(llvm_ir);

    let mut cmd = Command::new(&clang);
    cmd.arg("-O2")
        .arg("-Wno-override-module")
        .arg("-march=native")
        .arg(&ll_path);

    // Link the C runtime if needed.
    if needs_rt {
        let rt_dir = std::env::temp_dir().join(format!("axiom_bench_rt_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&rt_dir);
        let rt_path = rt_dir.join("axiom_rt.c");
        // Write the embedded runtime files
        std::fs::write(&rt_path, include_str!("../../axiom-driver/runtime/axiom_rt.c")).ok();
        for (name, content) in &[
            ("axiom_rt_core.c", include_str!("../../axiom-driver/runtime/axiom_rt_core.c")),
            ("axiom_rt_io.c", include_str!("../../axiom-driver/runtime/axiom_rt_io.c")),
            ("axiom_rt_coroutines.c", include_str!("../../axiom-driver/runtime/axiom_rt_coroutines.c")),
            ("axiom_rt_threading.c", include_str!("../../axiom-driver/runtime/axiom_rt_threading.c")),
            ("axiom_rt_strings.c", include_str!("../../axiom-driver/runtime/axiom_rt_strings.c")),
            ("axiom_rt_vec.c", include_str!("../../axiom-driver/runtime/axiom_rt_vec.c")),
            ("axiom_rt_trace.c", include_str!("../../axiom-driver/runtime/axiom_rt_trace.c")),
        ] {
            std::fs::write(rt_dir.join(name), content).ok();
        }
        cmd.arg(&rt_path);
        #[cfg(not(target_os = "windows"))]
        cmd.arg("-lpthread");
        #[cfg(target_os = "windows")]
        { cmd.arg("-lgdi32").arg("-luser32"); }
    }

    cmd.arg("-o").arg(output);

    // Stack size for large allocas
    #[cfg(target_os = "windows")]
    cmd.arg("-Wl,/STACK:67108864");

    let child = cmd.output();

    // Clean up .ll file regardless.
    let _ = std::fs::remove_file(&ll_path);

    let child = child?;
    if !child.status.success() {
        let stderr = String::from_utf8_lossy(&child.stderr);
        return Err(BenchmarkError::CompilationFailed(format!(
            "clang exited with {}: {}",
            child.status, stderr
        )));
    }

    Ok(())
}

/// Find a clang binary on PATH.
fn find_clang() -> Option<String> {
    let candidates = [
        "clang",
        "clang-19",
        "clang-18",
        "clang-17",
        "clang-16",
        "clang-15",
    ];
    for name in &candidates {
        let ok = Command::new(name)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some((*name).to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- statistics unit tests --

    #[test]
    fn stats_single_value() {
        let result = compute_stats(vec![42.0]);
        assert!((result.median_ms - 42.0).abs() < f64::EPSILON);
        assert!((result.mean_ms - 42.0).abs() < f64::EPSILON);
        assert!((result.min_ms - 42.0).abs() < f64::EPSILON);
        assert!((result.max_ms - 42.0).abs() < f64::EPSILON);
        assert!((result.stddev_ms - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn stats_odd_count() {
        // Median of [1, 2, 3, 4, 5] = 3
        let result = compute_stats(vec![5.0, 1.0, 3.0, 4.0, 2.0]);
        assert!((result.median_ms - 3.0).abs() < f64::EPSILON);
        assert!((result.mean_ms - 3.0).abs() < f64::EPSILON);
        assert!((result.min_ms - 1.0).abs() < f64::EPSILON);
        assert!((result.max_ms - 5.0).abs() < f64::EPSILON);
        // stddev of [1,2,3,4,5] = sqrt(10/4) = sqrt(2.5)
        let expected_stddev = (2.5_f64).sqrt();
        assert!(
            (result.stddev_ms - expected_stddev).abs() < 1e-10,
            "expected stddev {expected_stddev}, got {}",
            result.stddev_ms
        );
    }

    #[test]
    fn stats_even_count() {
        // Median of [1, 2, 3, 4] = (2+3)/2 = 2.5
        let result = compute_stats(vec![4.0, 2.0, 1.0, 3.0]);
        assert!((result.median_ms - 2.5).abs() < f64::EPSILON);
        assert!((result.mean_ms - 2.5).abs() < f64::EPSILON);
        assert!((result.min_ms - 1.0).abs() < f64::EPSILON);
        assert!((result.max_ms - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn stats_identical_values() {
        let result = compute_stats(vec![7.0, 7.0, 7.0, 7.0]);
        assert!((result.median_ms - 7.0).abs() < f64::EPSILON);
        assert!((result.mean_ms - 7.0).abs() < f64::EPSILON);
        assert!((result.stddev_ms - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn stats_two_values() {
        let result = compute_stats(vec![10.0, 20.0]);
        assert!((result.median_ms - 15.0).abs() < f64::EPSILON);
        assert!((result.mean_ms - 15.0).abs() < f64::EPSILON);
        assert!((result.min_ms - 10.0).abs() < f64::EPSILON);
        assert!((result.max_ms - 20.0).abs() < f64::EPSILON);
        // sample stddev of [10, 20] = sqrt((25+25)/1) = sqrt(50)
        let expected_stddev = (50.0_f64).sqrt();
        assert!(
            (result.stddev_ms - expected_stddev).abs() < 1e-10,
            "expected stddev {expected_stddev}, got {}",
            result.stddev_ms
        );
    }

    #[test]
    fn stats_known_stddev() {
        // [2, 4, 4, 4, 5, 5, 7, 9] — classic example
        let result = compute_stats(vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
        // mean = 40/8 = 5
        assert!((result.mean_ms - 5.0).abs() < f64::EPSILON);
        // median of 8 values = (4+5)/2 = 4.5
        assert!((result.median_ms - 4.5).abs() < f64::EPSILON);
        // sample variance = sum of (xi-mean)^2 / (n-1)
        // = (9+1+1+1+0+0+4+16) / 7 = 32/7
        let expected_stddev = (32.0_f64 / 7.0).sqrt();
        assert!(
            (result.stddev_ms - expected_stddev).abs() < 1e-10,
            "expected stddev {expected_stddev}, got {}",
            result.stddev_ms
        );
    }

    // -- config defaults --

    #[test]
    fn default_config() {
        let config = BenchmarkConfig::default();
        assert_eq!(config.warmup_runs, 3);
        assert_eq!(config.measurement_runs, 5);
        assert_eq!(config.timeout_ms, 30_000);
    }

    // -- error cases --

    #[test]
    fn binary_not_found() {
        let config = BenchmarkConfig::default();
        let result = benchmark_binary("/nonexistent/path/to/binary", &config);
        assert!(matches!(result, Err(BenchmarkError::BinaryNotFound(_))));
    }

    #[test]
    fn zero_measurement_runs() {
        let config = BenchmarkConfig {
            warmup_runs: 0,
            measurement_runs: 0,
            timeout_ms: 1000,
        };
        let result = benchmark_binary("anything", &config);
        assert!(matches!(result, Err(BenchmarkError::NoMeasurementRuns)));
    }

    // -- display --

    #[test]
    fn result_display() {
        let result = compute_stats(vec![1.0, 2.0, 3.0]);
        let display = format!("{result}");
        assert!(display.contains("median:"));
        assert!(display.contains("mean:"));
        assert!(display.contains("min:"));
        assert!(display.contains("max:"));
        assert!(display.contains("stddev:"));
        assert!(display.contains("3 runs"));
    }

    // -- temp path --

    #[test]
    fn temp_binary_path_platform() {
        let p = temp_binary_path();
        if cfg!(windows) {
            assert!(p.to_str().map_or(false, |s| s.ends_with(".exe")));
        }
        assert!(p.starts_with(std::env::temp_dir()));
    }
}
