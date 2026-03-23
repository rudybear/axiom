//! AXIOM compiler CLI driver.

mod compile;

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

    /// Run the optimization protocol on an AXIOM source file
    Optimize {
        /// Input .axm file
        input: String,

        /// Target architecture
        #[arg(long, default_value = "native")]
        target: String,

        /// Agent identifier
        #[arg(long, default_value = "axiom-optimizer")]
        agent: String,

        /// Number of optimization iterations
        #[arg(long, default_value = "1")]
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

        Commands::Optimize {
            input,
            target,
            agent,
            iterations,
        } => run_optimize(&input, &target, &agent, iterations),

        Commands::Compile { input, output, emit } => {
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

                    compile::compile_to_binary(&llvm_ir, &output_path)?;
                    eprintln!("compiled {} -> {}", input, output_path);
                }
            }

            Ok(())
        }
    }
}

/// Run the optimization protocol: extract surfaces, generate proposals,
/// benchmark, and record results.
fn run_optimize(
    input: &str,
    target: &str,
    agent: &str,
    iterations: usize,
) -> miette::Result<()> {
    use axiom_optimize::history::{OptHistory, OptRecord};
    use std::collections::HashMap;

    // 1. Read source
    let source = std::fs::read_to_string(input)
        .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;

    // 2. Extract surfaces
    let surfaces = axiom_optimize::extract_surfaces(&source).map_err(|errs| {
        miette::miette!("Failed to extract surfaces: {}", errs.join("; "))
    })?;

    if surfaces.is_empty() {
        eprintln!("No optimization surfaces found in {input}.");
        return Ok(());
    }

    eprintln!("Found {} optimization surface(s) in {input}:", surfaces.len());
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

    // 3. Load existing history (if any)
    let history_path = format!("{input}.opt_history.json");
    let mut history = match std::fs::read_to_string(&history_path) {
        Ok(json) => OptHistory::from_json(&json)
            .map_err(|e| miette::miette!("Failed to parse history {}: {}", history_path, e))?,
        Err(_) => OptHistory::new(),
    };

    eprintln!(
        "\nStarting optimization ({iterations} iteration(s), target={target}, agent={agent})..."
    );

    // 4. Run iterations
    for i in 0..iterations {
        let version = history.next_version();
        eprintln!("\n--- Iteration {}/{iterations} ({version}) ---", i + 1);

        // Generate a proposal by grid-searching the first integer hole.
        // For the first iteration, use the midpoint of each hole's range.
        // For subsequent iterations, offset from the midpoint.
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

        // Validate the proposal
        if let Err(errors) = axiom_optimize::validate_proposal(&proposal, &surfaces) {
            eprintln!("  Proposal validation failed:");
            for err in &errors {
                eprintln!("    {err}");
            }
            continue;
        }

        eprintln!("  Proposal:");
        for (name, value) in &params {
            eprintln!("    ?{name} = {value}");
        }

        // Benchmark: compile and time the source.
        // We use a lightweight config for optimization iterations.
        let bench_config = axiom_optimize::benchmark::BenchmarkConfig {
            warmup_runs: 1,
            measurement_runs: 3,
            timeout_ms: 30_000,
        };

        let mut metrics: HashMap<String, f64> = HashMap::new();
        match axiom_optimize::benchmark::benchmark_source(&source, &bench_config) {
            Ok(result) => {
                eprintln!("  Benchmark: median={:.3}ms, mean={:.3}ms, stddev={:.3}ms",
                    result.median_ms, result.mean_ms, result.stddev_ms);
                metrics.insert("time_ms".to_string(), result.median_ms);
                metrics.insert("mean_ms".to_string(), result.mean_ms);
                metrics.insert("min_ms".to_string(), result.min_ms);
                metrics.insert("max_ms".to_string(), result.max_ms);
                metrics.insert("stddev_ms".to_string(), result.stddev_ms);
            }
            Err(e) => {
                eprintln!("  Benchmark failed: {e}");
                // Record with no metrics — still useful for tracking what was tried
            }
        }

        // Get a timestamp
        let timestamp = get_timestamp();

        // Record this iteration
        history.add_record(OptRecord {
            version,
            params,
            metrics,
            agent: Some(agent.to_string()),
            target: Some(target.to_string()),
            timestamp,
        });
    }

    // 5. Print summary
    eprintln!("\n=== Optimization Summary ===");
    eprintln!("Total records: {}", history.records.len());

    if let Some(best) = history.best_by_metric("time_ms") {
        eprintln!(
            "Best result: {} (time_ms={:.3})",
            best.version,
            best.metrics.get("time_ms").copied().unwrap_or(f64::NAN)
        );
        eprintln!("  Parameters:");
        for (name, value) in &best.params {
            eprintln!("    ?{name} = {value}");
        }
    }

    // 6. Save history
    let json = history
        .to_json()
        .map_err(|e| miette::miette!("Failed to serialize history: {e}"))?;
    std::fs::write(&history_path, &json)
        .map_err(|e| miette::miette!("Failed to write history to {}: {}", history_path, e))?;

    eprintln!("\nHistory saved to {history_path}");
    // Print the JSON to stdout so it can be piped / captured
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
