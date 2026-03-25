//! AXIOM compiler CLI driver.

mod compile;
mod lsp;
mod mcp;

use clap::{Parser, Subcommand};
use std::collections::HashSet;
use std::path::Path;

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

    /// Format an AXIOM source file (parse → HIR → pretty-print)
    Fmt {
        /// Input .axm file
        input: String,
    },

    /// Generate documentation from AXIOM source
    Doc {
        /// Input .axm file
        input: String,
    },

    /// Start a minimal LSP server over stdio (E4)
    Lsp {},

    /// Profile-Guided Optimization: instrument, train, recompile (L4)
    Pgo {
        /// Input .axm file
        input: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,

        /// Arguments to pass to the training run
        #[arg(long, num_args = 0..)]
        training_args: Vec<String>,
    },

    /// Watch a file for changes and recompile on save (G4)
    Watch {
        /// Input .axm file to watch
        input: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Build project from axiom.toml manifest (E5)
    Build {
        /// Path to axiom.toml (defaults to ./axiom.toml)
        #[arg(long)]
        manifest: Option<String>,
    },

    /// AI-driven source-to-source rewrite (S3)
    Rewrite {
        /// Input .axm file
        input: String,

        /// Just print the LLM prompt without calling the API
        #[arg(long)]
        dry_run: bool,

        /// Anthropic API key (or set ANTHROPIC_API_KEY env var)
        #[arg(long)]
        api_key: Option<String>,

        /// Write rewritten source to this output file
        #[arg(short, long)]
        output: Option<String>,
    },
}

/// Process `@include "path.axm"` directives in source code.
///
/// Scans the source for lines matching `@include "path"` and replaces them
/// with the contents of the referenced file. Supports recursive includes up
/// to a depth of 10. Paths are resolved relative to `base_dir`.
fn process_includes(source: &str, base_dir: &Path, depth: usize, visited: &mut HashSet<String>) -> miette::Result<String> {
    if depth > 10 {
        return Err(miette::miette!("@include depth exceeds 10 — possible circular include"));
    }

    let mut output = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@include") {
            let rest = rest.trim();
            // Extract the path from quotes
            if let Some(path_str) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                let include_path = base_dir.join(path_str);
                let canonical = include_path.display().to_string();
                if visited.contains(&canonical) {
                    // Already included — skip to avoid infinite recursion
                    output.push_str(&format!("// [already included: {path_str}]\n"));
                    continue;
                }
                visited.insert(canonical.clone());
                let included_source = std::fs::read_to_string(&include_path)
                    .map_err(|e| miette::miette!("@include failed for \"{}\": {}", include_path.display(), e))?;
                let included_dir = include_path.parent().unwrap_or(base_dir);
                let processed = process_includes(&included_source, included_dir, depth + 1, visited)?;
                output.push_str(&processed);
                output.push('\n');
            } else {
                // Not a valid include — pass through as-is
                output.push_str(line);
                output.push('\n');
            }
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    Ok(output)
}

/// Read a source file and process `@include` directives.
fn read_source_with_includes(input: &str) -> miette::Result<String> {
    let source = std::fs::read_to_string(input)
        .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;
    let base_dir = Path::new(input).parent().unwrap_or(Path::new("."));
    let mut visited = HashSet::new();
    visited.insert(
        std::fs::canonicalize(input)
            .unwrap_or_else(|_| Path::new(input).to_path_buf())
            .display()
            .to_string(),
    );
    process_includes(&source, base_dir, 0, &mut visited)
}

/// Generate markdown documentation from an AXIOM source file.
///
/// Parses the source, extracts `@intent` and `@module` annotations, lists all
/// function signatures, and prints a markdown document to stdout.
fn run_doc(input: &str) -> miette::Result<()> {
    let source = read_source_with_includes(input)?;

    let result = axiom_parser::parse(&source);
    if result.has_errors() {
        eprintln!("--- Parse Errors ---");
        for err in &result.errors {
            eprintln!("  {err}");
        }
        return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
    }

    let module = &result.module;

    // Header
    let module_name = module.name.as_ref().map(|s| s.node.as_str()).unwrap_or("(unnamed)");
    println!("# Module: {module_name}");
    println!();

    // Module-level annotations
    let mut has_module_annotations = false;
    for ann in &module.annotations {
        match &ann.node {
            axiom_parser::ast::Annotation::Intent(text) => {
                if !has_module_annotations {
                    println!("## Module Annotations");
                    println!();
                    has_module_annotations = true;
                }
                println!("- **Intent**: {text}");
            }
            axiom_parser::ast::Annotation::Module(name) => {
                if !has_module_annotations {
                    println!("## Module Annotations");
                    println!();
                    has_module_annotations = true;
                }
                println!("- **Module**: {name}");
            }
            _ => {}
        }
    }
    if has_module_annotations {
        println!();
    }

    // Functions
    println!("## Functions");
    println!();

    for item in &module.items {
        match &item.node {
            axiom_parser::ast::Item::Function(f) => {
                let name = &f.name.node;
                let params: Vec<String> = f.params.iter().map(|p| {
                    format!("{}: {:?}", p.name.node, p.ty)
                }).collect();
                let ret = format!("{:?}", f.return_type);
                println!("### `fn {name}({})`", params.join(", "));
                println!();
                println!("- **Returns**: `{ret}`");

                // Extract annotations
                for ann in &f.annotations {
                    match &ann.node {
                        axiom_parser::ast::Annotation::Intent(text) => {
                            println!("- **Intent**: {text}");
                        }
                        axiom_parser::ast::Annotation::Pure => {
                            println!("- **Pure**: yes");
                        }
                        axiom_parser::ast::Annotation::Const => {
                            println!("- **Const**: yes (compile-time evaluable)");
                        }
                        axiom_parser::ast::Annotation::Complexity(c) => {
                            println!("- **Complexity**: {c}");
                        }
                        axiom_parser::ast::Annotation::Export => {
                            println!("- **Exported**: yes");
                        }
                        axiom_parser::ast::Annotation::Inline(hint) => {
                            println!("- **Inline**: {hint:?}");
                        }
                        _ => {}
                    }
                }
                println!();
            }
            axiom_parser::ast::Item::ExternFunction(f) => {
                let name = &f.name.node;
                let params: Vec<String> = f.params.iter().map(|p| {
                    format!("{}: {:?}", p.name.node, p.ty)
                }).collect();
                let ret = format!("{:?}", f.return_type);
                println!("### `extern fn {name}({})`", params.join(", "));
                println!();
                println!("- **Returns**: `{ret}`");
                println!("- **External**: yes");
                println!();
            }
            axiom_parser::ast::Item::Struct(s) => {
                let name = &s.name.node;
                println!("### `struct {name}`");
                println!();
                for field in &s.fields {
                    println!("- `{}`: `{:?}`", field.name.node, field.ty);
                }
                println!();
            }
            _ => {}
        }
    }

    Ok(())
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

        Commands::Fmt { input } => {
            let source = std::fs::read_to_string(&input)
                .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;
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
                    Ok(())
                }
                Err(errors) => {
                    eprintln!("--- HIR Lowering Errors ---");
                    for err in &errors {
                        eprintln!("  {err}");
                    }
                    Err(miette::miette!("HIR lowering failed with {} error(s)", errors.len()))
                }
            }
        }

        Commands::Compile { input, output, emit, target } => {
            let source = read_source_with_includes(&input)?;

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
                    return Err(miette::miette!("unknown --emit level '{}'. Valid: tokens, ast, hir, llvm-ir", level));
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
                        ir_text: Some(llvm_ir.clone()),
                    };

                    compile::compile_to_binary_with_options(&llvm_ir, &output_path, &compile_opts)?;
                    eprintln!("compiled {} -> {}", input, output_path);
                }
            }

            Ok(())
        }

        Commands::Doc { input } => run_doc(&input),

        // E4: LSP Server
        Commands::Lsp {} => {
            lsp::run_lsp_server()
                .map_err(|e| miette::miette!("LSP server error: {e}"))?;
            Ok(())
        }

        // L4: PGO Bootstrap
        Commands::Pgo { input, output, training_args } => {
            let source = read_source_with_includes(&input)?;
            let result = axiom_parser::parse(&source);
            if result.has_errors() {
                for err in &result.errors {
                    eprintln!("  {err}");
                }
                return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
            }
            let hir_module = axiom_hir::lower(&result.module).map_err(|errors| {
                for err in &errors { eprintln!("  {err}"); }
                miette::miette!("HIR lowering failed with {} error(s)", errors.len())
            })?;
            let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
                for err in &errors { eprintln!("  {err}"); }
                miette::miette!("codegen failed with {} error(s)", errors.len())
            })?;

            let output_path = output.unwrap_or_else(|| {
                if cfg!(windows) { "a.exe".into() } else { "a.out".into() }
            });

            let message = compile::compile_with_pgo(&llvm_ir, &output_path, &training_args)?;
            eprintln!("{message}");
            Ok(())
        }

        // G4: Watch (Hot Reload concept)
        Commands::Watch { input, output } => {
            run_watch(&input, output.as_deref())
        }

        // E5: Build from axiom.toml
        Commands::Build { manifest } => {
            let manifest_path = manifest.unwrap_or_else(|| "axiom.toml".to_string());
            run_build(&manifest_path)
        }

        // S3: Source-to-Source AI Rewrite
        Commands::Rewrite { input, dry_run, api_key, output } => {
            run_rewrite(&input, dry_run, api_key.as_deref(), output.as_deref())
        }
    }
}

// ---------------------------------------------------------------------------
// G4: Watch — poll file mtime and recompile on change
// ---------------------------------------------------------------------------

fn run_watch(input: &str, output: Option<&str>) -> miette::Result<()> {
    use std::time::{Duration, SystemTime};

    let output_path = output.map(|s| s.to_string()).unwrap_or_else(|| {
        if cfg!(windows) { "a.exe".into() } else { "a.out".into() }
    });

    eprintln!("[AXIOM Watch] Watching {} -> {}", input, output_path);
    eprintln!("[AXIOM Watch] Press Ctrl+C to stop\n");

    let mut last_mtime: Option<SystemTime> = None;

    loop {
        // Check file modification time
        let metadata = match std::fs::metadata(input) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[AXIOM Watch] Error reading {}: {}", input, e);
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        };

        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        let changed = match last_mtime {
            Some(prev) => mtime != prev,
            None => true, // First iteration — always compile
        };

        if changed {
            last_mtime = Some(mtime);
            let start = std::time::Instant::now();

            // Attempt compilation
            match try_compile(input, &output_path) {
                Ok(()) => {
                    let elapsed = start.elapsed();
                    eprintln!(
                        "[AXIOM Watch] Compiled OK in {:.3}ms -> {}",
                        elapsed.as_secs_f64() * 1000.0,
                        output_path
                    );
                }
                Err(e) => {
                    let elapsed = start.elapsed();
                    eprintln!(
                        "[AXIOM Watch] Error after {:.3}ms: {}",
                        elapsed.as_secs_f64() * 1000.0,
                        e
                    );
                }
            }
        }

        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Try to compile a single .axm file to a binary. Returns Ok on success.
fn try_compile(input: &str, output_path: &str) -> miette::Result<()> {
    let source = read_source_with_includes(input)?;
    let result = axiom_parser::parse(&source);
    if result.has_errors() {
        let msgs: Vec<String> = result.errors.iter().map(|e| format!("{e}")).collect();
        return Err(miette::miette!("parse errors:\n  {}", msgs.join("\n  ")));
    }
    let hir_module = axiom_hir::lower(&result.module).map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
        miette::miette!("HIR errors:\n  {}", msgs.join("\n  "))
    })?;
    let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
        miette::miette!("codegen errors:\n  {}", msgs.join("\n  "))
    })?;

    let constraints = axiom_optimize::llm_optimizer::extract_constraints_from_source(&source);
    let optimize_for = constraints.iter()
        .find(|c| c.key == "optimize_for")
        .map(|c| c.value.clone());
    let compile_opts = compile::CompileOptions {
        target_arch: None,
        optimize_for,
        ir_text: Some(llvm_ir.clone()),
    };

    compile::compile_to_binary_with_options(&llvm_ir, output_path, &compile_opts)
}

// ---------------------------------------------------------------------------
// E5: Package Manager — build from axiom.toml manifest
// ---------------------------------------------------------------------------

/// Minimal axiom.toml manifest format.
#[derive(Debug)]
struct AxiomManifest {
    package: PackageInfo,
    dependencies: std::collections::HashMap<String, DependencySpec>,
}

#[derive(Debug)]
struct PackageInfo {
    name: String,
    version: String,
}

#[derive(Debug)]
struct DependencySpec {
    path: String,
}

fn run_build(manifest_path: &str) -> miette::Result<()> {
    let manifest_str = std::fs::read_to_string(manifest_path)
        .map_err(|e| miette::miette!("Failed to read {}: {}", manifest_path, e))?;

    // Parse as TOML manually (minimal — we don't want to add a toml crate dep)
    let manifest = parse_axiom_toml(&manifest_str)?;

    eprintln!("[AXIOM Build] Building package: {} v{}", manifest.package.name, manifest.package.version);

    let manifest_dir = Path::new(manifest_path).parent().unwrap_or(Path::new("."));

    // Collect dependency sources
    let mut combined_source = String::new();

    for (dep_name, dep_spec) in &manifest.dependencies {
        let dep_dir = manifest_dir.join(&dep_spec.path);
        eprintln!("[AXIOM Build] Including dependency: {dep_name} ({})", dep_dir.display());

        // Find all .axm files in the dependency directory
        let axm_files = find_axm_files(&dep_dir);
        if axm_files.is_empty() {
            eprintln!("[AXIOM Build] Warning: no .axm files found in {}", dep_dir.display());
        }
        for file in &axm_files {
            let source = std::fs::read_to_string(file)
                .map_err(|e| miette::miette!("Failed to read {}: {}", file.display(), e))?;
            combined_source.push_str(&format!("// --- dependency: {} ({}) ---\n", dep_name, file.display()));
            combined_source.push_str(&source);
            combined_source.push('\n');
        }
    }

    // Find the main source file: look for main.axm or src/main.axm
    let main_candidates = [
        manifest_dir.join("main.axm"),
        manifest_dir.join("src").join("main.axm"),
    ];
    let main_file = main_candidates.iter().find(|p| p.exists());

    match main_file {
        Some(main_path) => {
            let main_source = read_source_with_includes(&main_path.display().to_string())?;
            combined_source.push_str(&format!("// --- main: {} ---\n", main_path.display()));
            combined_source.push_str(&main_source);

            // Compile
            let output_path = if cfg!(windows) {
                format!("{}.exe", manifest.package.name)
            } else {
                manifest.package.name.clone()
            };

            let result = axiom_parser::parse(&combined_source);
            if result.has_errors() {
                for err in &result.errors {
                    eprintln!("  {err}");
                }
                return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
            }
            let hir_module = axiom_hir::lower(&result.module).map_err(|errors| {
                for err in &errors { eprintln!("  {err}"); }
                miette::miette!("HIR lowering failed with {} error(s)", errors.len())
            })?;
            let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
                for err in &errors { eprintln!("  {err}"); }
                miette::miette!("codegen failed with {} error(s)", errors.len())
            })?;

            compile::compile_to_binary(&llvm_ir, &output_path)?;
            eprintln!("[AXIOM Build] Built {} -> {}", manifest.package.name, output_path);
            Ok(())
        }
        None => {
            eprintln!("[AXIOM Build] No main.axm found (checked main.axm and src/main.axm)");
            if combined_source.is_empty() {
                return Err(miette::miette!("No source files to build"));
            }
            eprintln!("[AXIOM Build] Dependencies collected ({} bytes of source)", combined_source.len());
            Ok(())
        }
    }
}

/// Minimal TOML parser for axiom.toml. Uses serde_json as intermediate.
/// This avoids adding a toml dependency.
fn parse_axiom_toml(input: &str) -> miette::Result<AxiomManifest> {
    let mut package_name = String::new();
    let mut package_version = String::new();
    let mut deps: std::collections::HashMap<String, DependencySpec> = std::collections::HashMap::new();

    let mut current_section = String::new();

    for line in input.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Section header
        if trimmed.starts_with('[') {
            current_section = trimmed
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_string();
            continue;
        }

        // Key = value
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            let unquoted = value.trim_matches('"');

            match current_section.as_str() {
                "package" => {
                    match key {
                        "name" => package_name = unquoted.to_string(),
                        "version" => package_version = unquoted.to_string(),
                        _ => {}
                    }
                }
                "dependencies" => {
                    // Parse: dep_name = { path = "..." }
                    if let Some(path_start) = value.find("path") {
                        let rest = &value[path_start..];
                        if let Some(eq_pos) = rest.find('=') {
                            let path_val = rest[eq_pos + 1..]
                                .trim()
                                .trim_matches(|c: char| c == '"' || c == '\'' || c == '{' || c == '}' || c == ' ');
                            deps.insert(key.to_string(), DependencySpec {
                                path: path_val.to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if package_name.is_empty() {
        return Err(miette::miette!("axiom.toml missing [package] name"));
    }

    Ok(AxiomManifest {
        package: PackageInfo {
            name: package_name,
            version: if package_version.is_empty() { "0.0.0".to_string() } else { package_version },
        },
        dependencies: deps,
    })
}

/// Find all .axm files in a directory (non-recursive).
fn find_axm_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "axm") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
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

// ---------------------------------------------------------------------------
// S3: Source-to-Source AI Rewrite
// ---------------------------------------------------------------------------

fn run_rewrite(input: &str, dry_run: bool, api_key: Option<&str>, output: Option<&str>) -> miette::Result<()> {
    use axiom_optimize::llm_optimizer::{self, LlmResult};

    // Resolve API key
    let resolved_api_key: Option<String> = api_key
        .map(|k| k.to_string())
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());

    // Read source
    let source = read_source_with_includes(input)?;

    eprintln!("=== AXIOM Source-to-Source Rewrite ===\n");
    eprintln!("Source: {input} ({} bytes)", source.len());
    eprintln!(
        "Mode: {}",
        if dry_run {
            "dry-run (prompt only)"
        } else if resolved_api_key.is_some() {
            "Claude API"
        } else {
            "auto-detect (claude CLI or dry-run)"
        }
    );
    eprintln!();

    // Verify source parses correctly
    let result = axiom_parser::parse(&source);
    if result.has_errors() {
        eprintln!("Warning: source has parse errors:");
        for err in &result.errors {
            eprintln!("  {err}");
        }
        eprintln!();
    }

    // Extract constraints for reporting
    let constraints = llm_optimizer::extract_constraints_from_source(&source);
    if !constraints.is_empty() {
        eprintln!("Detected constraints:");
        for c in &constraints {
            eprintln!("  {}: {}", c.key, c.value);
        }
        eprintln!();
    }

    // Run rewrite
    let rewrite_result = llm_optimizer::run_rewrite(
        &source,
        resolved_api_key.as_deref(),
        dry_run,
    );

    match rewrite_result {
        LlmResult::Suggestion(suggestion) => {
            eprintln!("Rewrite suggestion (confidence: {:.0}%):\n", suggestion.confidence * 100.0);

            if !suggestion.code_changes.is_empty() {
                eprintln!("Changes applied:");
                for change in &suggestion.code_changes {
                    let line_str = change
                        .line
                        .map(|l| format!(" (line {l})"))
                        .unwrap_or_default();
                    eprintln!("  - {}{}", change.description, line_str);
                }
                eprintln!();
            }

            // The rewritten source is stored in the reasoning field
            let rewritten = &suggestion.reasoning;

            match output {
                Some(out_path) => {
                    std::fs::write(out_path, rewritten)
                        .map_err(|e| miette::miette!("Failed to write {}: {}", out_path, e))?;
                    eprintln!("Rewritten source written to: {out_path}");
                }
                None => {
                    println!("{rewritten}");
                }
            }
        }

        LlmResult::DryRun { prompt_path, prompt } => {
            eprintln!("[DRY RUN] Prompt written to: {prompt_path}");
            eprintln!("Prompt length: {} chars\n", prompt.len());
            println!("{prompt}");
        }

        LlmResult::Error(err) => {
            return Err(miette::miette!("Rewrite failed: {err}"));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests for new features (E5: manifest parsing, G4/L4 helpers)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // E5: axiom.toml manifest parsing

    #[test]
    fn test_parse_axiom_toml_basic() {
        let input = r#"
[package]
name = "my-game"
version = "0.1.0"

[dependencies]
math = { path = "lib/math" }
"#;
        let manifest = parse_axiom_toml(input).unwrap();
        assert_eq!(manifest.package.name, "my-game");
        assert_eq!(manifest.package.version, "0.1.0");
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(manifest.dependencies["math"].path, "lib/math");
    }

    #[test]
    fn test_parse_axiom_toml_no_deps() {
        let input = r#"
[package]
name = "simple"
version = "1.0.0"
"#;
        let manifest = parse_axiom_toml(input).unwrap();
        assert_eq!(manifest.package.name, "simple");
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn test_parse_axiom_toml_missing_name() {
        let input = r#"
[package]
version = "1.0.0"
"#;
        assert!(parse_axiom_toml(input).is_err());
    }

    #[test]
    fn test_parse_axiom_toml_multiple_deps() {
        let input = r#"
[package]
name = "game"
version = "0.2.0"

[dependencies]
math = { path = "lib/math" }
physics = { path = "lib/physics" }
"#;
        let manifest = parse_axiom_toml(input).unwrap();
        assert_eq!(manifest.dependencies.len(), 2);
        assert_eq!(manifest.dependencies["math"].path, "lib/math");
        assert_eq!(manifest.dependencies["physics"].path, "lib/physics");
    }

    #[test]
    fn test_parse_axiom_toml_comments() {
        let input = r#"
# This is a comment
[package]
name = "test"
version = "0.1.0"

# Another comment
[dependencies]
# dep comment
"#;
        let manifest = parse_axiom_toml(input).unwrap();
        assert_eq!(manifest.package.name, "test");
    }

    // G4: Watch helper (try_compile)

    #[test]
    fn test_try_compile_nonexistent_file() {
        let result = try_compile("nonexistent_file_12345.axm", "out.exe");
        assert!(result.is_err());
    }

    // S3: Rewrite command integration

    #[test]
    fn test_process_includes_basic() {
        let source = "let x: i32 = 1;\n";
        let base_dir = Path::new(".");
        let mut visited = HashSet::new();
        let result = process_includes(source, base_dir, 0, &mut visited).unwrap();
        assert_eq!(result, "let x: i32 = 1;\n");
    }

    #[test]
    fn test_process_includes_depth_limit() {
        // Simulate deep include chain — should error at depth > 10
        let source = "@include \"nonexistent.axm\"\n";
        let base_dir = Path::new(".");
        let mut visited = HashSet::new();
        let result = process_includes(source, base_dir, 11, &mut visited);
        assert!(result.is_err());
    }
}
