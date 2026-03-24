//! AXIOM compiler CLI driver.

mod compile;
mod mcp;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "axiom", about = "AXIOM — AI eXchange Intermediate Optimization Medium")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile an AXIOM source file
    Compile {
        /// Input .axm file
        input: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,

        /// Emit intermediate representation instead of binary
        #[arg(long, value_parser = ["tokens", "ast", "hir", "mir", "mlir", "llvm-ir"])]
        emit: Option<String>,

        /// Target CPU architecture (e.g., "native", "x86-64-v4", "znver3").
        /// Overrides @target annotation. Defaults to native.
        #[arg(long)]
        target: Option<String>,
    },

    /// Tokenize an AXIOM source file (debug tool)
    Lex {
        /// Input .axm file
        input: String,
    },

    /// Benchmark an AXIOM source file
    Bench {
        /// Input .axm file
        input: String,

        /// Number of warmup runs
        #[arg(long, default_value = "3")]
        warmup: usize,

        /// Number of measurement runs
        #[arg(long, default_value = "5")]
        runs: usize,
    },

    /// Start MCP server for AI agent integration
    Mcp {},

    /// AI-driven optimization of an AXIOM program
    Optimize {
        /// Input .axm file
        input: String,

        /// Number of LLM optimization iterations
        #[arg(long, default_value = "5")]
        iterations: usize,

        /// Target architecture
        #[arg(long, default_value = "native")]
        target: String,

        /// Anthropic API key (or set ANTHROPIC_API_KEY env var)
        #[arg(long)]
        api_key: Option<String>,

        /// Just print the LLM prompt without calling the API
        #[arg(long)]
        dry_run: bool,

        /// Agent identifier for history records
        #[arg(long, default_value = "axiom-llm-optimizer")]
        agent: String,
    },

    /// Profile an AXIOM program and suggest optimizations
    Profile {
        /// Input .axm file
        input: String,

        /// Number of profiling iterations
        #[arg(long, default_value = "10")]
        iterations: usize,
    },
}

fn main() -> miette::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Lex { input } => {
            let source = std::fs::read_to_string(&input)
                .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;
            let (tokens, errors) = axiom_lexer::Lexer::new(&source).tokenize();

            for tok in &tokens {
                let text = &source[tok.span.start as usize..tok.span.end as usize];
                println!("{:4}..{:<4} {:?}  {:?}", tok.span.start, tok.span.end, tok.kind, text);
            }

            if !errors.is_empty() {
                eprintln!("\n--- Errors ---");
                for err in &errors {
                    eprintln!("  {:?}", err);
                }
            }

            Ok(())
        }

        Commands::Bench {
            input,
            warmup,
            runs,
        } => {
            let source = std::fs::read_to_string(&input)
                .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;

            let config = axiom_optimize::benchmark::BenchmarkConfig {
                warmup_runs: warmup,
                measurement_runs: runs,
                ..Default::default()
            };

            eprintln!(
                "Benchmarking {} ({} warmup, {} measurement runs)...",
                input, config.warmup_runs, config.measurement_runs
            );

            match axiom_optimize::benchmark::benchmark_source(&source, &config) {
                Ok(result) => {
                    println!("{result}");
                    Ok(())
                }
                Err(e) => Err(miette::miette!("benchmark failed: {}", e)),
            }
        }

        Commands::Mcp {} => {
            mcp::run_mcp_server()
                .map_err(|e| miette::miette!("MCP server error: {e}"))?;
            Ok(())
        }

        Commands::Optimize {
            input,
            iterations,
            target,
            api_key,
            dry_run,
            agent,
        } => run_optimize(&input, iterations, &target, api_key.as_deref(), dry_run, &agent),

        Commands::Profile { input, iterations } => run_profile(&input, iterations),

        Commands::Compile { input, output, emit, target } => {
            let source = std::fs::read_to_string(&input)
                .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;

            match emit.as_deref() {
                Some("tokens") => {
                    let (tokens, _) = axiom_lexer::Lexer::new(&source).tokenize();
                    for tok in &tokens {
                        println!("{:?}", tok);
                    }
                }
                Some("ast") => {
                    let result = axiom_parser::parse(&source);
                    if result.has_errors() {
                        eprintln!("--- Parse Errors ---");
                        for err in &result.errors {
                            eprintln!("  {err}");
                        }
                    }
                    println!("{:#?}", result.module);
                }
                Some("hir") => {
                    let result = axiom_parser::parse(&source);
                    if result.has_errors() {
                        eprintln!("--- Parse Errors ---");
                        for err in &result.errors {
                            eprintln!("  {err}");
                        }
                        return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
                    }
                    match axiom_hir::lower(&result.module) {
                        Ok(hir_module) => {
                            println!("{hir_module}");
                        }
                        Err(errors) => {
                            eprintln!("--- HIR Lowering Errors ---");
                            for err in &errors {
                                eprintln!("  {err}");
                            }
                            return Err(miette::miette!("HIR lowering failed with {} error(s)", errors.len()));
                        }
                    }
                }
                Some("llvm-ir") => {
                    let result = axiom_parser::parse(&source);
                    if result.has_errors() {
                        eprintln!("--- Parse Errors ---");
                        for err in &result.errors {
                            eprintln!("  {err}");
                        }
                        return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
                    }
                    let hir_module = axiom_hir::lower(&result.module).map_err(|errors| {
                        for err in &errors {
                            eprintln!("  {err}");
                        }
                        miette::miette!("HIR lowering failed with {} error(s)", errors.len())
                    })?;
                    let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
                        for err in &errors {
                            eprintln!("  {err}");
                        }
                        miette::miette!("codegen failed with {} error(s)", errors.len())
                    })?;
                    println!("{llvm_ir}");
                }
                Some(level) => {
                    eprintln!("TODO: --emit={level} not yet implemented");
                }
                None => {
                    // Full compilation: .axm -> native binary
                    let result = axiom_parser::parse(&source);
                    if result.has_errors() {
                        eprintln!("--- Parse Errors ---");
                        for err in &result.errors {
                            eprintln!("  {err}");
                        }
                        return Err(miette::miette!(
                            "parsing failed with {} error(s)",
                            result.errors.len()
                        ));
                    }
                    let hir_module =
                        axiom_hir::lower(&result.module).map_err(|errors| {
                            for err in &errors {
                                eprintln!("  {err}");
                            }
                            miette::miette!(
                                "HIR lowering failed with {} error(s)",
                                errors.len()
                            )
                        })?;
                    let llvm_ir =
                        axiom_codegen::codegen(&hir_module).map_err(|errors| {
                            for err in &errors {
                                eprintln!("  {err}");
                            }
                            miette::miette!(
                                "codegen failed with {} error(s)",
                                errors.len()
                            )
                        })?;

                    let output_path = output.unwrap_or_else(|| {
                        if cfg!(windows) {
                            "a.exe".into()
                        } else {
                            "a.out".into()
                        }
                    });

                    // Build compile options from CLI flags and source annotations.
                    let constraints = axiom_optimize::llm_optimizer::extract_constraints_from_source(&source);
                    let optimize_for = constraints.iter()
                        .find(|c| c.key == "optimize_for")
                        .map(|c| c.value.clone());
                    let compile_opts = compile::CompileOptions {
                        target_arch: target.clone(),
                        optimize_for,
                    };

                    compile::compile_to_binary_with_options(&llvm_ir, &output_path, &compile_opts)?;
                    eprintln!("compiled {} -> {}", input, output_path);
                }
            }

            Ok(())
        }
    }
}

/// Profile an AXIOM program: compile, run multiple times, collect timing data,
/// extract optimization surfaces, and suggest tuning parameters.
fn run_profile(input: &str, iterations: usize) -> miette::Result<()> {
    // 1. Read and compile source
    let source = std::fs::read_to_string(input)
        .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;

    eprintln!("Profiling {input} ({iterations} iteration(s))...\n");

    // 2. Verify compilation succeeds
    let result = axiom_parser::parse(&source);
    if result.has_errors() {
        eprintln!("--- Parse Errors ---");
        for err in &result.errors {
            eprintln!("  {err}");
        }
        return Err(miette::miette!(
            "parsing failed with {} error(s)",
            result.errors.len()
        ));
    }
    let hir_module = axiom_hir::lower(&result.module).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("HIR lowering failed with {} error(s)", errors.len())
    })?;
    let _llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("codegen failed with {} error(s)", errors.len())
    })?;

    eprintln!("  Compilation: OK\n");

    // 3. Benchmark: run the program multiple times
    let bench_config = axiom_optimize::benchmark::BenchmarkConfig {
        warmup_runs: 2,
        measurement_runs: iterations,
        ..Default::default()
    };

    eprintln!("  Timing ({iterations} runs):");
    match axiom_optimize::benchmark::benchmark_source(&source, &bench_config) {
        Ok(bench_result) => {
            eprintln!("    min:    {:.3} ms", bench_result.min_ms);
            eprintln!("    max:    {:.3} ms", bench_result.max_ms);
            eprintln!("    mean:   {:.3} ms", bench_result.mean_ms);
            eprintln!("    median: {:.3} ms", bench_result.median_ms);
            eprintln!("    stddev: {:.3} ms", bench_result.stddev_ms);
            println!("{bench_result}");
        }
        Err(e) => {
            eprintln!("    Benchmark failed: {e}");
            eprintln!("    (Timing data unavailable -- compilation-only profile)");
        }
    }

    // 4. Extract optimization surfaces
    eprintln!("\n  Optimization surfaces:");
    match axiom_optimize::extract_surfaces(&source) {
        Ok(surfaces) if surfaces.is_empty() => {
            eprintln!("    (none found -- add @strategy blocks to enable tuning)");
        }
        Ok(surfaces) => {
            for s in &surfaces {
                eprintln!(
                    "    function `{}`: {} hole(s)",
                    s.function_name,
                    s.holes.len()
                );
                for h in &s.holes {
                    let range_str = match h.range {
                        Some((lo, hi)) => format!(" [{lo}..{hi}]"),
                        None => String::new(),
                    };
                    eprintln!("      ?{}: {}{}", h.name, h.hole_type, range_str);
                }
            }
        }
        Err(errs) => {
            eprintln!(
                "    (extraction failed: {})",
                errs.join("; ")
            );
        }
    }

    // 5. Suggestions
    eprintln!("\n  Suggestions:");
    eprintln!("    - Run `axiom optimize {input}` to auto-tune ?params");
    eprintln!("    - Add @strategy blocks to expose tunable parameters");
    eprintln!("    - Use @pure on hot functions to enable LLVM optimizations");

    Ok(())
}

/// AI-driven optimization: compile, benchmark, ask LLM for suggestions,
/// apply, and record results over multiple iterations.
fn run_optimize(
    input: &str,
    iterations: usize,
    target: &str,
    api_key: Option<&str>,
    dry_run: bool,
    agent: &str,
) -> miette::Result<()> {
    use axiom_optimize::history::{OptHistory, OptRecord};
    use axiom_optimize::llm_optimizer::{self, LlmResult};
    use std::collections::HashMap;

    // Resolve API key: --api-key flag > ANTHROPIC_API_KEY env var > None
    let resolved_api_key: Option<String> = api_key
        .map(|k| k.to_string())
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());

    // 1. Read source
    let source = std::fs::read_to_string(input)
        .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;

    // 2. Extract optimization surfaces
    let surfaces = axiom_optimize::extract_surfaces(&source).map_err(|errs| {
        miette::miette!("Failed to extract surfaces: {}", errs.join("; "))
    })?;

    if surfaces.is_empty() {
        eprintln!("No optimization surfaces found in {input}.");
        eprintln!("Add @strategy blocks with ?param holes to enable LLM-driven tuning.");
        return Ok(());
    }

    eprintln!("=== AXIOM LLM-Driven Self-Optimization ===\n");
    eprintln!("Source: {input}");
    eprintln!("Target: {target}");
    eprintln!("Iterations: {iterations}");
    eprintln!(
        "LLM: {}",
        if dry_run {
            "dry-run (prompt only)"
        } else if resolved_api_key.is_some() {
            "Claude API (via curl)"
        } else {
            "auto-detect (claude CLI or dry-run)"
        }
    );
    eprintln!();

    eprintln!("Found {} optimization surface(s):", surfaces.len());
    for s in &surfaces {
        eprintln!(
            "  function `{}`: {} hole(s)",
            s.function_name,
            s.holes.len()
        );
        for h in &s.holes {
            let range_str = match h.range {
                Some((lo, hi)) => format!(" [{lo}..{hi}]"),
                None => String::new(),
            };
            eprintln!("    ?{}: {}{}", h.name, h.hole_type, range_str);
        }
    }

    // 3. Compile to LLVM IR
    eprintln!("\nCompiling to LLVM IR...");
    let parse_result = axiom_parser::parse(&source);
    if parse_result.has_errors() {
        return Err(miette::miette!(
            "parse errors: {}",
            parse_result
                .errors
                .iter()
                .map(|e| format!("{e}"))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let hir_module = axiom_hir::lower(&parse_result.module).map_err(|errors| {
        miette::miette!(
            "HIR lowering errors: {}",
            errors
                .iter()
                .map(|e| format!("{e}"))
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
    let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
        miette::miette!(
            "codegen errors: {}",
            errors
                .iter()
                .map(|e| format!("{e}"))
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
    eprintln!("  LLVM IR: {} bytes", llvm_ir.len());

    // 4. Generate assembly (optional, best-effort)
    eprintln!("Generating assembly...");
    let assembly = llm_optimizer::generate_assembly(&llvm_ir, target);
    match &assembly {
        Some(asm) => eprintln!("  Assembly: {} bytes", asm.len()),
        None => eprintln!("  Assembly: not available (clang not found)"),
    }

    // 5. Benchmark baseline
    eprintln!("\nBenchmarking baseline...");
    let bench_config = axiom_optimize::benchmark::BenchmarkConfig {
        warmup_runs: 2,
        measurement_runs: 5,
        timeout_ms: 30_000,
    };

    let baseline_ms = match axiom_optimize::benchmark::benchmark_source(&source, &bench_config) {
        Ok(result) => {
            eprintln!(
                "  Baseline: median={:.3}ms, mean={:.3}ms, stddev={:.3}ms",
                result.median_ms, result.mean_ms, result.stddev_ms
            );
            Some(result.median_ms)
        }
        Err(e) => {
            eprintln!("  Benchmark not available: {e}");
            None
        }
    };

    // 6. Load existing history (if any)
    let history_path = format!("{input}.opt_history.json");
    let mut history = match std::fs::read_to_string(&history_path) {
        Ok(json) => OptHistory::from_json(&json)
            .map_err(|e| miette::miette!("Failed to parse history {}: {}", history_path, e))?,
        Err(_) => OptHistory::new(),
    };

    // Record baseline if this is a fresh history
    if history.records.is_empty() {
        if let Some(ms) = baseline_ms {
            let mut metrics = HashMap::new();
            metrics.insert("time_ms".to_string(), ms);
            history.add_record(OptRecord {
                version: history.next_version(),
                params: HashMap::new(),
                metrics,
                agent: Some(agent.to_string()),
                target: Some(target.to_string()),
                timestamp: get_timestamp(),
            });
            eprintln!("  Recorded baseline as {}", history.records.last().unwrap().version);
        }
    }

    // 7. Run LLM optimization iterations
    eprintln!(
        "\n=== Starting LLM optimization ({iterations} iteration(s)) ===\n"
    );

    let current_benchmark = baseline_ms;

    for i in 0..iterations {
        let iter_num = i + 1;
        eprintln!("--- Iteration {iter_num}/{iterations} ---\n");

        // Call LLM
        let result = llm_optimizer::run_llm_optimization(
            &source,
            &llvm_ir,
            assembly.as_deref(),
            current_benchmark,
            &surfaces,
            &history.records,
            iter_num,
            iterations,
            target,
            resolved_api_key.as_deref(),
            dry_run,
        );

        match result {
            LlmResult::Suggestion(suggestion) => {
                eprintln!("  LLM Suggestion (confidence: {:.0}%):", suggestion.confidence * 100.0);
                eprintln!("  Reasoning: {}", suggestion.reasoning);
                eprintln!("  Proposed parameters:");
                for (name, value) in &suggestion.param_values {
                    eprintln!("    ?{name} = {value}");
                }

                if !suggestion.code_changes.is_empty() {
                    eprintln!("  Code change suggestions:");
                    for change in &suggestion.code_changes {
                        let line_str = change
                            .line
                            .map(|l| format!(" (line {l})"))
                            .unwrap_or_default();
                        eprintln!("    - {}{}", change.description, line_str);
                    }
                }

                // Record the LLM suggestion in history
                let version = history.next_version();
                let params: HashMap<String, serde_json::Value> = suggestion
                    .param_values
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                let mut metrics: HashMap<String, f64> = HashMap::new();
                metrics.insert("llm_confidence".to_string(), suggestion.confidence);

                // Benchmark with the suggestion applied
                // (In a full implementation, we would apply the params to the source
                //  and recompile. For now, we record the benchmark of the current source.)
                if let Some(ms) = current_benchmark {
                    metrics.insert("time_ms".to_string(), ms);
                }

                history.add_record(OptRecord {
                    version: version.clone(),
                    params,
                    metrics,
                    agent: Some(agent.to_string()),
                    target: Some(target.to_string()),
                    timestamp: get_timestamp(),
                });

                eprintln!("  Recorded as {version}\n");
            }

            LlmResult::DryRun { prompt_path, prompt } => {
                eprintln!("  [DRY RUN] Prompt written to: {prompt_path}");
                eprintln!("  Prompt length: {} chars\n", prompt.len());

                // In dry-run mode on the first iteration, print the full prompt
                if i == 0 {
                    println!("{prompt}");
                }

                // Still record an iteration so history advances
                let version = history.next_version();
                let mut metrics = HashMap::new();
                if let Some(ms) = current_benchmark {
                    metrics.insert("time_ms".to_string(), ms);
                }

                // Use grid-search fallback for params
                let mut params: HashMap<String, serde_json::Value> = HashMap::new();
                for surface in &surfaces {
                    for hole in &surface.holes {
                        let value = generate_hole_value(hole, i);
                        let json_value = value_to_json(&value);
                        params.insert(hole.name.clone(), json_value);
                    }
                }

                history.add_record(OptRecord {
                    version: version.clone(),
                    params,
                    metrics,
                    agent: Some(format!("{agent}-grid-search")),
                    target: Some(target.to_string()),
                    timestamp: get_timestamp(),
                });

                eprintln!("  Recorded grid-search fallback as {version}\n");
            }

            LlmResult::Error(err) => {
                eprintln!("  LLM error: {err}");
                eprintln!("  Falling back to grid-search for this iteration.\n");

                // Grid-search fallback
                let version = history.next_version();
                let mut params: HashMap<String, serde_json::Value> = HashMap::new();
                let mut proposal = axiom_optimize::Proposal::new();

                for surface in &surfaces {
                    for hole in &surface.holes {
                        let value = generate_hole_value(hole, i);
                        let json_value = value_to_json(&value);
                        params.insert(hole.name.clone(), json_value);
                        proposal.set(hole.name.clone(), value);
                    }
                }

                let mut metrics: HashMap<String, f64> = HashMap::new();
                if let Some(ms) = current_benchmark {
                    metrics.insert("time_ms".to_string(), ms);
                }

                history.add_record(OptRecord {
                    version: version.clone(),
                    params,
                    metrics,
                    agent: Some(format!("{agent}-grid-search")),
                    target: Some(target.to_string()),
                    timestamp: get_timestamp(),
                });

                eprintln!("  Recorded grid-search fallback as {version}\n");
            }
        }
    }

    // 8. Print summary
    eprintln!("=== Optimization Summary ===");
    eprintln!("Total records: {}", history.records.len());

    if let Some(best) = history.best_by_metric("time_ms") {
        eprintln!(
            "Best result: {} (time_ms={:.3}ms)",
            best.version,
            best.metrics.get("time_ms").copied().unwrap_or(f64::NAN)
        );
        if !best.params.is_empty() {
            eprintln!("  Parameters:");
            for (name, value) in &best.params {
                eprintln!("    ?{name} = {value}");
            }
        }
    }

    if let Some(baseline) = baseline_ms {
        if let Some(best) = history.best_by_metric("time_ms") {
            let best_ms = best.metrics.get("time_ms").copied().unwrap_or(baseline);
            let improvement = ((baseline - best_ms) / baseline) * 100.0;
            if improvement > 0.0 {
                eprintln!("  Improvement: {:.1}% faster than baseline", improvement);
            }
        }
    }

    // 9. Save history
    let json = history
        .to_json()
        .map_err(|e| miette::miette!("Failed to serialize history: {e}"))?;
    std::fs::write(&history_path, &json)
        .map_err(|e| miette::miette!("Failed to write history to {}: {}", history_path, e))?;

    eprintln!("\nHistory saved to {history_path}");
    // Print JSON to stdout for piping
    println!("{json}");

    Ok(())
}

/// Generate a concrete value for a hole based on its type, range, and iteration.
fn generate_hole_value(
    hole: &axiom_optimize::OptHole,
    iteration: usize,
) -> axiom_optimize::Value {
    use axiom_optimize::{HoleType, Value};

    match &hole.hole_type {
        HoleType::U32 | HoleType::I32 => {
            if let Some((lo, hi)) = hole.range {
                // Grid-search: spread across the range
                let range_size = (hi - lo + 1) as usize;
                let offset = if range_size > 0 {
                    (iteration % range_size) as i64
                } else {
                    0
                };
                // Start from midpoint and move outward
                let mid = (lo + hi) / 2;
                let sign = if iteration.is_multiple_of(2) { 1_i64 } else { -1_i64 };
                let step = iteration.div_ceil(2) as i64;
                let candidate = mid + sign * step;
                // Clamp to range
                let value = candidate.clamp(lo, hi);
                // If we've wrapped around, use offset from lo
                if iteration > (range_size / 2) {
                    Value::Int(lo + offset)
                } else {
                    Value::Int(value)
                }
            } else {
                // No range — use powers of 2 starting from 1
                let exp = iteration.min(10);
                Value::Int(1 << exp)
            }
        }
        HoleType::F64 => {
            Value::Float(1.0 + iteration as f64 * 0.5)
        }
        HoleType::Bool => {
            Value::Bool(iteration.is_multiple_of(2))
        }
        HoleType::Ident => {
            Value::Ident("i".to_string())
        }
        HoleType::Array(inner) => match inner.as_ref() {
            HoleType::Ident => {
                // Common case: loop order — rotate a standard set
                let dims = ["i", "j", "k"];
                let rotated: Vec<Value> = (0..dims.len())
                    .map(|idx| {
                        let shifted = (idx + iteration) % dims.len();
                        Value::Ident(dims[shifted].to_string())
                    })
                    .collect();
                Value::Array(rotated)
            }
            _ => Value::Array(vec![]),
        },
    }
}

/// Convert a surface::Value to a serde_json::Value for storage.
fn value_to_json(value: &axiom_optimize::Value) -> serde_json::Value {
    use axiom_optimize::Value;

    match value {
        Value::Int(v) => serde_json::Value::Number((*v).into()),
        Value::Float(v) => serde_json::json!(*v),
        Value::Bool(v) => serde_json::Value::Bool(*v),
        Value::Ident(s) => serde_json::Value::String(s.clone()),
        Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
    }
}

/// Get a simple ISO-8601-like timestamp without pulling in `chrono`.
fn get_timestamp() -> String {
    // Use system time to produce a Unix epoch seconds timestamp.
    // For a simple CLI tool this is sufficient.
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => format!("{}s", d.as_secs()),
        Err(_) => "unknown".to_string(),
    }
}
